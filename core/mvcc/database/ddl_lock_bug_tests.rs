//! Each test asserts the correct behavior and fails on PR #9894's DDL-lock
//! design, so each failure marks one distinct bug.

use super::{get_rows, step_until_yield, FixedYieldInjector, MvccTestDbNoConn};
use crate::mvcc::database::CommitYieldPoint;
use crate::mvcc::yield_hooks::YieldPointMarker;
use crate::{Connection, DatabaseOpts, LimboError};
use std::sync::Arc;
use std::time::Duration;

fn names(conn: &Arc<Connection>) -> Vec<String> {
    get_rows(
        conn,
        "SELECT name FROM sqlite_schema WHERE name NOT LIKE '__turso_internal%' ORDER BY name",
    )
    .into_iter()
    .map(|row| row[0].to_string())
    .collect()
}

fn db_with_t() -> MvccTestDbNoConn {
    let db = MvccTestDbNoConn::new_with_random_db();
    let setup = db.connect();
    setup
        .execute("CREATE TABLE t(id INTEGER PRIMARY KEY, c INTEGER)")
        .unwrap();
    setup.execute("INSERT INTO t VALUES (1, 10)").unwrap();
    setup.close().unwrap();
    db
}

/// Two concurrent transactions run one locked DDL each, then a schema write
/// that registers no lock name. Both lazily create the same system table.
fn two_concurrent_txs_then_unlocked_schema_write(
    db: &mut MvccTestDbNoConn,
    unlocked_a: &str,
    unlocked_b: &str,
) {
    {
        let a = db.connect();
        let b = db.connect();
        a.execute("BEGIN CONCURRENT").unwrap();
        a.execute("CREATE TABLE ta(x)").unwrap();
        a.execute(unlocked_a).unwrap();
        b.execute("BEGIN CONCURRENT").unwrap();
        b.execute("CREATE TABLE tb(x)").unwrap();
        let second = b.execute(unlocked_b);
        let commit_a = a.execute("COMMIT");
        let commit_b = second.and_then(|_| b.execute("COMMIT"));
        assert!(commit_a.is_ok(), "first commit failed: {commit_a:?}");
        assert!(
            commit_b.is_err(),
            "the second transaction must not be allowed to create the same system table"
        );
    }
    db.restart_result()
        .expect("database must reopen after two overlapping DDL transactions");
    let conn = db.connect();
    assert_eq!(names(&conn), vec!["ta".to_string()]);
}

#[test]
fn concurrent_create_type_must_not_duplicate_internal_types_table() {
    let mut db =
        MvccTestDbNoConn::new_with_random_db_with_opts(DatabaseOpts::new().with_custom_types(true));
    two_concurrent_txs_then_unlocked_schema_write(
        &mut db,
        "CREATE TYPE t_one(value any) BASE text ENCODE e(value) DECODE d(value)",
        "CREATE TYPE t_two(value any) BASE text ENCODE e(value) DECODE d(value)",
    );
}

#[test]
fn concurrent_cdc_enable_must_not_duplicate_system_table() {
    let mut db = MvccTestDbNoConn::new_with_random_db();
    two_concurrent_txs_then_unlocked_schema_write(
        &mut db,
        "PRAGMA capture_data_changes_conn('full')",
        "PRAGMA capture_data_changes_conn('full')",
    );
}

#[test]
fn autoincrement_create_and_insert_in_concurrent_tx_commits() {
    let db = MvccTestDbNoConn::new_with_random_db();
    let conn = db.connect();
    conn.execute("BEGIN CONCURRENT").unwrap();
    conn.execute("CREATE TABLE ai(id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO ai(v) VALUES ('x')").unwrap();
    conn.execute("COMMIT").unwrap();
    let rows = get_rows(&db.connect(), "SELECT id, v FROM ai");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0].as_int(), Some(1));
}

#[test]
fn rolled_back_autoincrement_ddl_leaves_database_usable() {
    let db = MvccTestDbNoConn::new_with_random_db();
    let conn = db.connect();
    conn.execute("BEGIN CONCURRENT").unwrap();
    conn.execute("CREATE TABLE ai(id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO ai(v) VALUES ('x')").unwrap();
    conn.execute("ROLLBACK").unwrap();

    let other = db.connect();
    other
        .execute("CREATE TABLE after_rollback(x)")
        .expect("an unrelated connection must not be wedged by a rolled-back DDL");
    assert_eq!(names(&other), vec!["after_rollback".to_string()]);
}

#[test]
fn rolled_back_sequence_ddl_after_nextval_leaves_database_usable() {
    let db = MvccTestDbNoConn::new_with_random_db();
    let conn = db.connect();
    conn.execute("BEGIN CONCURRENT").unwrap();
    conn.execute("CREATE SEQUENCE s").unwrap();
    let rows = get_rows(&conn, "SELECT nextval('s')");
    assert_eq!(rows[0][0].as_int(), Some(1));
    conn.execute("ROLLBACK").unwrap();

    let other = db.connect();
    other
        .execute("CREATE TABLE after_rollback(x)")
        .expect("an unrelated connection must not be wedged by a rolled-back CREATE SEQUENCE");
    assert_eq!(names(&other), vec!["after_rollback".to_string()]);
}

#[test]
fn header_field_after_restart_matches_value_before_restart() {
    let mut db = MvccTestDbNoConn::new_with_random_db();
    let before = {
        let a = db.connect();
        let b = db.connect();
        a.execute("BEGIN CONCURRENT").unwrap();
        a.execute("PRAGMA user_version = 5").unwrap();
        b.execute("BEGIN CONCURRENT").unwrap();
        b.execute("PRAGMA user_version = 6").unwrap();
        a.set_yield_injector(Some(FixedYieldInjector::new([
            CommitYieldPoint::BeforeGlobalHeaderUpdate.point(),
        ])));
        let mut commit_a = a.prepare("COMMIT").unwrap();
        assert!(step_until_yield(&mut commit_a, 400));
        b.execute("COMMIT").unwrap();
        a.set_yield_injector(None);
        commit_a.run_ignore_rows().unwrap();
        drop(commit_a);
        get_rows(&db.connect(), "PRAGMA user_version")[0][0].as_int()
    };
    db.restart();
    let after = get_rows(&db.connect(), "PRAGMA user_version")[0][0].as_int();
    assert_eq!(before, after, "user_version changed across restart");
}

/// Two overlapping CREATE TABLE commits. The second commit's schema has the
/// same version as the first, so the shared schema never learns about it.
fn two_overlapping_creates(db: &MvccTestDbNoConn) -> Arc<Connection> {
    let stale = db.connect();
    let a = db.connect();
    let b = db.connect();
    a.execute("BEGIN CONCURRENT").unwrap();
    a.execute("CREATE TABLE ta(x)").unwrap();
    b.execute("BEGIN CONCURRENT").unwrap();
    b.execute("CREATE TABLE tb(x)").unwrap();
    b.execute("INSERT INTO tb VALUES (2)").unwrap();
    a.execute("COMMIT").unwrap();
    b.execute("COMMIT").unwrap();
    stale
}

#[test]
fn table_committed_by_overlapping_ddl_is_visible_to_other_connections() {
    let db = MvccTestDbNoConn::new_with_random_db();
    let stale = two_overlapping_creates(&db);
    let rows = stale
        .prepare("SELECT x FROM tb")
        .and_then(|mut s| {
            let mut out = Vec::new();
            s.run_with_row_callback(|row| {
                out.push(row.get_value(0).as_int());
                Ok(())
            })?;
            Ok(out)
        })
        .expect("tb is committed, so the query must not fail with no such table");
    assert_eq!(rows, vec![Some(2)]);
}

#[test]
fn autocommit_write_to_unrelated_table_is_not_blocked_by_open_ddl() {
    let db = db_with_t();
    let setup = db.connect();
    setup.execute("CREATE TABLE u(x)").unwrap();
    let writer = db.connect();
    writer.set_busy_timeout(Duration::from_millis(50));
    let open_dml = db.connect();
    open_dml.execute("BEGIN CONCURRENT").unwrap();
    open_dml.execute("INSERT INTO t VALUES (2, 20)").unwrap();
    writer
        .execute("INSERT INTO u VALUES (0)")
        .expect("control: an open non-DDL writer does not block autocommit writes");
    open_dml.execute("COMMIT").unwrap();
    let ddl = db.connect();
    ddl.execute("BEGIN CONCURRENT").unwrap();
    ddl.execute("CREATE TABLE brand_new(x)").unwrap();
    writer
        .execute("INSERT INTO u VALUES (1)")
        .expect("u has no DDL in flight, so an autocommit insert must not get Busy");
    ddl.execute("COMMIT").unwrap();
}

#[test]
fn stale_snapshot_ddl_fails_with_a_non_retryable_error() {
    let db = db_with_t();
    let ddl = db.connect();
    ddl.execute("BEGIN CONCURRENT").unwrap();
    get_rows(&ddl, "SELECT count(*) FROM t");
    db.connect()
        .execute("INSERT INTO t VALUES (2, 20)")
        .unwrap();
    let err = ddl.execute("CREATE INDEX ic ON t(c)").unwrap_err();
    assert!(
        matches!(err, LimboError::BusySnapshot | LimboError::SchemaConflict),
        "this snapshot can never become current, so a lock error that invites retry is wrong: {err:?}"
    );
}

/// `a` runs `ddl` inside BEGIN CONCURRENT and stops its COMMIT after the
/// log record is built. `b` begins in that window, then runs `after` once
/// `a` has finished. Returns `b`'s results.
fn begin_in_publish_window(
    db: &MvccTestDbNoConn,
    ddl: &str,
    after: &[&str],
) -> Vec<Result<(), LimboError>> {
    let a = db.connect();
    let b = db.connect();
    a.execute("BEGIN CONCURRENT").unwrap();
    a.execute(ddl).unwrap();
    a.set_yield_injector(Some(FixedYieldInjector::new([
        CommitYieldPoint::LogRecordPrepared.point(),
    ])));
    let mut commit = a.prepare("COMMIT").unwrap();
    assert!(step_until_yield(&mut commit, 400), "COMMIT must yield");
    b.execute("BEGIN CONCURRENT").unwrap();
    get_rows(&b, "SELECT count(*) FROM u");
    a.set_yield_injector(None);
    commit.run_ignore_rows().unwrap();
    drop(commit);
    after.iter().map(|sql| b.execute(*sql)).collect()
}

fn db_with_t_and_u() -> MvccTestDbNoConn {
    let db = db_with_t();
    db.connect().execute("CREATE TABLE u(x)").unwrap();
    db
}

#[test]
fn insert_that_began_in_create_index_publish_window_is_indexed() {
    let mut db = db_with_t_and_u();
    let results = begin_in_publish_window(
        &db,
        "CREATE INDEX ic ON t(c)",
        &["INSERT INTO t VALUES (2, 20)", "COMMIT"],
    );
    db.restart();
    let conn = db.connect();
    let expected = if results.iter().all(|r| r.is_ok()) {
        vec![vec![crate::Value::from_i64(2)]]
    } else {
        vec![]
    };
    assert_eq!(
        get_rows(&conn, "SELECT id FROM t INDEXED BY ic WHERE c = 20"),
        expected
    );
    assert_eq!(
        get_rows(&conn, "PRAGMA integrity_check")[0][0].to_string(),
        "ok"
    );
}

#[test]
fn insert_that_began_in_drop_table_publish_window_is_not_silently_lost() {
    let db = db_with_t_and_u();
    let results = begin_in_publish_window(
        &db,
        "DROP TABLE t",
        &["INSERT INTO t VALUES (2, 20)", "COMMIT"],
    );
    assert!(
        results.iter().any(|r| r.is_err()),
        "t was dropped, so a successful COMMIT means the inserted row was lost: {results:?}"
    );
}

#[test]
fn index_that_began_in_create_table_publish_window_cannot_reuse_the_name() {
    let mut db = db_with_t_and_u();
    let results = begin_in_publish_window(
        &db,
        "CREATE TABLE x(v)",
        &["CREATE INDEX x ON t(c)", "COMMIT"],
    );
    db.restart_result().unwrap_or_else(|e| {
        panic!("database must reopen; second transaction got {results:?}: {e:?}")
    });
    let conn = db.connect();
    assert_eq!(
        get_rows(&conn, "SELECT type FROM sqlite_schema WHERE name = 'x'")
            .into_iter()
            .map(|r| r[0].to_string())
            .collect::<Vec<_>>(),
        vec!["table".to_string()]
    );
}
