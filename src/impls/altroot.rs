//! A file system with its root in a particular directory of another filesystem


use crate::{VfsPath, FileSystem, VfsResult, SeekAndRead, VfsMetadata};
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
    // Create a new root FileSystem at the given virtual path
    pub fn new(root: VfsPath) -> Self {
        AltrootFS {
            root,
        }
    }
}

impl AltrootFS {
    fn path(&self, path: &str) -> VfsPath {
        if path.is_empty() {
            return self.root.clone();
        }
        if path.starts_with("/") {
            return self.root.join(&path[1..]);
        }
        self.path(path)
    }
}


impl FileSystem for AltrootFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item=String>>> {
        self.path(path).read_dir().map(|result| result.map(|path| path.filename())).map(|entries| Box::new(entries) as Box<dyn Iterator<Item=String>>)
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path).create_dir()
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>> {
        self.path(path).open_file()
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        self.path(path).create_file()
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        self.path(path).append_file()
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.path(path).metadata()
    }

    fn exists(&self, path: &str) -> bool {
        self.path(path).exists()
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.path(path).remove_file()
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path).remove_dir()
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryFS;
    test_vfs!({
        let memory_root: VfsPath = MemoryFS::new().into();
        let altroot_path = memory_root.join("altroot");
        altroot_path.create_dir().unwrap();
        AltrootFS::new(altroot_path)
    });

    #[test]
    fn parent() {
        let memory_root: VfsPath = MemoryFS::new().into();
        let altroot_path = memory_root.join("altroot");
        altroot_path.create_dir().unwrap();
        let altroot : VfsPath = AltrootFS::new(altroot_path.clone()).into();
        assert_eq!(altroot.parent(), None);
        assert_eq!(altroot_path.parent(), Some(memory_root));
    }



}
