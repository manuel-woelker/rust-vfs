//! An ephemeral in-memory file system, intended mainly for unit tests

use crate::error::VfsErrorKind;
use crate::{VfsResult, FileSystem, SeekAndRead, VfsFileType, VfsMetadata, VfsAccess};
use core::cmp;
use std::collections::{HashSet, HashMap};
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
    /// Create a new in-memory filesystem
    pub fn new() -> Self {
        MemoryFS {
            handle: Arc::new(RwLock::new(MemoryFsImpl::new())),
        }
    }

    fn ensure_has_parent(&self, path: &str) -> VfsResult<()> {
        let separator = path.rfind('/');
        if let Some(index) = separator {
            if self.exists(&path[..index])? {
                return Ok(());
            }
        }
        Err(VfsErrorKind::Other("Parent path does not exist".into()).into())
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
        swap(&mut content, self.content.get_mut());
        self.fs.write().unwrap().files.insert(
            self.destination.clone(),
            MemoryFile {
                file_type: VfsFileType::File,
                content: Arc::new(content),
            },
        );
    }
}

struct ReadableFile {
    #[allow(clippy::rc_buffer)] // to allow accessing the same object as writable
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

struct RandomAccessFile {
    content: Cursor<Vec<u8>>,
    destination: String,
    fs: MemoryFsHandle,
}

impl RandomAccessFile {
    fn from_file(value: Arc<Vec<u8>>, destination: String, fs: MemoryFsHandle) -> Self {
        Self {
            content: Cursor::new(value.to_vec()),
            destination,
            fs,
        }
    }
}

impl Write for RandomAccessFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.content.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.content.flush()
    }
}

impl Read for RandomAccessFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.content.read(buf)
    }
}
impl Seek for RandomAccessFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.content.seek(pos)
    }
}
impl Drop for RandomAccessFile {
    fn drop(&mut self) {
        let mut content = vec![];
        swap(&mut content, self.content.get_mut());
        self.fs.write().unwrap().files.insert(
            self.destination.clone(),
            MemoryFile {
                file_type: VfsFileType::File,
                content: Arc::new(content),
            },
        );
    }
}

impl FileSystem for MemoryFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>> {
        let prefix = format!("{}/", path);
        let handle = self.handle.read().unwrap();
        let mut found_directory = false;
        #[allow(clippy::needless_collect)] // need collect to satisfy lifetime requirements
        let entries: Vec<_> = handle
            .files
            .iter()
            .filter_map(|(candidate_path, _)| {
                if candidate_path == path {
                    found_directory = true;
                }
                if candidate_path.starts_with(&prefix) {
                    let rest = &candidate_path[prefix.len()..];
                    if !rest.contains('/') {
                        return Some(rest.to_string());
                    }
                }
                None
            })
            .collect();
        if !found_directory {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.ensure_has_parent(path)?;
        self.handle.write().unwrap().files.insert(
            path.to_string(),
            MemoryFile {
                file_type: VfsFileType::Directory,
                content: Default::default(),
            },
        );
        Ok(())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>> {
        let handle = self.handle.read().unwrap();
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        ensure_file(file)?;
        Ok(Box::new(ReadableFile {
            content: file.content.clone(),
            position: 0,
        }))
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        self.ensure_has_parent(path)?;
        let content = Arc::new(Vec::<u8>::new());
        self.handle.write().unwrap().files.insert(
            path.to_string(),
            MemoryFile {
                file_type: VfsFileType::File,
                content,
            },
        );
        let writer = WritableFile {
            content: Cursor::new(vec![]),
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        let handle = self.handle.write().unwrap();
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        let mut content = Cursor::new(file.content.as_ref().clone());
        content.seek(SeekFrom::End(0))?;
        let writer = WritableFile {
            content,
            destination: path.to_string(),
            fs: Arc::clone(&self.handle),
        };
        Ok(Box::new(writer))
    }

    fn update_file(&self, path: &str) -> VfsResult<Box<dyn crate::SeekAndReadAndWrite>> {
        let handle = self.handle.read().unwrap();
        let file = handle
            .files
            .get(path)
            .ok_or_else(|| VfsErrorKind::FileNotFound)?;
        ensure_file(file)?;

        Ok(Box::new(RandomAccessFile::from_file(
            Arc::clone(&file.content),
            path.to_string(),
            Arc::clone(&self.handle),
        )))
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let guard = self.handle.read().unwrap();
        let files = &guard.files;
        let file = files.get(path).ok_or(VfsErrorKind::FileNotFound)?;

        Ok(VfsMetadata {
            file_type: file.file_type,
            len: file.content.len() as u64,
            access: HashSet::from([VfsAccess::Read, VfsAccess::Write]),
        })
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        Ok(self.handle.read().unwrap().files.contains_key(path))
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        let mut handle = self.handle.write().unwrap();
        handle
            .files
            .remove(path)
            .ok_or(VfsErrorKind::FileNotFound)?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        if self.read_dir(path)?.next().is_some() {
            return Err(VfsErrorKind::Other("Directory to remove is not empty".into()).into());
        }
        let mut handle = self.handle.write().unwrap();
        handle
            .files
            .remove(path)
            .ok_or(VfsErrorKind::FileNotFound)?;
        Ok(())
    }

    fn sync(&self, _path: &str) -> VfsResult<()> {
        Ok(())
    }
}

struct MemoryFsImpl {
    files: HashMap<String, MemoryFile>,
}

impl MemoryFsImpl {
    pub fn new() -> Self {
        let mut files = HashMap::new();
        // Add root directory
        files.insert(
            "".to_string(),
            MemoryFile {
                file_type: VfsFileType::Directory,
                content: Arc::new(vec![]),
            },
        );
        Self { files }
    }
}

struct MemoryFile {
    file_type: VfsFileType,
    #[allow(clippy::rc_buffer)] // to allow accessing the same object as writable
    content: Arc<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VfsPath;
    test_vfs!(MemoryFS::new());

    #[test]
    fn write_and_read_file() -> VfsResult<()> {
        let root = VfsPath::new(MemoryFS::new());
        let path = root.join("foobar.txt").unwrap();
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
        assert!(path.exists()?);
        assert!(!root.join("foo").unwrap().exists()?);
        let metadata = path.metadata().unwrap();
        assert_eq!(metadata.len, 12);
        assert_eq!(metadata.file_type, VfsFileType::File);
        Ok(())
    }

    #[test]
    fn append_file() {
        let root = VfsPath::new(MemoryFS::new());
        let _string = String::new();
        let path = root.join("test_append.txt").unwrap();
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
        let root = VfsPath::new(MemoryFS::new());
        let _string = String::new();
        let path = root.join("foo").unwrap();
        path.create_dir().unwrap();
        let metadata = path.metadata().unwrap();
        assert_eq!(metadata.file_type, VfsFileType::Directory);
    }

    #[test]
    fn remove_dir_error_message() {
        let root = VfsPath::new(MemoryFS::new());
        let path = root.join("foo").unwrap();
        let result = path.remove_dir();
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "Could not remove directory for '/foo': The file or directory could not be found"
        );
    }

    #[test]
    fn read_dir_error_message() {
        let root = VfsPath::new(MemoryFS::new());
        let path = root.join("foo").unwrap();
        let result = path.read_dir();
        match result {
            Ok(_) => panic!("Error expected"),
            Err(err) => {
                assert_eq!(
                    format!("{}", err),
                    "Could not read directory for '/foo': The file or directory could not be found"
                );
            }
        }
    }

    #[test]
    fn copy_file_across_filesystems() -> VfsResult<()> {
        let root_a = VfsPath::new(MemoryFS::new());
        let root_b = VfsPath::new(MemoryFS::new());
        let src = root_a.join("a.txt")?;
        let dest = root_b.join("b.txt")?;
        src.create_file()?.write_all(b"Hello World")?;
        src.copy_file(&dest)?;
        assert_eq!(&dest.read_to_string()?, "Hello World");
        Ok(())
    }
}

fn ensure_file(file: &MemoryFile) -> VfsResult<()> {
    if file.file_type != VfsFileType::File {
        return Err(VfsErrorKind::Other("Not a file".into()).into());
    }
    Ok(())
}
