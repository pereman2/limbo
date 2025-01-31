use super::{Completion, File, IO};
use anyhow::{Ok, Result};
use std::rc::Rc;
use std::cell::RefCell;
use std::io::{Read, Seek};
use log::trace;

pub struct WindowsIO {}

impl WindowsIO {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

impl IO for WindowsIO {
    fn open_file(&self, path: &str) -> Result<Box<dyn File>> {
        trace!("open_file(path = {})", path);
        let file = std::fs::File::open(path)?;
        Ok(Box::new(WindowsFile {
            file: RefCell::new(file),
        }))
    }

    fn run_once(&self) -> Result<()> {
        Ok(())
    }
}

pub struct WindowsFile {
    file: RefCell<std::fs::File>,
}

impl File for WindowsFile {
    fn pread(&self, pos: usize, c: Rc<Completion>) -> Result<()> {
        let mut file = self.file.borrow_mut();
        file.seek(std::io::SeekFrom::Start(pos as u64))?;
        {
            let mut buf = c.buf_mut();
            let buf = buf.as_mut_slice();
            file.read_exact(buf)?;
        }
        c.complete();
        Ok(())
    }
}
