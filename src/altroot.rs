//! A "physical" file system implementation using the underlying OS file system,
//! with its root in a particular directory.  Similar to a chroot but done purely
//! by path manipulation; thus, it's harder to guarentee that it's impossible for
//! a malicious user to escape the given directory.
//!
//! Use it for convenience, not security.

use std::path::{self, Path, PathBuf};
use std::fs::{File, DirBuilder, Metadata, OpenOptions, ReadDir, DirEntry, remove_file, remove_dir, remove_dir_all};
use std::io::Result;
use std::borrow::Cow;
use std::sync::Arc;
use {VFS, VPath, VFile, VMetadata};

#[derive(Debug, Clone)]
pub struct AltrootFS {
    root: Arc<PathBuf>,
}

impl AltrootFS {
    pub fn new<T>(root: T) -> Self where PathBuf: From<T> {
        AltrootFS {
            root: Arc::new(PathBuf::from(root))
        }
    }
}


/// A structure representing a PathBuf that must be rooted within an AltrootFS.
///
/// It must be absolute.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AltPath {
    root: Arc<PathBuf>,
    full_path: PathBuf,
}

/// Helper function to turn a path::Component into an Option<String> iff the Component
/// is a normal portion.
///
/// Basically this is to help turn a canonicalized absolute path into a relative path.
fn component_filter(comp: path::Component) -> Option<String> {
    match comp {
        path::Component::Normal(osstr) => Some(osstr.to_string_lossy().into_owned()),
        _ => None
    }
}

impl AltPath {
    pub fn new<T>(fs: &AltrootFS, path: T) -> Self where PathBuf: From<T> {
        let pathbuf = PathBuf::from(path);

        let relative_path = pathbuf.components().filter_map(component_filter);
        let root_path = fs.root.clone();
        let mut full_path = (*root_path).clone();
        full_path.extend(relative_path);
        full_path.canonicalize().unwrap();
        if !full_path.starts_with(&*fs.root) {
            panic!("Tried to create an AltPath that exits the AltrootFS's root dir");
        }
        
        AltPath {
            root: root_path,
            full_path: full_path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AltMetadata(Metadata);

impl VMetadata for AltMetadata {
    fn is_dir(&self) -> bool {
        self.0.is_dir()
    }
    fn is_file(&self) -> bool {
        self.0.is_file()
    }
    fn len(&self) -> u64 {
        self.0.len()
    }
}

impl VFS for AltrootFS {
    type PATH = AltPath;
    type FILE = File;
    type METADATA = Metadata;

    fn path<T: Into<String>>(&self, path: T) -> AltPath {
        AltPath::new(&self, PathBuf::from(path.into()))
    }
}

use std::convert;
impl convert::AsRef<Path> for AltPath {
    fn as_ref(&self) -> &Path {
        &self.full_path
    }
}



impl VPath for AltPath {
    fn open_with_options(&self, open_options: &::OpenOptions) -> Result<Box<VFile>> {
        OpenOptions::new()
            .read(open_options.read)
            .write(open_options.write)
            .create(open_options.create)
            .append(open_options.append)
            .truncate(open_options.truncate)
            .create(open_options.create)
            .open(&self.full_path)
            .map(|x| Box::new(x) as Box<VFile>)
    }

    fn open(&self) -> Result<Box<VFile>> {
        File::open(&self.full_path).map(|x| Box::new(x) as Box<VFile>)
    }

    fn create(&self) -> Result<Box<VFile>> {
        File::create(&self.full_path).map(|x| Box::new(x) as Box<VFile>)
    }

    fn append(&self) -> Result<Box<VFile>> {
        OpenOptions::new()
            .write(true)
            .append(true)
            .open(&self.full_path)
            .map(|x| Box::new(x) as Box<VFile>)
    }

    fn parent(&self) -> Option<Box<VPath>> {
        match <Path>::parent(&self.full_path) {
            Some(path) => {
                if path.starts_with(&*self.root) {
                    Some(Box::new(path.to_path_buf()))
                } else {
                    None
                }
            },
            None => None,
        }
    }

    fn file_name(&self) -> Option<String> {
        match <Path>::file_name(&self.full_path) {
            Some(name) => Some(name.to_string_lossy().into_owned()),
            None => None,
        }
    }

    fn extension(&self) -> Option<String> {
        match <Path>::extension(&self.full_path) {
            Some(name) => Some(name.to_string_lossy().into_owned()),
            None => None,
        }
    }

    fn resolve(&self, path: &String) -> Box<VPath> {
        let mut result = self.full_path.clone();
        <PathBuf>::push(&mut result, path);
        return Box::new(result);
    }

    fn mkdir(&self) -> Result<()> {
        DirBuilder::new()
            .recursive(true)
            .create(&self.full_path)
    }

    fn rm(&self) -> Result<()> {
        if self.full_path.is_dir() {
            remove_dir(&self.full_path)
        } else {
            remove_file(&self.full_path)
        }
    }

    fn rmrf(&self) -> Result<()> {
        if self.full_path.is_dir() {
            remove_dir_all(&self.full_path)
        } else {
            remove_file(&self.full_path)
        }
    }


    fn exists(&self) -> bool {
        <Path>::exists(&self.full_path)
    }

    fn metadata(&self) -> Result<Box<VMetadata>> {
        <Path>::metadata(&self.full_path).map(|x| Box::new(x) as Box<VMetadata>)
    }

    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<Box<VPath>>>>> {
        <Path>::read_dir(&self.full_path).map(|inner| {
            Box::new(PhysicalReadDir { inner: inner }) as Box<Iterator<Item = Result<Box<VPath>>>>
        })
    }

    fn to_string(&self) -> Cow<str> {
        <Path>::to_string_lossy(&self.full_path)
    }

    fn box_clone(&self) -> Box<VPath> {
        Box::new((*self).clone())
    }

    fn to_path_buf(&self) -> Option<PathBuf> {
        let relative_path = self.full_path.strip_prefix(&*self.root).expect("Should always succeed");
        Some(relative_path.into())
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
        let altroot = AltrootFS::new(env!("CARGO_MANIFEST_DIR"));
        let path = AltPath::new(&altroot, "/Cargo.toml");
        let mut file = path.open().unwrap();
        let mut string = String::new();
        file.read_to_string(&mut string).unwrap();
        assert!(string.len() > 10);
        assert!(path.exists());
        assert!(path.metadata().unwrap().is_file());

        let root_dir = AltPath::new(&altroot, "/");
        assert!(root_dir.metadata().unwrap().is_dir());
    }
    #[test]
    fn parent() {
        let altroot = AltrootFS::new(env!("CARGO_MANIFEST_DIR"));
        let src = AltPath::new(&altroot, "/src");
        let parent = AltPath::new(&altroot, "/");
        assert_eq!(src.parent().unwrap().to_string(), parent.to_string());
        assert!(parent.parent().is_none());
    }

    #[test]
    fn read_dir() {
        let altroot = AltrootFS::new(env!("CARGO_MANIFEST_DIR"));
        let src = AltPath::new(&altroot, "/src");
        let entries: Vec<Result<Box<VPath>>> = src.read_dir().unwrap().collect();
        println!("{:#?}", entries);
    }

    #[test]
    fn file_name() {
        let altroot = AltrootFS::new(env!("CARGO_MANIFEST_DIR"));
        let src = AltPath::new(&altroot, "/src/lib.rs");
        assert_eq!(src.file_name(), Some("lib.rs".to_owned()));
        assert_eq!(src.extension(), Some("rs".to_owned()));
    }

    #[test]
    fn to_path_buf() {
        let altroot = AltrootFS::new(env!("CARGO_MANIFEST_DIR"));
        let src = AltPath::new(&altroot, "/src/lib.rs");
        assert_eq!(Some(PathBuf::from("src/lib.rs")), src.to_path_buf());
    }


}
