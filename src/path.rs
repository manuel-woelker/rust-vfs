//! Virtual filesystem path
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)

use crate::error::VfsResultExt;
use crate::{FileSystem, VfsError, VfsResult};
use std::io::{Read, Seek, Write};
use std::sync::Arc;

pub trait SeekAndRead: Seek + Read {}

impl<T> SeekAndRead for T where T: Seek + Read {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VfsFileType {
    File,
    Directory,
}

#[derive(Debug)]
pub struct VfsMetadata {
    pub file_type: VfsFileType,
    pub len: u64,
}

#[derive(Debug)]
pub struct VFS {
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
        self.fs
            .fs
            .create_dir(&self.path)
            .with_context(|| format!("Could not create directory '{}'", &self.path))
    }

    /// Creates the directory at this path, also creating parent directories as necessary
    pub fn create_dir_all(&self) -> VfsResult<()> {
        let mut pos = 1;
        let path = &self.path;
        loop {
            // Iterate over path segments
            let end = path[pos..]
                .find('/')
                .map(|it| it + pos)
                .unwrap_or_else(|| path.len());
            let directory = &path[..end];
            if !self.fs.fs.exists(directory) {
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

    /// Creates a file at this path for writing
    pub fn create_file(&self) -> VfsResult<Box<dyn Write>> {
        self.fs
            .fs
            .create_file(&self.path)
            .with_context(|| format!("Could not create file '{}'", &self.path))
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
        if !self.exists() {
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
            .with_context(|| format!("Could get metadata for '{}'", &self.path))
    }

    /// Returns true if a file or directory exists at this path, false otherwise
    pub fn exists(&self) -> bool {
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
        let index = self.path.rfind('/').map(|x| x);
        index.map(|idx| VfsPath {
            path: self.path[..idx].to_string(),
            fs: self.fs.clone(),
        })
    }
}
