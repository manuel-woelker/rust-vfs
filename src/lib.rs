//! Virtual file system abstraction
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different filesystem implementations (i.e. an in memory implementation for unit tests)
//!
//! The main interaction with the virtual filesystem is by using virtual paths ([`VPath`](struct.VPath.html)).
//!
//! This crate currently has the following implementations:
//!
//!  * **PhysicalFS** - the actual filesystem of the underlying OS
//!  * **MemoryFS** - an ephemeral in-memory implementation (intended for unit tests)

#[cfg(test)]
#[macro_use]
pub mod test_macros;

pub mod memory;
pub mod physical;

use std::fmt::{Debug, Display};
use std::io::{Read, Seek, Write};
use std::sync::Arc;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VfsError {
    #[error("data store disconnected")]
    IoError(#[from] std::io::Error),
    #[error("the file or directory `{path}` could not be found")]
    FileNotFound { path: String },
    #[error("other VFS error: {message}")]
    Other { message: String },
    #[error("{context}, cause: {cause}")]
    WithContext {
        context: String,
        #[source]
        cause: Box<VfsError>,
    },
}

pub type Result<T> = std::result::Result<T, VfsError>;

pub trait ResultExt<T> {
    fn with_context<C, F>(self, f: F) -> Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T> ResultExt<T> for Result<T> {
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|error| VfsError::WithContext {
            context: context().to_string(),
            cause: Box::new(error),
        })
    }
}

pub trait SeekAndRead: Seek + Read {}

impl<T> SeekAndRead for T where T: Seek + Read {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VFileType {
    File,
    Directory,
}

#[derive(Debug)]
pub struct VMetadata {
    pub file_type: VFileType,
    pub len: u64,
}

pub trait VFS: Debug + Sync + Send {
    fn read_dir(&self, path: &str) -> Result<Box<dyn Iterator<Item = String>>>;
    fn create_dir(&self, path: &str) -> Result<()>;
    fn open_file(&self, path: &str) -> Result<Box<dyn SeekAndRead>>;
    fn create_file(&self, path: &str) -> Result<Box<dyn Write>>;
    fn append_file(&self, path: &str) -> Result<Box<dyn Write>>;
    fn metadata(&self, path: &str) -> Result<VMetadata>;
    fn exists(&self, path: &str) -> bool;
    fn remove_file(&self, path: &str) -> Result<()>;
    fn remove_dir(&self, path: &str) -> Result<()>;
}

#[derive(Debug)]
pub struct FileSystem {
    vfs: Box<dyn VFS>,
}

#[derive(Clone, Debug)]
pub struct VPath {
    path: String,
    fs: Arc<FileSystem>,
}

impl PartialEq for VPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && Arc::ptr_eq(&self.fs, &other.fs)
    }
}

impl Eq for VPath {}

impl VPath {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn join(&self, path: &str) -> Self {
        VPath {
            path: format!("{}/{}", self.path, path),
            fs: self.fs.clone(),
        }
    }

    pub fn read_dir(&self) -> Result<Box<dyn Iterator<Item = VPath>>> {
        let parent = self.path.clone();
        let fs = self.fs.clone();
        Ok(Box::new(
            self.fs
                .vfs
                .read_dir(&self.path)
                .with_context(|| format!("Could not read directory '{}'", &self.path))?
                .map(move |path| VPath {
                    path: format!("{}/{}", parent, path),
                    fs: fs.clone(),
                }),
        ))
    }

    pub fn create_dir(&self) -> Result<()> {
        self.fs
            .vfs
            .create_dir(&self.path)
            .with_context(|| format!("Could not create directory '{}'", &self.path))
    }

    pub fn create_dir_all(&self) -> Result<()> {
        let mut pos = 1;
        let path = &self.path;
        loop {
            // Iterate over path segments
            let end = path[pos..]
                .find('/')
                .map(|it| it + pos)
                .unwrap_or_else(|| path.len());
            let directory = &path[..end];
            if !self.fs.vfs.exists(directory) {
                self.fs.vfs.create_dir(directory)?;
            }
            if end == path.len() {
                break;
            }
            pos = end + 1;
        }
        Ok(())
    }

    pub fn open_file(&self) -> Result<Box<dyn SeekAndRead>> {
        self.fs
            .vfs
            .open_file(&self.path)
            .with_context(|| format!("Could not open file '{}'", &self.path))
    }
    pub fn create_file(&self) -> Result<Box<dyn Write>> {
        self.fs
            .vfs
            .create_file(&self.path)
            .with_context(|| format!("Could not create file '{}'", &self.path))
    }
    pub fn append_file(&self) -> Result<Box<dyn Write>> {
        self.fs
            .vfs
            .append_file(&self.path)
            .with_context(|| format!("Could not open file '{}' for appending", &self.path))
    }
    pub fn remove_file(&self) -> Result<()> {
        self.fs
            .vfs
            .remove_file(&self.path)
            .with_context(|| format!("Could not remove file '{}'", &self.path))
    }

    pub fn remove_dir(&self) -> Result<()> {
        self.fs
            .vfs
            .remove_dir(&self.path)
            .with_context(|| format!("Could not remove directory '{}'", &self.path))
    }

    pub fn remove_dir_all(&self) -> Result<()> {
        if !self.exists() {
            return Ok(());
        }
        for child in self.read_dir()? {
            let metadata = child.metadata()?;
            match metadata.file_type {
                VFileType::File => child.remove_file()?,
                VFileType::Directory => child.remove_dir_all()?,
            }
        }
        self.remove_dir()?;
        Ok(())
    }

    pub fn metadata(&self) -> Result<VMetadata> {
        self.fs
            .vfs
            .metadata(&self.path)
            .with_context(|| format!("Could get metadata for '{}'", &self.path))
    }

    pub fn exists(&self) -> bool {
        self.fs.vfs.exists(&self.path)
    }
    pub fn create<T: VFS + 'static>(vfs: T) -> Result<Self> {
        Ok(VPath {
            path: "".to_string(),
            fs: Arc::new(FileSystem { vfs: Box::new(vfs) }),
        })
    }

    pub fn filename(&self) -> String {
        let index = self.path.rfind('/').map(|x| x + 1).unwrap_or(0);
        self.path[index..].to_string()
    }

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

    pub fn parent(&self) -> Option<Self> {
        let index = self.path.rfind('/').map(|x| x);
        index.map(|idx| VPath {
            path: self.path[..idx].to_string(),
            fs: self.fs.clone(),
        })
    }
}
