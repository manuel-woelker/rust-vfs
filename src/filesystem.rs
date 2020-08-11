//! The filesystem trait definitions needed to implement new virtual filesystems

use crate::{SeekAndRead, VfsError, VfsMetadata, VfsPath, VfsResult};
use std::fmt::Debug;
use std::io::Write;

/// File system implementations mus implement this trait
pub trait FileSystem: Debug + Sync + Send + 'static {
    /// Iterates over all entries of this directory path
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>>;
    /// Creates the directory at this path
    ///
    /// Note that the parent directory must exist.
    fn create_dir(&self, path: &str) -> VfsResult<()>;
    /// Opens the file at this path for reading
    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>>;
    /// Creates a file at this path for writing
    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    /// Opens the file at this path for appending
    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    /// Returns the file metadata for the file at this path
    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;
    fn exists(&self, path: &str) -> bool;
    /// Removes the file at this path
    fn remove_file(&self, path: &str) -> VfsResult<()>;
    /// Removes the directory at this path
    fn remove_dir(&self, path: &str) -> VfsResult<()>;
    /// Copies the src path to the destination path within the same filesystem (optional)
    fn copy_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
    /// Moves the src path to the destination path within the same filesystem (optional)
    fn move_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
    /// Moves the src directory to the destination path within the same filesystem (optional)
    fn move_dir(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
}

impl<T: FileSystem> From<T> for VfsPath {
    fn from(filesystem: T) -> Self {
        VfsPath::new(filesystem)
    }
}
