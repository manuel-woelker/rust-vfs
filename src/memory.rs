//! An ephemeral in-memory file system, intended mainly for unit tests

use crate::{Result, VfsError};
use crate::{SeekAndRead, VMetadata};
use crate::{VFileType, VFS};
use core::cmp;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::mem::swap;
use std::sync::{Arc, RwLock};

type MemoryFsHandle = Arc<RwLock<MemoryFsImpl>>;

/// An ephemeral in-memory file system, intended mainly for unit tests
pub struct MemoryFS {
    handle: MemoryFsHandle,
}

impl Debug for MemoryFS {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("In Memory File System")
    }
}

impl MemoryFS {
    pub fn new() -> Self {
        MemoryFS {
            handle: Arc::new(RwLock::new(MemoryFsImpl::new())),
        }
    }
}

impl Default for MemoryFS {
    fn default() -> Self {
        Self::new()
    }
}

struct WritableFile {
    content: Cursor<Vec<u8>>,
    destination: String,
    fs: MemoryFsHandle,
}

impl Write for WritableFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.content.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.content.flush()
    }
}

impl Drop for WritableFile {
    fn drop(&mut self) {
        let mut content = vec![];
        swap(&mut content, &mut self.content.get_mut());
        self.fs.write().unwrap().files.insert(
            self.destination.clone(),
            MemoryFile {
                file_type: VFileType::File,
                content: Arc::new(content),
            },
        );
    }
}

struct ReadableFile {
    content: Arc<Vec<u8>>,
    position: u64,
}

impl ReadableFile {
    fn len(&self) -> u64 {
        self.content.len() as u64 - self.position
    }
}

impl Read for ReadableFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let amt = cmp::min(buf.len(), self.len() as usize);

        if amt == 1 {
            buf[0] = self.content[self.position as usize];
        } else {
            buf[..amt].copy_from_slice(
                &self.content.as_slice()[self.position as usize..self.position as usize + amt],
            );
        }
        self.position += amt as u64;
        Ok(amt)
    }
}

impl Seek for ReadableFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Start(offset) => self.position = offset,
            SeekFrom::Current(offset) => self.position = (self.position as i64 + offset) as u64,
            SeekFrom::End(offset) => self.position = (self.content.len() as i64 + offset) as u64,
        }
        Ok(self.position)
    }
}

impl VFS for MemoryFS {
    fn read_dir(&self, path: &str) -> Result<Box<dyn Iterator<Item = String>>> {
        let prefix = format!("{}/", path);
        let handle = self.handle.read().unwrap();
        let entries: Vec<_> = handle
            .files
            .iter()
            .filter_map(|(candidate_path, _)| {
                if let Some(rest) = candidate_path.strip_prefix(&prefix) {
                    if !rest.contains('/') {
                        return Some(rest.to_string());
                    }
                }
                None
            })
            .collect();
        Ok(Box::new(entries.into_iter()))
    }

    fn create_dir(&self, path: &str) -> Result<()> {
        self.handle.write().unwrap().files.insert(
            path.to_string(),
            MemoryFile {
                file_type: VFileType::Directory,
                content: Default::default(),
            },
        );
        Ok(())
    }

    fn open_file(&self, path: &str) -> Result<Box<dyn SeekAndRead>> {
        let handle = self.handle.read().unwrap();
        let file = handle.files.get(path).unwrap();
        Ok(Box::new(ReadableFile {
            content: file.content.clone(),
            position: 0,
        }))
    }

    fn create_file(&self, path: &str) -> Result<Box<dyn Write>> {
        let content = Arc::new(Vec::<u8>::new());
        self.handle.write().unwrap().files.insert(
            path.to_string(),
            MemoryFile {
                file_type: VFileType::File,
                content,
            },
        );
        let writer = WritableFile {
            content: Default::default(),
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    fn append_file(&self, path: &str) -> Result<Box<dyn Write>> {
        let handle = self.handle.write().unwrap();
        let file = handle.files.get(path).unwrap();
        let mut content = Cursor::new(file.content.as_ref().clone());
        content.seek(SeekFrom::End(0))?;
        let writer = WritableFile {
            content,
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    fn metadata(&self, path: &str) -> Result<VMetadata> {
        let guard = self.handle.read().unwrap();
        let files = &guard.files;
        let file = files.get(path).unwrap();
        Ok(VMetadata {
            file_type: file.file_type,
            len: file.content.len() as u64,
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.handle.read().unwrap().files.contains_key(path)
    }

    fn remove_file(&self, path: &str) -> Result<()> {
        let mut handle = self.handle.write().unwrap();
        handle
            .files
            .remove(path)
            .ok_or_else(|| VfsError::FileNotFound(path.to_string()))?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> Result<()> {
        if self.read_dir(path)?.next().is_some() {
            return Err(VfsError::Other(
                "Directory to remove is not empty".to_string(),
            ));
        }
        let mut handle = self.handle.write().unwrap();
        handle
            .files
            .remove(path)
            .ok_or_else(|| VfsError::FileNotFound(path.to_string()))?;
        Ok(())
    }
}

struct MemoryFsImpl {
    files: HashMap<String, MemoryFile>,
}

impl MemoryFsImpl {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }
}

struct MemoryFile {
    file_type: VFileType,
    content: Arc<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VPath;
    test_vfs!(MemoryFS::new());

    #[test]
    fn write_and_read_file() {
        let root = VPath::create(MemoryFS::new()).unwrap();
        let path = root.join("foobar.txt");
        let _send = &path as &dyn Send;
        {
            let mut file = path.create_file().unwrap();
            write!(file, "Hello world").unwrap();
            write!(file, "!").unwrap();
        }
        {
            let mut file = path.open_file().unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).unwrap();
            assert_eq!(string, "Hello world!");
        }
        assert!(path.exists());
        assert!(!root.join("foo").exists());
        let metadata = path.metadata().unwrap();
        assert_eq!(metadata.len, 12);
        assert_eq!(metadata.file_type, VFileType::File);
    }

    #[test]
    fn append_file() {
        let root = VPath::create(MemoryFS::new()).unwrap();
        let _string = String::new();
        let path = root.join("test_append.txt");
        path.create_file().unwrap().write_all(b"Testing 1").unwrap();
        path.append_file().unwrap().write_all(b"Testing 2").unwrap();
        {
            let mut file = path.open_file().unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).unwrap();
            assert_eq!(string, "Testing 1Testing 2");
        }
    }

    #[test]
    fn create_dir() {
        let root = VPath::create(MemoryFS::new()).unwrap();
        let _string = String::new();
        let path = root.join("foo");
        path.create_dir().unwrap();
        let metadata = path.metadata().unwrap();
        assert_eq!(metadata.file_type, VFileType::Directory);
    }
}
