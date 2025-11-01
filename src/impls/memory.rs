use crate::{
    FileSystem, SeekAndRead, SeekAndWrite, VfsFileType, VfsMetadata, VfsResult, error::VfsErrorKind,
};
use core::cmp;
use std::{
    collections::{HashMap, hash_map::Entry},
    fmt::{self, Debug, Formatter},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    mem::swap,
    sync::{Arc, RwLock},
    time::SystemTime,
};

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
        if let Some(index) = separator
            && self.exists(&path[..index])?
        {
            return Ok(());
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

impl Seek for WritableFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.content.seek(pos)
    }
}

impl Write for WritableFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.content.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.content.flush()?;
        let mut content = self.content.get_ref().clone();
        swap(&mut content, self.content.get_mut());
        let mut handle = self.fs.write().unwrap();
        let previous_file = handle.files.get(&self.destination);

        let new_file = MemoryFile {
            file_type: VfsFileType::File,
            content: Arc::new(content),
            created: previous_file
                .map(|file| file.created)
                .unwrap_or(SystemTime::now()),
            modified: Some(SystemTime::now()),
            accessed: previous_file.map(|file| file.accessed).unwrap_or(None),
        };

        handle.files.insert(self.destination.clone(), new_file);
        Ok(())
    }
}

impl Drop for WritableFile {
    fn drop(&mut self) {
        self.flush()
            .expect("Flush failed while dropping in-memory file");
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

impl FileSystem for MemoryFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let prefix = format!("{path}/");
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
        let map = &mut self.handle.write().unwrap().files;
        let entry = map.entry(path.to_string());
        match entry {
            Entry::Occupied(file) => {
                return match file.get().file_type {
                    VfsFileType::File => Err(VfsErrorKind::FileExists.into()),
                    VfsFileType::Directory => Err(VfsErrorKind::DirectoryExists.into()),
                };
            }
            Entry::Vacant(_) => {
                map.insert(
                    path.to_string(),
                    MemoryFile {
                        file_type: VfsFileType::Directory,
                        content: Default::default(),
                        created: SystemTime::now(),
                        modified: Some(SystemTime::now()),
                        accessed: Some(SystemTime::now()),
                    },
                );
            }
        }
        Ok(())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.set_access_time(path, SystemTime::now())?;

        let handle = self.handle.read().unwrap();
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        ensure_file(file)?;
        Ok(Box::new(ReadableFile {
            content: file.content.clone(),
            position: 0,
        }))
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.ensure_has_parent(path)?;
        let content = Arc::new(Vec::<u8>::new());
        self.handle.write().unwrap().files.insert(
            path.to_string(),
            MemoryFile {
                file_type: VfsFileType::File,
                content,
                created: SystemTime::now(),
                modified: Some(SystemTime::now()),
                accessed: Some(SystemTime::now()),
            },
        );
        let writer = WritableFile {
            content: Cursor::new(vec![]),
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        let handle = self.handle.write().unwrap();
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        let mut content = Cursor::new(file.content.as_ref().clone());
        content.seek(SeekFrom::End(0))?;
        let writer = WritableFile {
            content,
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let guard = self.handle.read().unwrap();
        let files = &guard.files;
        let file = files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        Ok(VfsMetadata {
            file_type: file.file_type,
            len: file.content.len() as u64,
            modified: file.modified,
            created: Some(file.created),
            accessed: file.accessed,
        })
    }

    fn set_creation_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        let mut guard = self.handle.write().unwrap();
        let files = &mut guard.files;
        let file = files.get_mut(path).ok_or(VfsErrorKind::FileNotFound)?;

        file.created = time;

        Ok(())
    }

    fn set_modification_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        let mut guard = self.handle.write().unwrap();
        let files = &mut guard.files;
        let file = files.get_mut(path).ok_or(VfsErrorKind::FileNotFound)?;

        file.modified = Some(time);

        Ok(())
    }

    fn set_access_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        let mut guard = self.handle.write().unwrap();
        let files = &mut guard.files;
        let file = files.get_mut(path).ok_or(VfsErrorKind::FileNotFound)?;

        file.accessed = Some(time);

        Ok(())
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
                created: SystemTime::now(),
                modified: None,
                accessed: None,
            },
        );
        Self { files }
    }
}

struct MemoryFile {
    file_type: VfsFileType,
    #[allow(clippy::rc_buffer)] // to allow accessing the same object as writable
    content: Arc<Vec<u8>>,

    created: SystemTime,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
}

fn ensure_file(file: &MemoryFile) -> VfsResult<()> {
    if file.file_type != VfsFileType::File {
        return Err(VfsErrorKind::Other("Not a file".into()).into());
    }
    Ok(())
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
    fn write_and_seek_and_read_file() -> VfsResult<()> {
        let root = VfsPath::new(MemoryFS::new());
        let path = root.join("foobar.txt").unwrap();
        let _send = &path as &dyn Send;
        {
            let mut file = path.create_file().unwrap();
            write!(file, "Hello world").unwrap();
            write!(file, "!").unwrap();
            write!(file, " Before seek!!").unwrap();
            file.seek(SeekFrom::Current(-2)).unwrap();
            write!(file, " After the Seek!").unwrap();
        }
        {
            let mut file = path.open_file().unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).unwrap();
            assert_eq!(string, "Hello world! Before seek After the Seek!");
        }
        assert!(path.exists()?);
        assert!(!root.join("foo").unwrap().exists()?);
        let metadata = path.metadata().unwrap();
        assert_eq!(metadata.len, 40);
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
    fn append_file_with_seek() {
        let root = VfsPath::new(MemoryFS::new());
        let _string = String::new();
        let path = root.join("test_append.txt").unwrap();
        path.create_file().unwrap().write_all(b"Testing 1").unwrap();
        path.append_file().unwrap().write_all(b"Testing 2").unwrap();
        {
            let mut file = path.append_file().unwrap();
            file.seek(SeekFrom::End(-1)).unwrap();
            file.write_all(b"Testing 3").unwrap();
        }
        {
            let mut file = path.open_file().unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).unwrap();
            assert_eq!(string, "Testing 1Testing Testing 3");
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

    // cf. https://github.com/manuel-woelker/rust-vfs/issues/70
    #[test]
    fn flush_then_read_with_new_handle() {
        let root = VfsPath::new(MemoryFS::new());
        let path = root.join("test.txt").unwrap();
        let mut write_handle = path.create_file().unwrap();
        write_handle.write_all(b"Testing 1").unwrap();

        // Ensure flushed data can be read
        write_handle.flush().unwrap();
        let mut read_handle = path.open_file().unwrap();
        let mut string: String = String::new();
        read_handle.read_to_string(&mut string).unwrap();
        assert_eq!(string, "Testing 1");

        // Ensure second flush data can be read
        write_handle.write_all(b"Testing 2").unwrap();
        write_handle.flush().unwrap();
        let mut read_handle = path.open_file().unwrap();
        let mut string: String = String::new();
        read_handle.read_to_string(&mut string).unwrap();
        assert_eq!(string, "Testing 1Testing 2");

        // Ensure everything can be read on drop
        write_handle.write_all(b"Testing 3").unwrap();
        drop(write_handle);
        let mut read_handle = path.open_file().unwrap();
        let mut string: String = String::new();
        read_handle.read_to_string(&mut string).unwrap();
        assert_eq!(string, "Testing 1Testing 2Testing 3");
    }
}
