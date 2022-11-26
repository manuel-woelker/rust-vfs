//! The filesystem trait definitions needed to implement new virtual filesystems

use crate::error::VfsErrorKind;
use crate::{SeekAndRead, SeekAndReadAndWrite, VfsMetadata, VfsPath, VfsResult};
use std::fmt::Debug;
use std::io::Write;

/// File system implementations must implement this trait
/// All path parameters are absolute, starting with '/', except for the root directory
/// which is simply the empty string (i.e. "")
/// The character '/' is used to delimit directories on all platforms.
/// Path components may be any UTF-8 string, except "/", "." and ".."
///
/// Please use the test_macros [test_macros::test_vfs!] and [test_macros::test_vfs_readonly!]
pub trait FileSystem: Debug + Sync + Send + 'static {
    /// Iterates over all direct children of this directory path
    /// NOTE: the returned String items denote the local bare filenames, i.e. they should not contain "/" anywhere
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>>;
    /// Creates the directory at this path
    ///
    /// Note that the parent directory must already exist.
    fn create_dir(&self, path: &str) -> VfsResult<()>;
    /// Opens the file at this path for reading
    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>>;
    /// Creates a file at this path for writing
    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    /// Opens the file at this path for appending
    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    /// Returns the file metadata for the file at this path
    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;
    /// Returns true if a file or directory at path exists, false otherwise
    fn exists(&self, path: &str) -> VfsResult<bool>;
    /// Removes the file at this path
    fn remove_file(&self, path: &str) -> VfsResult<()>;
    /// Removes the directory at this path
    fn remove_dir(&self, path: &str) -> VfsResult<()>;
    /// Copies the src path to the destination path within the same filesystem (optional)
    fn copy_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
    /// Moves the src path to the destination path within the same filesystem (optional)
    fn move_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
    /// Moves the src directory to the destination path within the same filesystem (optional)
    fn move_dir(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    /// This obtains the potential size of the current path in the filesystem.
    ///
    /// This, by default, queries the current size.
    fn size_hint(&self, path: &str) -> VfsResult<u64> {
        self.metadata(path).map(|f| f.len)
    }

    /// Informs the filesystem to 'flush' its potentially cached information.
    fn sync(&self, path: &str) -> VfsResult<()>;

    /// Set a size hint for the associated path.
    ///
    /// This is, by default, a no-op.
    fn set_size_hint(&self, _hint: usize, _path: &str) -> VfsResult<()> {
        Ok(())
    }

    /// Opens the file at this path for reading and writing
    fn update_file(&self, _path: &str) -> VfsResult<Box<dyn SeekAndReadAndWrite>> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

impl<T: FileSystem> From<T> for VfsPath {
    fn from(filesystem: T) -> Self {
        VfsPath::new(filesystem)
    }
}
