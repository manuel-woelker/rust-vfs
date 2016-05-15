use std::path::{Path, PathBuf};
use std::fs::{File, DirBuilder, Metadata, OpenOptions, ReadDir, DirEntry};
use std::io::Result;
use ::{VFS, VPath, VMetadata};


/// A "physical" file system implementation using the underlying OS file system
pub struct PhysicalFS {

}

impl VMetadata for Metadata {
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

impl VFS for PhysicalFS {
    type PATH = PathBuf;
    type FILE = File;
    type METADATA = Metadata;

    fn path<T: Into<String>>(&self, path: T) -> PathBuf {
        PathBuf::from(path.into())
    }
}



impl VPath for PathBuf {
    type FS = PhysicalFS;
    fn open(&self) -> Result<File> {
        File::open(&self)
    }

    fn create(&self) -> Result<File> {
        File::create(&self)
    }

    fn append(&self) -> Result<File> {
        OpenOptions::new()
            .write(true)
            .append(true)
            .open(&self)
    }

    fn parent(&self) -> Option<Self> {
        match <Path>::parent(&self) {
            Some(path) => Some(path.to_path_buf()),
            None => None,
        }
    }

    fn file_name(&self) -> Option<String> {
        match <Path>::file_name(&self) {
            Some(name) => Some(name.to_string_lossy().into_owned()),
            None => None,
        }
    }

    fn push<'a, T: Into<&'a str>>(&mut self, path: T) {
        <PathBuf>::push(self, path.into());
    }

    fn mkdir(&self) -> Result<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self)
    }

    fn exists(&self) -> bool {
        <Path>::exists(self)
    }

    fn metadata(&self) -> Result<Metadata> {
        <Path>::metadata(self)
    }

    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<PathBuf>>>> {
        <Path>::read_dir(self).map(|inner| Box::new(PhysicalReadDir {inner: inner})  as Box<Iterator<Item = Result<PathBuf>>>)
    }
}

struct PhysicalReadDir {
    inner: ReadDir
}

impl Iterator for PhysicalReadDir {
    type Item = Result<PathBuf>;
    fn next(&mut self) -> Option<Result<PathBuf>> {
        self.inner.next().map(|result| result.map(|entry| entry.path()))
    }
}


#[cfg(test)]
mod tests {
    use std::io::{Read, Result};
    use std::path::PathBuf;

    use super::*;
    use VPath;
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
        assert_eq!(src.parent().unwrap(), parent);
        assert_eq!(PathBuf::from("/").parent(), None);
    }

    #[test]
    fn read_dir() {
        let src = PathBuf::from("./src");
        let entries: Vec<Result<PathBuf>> = src.read_dir().unwrap().collect();
        println!("{:#?}", entries);
    }


}
