use crate::common::{ExecRows, TempDatabase};
use turso_core::{Database, DatabaseOpts, OpenFlags, SqliteDialect};

fn passive_db() -> TempDatabase {
    TempDatabase::builder()
        .with_opts(DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true))
        .with_mvcc(true)
        .build()
}

fn ids(conn: &std::sync::Arc<turso_core::Connection>) -> Vec<i64> {
    conn.exec_rows("SELECT id FROM t ORDER BY id")
        .into_iter()
        .map(|(id,)| id)
        .collect()
}

#[test]
fn restart_after_passive_checkpoint_keeps_committed_rows() {
    let tmp = passive_db();
    let conn = tmp.connect_limbo();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'a')").unwrap();
    conn.execute("INSERT INTO t VALUES (2, 'b')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    conn.close().unwrap();

    let db = Database::open_file_with_flags(
        tmp.io.clone(),
        tmp.path.to_str().unwrap(),
        OpenFlags::default(),
        DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true),
        None,
        std::sync::Arc::new(SqliteDialect),
    )
    .unwrap();
    let conn = db.connect().unwrap();
    assert_eq!(ids(&conn), vec![1, 2]);
}

#[test]
fn restart_after_passive_checkpoint_and_large_follow_up_txn() {
    let tmp = passive_db();
    let conn = tmp.connect_limbo();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'seed')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    conn.execute("BEGIN").unwrap();
    for id in 2..=400 {
        conn.execute(format!("INSERT INTO t VALUES ({id}, 'r{id}')"))
            .unwrap();
    }
    conn.execute("COMMIT").unwrap();
    conn.close().unwrap();

    let db = Database::open_file_with_flags(
        tmp.io.clone(),
        tmp.path.to_str().unwrap(),
        OpenFlags::default(),
        DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true),
        None,
        std::sync::Arc::new(SqliteDialect),
    )
    .unwrap();
    let conn = db.connect().unwrap();
    let count: Vec<(i64,)> = conn.exec_rows("SELECT count(*) FROM t");
    assert_eq!(count, vec![(400,)]);
}

#[test]
fn mixed_truncate_after_passive_then_restart() {
    let tmp = passive_db();
    let conn = tmp.connect_limbo();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'a')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    conn.execute("INSERT INTO t VALUES (2, 'b')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    conn.execute("INSERT INTO t VALUES (3, 'c')").unwrap();
    conn.close().unwrap();

    let db = Database::open_file_with_flags(
        tmp.io.clone(),
        tmp.path.to_str().unwrap(),
        OpenFlags::default(),
        DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true),
        None,
        std::sync::Arc::new(SqliteDialect),
    )
    .unwrap();
    let conn = db.connect().unwrap();
    assert_eq!(ids(&conn), vec![1, 2, 3]);
}

#[test]
fn exported_db_file_after_retained_passive_is_missing_live_tail() {
    let tmp = passive_db();
    let conn = tmp.connect_limbo();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'base')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    conn.execute("INSERT INTO t VALUES (2, 'tail')").unwrap();
    conn.close().unwrap();

    let export_dir = tempfile::TempDir::new().unwrap();
    let export_path = export_dir.path().join("replica.db");
    std::fs::copy(&tmp.path, &export_path).unwrap();

    let db = Database::open_file_with_flags(
        tmp.io.clone(),
        export_path.to_str().unwrap(),
        OpenFlags::default(),
        DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true),
        None,
        std::sync::Arc::new(SqliteDialect),
    )
    .unwrap();
    let replica = db.connect().unwrap();
    assert_eq!(
        ids(&replica),
        vec![1, 2],
        "a replica bootstrapped from the Passive DB file must also see the live log tail"
    );
}
