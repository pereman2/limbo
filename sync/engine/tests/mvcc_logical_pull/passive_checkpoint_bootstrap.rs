use turso_sync_engine::Result;

use crate::harness::Remote;

#[test]
fn replica_bootstrap_after_retained_passive_sees_live_tail() -> Result<()> {
    let remote = Remote::new_passive(&[
        "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)",
        "INSERT INTO t VALUES (1, 'base')",
    ])?;
    remote.checkpoint_passive()?;
    remote.execute_transaction(&["INSERT INTO t VALUES (2, 'tail')"])?;

    let replica = remote.bootstrap_replica_from_current_image()?;
    assert_eq!(
        replica.snapshot()?,
        remote.snapshot()?,
        "a new replica must catch up the live log tail after a retained Passive checkpoint"
    );
    Ok(())
}
