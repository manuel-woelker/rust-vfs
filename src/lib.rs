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

pub mod memory;
pub use memory::MemoryFS;

pub mod util;

use std::path::Path;
use std::convert::AsRef;

use std::fmt::Debug;
use std::io::{Read, Write, Seek, Result};


/// A abstract path to a location in a filesystem
pub trait VPath: Clone + Debug {
    /// The kind of filesystem
    type FS: VFS;
    /// Open the file at this path with the given options
    fn open(&self, openOptions: &OpenOptions) -> Result<<Self::FS as VFS>::FILE>;
    /// Open the file at this path for reading
    fn read(&self) -> Result<<Self::FS as VFS>::FILE> {
        self.open(OpenOptions::new().read(true))
    }
    /// Open the file at this path for writing, truncating it if it exists already
    fn create(&self) -> Result<<Self::FS as VFS>::FILE> {
        self.open(OpenOptions::new().write(true).create(true).truncate(true))
    }
    /// Open the file at this path for appending, creating it if necessary
    fn append(&self) -> Result<<Self::FS as VFS>::FILE> {
        self.open(OpenOptions::new().write(true).create(true).append(true))
    }
    /// Create a directory at the location by this path
    fn mkdir(&self) -> Result<()>;

    /// Get the parent path
    fn parent(&self) -> Option<Self>;

    /// The file name of this path
    fn file_name(&self) -> Option<String>;

    /// The extension of this filename
    fn extension(&self) -> Option<String>;

    /// append a segment to this path
    fn push<'a, T: Into<&'a str>>(&mut self, path: T);

    /// Check if the file existst
    fn exists(&self) -> bool;

    /// Get the file's metadata
    fn metadata(&self) -> Result<<Self::FS as VFS>::METADATA>;

    /// Retrieve the path entries in this path
    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<Self>>>>;
}

/// An abstract file object
pub trait VFile: Read + Write + Seek + Debug {}

impl<T> VFile for T where T: Read + Write + Seek + Debug {}

/// File metadata abstraction
pub trait VMetadata {
    fn is_dir(&self) -> bool;
    fn is_file(&self) -> bool;
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

#[derive(Debug, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    append: bool,
    truncate: bool,
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        Default::default()
    }

    pub fn read(&mut self, read: bool) -> &mut OpenOptions {
        self.read = read;
        self
    }

    pub fn write(&mut self, write: bool) -> &mut OpenOptions {
        self.write = write;
        self
    }

    pub fn append(&mut self, append: bool) -> &mut OpenOptions {
        self.append = append;
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut OpenOptions {
        self.truncate = truncate;
        self
    }

    pub fn create(&mut self, create: bool) -> &mut OpenOptions {
        self.create = create;
        self
    }

    pub fn open<P: VPath>(&self, path: &P) -> Result<<P::FS as VFS>::FILE> {
        path.open(self)
    }
}
