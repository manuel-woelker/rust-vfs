//! A file system with its root in a particular directory of another filesystem

use crate::{error::VfsErrorKind, FileSystem, SeekAndRead, VfsMetadata, VfsPath, VfsResult};
use std::io::Write;

/// Similar to a chroot but done purely by path manipulation
///
/// NOTE: This mechanism should only be used for convenience, NOT FOR SECURITY
///
/// Symlinks, hardlinks, remounts, side channels and other file system mechanisms can be exploited
/// to circumvent this mechanism
#[derive(Debug, Clone)]
pub struct AltrootFS {
    root: VfsPath,
}

impl AltrootFS {
    /// Create a new root FileSystem at the given virtual path
    pub fn new(root: VfsPath) -> Self {
        AltrootFS { root }
    }
}

impl AltrootFS {
    #[allow(clippy::manual_strip)] // strip prefix manually for MSRV 1.32
    fn path(&self, path: &str) -> VfsResult<VfsPath> {
        if path.is_empty() {
            return Ok(self.root.clone());
        }
        if path.starts_with('/') {
            return self.root.join(&path[1..]);
        }
        self.root.join(path)
    }
}

impl FileSystem for AltrootFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.path(path)?
            .read_dir()
            .map(|result| result.map(|path| path.filename()))
            .map(|entries| Box::new(entries) as Box<dyn Iterator<Item = String> + Send>)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.create_dir()
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.path(path)?.open_file()
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        self.path(path)?.create_file()
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        self.path(path)?.append_file()
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.path(path)?.metadata()
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        self.path(path)
            .map(|path| path.exists())
            .unwrap_or(Ok(false))
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.remove_file()
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.remove_dir()
    }

    fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        if dest.is_empty() {
            return Err(VfsErrorKind::NotSupported.into());
        }
        self.path(src)?.copy_file(&self.path(dest)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryFS;
    test_vfs!({
        let memory_root: VfsPath = MemoryFS::new().into();
        let altroot_path = memory_root.join("altroot").unwrap();
        altroot_path.create_dir().unwrap();
        AltrootFS::new(altroot_path)
    });

    #[test]
    fn parent() {
        let memory_root: VfsPath = MemoryFS::new().into();
        let altroot_path = memory_root.join("altroot").unwrap();
        altroot_path.create_dir().unwrap();
        let altroot: VfsPath = AltrootFS::new(altroot_path.clone()).into();
        assert_eq!(altroot.parent(), altroot.root());
        assert_eq!(altroot_path.parent(), memory_root);
    }
}

#[cfg(test)]
mod tests_physical {
    use super::*;
    use crate::PhysicalFS;
    test_vfs!({
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let physical_root: VfsPath = PhysicalFS::new(dir).into();
        let altroot_path = physical_root.join("altroot").unwrap();
        altroot_path.create_dir().unwrap();
        AltrootFS::new(altroot_path)
    });

    test_vfs_readonly!({
        let physical_root: VfsPath = PhysicalFS::new("test").into();
        let altroot_path = physical_root.join("test_directory").unwrap();
        AltrootFS::new(altroot_path)
    });
}
