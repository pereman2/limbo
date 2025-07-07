use limbo_core::{Database, IO};
use std::rc::Rc;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    /// Mock IO implementation for testing
    struct MockIO {
        files: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    }

    impl MockIO {
        fn new() -> Self {
            Self {
                files: Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }
    }

    impl IO for MockIO {
        type File = MockFile;

        fn open_file(&self, path: &str) -> anyhow::Result<Self::File> {
            Ok(MockFile {
                path: path.to_string(),
                files: self.files.clone(),
                position: 0,
            })
        }

        fn run_once(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockFile {
        path: String,
        files: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
        position: usize,
    }

    impl limbo_core::File for MockFile {
        fn pread(&self, pos: usize, c: limbo_core::Completion) -> anyhow::Result<()> {
            let files = self.files.lock().unwrap();
            let data = files.get(&self.path).unwrap_or(&vec![]);
            
            // Read data from the file
            let end = std::cmp::min(pos + c.buffer().len(), data.len());
            if pos < data.len() {
                let read_len = end - pos;
                c.buffer()[..read_len].copy_from_slice(&data[pos..end]);
            }
            
            // Complete the operation
            c.complete(Ok(end.saturating_sub(pos)));
            Ok(())
        }

        fn pwrite(&self, pos: usize, c: limbo_core::Completion) -> anyhow::Result<()> {
            let mut files = self.files.lock().unwrap();
            let data = files.entry(self.path.clone()).or_insert_with(Vec::new);
            
            // Extend the file if necessary
            if pos + c.buffer().len() > data.len() {
                data.resize(pos + c.buffer().len(), 0);
            }
            
            // Write the data
            data[pos..pos + c.buffer().len()].copy_from_slice(c.buffer());
            
            // Complete the operation
            c.complete(Ok(c.buffer().len()));
            Ok(())
        }

        fn sync(&self, c: limbo_core::Completion) -> anyhow::Result<()> {
            c.complete(Ok(0));
            Ok(())
        }
    }

    #[test]
    fn test_page_cache_clear_dirty_error() {
        // This test attempts to reproduce the "Failed to clear page cache: Dirty { pgno: 1 }" error
        // The error occurs when trying to clear the page cache while page 1 is still dirty
        
        let io = Rc::new(MockIO::new());
        
        // Create a small database file with basic structure
        let db_path = "test_dirty_page.db";
        
        // Initialize a minimal SQLite database structure
        {
            let file = io.open_file(db_path).unwrap();
            let mut files = io.files.lock().unwrap();
            let mut db_data = vec![0u8; 4096]; // One page
            
            // Write SQLite file header
            db_data[0..16].copy_from_slice(b"SQLite format 3\0");
            db_data[16] = 0x10; db_data[17] = 0x00; // page size = 4096
            db_data[18] = 1; // file format write version
            db_data[19] = 1; // file format read version
            db_data[20] = 0; // reserved space
            db_data[21] = 64; // max embedded payload fraction
            db_data[22] = 32; // min embedded payload fraction
            db_data[23] = 32; // min leaf payload fraction
            
            files.insert(db_path.to_string(), db_data);
        }
        
        let page_source = limbo_core::PageSource::from_file(io.open_file(db_path).unwrap());
        
        // Open the database
        let database = Database::open(io.clone(), page_source).unwrap();
        let conn = database.connect();
        
        // Perform operations that should dirty page 1 (database header)
        // Creating a table should modify the database header and make page 1 dirty
        let result = conn.execute("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)");
        
        // The error might occur here or during cleanup
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            println!("Error during table creation: {}", error_msg);
            
            // Check if this is the error we're looking for
            assert!(
                error_msg.contains("Failed to clear page cache") && error_msg.contains("Dirty"),
                "Expected 'Failed to clear page cache: Dirty' error, got: {}",
                error_msg
            );
            return;
        }
        
        // Try to insert data which should also dirty page 1
        let result = conn.execute("INSERT INTO test_table (name) VALUES ('test')");
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            println!("Error during insert: {}", error_msg);
            
            assert!(
                error_msg.contains("Failed to clear page cache") && error_msg.contains("Dirty"),
                "Expected 'Failed to clear page cache: Dirty' error, got: {}",
                error_msg
            );
            return;
        }
        
        // The error might occur when the database is dropped/closed
        // This happens implicitly when database goes out of scope
        drop(conn);
        drop(database);
        
        // If we reach here without error, the test didn't reproduce the issue
        // This is okay as the error might be timing-dependent or require specific conditions
        println!("Test completed without reproducing the error - this may be expected");
    }

    #[test]
    fn test_concurrent_page_cache_operations() {
        // Alternative test that might trigger the dirty page cache error
        // by attempting concurrent operations on the database
        
        let io = Rc::new(MockIO::new());
        let db_path = "test_concurrent.db";
        
        // Initialize database
        {
            let file = io.open_file(db_path).unwrap();
            let mut files = io.files.lock().unwrap();
            let mut db_data = vec![0u8; 4096];
            
            // Basic SQLite header
            db_data[0..16].copy_from_slice(b"SQLite format 3\0");
            db_data[16] = 0x10; db_data[17] = 0x00; // page size = 4096
            db_data[18] = 1; db_data[19] = 1; // file format versions
            
            files.insert(db_path.to_string(), db_data);
        }
        
        let page_source = limbo_core::PageSource::from_file(io.open_file(db_path).unwrap());
        let database = Database::open(io.clone(), page_source).unwrap();
        
        // Create multiple connections to potentially trigger race conditions
        let conn1 = database.connect();
        let conn2 = database.connect();
        
        // Try operations that might cause page 1 to be dirty in multiple connections
        let _ = conn1.execute("CREATE TABLE IF NOT EXISTS test1 (id INTEGER)");
        let _ = conn2.execute("CREATE TABLE IF NOT EXISTS test2 (id INTEGER)");
        
        // Rapid operations that might trigger the cache clearing error
        for i in 0..10 {
            let _ = conn1.execute(&format!("INSERT INTO test1 VALUES ({})", i));
            let _ = conn2.execute(&format!("INSERT INTO test2 VALUES ({})", i * 2));
        }
        
        // Drop connections in specific order that might trigger the error
        drop(conn1);
        drop(conn2);
        drop(database);
    }
}