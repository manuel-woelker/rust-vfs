//! An async implementation of a "physical" file system using tokio
use crate::async_vfs::{AsyncFileSystem, SeekAndRead};
use crate::error::VfsErrorKind;
use crate::path::VfsFileType;
use crate::{VfsError, VfsMetadata, VfsResult};

use async_trait::async_trait;
use filetime::FileTime;
use futures::io::AsyncWrite;
use futures::stream::Stream;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::SystemTime;
use tokio::runtime::Handle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// A physical filesystem implementation using tokio for async I/O
#[derive(Debug)]
pub struct AsyncPhysicalFS {
    root: Pin<PathBuf>,
}

impl AsyncPhysicalFS {
    /// Create a new physical filesystem rooted in `root`
    pub fn new<T: AsRef<Path>>(root: T) -> Self {
        AsyncPhysicalFS {
            root: Pin::new(root.as_ref().to_path_buf()),
        }
    }

    fn get_path(&self, mut path: &str) -> PathBuf {
        if path.starts_with('/') {
            path = &path[1..];
        }
        self.root.join(path)
    }
}

/// Runs normal blocking io on a tokio thread.
/// Requires a tokio runtime.
async fn blocking_io<F>(f: F) -> Result<(), VfsError>
where
    F: FnOnce() -> std::io::Result<()> + Send + 'static,
{
    if Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?
            .map_err(VfsError::from)
    } else {
        Err(VfsError::from(VfsErrorKind::NotSupported))
    }
}

#[async_trait]
impl AsyncFileSystem for AsyncPhysicalFS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>> {
        let mut read_dir = tokio::fs::read_dir(self.get_path(path)).await?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            entries.push(entry.file_name().into_string().unwrap());
        }
        Ok(Box::new(futures::stream::iter(entries)))
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        let fs_path = self.get_path(path);
        match tokio::fs::create_dir(&fs_path).await {
            Ok(()) => Ok(()),
            Err(e) => match e.kind() {
                ErrorKind::AlreadyExists => {
                    let metadata = tokio::fs::metadata(&fs_path).await.unwrap();
                    if metadata.is_dir() {
                        return Err(VfsError::from(VfsErrorKind::DirectoryExists));
                    }
                    Err(VfsError::from(VfsErrorKind::FileExists))
                }
                _ => Err(e.into()),
            },
        }
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        let file = tokio::fs::File::open(self.get_path(path)).await?;
        Ok(Box::new(file.compat()))
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn AsyncWrite + Send + Unpin>> {
        let file = tokio::fs::File::create(self.get_path(path)).await?;
        Ok(Box::new(file.compat_write()))
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn AsyncWrite + Send + Unpin>> {
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(self.get_path(path))
            .await?;
        Ok(Box::new(file.compat_write()))
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let metadata = tokio::fs::metadata(self.get_path(path)).await?;
        Ok(if metadata.is_dir() {
            VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
                accessed: metadata.accessed().ok(),
            }
        } else {
            VfsMetadata {
                file_type: VfsFileType::File,
                len: metadata.len(),
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
                accessed: metadata.accessed().ok(),
            }
        })
    }

    async fn set_modification_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        let path = self.get_path(path);

        blocking_io(move || filetime::set_file_mtime(path, FileTime::from(time))).await?;

        Ok(())
    }

    async fn set_access_time(&self, path: &str, time: SystemTime) -> VfsResult<()> {
        let path = self.get_path(path);

        blocking_io(move || filetime::set_file_atime(path, FileTime::from(time))).await?;

        Ok(())
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        Ok(tokio::fs::try_exists(self.get_path(path))
            .await
            .unwrap_or(false))
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        tokio::fs::remove_file(self.get_path(path)).await?;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        tokio::fs::remove_dir(self.get_path(path)).await?;
        Ok(())
    }

    async fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        tokio::fs::copy(self.get_path(src), self.get_path(dest)).await?;
        Ok(())
    }

    async fn move_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        tokio::fs::rename(self.get_path(src), self.get_path(dest)).await?;

        Ok(())
    }

    async fn move_dir(&self, src: &str, dest: &str) -> VfsResult<()> {
        let result = tokio::fs::rename(self.get_path(src), self.get_path(dest)).await;
        if result.is_err() {
            // Error possibly due to different filesystems, return not supported and let the fallback handle it
            return Err(VfsErrorKind::NotSupported.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_vfs::AsyncVfsPath;

    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use futures::stream::StreamExt;

    test_async_vfs!({
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        AsyncPhysicalFS::new(dir)
    });
    test_async_vfs_readonly!({ AsyncPhysicalFS::new("test/test_directory") });

    fn create_root() -> AsyncVfsPath {
        AsyncPhysicalFS::new(std::env::current_dir().unwrap()).into()
    }

    #[tokio::test]
    async fn open_file() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let mut string = String::new();
        root.join("Cargo.toml")
            .unwrap()
            .open_file()
            .await
            .unwrap()
            .read_to_string(&mut string)
            .await
            .unwrap();
        assert_eq!(string, expected);
    }

    #[tokio::test]
    async fn create_file() {
        use crate::async_vfs::test_macros::write_file;
        let root = create_root();
        let _ = std::fs::remove_file("target/test.txt");
        write_file(
            root.join("target/test.txt").unwrap().create_file().await.unwrap(),
            b"Testing only",
        )
        .await;
        let read = std::fs::read_to_string("target/test.txt").unwrap();
        assert_eq!(read, "Testing only");
    }

    #[tokio::test]
    async fn append_file() {
        use crate::async_vfs::test_macros::write_file;
        let root = create_root();
        let _ = std::fs::remove_file("target/test_append.txt");
        let path = Box::pin(root.join("target/test_append.txt").unwrap());
        write_file(path.create_file().await.unwrap(), b"Testing 1").await;
        write_file(path.append_file().await.unwrap(), b"Testing 2").await;
        let read = std::fs::read_to_string("target/test_append.txt").unwrap();
        assert_eq!(read, "Testing 1Testing 2");
    }

    #[tokio::test]
    async fn read_dir() {
        let _expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let entries: Vec<_> = root.read_dir().await.unwrap().collect().await;
        let map: Vec<_> = entries
            .iter()
            .map(|path: &AsyncVfsPath| path.as_str())
            .filter(|x| x.ends_with(".toml"))
            .collect();
        assert_eq!(&["/Cargo.toml"], &map[..]);
    }

    #[tokio::test]
    async fn create_dir() {
        let _ = std::fs::remove_dir("target/fs_test");
        let root = create_root();
        root.join("target/fs_test")
            .unwrap()
            .create_dir()
            .await
            .unwrap();
        let path = std::path::Path::new("target/fs_test");
        assert!(path.exists(), "Path was not created");
        assert!(path.is_dir(), "Path is not a directory");
        std::fs::remove_dir("target/fs_test").unwrap();
    }

    #[tokio::test]
    async fn file_metadata() {
        let expected = std::fs::read_to_string("Cargo.toml").unwrap();
        let root = create_root();
        let metadata = root.join("Cargo.toml").unwrap().metadata().await.unwrap();
        assert_eq!(metadata.len, expected.len() as u64);
        assert_eq!(metadata.file_type, VfsFileType::File);
    }

    #[tokio::test]
    async fn dir_metadata() {
        let root = create_root();
        let metadata = root.metadata().await.unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
        let metadata = root.join("src").unwrap().metadata().await.unwrap();
        assert_eq!(metadata.len, 0);
        assert_eq!(metadata.file_type, VfsFileType::Directory);
    }
}
