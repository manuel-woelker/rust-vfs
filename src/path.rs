use crate::error::VfsResultExt;
use crate::{FileSystem, VfsResult};
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
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn join(&self, path: &str) -> Self {
        VfsPath {
            path: format!("{}/{}", self.path, path),
            fs: self.fs.clone(),
        }
    }

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

    pub fn create_dir(&self) -> VfsResult<()> {
        self.fs
            .fs
            .create_dir(&self.path)
            .with_context(|| format!("Could not create directory '{}'", &self.path))
    }

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

    pub fn open_file(&self) -> VfsResult<Box<dyn SeekAndRead>> {
        self.fs
            .fs
            .open_file(&self.path)
            .with_context(|| format!("Could not open file '{}'", &self.path))
    }
    pub fn create_file(&self) -> VfsResult<Box<dyn Write>> {
        self.fs
            .fs
            .create_file(&self.path)
            .with_context(|| format!("Could not create file '{}'", &self.path))
    }
    pub fn append_file(&self) -> VfsResult<Box<dyn Write>> {
        self.fs
            .fs
            .append_file(&self.path)
            .with_context(|| format!("Could not open file '{}' for appending", &self.path))
    }
    pub fn remove_file(&self) -> VfsResult<()> {
        self.fs
            .fs
            .remove_file(&self.path)
            .with_context(|| format!("Could not remove file '{}'", &self.path))
    }

    pub fn remove_dir(&self) -> VfsResult<()> {
        self.fs
            .fs
            .remove_dir(&self.path)
            .with_context(|| format!("Could not remove directory '{}'", &self.path))
    }

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

    pub fn metadata(&self) -> VfsResult<VfsMetadata> {
        self.fs
            .fs
            .metadata(&self.path)
            .with_context(|| format!("Could get metadata for '{}'", &self.path))
    }

    pub fn exists(&self) -> bool {
        self.fs.fs.exists(&self.path)
    }
    pub fn create<T: FileSystem + 'static>(vfs: T) -> VfsResult<Self> {
        Ok(VfsPath {
            path: "".to_string(),
            fs: Arc::new(VFS { fs: Box::new(vfs) }),
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
        index.map(|idx| VfsPath {
            path: self.path[..idx].to_string(),
            fs: self.fs.clone(),
        })
    }
}
