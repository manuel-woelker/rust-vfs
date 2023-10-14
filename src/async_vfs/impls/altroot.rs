//! A file system with its root in a particular directory of another filesystem

use crate::async_vfs::{AsyncFileSystem, AsyncVfsPath, SeekAndRead};
use crate::{error::VfsErrorKind, VfsMetadata, VfsResult};

use async_std::io::Write;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};

/// Similar to a chroot but done purely by path manipulation
///
/// NOTE: This mechanism should only be used for convenience, NOT FOR SECURITY
///
/// Symlinks, hardlinks, remounts, side channels and other file system mechanisms can be exploited
/// to circumvent this mechanism
#[derive(Debug, Clone)]
pub struct AsyncAltrootFS {
    root: AsyncVfsPath,
}

impl AsyncAltrootFS {
    /// Create a new root FileSystem at the given virtual path
    pub fn new(root: AsyncVfsPath) -> Self {
        AsyncAltrootFS { root }
    }
}

impl AsyncAltrootFS {
    #[allow(clippy::manual_strip)] // strip prefix manually for MSRV 1.32
    fn path(&self, path: &str) -> VfsResult<AsyncVfsPath> {
        if path.is_empty() {
            return Ok(self.root.clone());
        }
        if path.starts_with('/') {
            return self.root.join(&path[1..]);
        }
        self.root.join(path)
    }
}

#[async_trait]
impl AsyncFileSystem for AsyncAltrootFS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Stream<Item = String> + Send + Unpin>> {
        self.path(path)?
            .read_dir()
            .await
            .map(|result| result.map(|path| path.filename()))
            .map(|entries| Box::new(entries) as Box<dyn Stream<Item = String> + Send + Unpin>)
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.create_dir().await
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        self.path(path)?.open_file().await
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.path(path)?.create_file().await
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.path(path)?.append_file().await
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.path(path)?.metadata().await
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        match self.path(path) {
            Ok(p) => p.exists().await,
            Err(_) => Ok(false),
        }
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.remove_file().await
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.path(path)?.remove_dir().await
    }

    async fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        if dest.is_empty() {
            return Err(VfsErrorKind::NotSupported.into());
        }
        self.path(src)?.copy_file(&self.path(dest)?).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_vfs::AsyncMemoryFS;

    test_async_vfs!(futures::executor::block_on(async {
        let memory_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        let altroot_path = memory_root.join("altroot").unwrap();
        altroot_path.create_dir().await.unwrap();
        AsyncAltrootFS::new(altroot_path)
    }));

    #[tokio::test]
    async fn parent() {
        let memory_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        let altroot_path = memory_root.join("altroot").unwrap();
        altroot_path.create_dir().await.unwrap();
        let altroot: AsyncVfsPath = AsyncAltrootFS::new(altroot_path.clone()).into();
        assert_eq!(altroot.parent(), altroot.root());
        assert_eq!(altroot_path.parent(), memory_root);
    }
}

#[cfg(test)]
mod tests_physical {
    use super::*;
    use crate::async_vfs::AsyncPhysicalFS;

    use async_std::io::ReadExt;

    test_async_vfs!(futures::executor::block_on(async {
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let physical_root: AsyncVfsPath = AsyncPhysicalFS::new(dir).into();
        let altroot_path = physical_root.join("altroot").unwrap();
        altroot_path.create_dir().await.unwrap();
        AsyncAltrootFS::new(altroot_path)
    }));

    test_async_vfs_readonly!({
        let physical_root: AsyncVfsPath = AsyncPhysicalFS::new("test").into();
        let altroot_path = physical_root.join("test_directory").unwrap();
        AsyncAltrootFS::new(altroot_path)
    });
}
