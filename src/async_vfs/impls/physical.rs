//! An async implementation of a "physical" file system implementation using the underlying OS file system
use crate::async_vfs::{AsyncFileSystem, SeekAndRead};
use crate::error::VfsErrorKind;
use crate::path::VfsFileType;
use crate::{VfsError, VfsMetadata, VfsResult};

use async_std::fs::{File, OpenOptions};
use async_std::io::{ErrorKind, Write};
use async_std::path::{Path, PathBuf};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;

/// A physical filesystem implementation using the underlying OS file system
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

#[async_trait]
impl AsyncFileSystem for AsyncPhysicalFS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>> {
        let entries = Box::new(
            self.get_path(path)
                .read_dir()
                .await?
                .map(|entry| entry.unwrap().file_name().into_string().unwrap()),
        );
        Ok(entries)
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        let fs_path = self.get_path(path);
        match async_std::fs::create_dir(&fs_path).await {
            Ok(()) => Ok(()),
            Err(e) => match e.kind() {
                ErrorKind::AlreadyExists => {
                    let metadata = async_std::fs::metadata(&fs_path).await.unwrap();
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
        Ok(Box::new(File::open(self.get_path(path)).await?))
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        Ok(Box::new(File::create(self.get_path(path)).await?))
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        Ok(Box::new(
            OpenOptions::new()
                .write(true)
                .append(true)
                .open(self.get_path(path))
                .await?,
        ))
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let metadata = self.get_path(path).metadata().await?;
        Ok(if metadata.is_dir() {
            VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
            }
        } else {
            VfsMetadata {
                file_type: VfsFileType::File,
                len: metadata.len(),
            }
        })
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        Ok(self.get_path(path).exists().await)
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        async_std::fs::remove_file(self.get_path(path)).await?;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        async_std::fs::remove_dir(self.get_path(path)).await?;
        Ok(())
    }

    async fn copy_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        async_std::fs::copy(self.get_path(src), self.get_path(dest)).await?;
        Ok(())
    }

    async fn move_file(&self, src: &str, dest: &str) -> VfsResult<()> {
        async_std::fs::rename(self.get_path(src), self.get_path(dest)).await?;

        Ok(())
    }

    async fn move_dir(&self, src: &str, dest: &str) -> VfsResult<()> {
        let result = async_std::fs::rename(self.get_path(src), self.get_path(dest)).await;
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

    use async_std::io::ReadExt;
    use async_std::io::WriteExt;
    use async_std::path::Path;
    use futures::stream::StreamExt;

    test_async_vfs!(futures::executor::block_on(async {
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        async_std::fs::create_dir_all(&dir).await.unwrap();
        AsyncPhysicalFS::new(dir)
    }));
    test_async_vfs_readonly!({ AsyncPhysicalFS::new("test/test_directory") });

    fn create_root() -> AsyncVfsPath {
        AsyncPhysicalFS::new(std::env::current_dir().unwrap()).into()
    }

    #[tokio::test]
    async fn open_file() {
        let expected = async_std::fs::read_to_string("Cargo.toml").await.unwrap();
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
        let root = create_root();
        let _string = String::new();
        let _ = async_std::fs::remove_file("target/test.txt").await;
        root.join("target/test.txt")
            .unwrap()
            .create_file()
            .await
            .unwrap()
            .write_all(b"Testing only")
            .await
            .unwrap();
        let read = std::fs::read_to_string("target/test.txt").unwrap();
        assert_eq!(read, "Testing only");
    }

    #[tokio::test]
    async fn append_file() {
        let root = create_root();
        let _string = String::new();
        let _ = async_std::fs::remove_file("target/test_append.txt").await;
        let path = Box::pin(root.join("target/test_append.txt").unwrap());
        path.create_file()
            .await
            .unwrap()
            .write_all(b"Testing 1")
            .await
            .unwrap();
        path.append_file()
            .await
            .unwrap()
            .write_all(b"Testing 2")
            .await
            .unwrap();
        let read = async_std::fs::read_to_string("target/test_append.txt")
            .await
            .unwrap();
        assert_eq!(read, "Testing 1Testing 2");
    }

    #[tokio::test]
    async fn read_dir() {
        let _expected = async_std::fs::read_to_string("Cargo.toml").await.unwrap();
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
        let _ = async_std::fs::remove_dir("target/fs_test").await;
        let root = create_root();
        root.join("target/fs_test")
            .unwrap()
            .create_dir()
            .await
            .unwrap();
        let path = Path::new("target/fs_test");
        assert!(path.exists().await, "Path was not created");
        assert!(path.is_dir().await, "Path is not a directory");
        async_std::fs::remove_dir("target/fs_test").await.unwrap();
    }

    #[tokio::test]
    async fn file_metadata() {
        let expected = async_std::fs::read_to_string("Cargo.toml").await.unwrap();
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
