//! An ephemeral in-memory file system, intended mainly for unit tests
use crate::async_vfs::{AsyncFileSystem, SeekAndRead};
use crate::error::VfsErrorKind;
use crate::path::VfsFileType;
use crate::{VfsMetadata, VfsResult};

use async_std::io::{prelude::SeekExt, Cursor, Read, Seek, SeekFrom, Write};
use async_std::sync::{Arc, RwLock};
use async_trait::async_trait;
use futures::task::{Context, Poll};
use futures::{Stream, StreamExt};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::mem::swap;
use std::pin::Pin;

type AsyncMemoryFsHandle = Arc<RwLock<AsyncMemoryFsImpl>>;

/// An ephemeral in-memory file system, intended mainly for unit tests
pub struct AsyncMemoryFS {
    handle: AsyncMemoryFsHandle,
}

impl Debug for AsyncMemoryFS {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("In Memory File System")
    }
}

impl AsyncMemoryFS {
    /// Create a new in-memory filesystem
    pub fn new() -> Self {
        AsyncMemoryFS {
            handle: Arc::new(RwLock::new(AsyncMemoryFsImpl::new())),
        }
    }

    async fn ensure_has_parent(&self, path: &str) -> VfsResult<()> {
        let separator = path.rfind('/');
        if let Some(index) = separator {
            if self.exists(&path[..index]).await? {
                return Ok(());
            }
        }
        Err(VfsErrorKind::Other("Parent path does not exist".into()).into())
    }
}

impl Default for AsyncMemoryFS {
    fn default() -> Self {
        Self::new()
    }
}

struct AsyncWritableFile {
    content: Cursor<Vec<u8>>,
    destination: String,
    fs: AsyncMemoryFsHandle,
}

impl Write for AsyncWritableFile {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, async_std::io::Error>> {
        let this = self.get_mut();
        let file = Pin::new(&mut this.content);
        file.poll_write(cx, buf)
    }
    // Flush any bytes left in the write buffer to the virtual file
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), async_std::io::Error>> {
        let this = self.get_mut();
        let file = Pin::new(&mut this.content);
        file.poll_flush(cx)
    }
    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), async_std::io::Error>> {
        let this = self.get_mut();
        let file = Pin::new(&mut this.content);
        file.poll_close(cx)
    }
}

impl Drop for AsyncWritableFile {
    fn drop(&mut self) {
        let mut content = vec![];
        swap(&mut content, self.content.get_mut());
        futures::executor::block_on(self.fs.write()).files.insert(
            self.destination.clone(),
            AsyncMemoryFile {
                file_type: VfsFileType::File,
                content: Arc::new(content),
            },
        );
    }
}

struct AsyncReadableFile {
    #[allow(clippy::rc_buffer)] // to allow accessing the same object as writable
    content: Arc<Vec<u8>>,
    // Position of the read cursor in the "file"
    cursor_pos: u64,
}

impl AsyncReadableFile {
    fn len(&self) -> u64 {
        self.content.len() as u64
    }
}

impl Read for AsyncReadableFile {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, async_std::io::Error>> {
        let this = self.get_mut();
        let bytes_left = this.len() - this.cursor_pos;
        let bytes_read = std::cmp::min(buf.len() as u64, bytes_left);
        if bytes_left == 0 {
            return Poll::Ready(Ok(0));
        }
        buf[..bytes_read as usize].copy_from_slice(
            &this.content[this.cursor_pos as usize..(this.cursor_pos + bytes_read) as usize],
        );
        this.cursor_pos += bytes_read;
        Poll::Ready(Ok(bytes_read as usize))
    }
}

impl Seek for AsyncReadableFile {
    fn poll_seek(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<Result<u64, async_std::io::Error>> {
        let this = self.get_mut();
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => this.cursor_pos as i64 - offset,
            SeekFrom::Current(offset) => this.cursor_pos as i64 + offset,
        };
        if new_pos < 0 || new_pos >= this.len() as i64 {
            Poll::Ready(Err(async_std::io::Error::new(
                async_std::io::ErrorKind::InvalidData,
                "Requested offset is outside the file!",
            )))
        } else {
            this.cursor_pos = new_pos as u64;
            Poll::Ready(Ok(new_pos as u64))
        }
    }
}

#[async_trait]
impl AsyncFileSystem for AsyncMemoryFS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>> {
        let prefix = format!("{}/", path);
        let handle = self.handle.read().await;
        let mut found_directory = false;
        #[allow(clippy::needless_collect)] // need collect to satisfy lifetime requirements
        let entries: Vec<String> = handle
            .files
            .iter()
            .filter_map(|(candidate_path, _)| {
                if candidate_path == path {
                    found_directory = true;
                }
                if candidate_path.starts_with(&prefix) {
                    let rest = &candidate_path[prefix.len()..];
                    if !rest.contains('/') {
                        return Some(rest.to_string());
                    }
                }
                None
            })
            .collect();
        if !found_directory {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        Ok(Box::new(futures::stream::iter(entries)))
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.ensure_has_parent(path).await?;
        let map = &mut self.handle.write().await.files;
        let entry = map.entry(path.to_string());
        match entry {
            Entry::Occupied(file) => {
                return match file.get().file_type {
                    VfsFileType::File => Err(VfsErrorKind::FileExists.into()),
                    VfsFileType::Directory => Err(VfsErrorKind::DirectoryExists.into()),
                }
            }
            Entry::Vacant(_) => {
                map.insert(
                    path.to_string(),
                    AsyncMemoryFile {
                        file_type: VfsFileType::Directory,
                        content: Default::default(),
                    },
                );
            }
        }
        Ok(())
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        let handle = self.handle.read().await;
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        ensure_file(file)?;
        Ok(Box::new(AsyncReadableFile {
            content: file.content.clone(),
            cursor_pos: 0,
        }))
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.ensure_has_parent(path).await?;
        let content = Arc::new(Vec::<u8>::new());
        self.handle.write().await.files.insert(
            path.to_string(),
            AsyncMemoryFile {
                file_type: VfsFileType::File,
                content,
            },
        );
        let writer = AsyncWritableFile {
            content: Cursor::new(vec![]),
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        let handle = self.handle.write().await;
        let file = handle.files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        let mut content = Cursor::new(file.content.as_ref().clone());
        content.seek(SeekFrom::End(0)).await?;
        let writer = AsyncWritableFile {
            content,
            destination: path.to_string(),
            fs: self.handle.clone(),
        };
        Ok(Box::new(writer))
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let guard = self.handle.read().await;
        let files = &guard.files;
        let file = files.get(path).ok_or(VfsErrorKind::FileNotFound)?;
        Ok(VfsMetadata {
            file_type: file.file_type,
            len: file.content.len() as u64,
        })
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        Ok(self.handle.read().await.files.contains_key(path))
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        let mut handle = self.handle.write().await;
        handle
            .files
            .remove(path)
            .ok_or(VfsErrorKind::FileNotFound)?;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        if self.read_dir(path).await?.next().await.is_some() {
            return Err(VfsErrorKind::Other("Directory to remove is not empty".into()).into());
        }
        let mut handle = self.handle.write().await;
        handle
            .files
            .remove(path)
            .ok_or(VfsErrorKind::FileNotFound)?;
        Ok(())
    }
}

#[derive(Debug)]
struct AsyncMemoryFsImpl {
    files: HashMap<String, AsyncMemoryFile>,
}

impl AsyncMemoryFsImpl {
    pub fn new() -> Self {
        let mut files = HashMap::new();
        // Add root directory
        files.insert(
            "".to_string(),
            AsyncMemoryFile {
                file_type: VfsFileType::Directory,
                content: Arc::new(vec![]),
            },
        );
        Self { files }
    }
}

#[derive(Debug)]
struct AsyncMemoryFile {
    file_type: VfsFileType,
    #[allow(clippy::rc_buffer)] // to allow accessing the same object as writable
    content: Arc<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_vfs::AsyncVfsPath;
    use async_std::io::{ReadExt, WriteExt};
    test_async_vfs!(AsyncMemoryFS::new());

    #[tokio::test]
    async fn write_and_read_file() -> VfsResult<()> {
        let root = AsyncVfsPath::new(AsyncMemoryFS::new());
        let path = root.join("foobar.txt").unwrap();
        let _send = &path as &dyn Send;
        {
            let mut file = path.create_file().await.unwrap();
            write!(file, "Hello world").await.unwrap();
            write!(file, "!").await.unwrap();
        }
        {
            let mut file = path.open_file().await.unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).await.unwrap();
            assert_eq!(string, "Hello world!");
        }
        assert!(path.exists().await?);
        assert!(!root.join("foo").unwrap().exists().await?);
        let metadata = path.metadata().await.unwrap();
        assert_eq!(metadata.len, 12);
        assert_eq!(metadata.file_type, VfsFileType::File);
        Ok(())
    }

    #[tokio::test]
    async fn append_file() {
        let root = AsyncVfsPath::new(AsyncMemoryFS::new());
        let _string = String::new();
        let path = root.join("test_append.txt").unwrap();
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
        {
            let mut file = path.open_file().await.unwrap();
            let mut string: String = String::new();
            file.read_to_string(&mut string).await.unwrap();
            assert_eq!(string, "Testing 1Testing 2");
        }
    }

    #[tokio::test]
    async fn create_dir() {
        let root = AsyncVfsPath::new(AsyncMemoryFS::new());
        let _string = String::new();
        let path = root.join("foo").unwrap();
        path.create_dir().await.unwrap();
        let metadata = path.metadata().await.unwrap();
        assert_eq!(metadata.file_type, VfsFileType::Directory);
    }

    #[tokio::test]
    async fn remove_dir_error_message() {
        let root = AsyncVfsPath::new(AsyncMemoryFS::new());
        let path = root.join("foo").unwrap();
        let result = path.remove_dir().await;
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "Could not remove directory for '/foo': The file or directory could not be found"
        );
    }

    #[tokio::test]
    async fn read_dir_error_message() {
        let root = AsyncVfsPath::new(AsyncMemoryFS::new());
        let path = root.join("foo").unwrap();
        let result = path.read_dir().await;
        match result {
            Ok(_) => panic!("Error expected"),
            Err(err) => {
                assert_eq!(
                    format!("{}", err),
                    "Could not read directory for '/foo': The file or directory could not be found"
                );
            }
        }
    }

    #[tokio::test]
    async fn copy_file_across_filesystems() -> VfsResult<()> {
        let root_a = AsyncVfsPath::new(AsyncMemoryFS::new());
        let root_b = AsyncVfsPath::new(AsyncMemoryFS::new());
        let src = root_a.join("a.txt")?;
        let dest = root_b.join("b.txt")?;
        src.create_file().await?.write_all(b"Hello World").await?;
        src.copy_file(&dest).await?;
        assert_eq!(&dest.read_to_string().await?, "Hello World");
        Ok(())
    }
}

fn ensure_file(file: &AsyncMemoryFile) -> VfsResult<()> {
    if file.file_type != VfsFileType::File {
        return Err(VfsErrorKind::Other("Not a file".into()).into());
    }
    Ok(())
}
