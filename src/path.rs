//! Virtual filesystem path
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)

use crate::error::VfsResultExt;
use crate::{FileSystem, VfsError, VfsResult};
use std::io::{Read, Seek, Write};
use std::sync::Arc;

/// Trait combining Seek and Read, return value for opening files
pub trait SeekAndRead: Seek + Read {}

impl<T> SeekAndRead for T where T: Seek + Read {}

/// Type of file
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VfsFileType {
    /// A plain file
    File,
    /// A Directory
    Directory,
}

/// File metadata information
#[derive(Debug)]
pub struct VfsMetadata {
    /// The type of file
    pub file_type: VfsFileType,
    /// Length of the file in bytes, 0 for directories
    pub len: u64,
}

#[derive(Debug)]
struct VFS {
    fs: Box<dyn FileSystem>,
}

/// A virtual filesystem path, identifying a single file or directory in this virtual filesystem
#[derive(Clone, Debug)]
pub struct VfsPath {
    path: String,
    fs: Arc<VFS>,
}

impl PartialEq for VfsPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && Arc::ptr_eq(&self.fs, &other.fs)
    }
}

impl Eq for VfsPath {}

impl VfsPath {
    /// Creates a root path for the given filesystem
    pub fn new<T: FileSystem>(filesystem: T) -> Self {
        VfsPath {
            path: "".to_string(),
            fs: Arc::new(VFS {
                fs: Box::new(filesystem),
            }),
        }
    }

    /// Returns the string representation of this path
    pub fn as_str(&self) -> &str {
        &self.path
    }

    /// Appends a path segment to this path, returning the result
    pub fn join(&self, path: &str) -> VfsResult<Self> {
        if path.is_empty() {
            return Ok(self.clone());
        }
        let mut new_components: Vec<&str> = vec![];
        let mut base_path = self.clone();
        for component in path.split('/') {
            if component == "" {
                return Err(VfsError::InvalidPath {
                    path: path.to_string(),
                });
            }
            if component == "." {
                continue;
            }
            if component == ".." {
                if !new_components.is_empty() {
                    new_components.truncate(new_components.len() - 1);
                } else if let Some(parent) = base_path.parent() {
                    base_path = parent;
                } else {
                    return Err(VfsError::InvalidPath {
                        path: path.to_string(),
                    });
                }
            } else {
                new_components.push(&component);
            }
        }
        let mut path = base_path.path;
        for component in new_components {
            path += "/";
            path += component
        }
        Ok(VfsPath {
            path,
            fs: self.fs.clone(),
        })
    }

    /// Iterates over all entries of this directory path
    pub fn read_dir(&self) -> VfsResult<Box<dyn Iterator<Item = VfsPath>>> {
        let parent = self.path.clone();
        let fs = self.fs.clone();
        Ok(Box::new(
            self.fs
                .fs
                .read_dir(&self.path)
                .with_context(|| format!("Could not read directory '{}'", &self.path))?
                .map(move |path| VfsPath {
                    path: format!("{}/{}", parent, path),
                    fs: fs.clone(),
                }),
        ))
    }

    /// Creates the directory at this path
    ///
    /// Note that the parent directory must exist.
    pub fn create_dir(&self) -> VfsResult<()> {
        self.get_parent("create directory")?;
        self.fs
            .fs
            .create_dir(&self.path)
            .with_context(|| format!("Could not create directory '{}'", &self.path))
    }

    /// Creates the directory at this path, also creating parent directories as necessary
    pub fn create_dir_all(&self) -> VfsResult<()> {
        let mut pos = 1;
        let path = &self.path;
        if path.is_empty() {
            // root exists always
            return Ok(());
        }
        loop {
            // Iterate over path segments
            let end = path[pos..]
                .find('/')
                .map(|it| it + pos)
                .unwrap_or_else(|| path.len());
            let directory = &path[..end];
            if !self.fs.fs.exists(directory)? {
                self.fs.fs.create_dir(directory)?;
            }
            if end == path.len() {
                break;
            }
            pos = end + 1;
        }
        Ok(())
    }

    /// Opens the file at this path for reading
    pub fn open_file(&self) -> VfsResult<Box<dyn SeekAndRead>> {
        self.fs
            .fs
            .open_file(&self.path)
            .with_context(|| format!("Could not open file '{}'", &self.path))
    }

    /// Creates a file at this path for writing, overwriting any existing file
    pub fn create_file(&self) -> VfsResult<Box<dyn Write>> {
        self.get_parent("create file")?;
        self.fs
            .fs
            .create_file(&self.path)
            .with_context(|| format!("Could not create file '{}'", &self.path))
    }

    /// Checks whether parent is a directory
    fn get_parent(&self, action: &str) -> VfsResult<()> {
        let parent = self.parent();
        match parent {
            None => {
                return Err(format!(
                    "Could not {} at '{}', not a valid location",
                    action, &self.path
                )
                .into());
            }
            Some(directory) => {
                if !directory.exists()? {
                    return Err(format!(
                        "Could not {} at '{}', parent directory does not exist",
                        action, &self.path
                    )
                    .into());
                }
                let metadata = directory.metadata()?;
                if metadata.file_type != VfsFileType::Directory {
                    return Err(format!(
                        "Could not {} at '{}', parent path is not a directory",
                        action, &self.path
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Opens the file at this path for appending
    pub fn append_file(&self) -> VfsResult<Box<dyn Write>> {
        self.fs
            .fs
            .append_file(&self.path)
            .with_context(|| format!("Could not open file '{}' for appending", &self.path))
    }

    /// Removes the file at this path
    pub fn remove_file(&self) -> VfsResult<()> {
        self.fs
            .fs
            .remove_file(&self.path)
            .with_context(|| format!("Could not remove file '{}'", &self.path))
    }

    /// Removes the directory at this path
    ///
    /// The directory must be empty.
    pub fn remove_dir(&self) -> VfsResult<()> {
        self.fs
            .fs
            .remove_dir(&self.path)
            .with_context(|| format!("Could not remove directory '{}'", &self.path))
    }

    /// Ensures that the directory at this path is removed, recursively deleting all contents if necessary
    ///
    /// Returns successfully if directory does not exist
    pub fn remove_dir_all(&self) -> VfsResult<()> {
        if !self.exists()? {
            return Ok(());
        }
        for child in self.read_dir()? {
            let metadata = child.metadata()?;
            match metadata.file_type {
                VfsFileType::File => child.remove_file()?,
                VfsFileType::Directory => child.remove_dir_all()?,
            }
        }
        self.remove_dir()?;
        Ok(())
    }

    /// Returns the file metadata for the file at this path
    pub fn metadata(&self) -> VfsResult<VfsMetadata> {
        self.fs
            .fs
            .metadata(&self.path)
            .with_context(|| format!("Could not get metadata for '{}'", &self.path))
    }

    /// Returns true if a file or directory exists at this path, false otherwise
    pub fn exists(&self) -> VfsResult<bool> {
        self.fs.fs.exists(&self.path)
    }

    /// Returns the filename portion of this path
    pub fn filename(&self) -> String {
        let index = self.path.rfind('/').map(|x| x + 1).unwrap_or(0);
        self.path[index..].to_string()
    }

    /// Returns the extension portion of this path
    pub fn extension(&self) -> Option<String> {
        let filename = self.filename();
        let mut parts = filename.rsplitn(2, '.');
        let after = parts.next();
        let before = parts.next();
        match before {
            None | Some("") => None,
            _ => after.map(|x| x.to_string()),
        }
    }

    /// Returns the parent path of this portion of this path
    ///
    /// Returns `None` if this is a root path
    pub fn parent(&self) -> Option<Self> {
        let index = self.path.rfind('/');
        index.map(|idx| VfsPath {
            path: self.path[..idx].to_string(),
            fs: self.fs.clone(),
        })
    }

    /// Recursively iterates over all the directories and files at this path
    ///
    /// Directories are visited before their children
    ///
    /// Note that the iterator items can contain errors, usually when directories are removed during the iteration.
    /// The returned paths may also point to non-existant files if there is concurrent removal.
    ///
    /// Also note that loops in the file system hierarchy may cause this iterator to never terminate.
    pub fn walk_dir(&self) -> VfsResult<WalkDirIterator> {
        Ok(WalkDirIterator {
            inner: Box::new(self.read_dir()?),
            todo: vec![],
        })
    }

    /// Reads a complete file to a string
    ///
    /// Returns an error if the file does not exist or is not valid UTF-8
    pub fn read_to_string(&self) -> VfsResult<String> {
        let metadata = self.metadata()?;
        if metadata.file_type != VfsFileType::File {
            return Err(VfsError::Other {
                message: format!("Could not read '{}' because it is a directory", self.path),
            });
        }
        let mut result = String::with_capacity(metadata.len as usize);
        self.open_file()?
            .read_to_string(&mut result)
            .map_err(From::from)
            .with_context(|| format!("Could not read '{}'", self.path))?;
        Ok(result)
    }

    /// Copies a file to a new destination
    ///
    /// The destination must not exist, but its parent directory must
    pub fn copy_file(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err("Destination exists already".to_string().into());
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.copy_file(&self.path, &destination.path);
                if let Err(VfsError::NotSupported) = result {
                    // continue
                } else {
                    return result;
                }
            }
            let mut src = self.open_file()?;
            let mut dest = destination.create_file()?;
            std::io::copy(&mut src, &mut dest)?;
            Ok(())
        }()
        .with_context(|| {
            format!(
                "Could not copy '{}' to '{}'",
                self.as_str(),
                destination.as_str()
            )
        })?;
        Ok(())
    }

    /// Moves or renames a file to a new destination
    ///
    /// The destination must not exist, but its parent directory must
    pub fn move_file(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err("Destination exists already".to_string().into());
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_file(&self.path, &destination.path);
                if let Err(VfsError::NotSupported) = result {
                    // continue
                } else {
                    return result;
                }
            }
            let mut src = self.open_file()?;
            let mut dest = destination.create_file()?;
            std::io::copy(&mut src, &mut dest)?;
            self.remove_file()?;
            Ok(())
        }()
        .with_context(|| {
            format!(
                "Could not move '{}' to '{}'",
                self.as_str(),
                destination.as_str()
            )
        })?;
        Ok(())
    }

    /// Copies a directory to a new destination, recursively
    ///
    /// The destination must not exist, but the parent directory must
    ///
    /// Returns the number of files copied
    pub fn copy_dir(&self, destination: &VfsPath) -> VfsResult<u64> {
        let mut files_copied = 0u64;
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err("Destination exists already".to_string().into());
            }
            destination.create_dir()?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            for file in self.walk_dir()? {
                let src_path: VfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata()?.file_type {
                    VfsFileType::Directory => dest_path.create_dir()?,
                    VfsFileType::File => src_path.copy_file(&dest_path)?,
                }
                files_copied += 1;
            }
            Ok(())
        }()
        .with_context(|| {
            format!(
                "Could not copy directory '{}' to '{}'",
                self.as_str(),
                destination.as_str()
            )
        })?;
        Ok(files_copied)
    }

    /// Moves a directory to a new destination, including subdirectories and files
    ///
    /// The destination must not exist, but its parent directory must
    pub fn move_dir(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err("Destination exists already".to_string().into());
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_dir(&self.path, &destination.path);
                if let Err(VfsError::NotSupported) = result {
                    // continue
                } else {
                    return result;
                }
            }
            destination.create_dir()?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            for file in self.walk_dir()? {
                let src_path: VfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata()?.file_type {
                    VfsFileType::Directory => dest_path.create_dir()?,
                    VfsFileType::File => src_path.copy_file(&dest_path)?,
                }
            }
            self.remove_dir_all()?;
            Ok(())
        }()
        .with_context(|| {
            format!(
                "Could not move directory '{}' to '{}'",
                self.as_str(),
                destination.as_str()
            )
        })?;
        Ok(())
    }
}

/// An iterator for recursively walking a file hierarchy
pub struct WalkDirIterator {
    /// the path iterator of the current directory
    inner: Box<dyn Iterator<Item = VfsPath>>,
    /// stack of subdirectories still to walk
    todo: Vec<VfsPath>,
}

impl std::fmt::Debug for WalkDirIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalkDirIterator")?;
        self.todo.fmt(f)
    }
}

impl Iterator for WalkDirIterator {
    type Item = VfsResult<VfsPath>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = loop {
            match self.inner.next() {
                Some(path) => break Some(Ok(path)),
                None => {
                    match self.todo.pop() {
                        None => return None, // all done!
                        Some(directory) => match directory.read_dir() {
                            Ok(iterator) => self.inner = iterator,
                            Err(err) => break Some(Err(err)),
                        },
                    }
                }
            }
        };
        if let Some(Ok(path)) = &result {
            let metadata = path.metadata();
            match metadata {
                Ok(metadata) => {
                    if metadata.file_type == VfsFileType::Directory {
                        self.todo.push(path.clone());
                    }
                }
                Err(err) => return Some(Err(err)),
            }
        }
        result
    }
}
