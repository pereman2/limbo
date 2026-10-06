use super::*;
use crate::mvcc::database::checkpoint_state_machine::{
    CheckpointState, CheckpointStateMachine, CheckpointYieldPoint,
};
use crate::state_machine::{StateTransition, TransitionResult};
use crate::storage::wal::CheckpointMode;
use crate::{Database, DatabaseOpts, OpenFlags, PlatformIO, SqliteDialect, StepResult};
use std::sync::Arc;

fn drive_checkpoint_sm_until(
    checkpoint_sm: &mut CheckpointStateMachine,
    pager: &crate::Pager,
    stop: impl Fn(&CheckpointState) -> bool,
) {
    for _ in 0..50_000 {
        if stop(&checkpoint_sm.state_for_test()) {
            return;
        }
        match checkpoint_sm.step(&()).unwrap() {
            TransitionResult::Io(io) => io.wait(pager.io.as_ref()).unwrap(),
            TransitionResult::Continue => {}
            TransitionResult::Done(_) => panic!("checkpoint finished before the stop state"),
        }
    }
    panic!(
        "checkpoint did not reach the stop state, last state {:?}",
        checkpoint_sm.state_for_test()
    );
}

fn finish_checkpoint_sm(checkpoint_sm: &mut CheckpointStateMachine, pager: &crate::Pager) {
    for _ in 0..50_000 {
        match checkpoint_sm.step(&()).unwrap() {
            TransitionResult::Io(io) => io.wait(pager.io.as_ref()).unwrap(),
            TransitionResult::Continue => {}
            TransitionResult::Done(_) => return,
        }
    }
    panic!("checkpoint did not finish");
}

fn ids(conn: &Arc<Connection>) -> Vec<i64> {
    get_rows(conn, "SELECT id FROM t ORDER BY id")
        .into_iter()
        .map(|row| row[0].as_int().unwrap())
        .collect()
}

fn new_truncate_sm(db: &MvccTestDbNoConn, conn: &Arc<Connection>) -> CheckpointStateMachine {
    CheckpointStateMachine::new(
        conn.pager.load().clone(),
        db.get_mvcc_store(),
        conn.clone(),
        true,
        conn.get_sync_mode(),
        crate::MAIN_DB_ID,
        CheckpointMode::Truncate {
            upper_bound_inclusive: None,
        },
    )
}

fn new_passive_sm(db: &MvccTestDbNoConn, conn: &Arc<Connection>) -> CheckpointStateMachine {
    CheckpointStateMachine::new(
        conn.pager.load().clone(),
        db.get_mvcc_store(),
        conn.clone(),
        true,
        conn.get_sync_mode(),
        crate::MAIN_DB_ID,
        CheckpointMode::Passive {
            upper_bound_inclusive: None,
        },
    )
}

#[test]
fn known_truncate_with_passive_flag_skips_acquire_lock() {
    let db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'seed')").unwrap();

    let pager = conn.pager.load().clone();
    let mut checkpoint_sm = new_truncate_sm(&db, &conn);
    match checkpoint_sm.step(&()).unwrap() {
        TransitionResult::Continue => {}
        other => panic!("PrepareCheckpoint should continue, got {other:?}"),
    }
    assert_eq!(
        checkpoint_sm.state_for_test(),
        CheckpointState::AcquireLock,
        "TRUNCATE must take the blocking lock even when the passive flag is on"
    );
    finish_checkpoint_sm(&mut checkpoint_sm, &pager);
}

#[test]
fn known_truncate_with_passive_flag_loses_commit_after_snapshot() {
    let mut db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'collected')")
        .unwrap();

    let pager = conn.pager.load().clone();
    let mut checkpoint_sm = new_truncate_sm(&db, &conn);
    drive_checkpoint_sm_until(&mut checkpoint_sm, &pager, |state| {
        *state == CheckpointState::BeginPagerTxn
    });

    let sibling = db.connect();
    sibling
        .execute("INSERT INTO t VALUES (2, 'after-snapshot')")
        .unwrap();
    finish_checkpoint_sm(&mut checkpoint_sm, &pager);

    assert_eq!(
        ids(&sibling),
        vec![1, 2],
        "commit after the TRUNCATE snapshot must stay visible"
    );

    drop(sibling);
    drop(conn);
    db.restart();
    let conn = db.connect();
    assert_eq!(
        ids(&conn),
        vec![1, 2],
        "commit after the TRUNCATE snapshot must survive reopen"
    );
}

#[test]
fn truncate_is_silent_noop_while_passive_checkpoint_is_running() {
    let db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'seed')").unwrap();

    let pager = conn.pager.load().clone();
    let mut passive_sm = new_passive_sm(&db, &conn);
    drive_checkpoint_sm_until(&mut passive_sm, &pager, |state| {
        *state == CheckpointState::BeginPagerTxn
    });

    let other = db.connect();
    other
        .execute("INSERT INTO t VALUES (2, 'must-be-truncated-or-busy')")
        .unwrap();
    let before = db.get_mvcc_store().get_logical_log_file().size().unwrap();
    other.execute("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    let after = db.get_mvcc_store().get_logical_log_file().size().unwrap();

    assert!(
        after < before,
        "TRUNCATE must wait or report Busy, not succeed as a no-op while a Passive checkpoint is running; log was {before} then {after}"
    );

    finish_checkpoint_sm(&mut passive_sm, &pager);
}

#[test]
fn restart_recovers_large_txn_after_passive_checkpoint() {
    let mut db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("BEGIN").unwrap();
    for id in 1..=500 {
        conn.execute(format!("INSERT INTO t VALUES ({id}, 'r{id}')"))
            .unwrap();
    }
    conn.execute("COMMIT").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();

    drop(conn);
    db.restart();
    let conn = db.connect();
    let count = get_rows(&conn, "SELECT count(*) FROM t")[0][0]
        .as_int()
        .unwrap();
    assert_eq!(
        count, 500,
        "all rows from the large txn must survive reopen"
    );
}

#[test]
fn restart_recovers_commit_after_retained_passive_checkpoint() {
    let mut db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'checkpointed')")
        .unwrap();

    let pager = conn.pager.load().clone();
    let mut checkpoint_sm = new_passive_sm(&db, &conn);
    drive_checkpoint_sm_until(&mut checkpoint_sm, &pager, |state| {
        *state == CheckpointState::BeginPagerTxn
    });

    let sibling = db.connect();
    sibling
        .execute("INSERT INTO t VALUES (2, 'live-tail')")
        .unwrap();
    finish_checkpoint_sm(&mut checkpoint_sm, &pager);

    let log_size = db.get_mvcc_store().get_logical_log_file().size().unwrap();
    assert!(
        log_size > 0,
        "Passive checkpoint must retain the live tail in the logical log"
    );

    drop(sibling);
    drop(conn);
    db.restart();
    let conn = db.connect();
    assert_eq!(
        ids(&conn),
        vec![1, 2],
        "row committed after the Passive snapshot must survive reopen"
    );
}

#[test]
fn mixed_passive_then_truncate_keeps_later_commit_after_restart() {
    let mut db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'a')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    conn.execute("INSERT INTO t VALUES (2, 'b')").unwrap();
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    conn.execute("INSERT INTO t VALUES (3, 'c')").unwrap();

    drop(conn);
    db.restart();
    let conn = db.connect();
    assert_eq!(ids(&conn), vec![1, 2, 3]);
}

#[test]
fn gc_after_passive_checkpoint_keeps_pinned_reader_from_seeing_later_commit() {
    let db = MvccTestDbNoConn::new_with_random_db_passive();
    let setup = db.connect();
    setup
        .execute("PRAGMA mvcc_checkpoint_threshold = 0")
        .unwrap();
    setup
        .execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    setup.execute("INSERT INTO t VALUES (1, 'old')").unwrap();

    let pinned = db.connect();
    pinned.execute("BEGIN CONCURRENT").unwrap();
    assert_eq!(ids(&pinned), vec![1]);

    let writer = db.connect();
    writer.execute("INSERT INTO t VALUES (2, 'new')").unwrap();
    writer
        .execute("UPDATE t SET v = 'newer' WHERE id = 1")
        .unwrap();

    assert_eq!(
        ids(&pinned),
        vec![1],
        "pinned reader must not see the post-snapshot insert"
    );
    let old = get_rows(&pinned, "SELECT v FROM t WHERE id = 1");
    assert_eq!(old[0][0].to_string(), "old");
}

#[test]
fn retained_passive_leaves_checkpointed_frames_in_the_log() {
    let db = MvccTestDbNoConn::new_with_random_db_passive();
    let conn = db.connect();
    conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
        .unwrap();
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
        .unwrap();
    for id in 1..=50 {
        conn.execute(format!("INSERT INTO t VALUES ({id}, 'x')"))
            .unwrap();
    }

    let pager = conn.pager.load().clone();
    let mut checkpoint_sm = new_passive_sm(&db, &conn);
    drive_checkpoint_sm_until(&mut checkpoint_sm, &pager, |state| {
        *state == CheckpointState::BeginPagerTxn
    });
    let sibling = db.connect();
    sibling
        .execute("INSERT INTO t VALUES (51, 'tail')")
        .unwrap();
    let retained = db.get_mvcc_store().get_logical_log_file().size().unwrap();
    finish_checkpoint_sm(&mut checkpoint_sm, &pager);
    let after_retain = db.get_mvcc_store().get_logical_log_file().size().unwrap();
    assert_eq!(
        after_retain, retained,
        "a retained Passive checkpoint must not rewrite the log prefix"
    );

    sibling.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
    let after_quiet = db.get_mvcc_store().get_logical_log_file().size().unwrap();
    assert!(
        after_quiet < after_retain,
        "a later quiet Passive checkpoint must drop frames that are now in the B-tree; log was {after_retain} then {after_quiet}"
    );
}

#[test]
fn page_export_after_retained_passive_misses_live_tail() {
    let db = MvccTestDbNoConn::new_with_random_db_passive();
    let db_path = db.path.as_ref().unwrap().clone();
    {
        let conn = db.connect();
        conn.execute("PRAGMA mvcc_checkpoint_threshold = -1")
            .unwrap();
        conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
            .unwrap();
        conn.execute("INSERT INTO t VALUES (1, 'base')").unwrap();
        conn.execute("PRAGMA wal_checkpoint(PASSIVE)").unwrap();
        conn.execute("INSERT INTO t VALUES (2, 'tail')").unwrap();
    }

    {
        let mut manager = crate::DATABASE_MANAGER.lock();
        manager.clear();
    }

    let export_dir = tempfile::TempDir::new().unwrap();
    let export_path = export_dir.path().join("replica.db");
    std::fs::copy(&db_path, &export_path).unwrap();

    let io = Arc::new(PlatformIO::new().unwrap());
    let replica = Database::open_file_with_flags(
        io,
        export_path.to_str().unwrap(),
        OpenFlags::default(),
        DatabaseOpts::new().with_experimental_mvcc_passive_checkpoint(true),
        None,
        Arc::new(SqliteDialect),
    )
    .expect("replica must open the exported DB page image");
    let conn = replica.connect().unwrap();
    let rows = get_rows(&conn, "SELECT id FROM t ORDER BY id");
    let got: Vec<i64> = rows.iter().map(|row| row[0].as_int().unwrap()).collect();
    assert_eq!(
        got,
        vec![1, 2],
        "a replica that starts from the Passive DB image must also receive the live log tail"
    );
}
