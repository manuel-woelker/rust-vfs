//! The filesystem trait definitions needed to implement new virtual filesystems

use crate::error::VfsErrorKind;
use crate::{SeekAndRead, SeekAndWrite, VfsError, VfsFileType, VfsMetadata, VfsPath, VfsResult};
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::SystemTime;

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
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>>;
    /// Creates the directory at this path
    ///
    /// Note that the parent directory must already exist.
    fn create_dir(&self, path: &str) -> VfsResult<()>;
    /// Opens the file at this path for reading
    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>>;

    /// Read a file into a ``Vec<u8>``. This can be overrided by filesystems like MemoryFS to
    /// improve performance/reduce cloning etc
    fn read_to_bytes(&self, path: &str) -> VfsResult<Vec<u8>> {
        let metadata = self.metadata(path).map_err(|err| {
            err.with_path(path)
                .with_context(|| "Could not get metadata")
        })?;

        if metadata.file_type != VfsFileType::File {
            return Err(
                VfsError::from(VfsErrorKind::Other("Path is a directory".into()))
                    .with_path(path)
                    .with_context(|| "Could not read path"),
            );
        }
        let mut file = self.open_file(path)?;
        let mut contents = Vec::with_capacity(metadata.len as usize);
        file.read_to_end(&mut contents).map_err(|source| {
            VfsError::from(source)
                .with_path(path)
                .with_context(|| "Could not read path")
        })?;
        Ok(contents)
    }

    /// Read a file into a ``String``. This can be overrided by filesystems like MemoryFS to
    /// improve performance/hold less locks etc
    fn read_to_string(&self, path: &str) -> VfsResult<String> {
        let metadata = self.metadata(path).map_err(|err| {
            err.with_path(path)
                .with_context(|| "Could not get metadata")
        })?;

        if metadata.file_type != VfsFileType::File {
            return Err(
                VfsError::from(VfsErrorKind::Other("Path is a directory".into()))
                    .with_path(path)
                    .with_context(|| "Could not read path"),
            );
        }

        let mut file = self.open_file(path)?;
        let mut contents = String::with_capacity(metadata.len as usize);

        file.read_to_string(&mut contents).map_err(|source| {
            VfsError::from(source)
                .with_path(path)
                .with_context(|| "Could not read path")
        })?;

        Ok(contents)
    }

    /// Creates a file at this path for writing
    fn create_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>>;
    /// Opens the file at this path for appending
    fn append_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>>;
    /// Returns the file metadata for the file at this path
    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;
    /// Sets the files creation timestamp, if the implementation supports it
    fn set_creation_time(&self, _path: &str, _time: SystemTime) -> VfsResult<()> {
        Err(VfsError::from(VfsErrorKind::NotSupported))
    }
    /// Sets the files modification timestamp, if the implementation supports it
    fn set_modification_time(&self, _path: &str, _time: SystemTime) -> VfsResult<()> {
        Err(VfsError::from(VfsErrorKind::NotSupported))
    }
    /// Sets the files access timestamp, if the implementation supports it
    fn set_access_time(&self, _path: &str, _time: SystemTime) -> VfsResult<()> {
        Err(VfsError::from(VfsErrorKind::NotSupported))
    }
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
    /// Returns the file list as bare files (e.g. /init.luau, /foo/bar.luau)
    fn file_list(&self) -> VfsResult<HashSet<String>> {
        Err(VfsErrorKind::NotSupported.into())
    }
    /// Returns when the filesystem as a whole was last 'reset'. Only applicable for MemoryFS
    fn fs_last_reset(&self) -> VfsResult<SystemTime> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

impl<T: FileSystem> From<T> for VfsPath {
    fn from(filesystem: T) -> Self {
        VfsPath::new(filesystem)
    }
}

impl FileSystem for Box<dyn FileSystem> {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.as_ref().read_dir(path)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.as_ref().create_dir(path)
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.as_ref().open_file(path)
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.as_ref().create_file(path)
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.as_ref().append_file(path)
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.as_ref().metadata(path)
    }

    fn set_creation_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_creation_time(path, time)
    }

    fn set_modification_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_modification_time(path, time)
    }

    fn set_access_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_access_time(path, time)
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        self.as_ref().exists(path)
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.as_ref().remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.as_ref().remove_dir(path)
    }

    fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().copy_file(src, dest)
    }

    fn move_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().move_file(src, dest)
    }

    fn move_dir(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().move_dir(src, dest)
    }
}

impl FileSystem for Arc<dyn FileSystem> {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.as_ref().read_dir(path)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.as_ref().create_dir(path)
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.as_ref().open_file(path)
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.as_ref().create_file(path)
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.as_ref().append_file(path)
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.as_ref().metadata(path)
    }

    fn set_creation_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_creation_time(path, time)
    }

    fn set_modification_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_modification_time(path, time)
    }

    fn set_access_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        self.as_ref().set_access_time(path, time)
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        self.as_ref().exists(path)
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.as_ref().remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.as_ref().remove_dir(path)
    }

    fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().copy_file(src, dest)
    }

    fn move_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().move_file(src, dest)
    }

    fn move_dir(&self, src: &str, dest: &str) -> VfsResult<()> {
        self.as_ref().move_dir(src, dest)
    }
}