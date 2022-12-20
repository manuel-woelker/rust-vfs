//! A "physical" file system implementation using the underlying OS file system

use crate::error::VfsErrorKind;
use crate::{FileSystem, VfsMetadata};
use crate::{SeekAndRead, VfsFileType};
use crate::{VfsError, VfsResult};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// A physical filesystem implementation using the underlying OS file system
#[derive(Debug)]
pub struct PhysicalFS {
    root: PathBuf,
}

impl PhysicalFS {
    /// Create a new physical filesystem rooted in `root`
    pub fn new<T: AsRef<Path>>(root: T) -> Self {
        PhysicalFS {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn get_path(&self, mut path: &str) -> PathBuf {
        if path.starts_with('/') {
            path = &path[1..];
        }
        self.root.join(path)
    }
}

impl FileSystem for PhysicalFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let entries = Box::new(
            self.get_path(path)
                .read_dir()?
                .map(|entry| entry.unwrap().file_name().into_string().unwrap()),
        );
        Ok(entries)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        let fs_path = self.get_path(path);
        std::fs::create_dir(&fs_path).map_err(|err| match err.kind() {
            ErrorKind::AlreadyExists => {
                let metadata = std::fs::metadata(&fs_path).unwrap();
                if metadata.is_dir() {
                    return VfsError::from(VfsErrorKind::DirectoryExists);
                }
                VfsError::from(VfsErrorKind::FileExists)
            }
            _ => err.into(),
        })?;
        Ok(())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        Ok(Box::new(File::open(self.get_path(path))?))
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        Ok(Box::new(File::create(self.get_path(path))?))
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        Ok(Box::new(
            OpenOptions::new()
                .write(true)
                .append(true)
                .open(self.get_path(path))?,
        ))
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let metadata = self.get_path(path).metadata()?;
        Ok(if metadata.is_dir() {
            VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
            }
        } else {
            VfsMetadata {
                file_type: VfsFileType::File,
                len: metadata.len(),
            }
        })
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        Ok(self.get_path(path).exists())
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        std::fs::remove_file(self.get_path(path))?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        std::fs::remove_dir(self.get_path(path))?;
        Ok(())
    }

    fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        std::fs::copy(self.get_path(src), self.get_path(dest))?;
        Ok(())
    }

    fn move_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        std::fs::rename(self.get_path(src), self.get_path(dest))?;

        Ok(())
    }

    fn move_dir(&self, src: &str, dest: &str) -> VfsResult<()> {
        let result = std::fs::rename(self.get_path(src), self.get_path(dest));
        if result.is_err() {
            // Error possibly due to different filesystems, return not supported and let the fallback handle it
            return Err(VfsErrorKind::NotSupported.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    use crate::VfsPath;
    test_vfs!({
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        PhysicalFS::new(dir)
    });
    test_vfs_readonly!({ PhysicalFS::new("test/test_directory") });

    fn create_root() -> VfsPath {
        PhysicalFS::new(std::env::current_dir().unwrap()).into()
    }

    #[test]
    fn open_file() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let mut string = String::new();
        root.join("Cargo.toml")
            .unwrap()
            .open_file()
            .unwrap()
            .read_to_string(&mut string)
            .unwrap();
        assert_eq!(string, expected);
    }

    #[test]
    fn create_file() {
        let root = create_root();
        let _string = String::new();
        let _ = std::fs::remove_file("target/test.txt");
        root.join("target/test.txt")
            .unwrap()
            .create_file()
            .unwrap()
            .write_all(b"Testing only")
            .unwrap();
        let read = std::fs::read_to_string("target/test.txt").unwrap();
        assert_eq!(read, "Testing only");
    }

    #[test]
    fn append_file() {
        let root = create_root();
        let _string = String::new();
        let _ = std::fs::remove_file("target/test_append.txt");
        let path = root.join("target/test_append.txt").unwrap();
        path.create_file().unwrap().write_all(b"Testing 1").unwrap();
        path.append_file().unwrap().write_all(b"Testing 2").unwrap();
        let read = std::fs::read_to_string("target/test_append.txt").unwrap();
        assert_eq!(read, "Testing 1Testing 2");
    }

    #[test]
    fn read_dir() {
        let _expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let entries: Vec<_> = root.read_dir().unwrap().collect();
        let map: Vec<_> = entries
            .iter()
            .map(|path: &VfsPath| path.as_str())
            .filter(|x| x.ends_with(".toml"))
            .collect();
        assert_eq!(&["/Cargo.toml"], &map[..]);
    }

    #[test]
    fn create_dir() {
        let _ = std::fs::remove_dir("target/fs_test");
        let root = create_root();
        root.join("target/fs_test").unwrap().create_dir().unwrap();
        let path = Path::new("target/fs_test");
        assert!(path.exists(), "Path was not created");
        assert!(path.is_dir(), "Path is not a directory");
        std::fs::remove_dir("target/fs_test").unwrap();
    }

    #[test]
    fn file_metadata() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let metadata = root.join("Cargo.toml").unwrap().metadata().unwrap();
        assert_eq!(metadata.len, expected.len() as u64);
        assert_eq!(metadata.file_type, VfsFileType::File);
    }

    #[test]
    fn dir_metadata() {
        let root = create_root();
        let metadata = root.metadata().unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
        let metadata = root.join("src").unwrap().metadata().unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
    }
}
