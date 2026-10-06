use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use turso_core::{Connection, Database, DatabaseOpts, OpenOptions, PlatformIO, SqliteDialect};
use turso_sync_engine::Result;

use super::replica::Replica;
use super::snapshot::Snapshot;

pub struct Remote {
    conn: Arc<Connection>,
    db_path: PathBuf,
    _db: Arc<Database>,
    _dir: TempDir,
}

impl Remote {
    pub fn new(setup: &[&str]) -> Result<Arc<Self>> {
        Self::open(setup, database_opts())
    }

    pub fn new_passive(setup: &[&str]) -> Result<Arc<Self>> {
        Self::open(
            setup,
            database_opts().with_experimental_mvcc_passive_checkpoint(true),
        )
    }

    fn open(setup: &[&str], opts: DatabaseOpts) -> Result<Arc<Self>> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("remote.db");
        let db = Database::open(
            Arc::new(PlatformIO::new()?),
            &db_path.to_string_lossy(),
            OpenOptions::new(Arc::new(SqliteDialect)).db_opts(opts),
        )?;
        let conn = db.connect()?;
        conn.execute("PRAGMA journal_mode = 'mvcc'")?;
        conn.set_portable_logical_changes_enabled(true);
        for sql in setup {
            conn.execute(sql)?;
        }
        Ok(Arc::new(Self {
            conn,
            db_path,
            _db: db,
            _dir: dir,
        }))
    }

    pub fn bootstrap_replica(self: &Arc<Self>) -> Result<Replica> {
        Replica::bootstrap(self.clone())
    }

    pub fn bootstrap_replica_from_current_image(self: &Arc<Self>) -> Result<Replica> {
        Replica::bootstrap_from_current_image(self.clone())
    }

    pub fn execute_transaction(&self, statements: &[&str]) -> Result<()> {
        self.conn.execute("BEGIN")?;
        for sql in statements {
            self.conn.execute(sql)?;
        }
        self.conn.execute("COMMIT")?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        Snapshot::capture(&self.conn)
    }

    pub(super) fn checkpoint_and_read_database_file(&self) -> Result<Vec<u8>> {
        self.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")?;
        Ok(std::fs::read(&self.db_path)?)
    }

    pub fn checkpoint_passive(&self) -> Result<()> {
        self.conn.execute("PRAGMA wal_checkpoint(PASSIVE)")?;
        Ok(())
    }

    pub(super) fn read_database_file(&self) -> Result<Vec<u8>> {
        Ok(std::fs::read(&self.db_path)?)
    }

    pub(super) fn read_logical_log(&self) -> Result<Vec<u8>> {
        match std::fs::read(self.db_path.with_extension("db-log")) {
            Ok(log) => Ok(log),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
}

pub(super) fn database_opts() -> DatabaseOpts {
    DatabaseOpts::new().with_generated_columns(true)
}
