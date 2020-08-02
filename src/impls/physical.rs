//! A "physical" file system implementation using the underlying OS file system

use crate::VfsResult;
use crate::{FileSystem, VfsMetadata};
use crate::{SeekAndRead, VfsFileType};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug)]
pub struct PhysicalFS {
    root: PathBuf,
}

impl PhysicalFS {
    pub fn new(root: PathBuf) -> Self {
        PhysicalFS { root }
    }

    fn get_path(&self, mut path: &str) -> PathBuf {
        if path.starts_with('/') {
            path = &path[1..];
        }
        self.root.join(path)
    }
}

impl FileSystem for PhysicalFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>> {
        let entries = Box::new(
            self.get_path(path)
                .read_dir()?
                .map(|entry| entry.unwrap().file_name().into_string().unwrap()),
        );
        Ok(entries)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        std::fs::create_dir(self.get_path(path))?;
        Ok(())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>> {
        Ok(Box::new(File::open(self.get_path(path))?))
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        Ok(Box::new(File::create(self.get_path(path))?))
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
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

    fn exists(&self, path: &str) -> bool {
        self.get_path(path).exists()
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        std::fs::remove_file(self.get_path(path))?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        std::fs::remove_dir(self.get_path(path))?;
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

    fn create_root() -> VfsPath {
        PhysicalFS::new(std::env::current_dir().unwrap()).into()
    }

    #[test]
    fn open_file() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let mut string = String::new();
        root.join("Cargo.toml")
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
        root.join("target/test.txt")
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
        let path = root.join("target/test_append.txt");
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
        root.join("target/fs_test").create_dir().unwrap();
        let path = Path::new("target/fs_test");
        assert!(path.exists(), "Path was not created");
        assert!(path.is_dir(), "Path is not a directory");
        std::fs::remove_dir("target/fs_test").unwrap();
    }

    #[test]
    fn file_metadata() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let metadata = root.join("Cargo.toml").metadata().unwrap();
        assert_eq!(metadata.len, expected.len() as u64);
        assert_eq!(metadata.file_type, VfsFileType::File);
    }

    #[test]
    fn dir_metadata() {
        let root = create_root();
        let metadata = root.metadata().unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
        let metadata = root.join("src").metadata().unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
    }
}
/*
use std::path::{Path, PathBuf};
use std::fs::{File, DirBuilder, Metadata, OpenOptions, ReadDir, DirEntry, remove_file, remove_dir, remove_dir_all};
use std::io::VfsResult;
use std::borrow::Cow;
use {FileSystem, VfsPath, VFile, VfsMetadata};


/// A "physical" file system implementation using the underlying OS file system
pub struct PhysicalFS {

}

impl VfsMetadata for Metadata {
    fn is_dir(&self) -> bool {
        self.is_dir()
    }
    fn is_file(&self) -> bool {
        self.is_file()
    }
    fn len(&self) -> u64 {
        self.len()
    }
}

impl FileSystem for PhysicalFS {
    type PATH = PathBuf;
    type FILE = File;
    type METADATA = Metadata;

    fn path<T: Into<String>>(&self, path: T) -> PathBuf {
        PathBuf::from(path.into())
    }
}



impl VfsPath for PathBuf {
    fn open_with_options(&self, open_options: &::OpenOptions) -> VfsResult<Box<VFile>> {
        OpenOptions::new()
            .read(open_options.read)
            .write(open_options.write)
            .create(open_options.create)
            .append(open_options.append)
            .truncate(open_options.truncate)
            .create(open_options.create)
            .open(self)
            .map(|x| Box::new(x) as Box<VFile>)
    }

    fn open(&self) -> VfsResult<Box<VFile>> {
        File::open(&self).map(|x| Box::new(x) as Box<VFile>)
    }

    fn create(&self) -> VfsResult<Box<VFile>> {
        File::create(&self).map(|x| Box::new(x) as Box<VFile>)
    }

    fn append(&self) -> VfsResult<Box<VFile>> {
        OpenOptions::new()
            .write(true)
            .append(true)
            .open(&self)
            .map(|x| Box::new(x) as Box<VFile>)
    }

    fn parent(&self) -> Option<Box<VfsPath>> {
        match <Path>::parent(&self) {
            Some(path) => Some(Box::new(path.to_path_buf())),
            None => None,
        }
    }

    fn file_name(&self) -> Option<String> {
        match <Path>::file_name(&self) {
            Some(name) => Some(name.to_string_lossy().into_owned()),
            None => None,
        }
    }

    fn extension(&self) -> Option<String> {
        match <Path>::extension(&self) {
            Some(name) => Some(name.to_string_lossy().into_owned()),
            None => None,
        }
    }

    fn resolve(&self, path: &String) -> Box<VfsPath> {
        let mut result = self.clone();
        <PathBuf>::push(&mut result, path);
        return Box::new(result);
    }

    fn mkdir(&self) -> VfsResult<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self)
    }

    fn rm(&self) -> VfsResult<()> {
        if self.is_dir() {
            remove_dir(&self)
        } else {
            remove_file(&self)
        }
    }

    fn rmrf(&self) -> VfsResult<()> {
        if self.is_dir() {
            remove_dir_all(&self)
        } else {
            remove_file(&self)
        }
    }


    fn exists(&self) -> bool {
        <Path>::exists(self)
    }

    fn metadata(&self) -> VfsResult<Box<VfsMetadata>> {
        <Path>::metadata(self).map(|x| Box::new(x) as Box<VfsMetadata>)
    }

    fn read_dir(&self) -> VfsResult<Box<Iterator<Item = VfsResult<Box<VfsPath>>>>> {
        <Path>::read_dir(self).map(|inner| {
            Box::new(PhysicalReadDir { inner: inner }) as Box<Iterator<Item = VfsResult<Box<VfsPath>>>>
        })
    }

    fn to_string(&self) -> Cow<str> {
        <Path>::to_string_lossy(self)
    }

    fn box_clone(&self) -> Box<VfsPath> {
        Box::new((*self).clone())
    }

    fn to_path_buf(&self) -> Option<PathBuf> {
        Some(self.clone())
    }
}

struct PhysicalReadDir {
    inner: ReadDir,
}

impl Iterator for PhysicalReadDir {
    type Item = VfsResult<Box<VfsPath>>;
    fn next(&mut self) -> Option<VfsResult<Box<VfsPath>>> {
        self.inner.next().map(|result| result.map(|entry| Box::new(entry.path()) as Box<VfsPath>))
    }
}


#[cfg(test)]
mod tests {
    use std::io::{Read, VfsResult};
    use std::path::PathBuf;

    use super::*;
    use VfsPath;
    #[test]
    fn read_file() {
        let path = PathBuf::from("Cargo.toml");
        let mut file = path.open().unwrap();
        let mut string: String = "".to_owned();
        file.read_to_string(&mut string).unwrap();
        assert!(string.len() > 10);
        assert!(path.exists());
        assert!(path.metadata().unwrap().is_file());
        assert!(PathBuf::from(".").metadata().unwrap().is_dir());
    }
    #[test]
    fn parent() {
        let src = PathBuf::from("./src");
        let parent = PathBuf::from(".");
        assert_eq!(src.parent().unwrap().to_string(), parent.to_string());
        assert!(PathBuf::from("/").parent().is_none());
    }

    #[test]
    fn read_dir() {
        let src = PathBuf::from("./src");
        let entries: Vec<VfsResult<Box<VfsPath>>> = src.read_dir().unwrap().collect();
        println!("{:#?}", entries);
    }

    #[test]
    fn file_name() {
        let src = PathBuf::from("./src/lib.rs");
        assert_eq!(src.file_name(), Some("lib.rs".to_owned()));
        assert_eq!(src.extension(), Some("rs".to_owned()));
    }

    #[test]
    fn to_path_buf() {
        let src = PathBuf::from("./src/lib.rs");
        assert_eq!(Some(src.clone()), src.to_path_buf());
    }


}
*/
