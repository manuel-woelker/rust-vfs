use std::path::{Path, PathBuf};
use std::fs::{File, DirBuilder, Metadata, OpenOptions, ReadDir, DirEntry};
use std::io::Result;
use std::borrow::Cow;
use {VFS, VPath, VFile, VMetadata};


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
    fn open_with_options(&self, open_options: &::OpenOptions) -> Result<Box<VFile>> {
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

    fn open(&self) -> Result<Box<VFile>> {
        File::open(&self).map(|x| Box::new(x) as Box<VFile>)
    }

    fn create(&self) -> Result<Box<VFile>> {
        File::create(&self).map(|x| Box::new(x) as Box<VFile>)
    }

    fn append(&self) -> Result<Box<VFile>> {
        OpenOptions::new()
            .write(true)
            .append(true)
            .open(&self)
            .map(|x| Box::new(x) as Box<VFile>)
    }

    fn parent(&self) -> Option<Box<VPath>> {
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

    fn resolve(&self, path: &String) -> Box<VPath> {
        let mut result = self.clone();
        <PathBuf>::push(&mut result, path);
        return Box::new(result);
    }

    fn mkdir(&self) -> Result<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self)
    }

    fn exists(&self) -> bool {
        <Path>::exists(self)
    }

    fn metadata(&self) -> Result<Box<VMetadata>> {
        <Path>::metadata(self).map(|x| Box::new(x) as Box<VMetadata>)
    }

    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<Box<VPath>>>>> {
        <Path>::read_dir(self).map(|inner| {
            Box::new(PhysicalReadDir { inner: inner }) as Box<Iterator<Item = Result<Box<VPath>>>>
        })
    }

    fn to_string(&self) -> Cow<str> {
        <Path>::to_string_lossy(self)
    }

    fn box_clone(&self) -> Box<VPath> {
        Box::new((*self).clone())
    }
}

struct PhysicalReadDir {
    inner: ReadDir,
}

impl Iterator for PhysicalReadDir {
    type Item = Result<Box<VPath>>;
    fn next(&mut self) -> Option<Result<Box<VPath>>> {
        self.inner.next().map(|result| result.map(|entry| Box::new(entry.path()) as Box<VPath>))
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
    // #[test]
    // fn parent() {
    // let src = PathBuf::from("./src");
    // let parent = PathBuf::from(".");
    // assert_eq!(src.parent().unwrap(), parent);
    // assert_eq!(PathBuf::from("/").parent(), None);
    // }
    //
    #[test]
    fn read_dir() {
        let src = PathBuf::from("./src");
        let entries: Vec<Result<Box<VPath>>> = src.read_dir().unwrap().collect();
        println!("{:#?}", entries);
    }

    #[test]
    fn file_name() {
        let src = PathBuf::from("./src/lib.rs");
        assert_eq!(src.file_name(), Some("lib.rs".to_owned()));
        assert_eq!(src.extension(), Some("rs".to_owned()));
    }


}
