use std::path::Path;
use std::convert::AsRef;

use std::fmt::Debug;
use std::io::{Read, Write, Seek, Result};


/// A abstract path to a location in a filesystem
pub trait VPath: Clone + Debug {
    /// The kind of filesystem
    type FS: VFS;
    /// Open the file at this path for reading
    fn open(&self) -> Result<<Self::FS as VFS>::FILE>;
    /// Open the file at this path for writing, truncating it if it exists already
    fn create(&self) -> Result<<Self::FS as VFS>::FILE>;
    /// Open the file at this path for appending, creating it if necessary
    fn append(&self) -> Result<<Self::FS as VFS>::FILE>;

    /// Create a directory at the location by this path
    fn mkdir(&self) -> Result<()>;

    /// Get the parent path
    fn parent(&self) -> Option<Self>;

    /// The file name of this path
    fn file_name(&self) -> Option<String>;

    /// append a segment to this path
    fn push<'a, T: Into<&'a str>>(&mut self, path: T);

    /// Check if the file existst
    fn exists(&self) -> bool;

    /// Get the file's metadata
    fn metadata(&self) -> Result<<Self::FS as VFS>::METADATA>;

    /// Retrieve the path entries in this path
    fn read_dir(&self) -> Result<Box<Iterator<Item = Result<<<Self as VPath>::FS as VFS>::PATH>>>>;
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
