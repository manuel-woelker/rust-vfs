//! The async filesystem trait definitions needed to implement new async virtual filesystems

use crate::async_vfs::{AsyncVfsPath, SeekAndRead};
use crate::error::VfsErrorKind;
use crate::{VfsMetadata, VfsResult};

use async_std::io::Write;
use async_std::stream::Stream;
use async_trait::async_trait;
use std::fmt::Debug;

/// File system implementations must implement this trait
/// All path parameters are absolute, starting with '/', except for the root directory
/// which is simply the empty string (i.e. "")
/// The character '/' is used to delimit directories on all platforms.
/// Path components may be any UTF-8 string, except "/", "." and ".."
///
/// Please use the test_macros [test_macros::test_async_vfs!] and [test_macros::test_async_vfs_readonly!]
#[async_trait]
pub trait AsyncFileSystem: Debug + Sync + Send + 'static {
    /// Iterates over all direct children of this directory path
    /// NOTE: the returned String items denote the local bare filenames, i.e. they should not contain "/" anywhere
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>>;
    /// Creates the directory at this path
    ///
    /// Note that the parent directory must already exist.
    async fn create_dir(&self, path: &str) -> VfsResult<()>;
    /// Opens the file at this path for reading
    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>>;
    /// Creates a file at this path for writing
    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>>;
    /// Opens the file at this path for appending
    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>>;
    /// Returns the file metadata for the file at this path
    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;
    /// Returns true if a file or directory at path exists, false otherwise
    async fn exists(&self, path: &str) -> VfsResult<bool>;
    /// Removes the file at this path
    async fn remove_file(&self, path: &str) -> VfsResult<()>;
    /// Removes the directory at this path
    async fn remove_dir(&self, path: &str) -> VfsResult<()>;
    /// Copies the src path to the destination path within the same filesystem (optional)
    async fn copy_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
    /// Moves the src path to the destination path within the same filesystem (optional)
    async fn move_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
    /// Moves the src directory to the destination path within the same filesystem (optional)
    async fn move_dir(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

impl<T: AsyncFileSystem> From<T> for AsyncVfsPath {
    fn from(filesystem: T) -> Self {
        AsyncVfsPath::new(filesystem)
    }
}
