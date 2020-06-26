//! Virtual file system abstraction
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different filesystem implementations (i.e. an in memory implementation for unit tests)
//!
//! A virtual filesystem consists of three basic types
//!
//!  * **Paths** - locations in the filesystem
//!  * **File** - actual file contents (think inodes)
//!  * **Metadata** - metadata information about paths
//!
//!
//! This crate currently has the following implementations:
//!
//!  * **PhysicalFS** - the actual filesystem of the underlying OS
//!  * **MemoryFS** - an ephemeral in-memory implementation (intended for unit tests)


use std::error::Error;

use std::sync::Arc;
use std::io::{Seek, Read, Write};
use std::fmt::Debug;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub trait SeekAndRead: Seek + Read {}

impl <T> SeekAndRead for T where T: Seek + Read {}

#[derive(Debug, Eq, PartialEq)]
pub enum VFileType {
    File,
    Directory,
}

#[derive(Debug)]
pub struct VMetadata {
    pub file_type: VFileType,
    pub len: u64,
}

pub trait VFS: Debug {
    fn read_dir(&self, path: &str) -> Result<Box<dyn Iterator<Item=String>>>;
    fn open_file(&self, path: &str) -> Result<Box<dyn SeekAndRead>>;
    fn create_file(&self, path: &str) -> Result<Box<dyn Write>>;
    fn metadata(&self, path: &str) -> Result<VMetadata>;
    fn exists(&self, path: &str) -> bool;
}


#[derive(Debug)]
pub struct FileSystem {
    vfs: Box<dyn VFS>,
}

#[derive(Debug)]
pub struct VPath {
    path: String,
    fs: Arc<FileSystem>,
}

impl VPath {
    fn path(&self) -> &str {
        &self.path
    }

    fn join(&self, path: &str) -> Self {
        VPath {
            path: format!("{}/{}", self.path, path),
            fs: self.fs.clone(),
        }
    }

    fn read_dir(&self) -> Result<Box<dyn Iterator<Item=VPath>>> {
        let parent = self.path.clone();
        let fs = self.fs.clone();
        Ok(Box::new(self.fs.vfs.read_dir(&self.path)?.map(move |path| VPath { path: format!("{}/{}", parent, path), fs: fs.clone() })))
    }

    fn open_file(&self) -> Result<Box<dyn SeekAndRead>> {
        self.fs.vfs.open_file(&self.path)
    }
    fn create_file(&self) -> Result<Box<dyn Write>> {
        self.fs.vfs.create_file(&self.path)
    }

    fn metadata(&self) -> Result<VMetadata> {
        self.fs.vfs.metadata(&self.path)
    }

    fn exists(&self) -> bool {
        self.fs.vfs.exists(&self.path)
    }
    fn create<T: VFS + 'static>(vfs: T) -> Result<Self> {
        Ok(VPath {
            path: "".to_string(),
            fs: Arc::new(FileSystem {
                vfs: Box::new(vfs)
            })
        })
    }
}

pub mod physical;

/*

#![allow(unused_imports)]
#![allow(unused_variables)]

#[macro_use]
mod macros {
    use std::io::{Result, Error, ErrorKind};
    use std;

    fn to_io_error<E: std::error::Error>(error: E) -> Error {
        Error::new(ErrorKind::Other, error.description())
    }

    pub fn to_io_result<T, E: std::error::Error>(result: std::result::Result<T, E>) -> Result<T> {
        match result {
            Ok(result) => Ok(result),
            Err(error) => Err(to_io_error(error)),
        }
    }

    macro_rules! ctry {
    ($result:expr) => (try!($crate::macros::to_io_result($result)));
    }


}



pub mod physical;
pub use physical::PhysicalFS;

pub mod altroot;
pub use altroot::AltrootFS;

pub mod memory;
pub use memory::MemoryFS;

pub mod util;

use std::path::{Path, PathBuf};
use std::convert::AsRef;

use std::fmt::Debug;
use std::io::{Read, Write, Seek, Result};
use std::borrow::Cow;

/// A abstract path to a location in a filesystem
pub trait VPath: Debug + std::marker::Send + std::marker::Sync {
    /// Open the file at this path with the given options
    fn open_with_options(&self, openOptions: &OpenOptions) -> Result<Box<VFile>>;
    /// Open the file at this path for reading
    fn open(&self) -> Result<Box<VFile>> {
        self.open_with_options(OpenOptions::new().read(true))
    }
    /// Open the file at this path for writing, truncating it if it exists already
    fn create(&self) -> Result<Box<VFile>> {
        self.open_with_options(OpenOptions::new().write(true).create(true).truncate(true))
    }
    /// Open the file at this path for appending, creating it if necessary
    fn append(&self) -> Result<Box<VFile>> {
        self.open_with_options(OpenOptions::new().write(true).create(true).append(true))
    }
    /// Create a directory at the location by this path
    fn mkdir(&self) -> Result<()>;

    /// Remove a file
    fn rm(&self) -> Result<()>;

    /// Remove a file or directory and all its contents
    fn rmrf(&self) -> Result<()>;


    /// The file name of this path
    fn file_name(&self) -> Option<String>;

    /// The extension of this filename
    fn extension(&self) -> Option<String>;

    /// append a segment to this path
    fn resolve(&self, path: &String) -> Box<VPath>;

    /// Get the parent path
    fn parent(&self) -> Option<Box<VPath>>;

    /// Check if the file existst
    fn exists(&self) -> bool;

    /// Get the file's metadata
    fn metadata(&self) -> Result<Box<VMetadata>>;

    /// Retrieve the path entries in this path
    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<Box<VPath>>>>>;

    /// Retrieve a string representation
    fn to_string(&self) -> Cow<str>;

    /// Retrieve a standard PathBuf, if available (usually only for PhysicalFS)
    fn to_path_buf(&self) -> Option<PathBuf>;

    fn box_clone(&self) -> Box<VPath>;
}

impl Clone for Box<VPath> {
    fn clone(&self) -> Box<VPath> {
        self.box_clone()
    }
}


/// Resolve the path relative to the given base returning a new path
pub fn resolve<S: Into<String>>(base: &VPath, path: S) -> Box<VPath> {
    base.resolve(&path.into())
}

/// An abstract file object
pub trait VFile: Read + Write + Seek + Debug {}

impl<T> VFile for T where T: Read + Write + Seek + Debug {}

/// File metadata abstraction
pub trait VMetadata {
    /// Returns true iff this path is a directory
    fn is_dir(&self) -> bool;
    /// Returns true iff this path is a file
    fn is_file(&self) -> bool;
    /// Returns the length of the file at this path
    fn len(&self) -> u64;
}

/// An abstract virtual file system
pub trait VFS {
    /// The type of file objects
    type PATH: VPath;
    /// The type of path objects
    type FILE: VFile;
    /// The type of metadata objects
    type METADATA: VMetadata;

    /// Create a new path within this filesystem
    fn path<T: Into<String>>(&self, path: T) -> Self::PATH;
}


/// Options for opening files
#[derive(Debug, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub append: bool,
    pub truncate: bool,
}

impl OpenOptions {
    /// Create a new instance
    pub fn new() -> OpenOptions {
        Default::default()
    }

    /// Open for reading
    pub fn read(&mut self, read: bool) -> &mut OpenOptions {
        self.read = read;
        self
    }

    /// Open for writing
    pub fn write(&mut self, write: bool) -> &mut OpenOptions {
        self.write = write;
        self
    }

    /// Create the file if it does not exist yet
    pub fn create(&mut self, create: bool) -> &mut OpenOptions {
        self.create = create;
        self
    }

    /// Append at the end of the file
    pub fn append(&mut self, append: bool) -> &mut OpenOptions {
        self.append = append;
        self
    }

    /// Truncate the file to 0 bytes after opening
    pub fn truncate(&mut self, truncate: bool) -> &mut OpenOptions {
        self.truncate = truncate;
        self
    }
}
*/
