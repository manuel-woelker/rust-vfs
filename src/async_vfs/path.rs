//! Virtual filesystem path
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)

use crate::async_vfs::AsyncFileSystem;
use crate::error::{VfsError, VfsErrorKind};
use crate::path::PathLike;
use crate::path::VfsFileType;
use crate::{VfsMetadata, VfsResult};

use async_recursion::async_recursion;
use async_std::io::{Read, ReadExt, Seek, Write};
use async_std::sync::Arc;
use async_std::task::{Context, Poll};
use futures::{future::BoxFuture, FutureExt, Stream, StreamExt};
use std::pin::Pin;

/// Trait combining Seek and Read, return value for opening files
pub trait SeekAndRead: Seek + Read {}

impl<T> SeekAndRead for T where T: Seek + Read {}

#[derive(Debug)]
struct AsyncVFS {
    fs: Box<dyn AsyncFileSystem>,
}

/// A virtual filesystem path, identifying a single file or directory in this virtual filesystem
#[derive(Clone, Debug)]
pub struct AsyncVfsPath {
    path: String,
    fs: Arc<AsyncVFS>,
}

impl PathLike for AsyncVfsPath {
    fn get_path(&self) -> String {
        self.path.clone()
    }
}

impl PartialEq for AsyncVfsPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && Arc::ptr_eq(&self.fs, &other.fs)
    }
}

impl Eq for AsyncVfsPath {}

impl AsyncVfsPath {
    /// Creates a root path for the given filesystem
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncPhysicalFS, AsyncVfsPath};
    /// let path = AsyncVfsPath::new(AsyncPhysicalFS::new("."));
    /// ````
    pub fn new<T: AsyncFileSystem>(filesystem: T) -> Self {
        AsyncVfsPath {
            path: "".to_string(),
            fs: Arc::new(AsyncVFS {
                fs: Box::new(filesystem),
            }),
        }
    }

    /// Returns the string representation of this path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncPhysicalFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// let path = AsyncVfsPath::new(AsyncPhysicalFS::new("."));
    ///
    /// assert_eq!(path.as_str(), "");
    /// assert_eq!(path.join("foo.txt")?.as_str(), "/foo.txt");
    /// # Ok::<(), VfsError>(())
    /// ````
    pub fn as_str(&self) -> &str {
        &self.path
    }

    /// Appends a path segment to this path, returning the result
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncPhysicalFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// let path = AsyncVfsPath::new(AsyncPhysicalFS::new("."));
    ///
    /// assert_eq!(path.join("foo.txt")?.as_str(), "/foo.txt");
    /// assert_eq!(path.join("foo/bar.txt")?.as_str(), "/foo/bar.txt");
    ///
    /// let foo = path.join("foo")?;
    ///
    /// assert_eq!(path.join("foo/bar.txt")?, foo.join("bar.txt")?);
    /// assert_eq!(path, foo.join("..")?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn join(&self, path: impl AsRef<str>) -> VfsResult<Self> {
        let new_path = self.join_internal(&self.path, path.as_ref())?;
        Ok(Self {
            path: new_path,
            fs: self.fs.clone(),
        })
    }

    /// Returns the root path of this filesystem
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType};
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo/bar")?;
    ///
    /// assert_eq!(directory.root(), path);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn root(&self) -> AsyncVfsPath {
        AsyncVfsPath {
            path: "".to_string(),
            fs: self.fs.clone(),
        }
    }

    /// Returns true if this is the root path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType};
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// assert!(path.is_root());
    /// let path = path.join("foo/bar")?;
    /// assert!(! path.is_root());
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn is_root(&self) -> bool {
        self.path.is_empty()
    }

    /// Creates the directory at this path
    ///
    /// Note that the parent directory must exist, while the given path must not exist.
    ///
    /// Returns VfsErrorKind::FileExists if a file already exists at the given path
    /// Returns VfsErrorKind::DirectoryExists if a directory already exists at the given path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    ///
    /// directory.create_dir().await?;
    ///
    /// assert!(directory.exists().await?);
    /// assert_eq!(directory.metadata().await?.file_type, VfsFileType::Directory);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn create_dir(&self) -> VfsResult<()> {
        self.get_parent("create directory").await?;
        self.fs.fs.create_dir(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not create directory")
        })
    }

    /// Creates the directory at this path, also creating parent directories as necessary
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo/bar")?;
    ///
    /// directory.create_dir_all().await?;
    ///
    /// assert!(directory.exists().await?);
    /// assert_eq!(directory.metadata().await?.file_type, VfsFileType::Directory);
    /// let parent = path.join("foo")?;
    /// assert!(parent.exists().await?);
    /// assert_eq!(parent.metadata().await?.file_type, VfsFileType::Directory);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn create_dir_all(&self) -> VfsResult<()> {
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
            if let Err(error) = self.fs.fs.create_dir(directory).await {
                match error.kind() {
                    VfsErrorKind::DirectoryExists => {}
                    _ => {
                        return Err(error.with_path(directory).with_context(|| {
                            format!("Could not create directories at '{}'", path)
                        }))
                    }
                }
            }
            if end == path.len() {
                break;
            }
            pos = end + 1;
        }
        Ok(())
    }

    /// Iterates over all entries of this directory path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use futures::stream::Collect;
    /// use futures::stream::StreamExt;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// path.join("foo")?.create_dir().await?;
    /// path.join("bar")?.create_dir().await?;
    ///
    /// let mut directories: Vec<_> = path.read_dir().await?.collect().await;
    ///
    /// directories.sort_by_key(|path| path.as_str().to_string());
    /// assert_eq!(directories, vec![path.join("bar")?, path.join("foo")?]);
    /// # Ok::<(), VfsError>(())
    ///  # });
    /// ```
    pub async fn read_dir(&self) -> VfsResult<Box<dyn Unpin + Stream<Item = AsyncVfsPath> + Send>> {
        let parent = self.path.clone();
        let fs = self.fs.clone();
        Ok(Box::new(
            self.fs
                .fs
                .read_dir(&self.path)
                .await
                .map_err(|err| {
                    err.with_path(&self.path)
                        .with_context(|| "Could not read directory")
                })?
                .map(move |path| {
                    println!("{:?}", path);
                    AsyncVfsPath {
                        path: format!("{}/{}", parent, path),
                        fs: fs.clone(),
                    }
                }),
        ))
    }

    /// Creates a file at this path for writing, overwriting any existing file
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use async_std::io:: {ReadExt, WriteExt};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo.txt")?;
    ///
    /// write!(file.create_file().await?, "Hello, world!").await?;
    ///
    /// let mut result = String::new();
    /// file.open_file().await?.read_to_string(&mut result).await?;
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn create_file(&self) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.get_parent("create file").await?;
        self.fs.fs.create_file(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not create file")
        })
    }

    /// Opens the file at this path for reading
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use async_std::io:: {ReadExt, WriteExt};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file().await?, "Hello, world!").await?;
    /// let mut result = String::new();
    ///
    /// file.open_file().await?.read_to_string(&mut result).await?;
    ///
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn open_file(&self) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        self.fs.fs.open_file(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not open file")
        })
    }

    /// Checks whether parent is a directory
    async fn get_parent(&self, action: &str) -> VfsResult<()> {
        let parent = self.parent();
        if !parent.exists().await? {
            return Err(VfsError::from(VfsErrorKind::Other(format!(
                "Could not {}, parent directory does not exist",
                action
            )))
            .with_path(&self.path));
        }
        let metadata = parent.metadata().await?;
        if metadata.file_type != VfsFileType::Directory {
            return Err(VfsError::from(VfsErrorKind::Other(format!(
                "Could not {}, parent path is not a directory",
                action
            )))
            .with_path(&self.path));
        }
        Ok(())
    }

    /// Opens the file at this path for appending
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use async_std::io:: {ReadExt, WriteExt};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file().await?, "Hello, ").await?;
    /// write!(file.append_file().await?, "world!").await?;
    /// let mut result = String::new();
    /// file.open_file().await?.read_to_string(&mut result).await?;
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn append_file(&self) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.fs.fs.append_file(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not open file for appending")
        })
    }

    /// Removes the file at this path
    ///
    /// ```
    /// use async_std::io:: {ReadExt, WriteExt};
    /// # use vfs::async_vfs::{AsyncMemoryFS , AsyncVfsPath};
    /// # use vfs::VfsError;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file().await?, "Hello, ").await?;
    /// assert!(file.exists().await?);
    ///
    /// file.remove_file().await?;
    ///
    /// assert!(!file.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn remove_file(&self) -> VfsResult<()> {
        self.fs.fs.remove_file(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not remove file")
        })
    }

    /// Removes the directory at this path
    ///
    /// The directory must be empty.
    ///
    /// ```
    /// # tokio_test::block_on(async {
    /// use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// use vfs::VfsError;
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir().await;
    /// assert!(directory.exists().await?);
    ///
    /// directory.remove_dir().await?;
    ///
    /// assert!(!directory.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn remove_dir(&self) -> VfsResult<()> {
        self.fs.fs.remove_dir(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not remove directory")
        })
    }

    /// Ensures that the directory at this path is removed, recursively deleting all contents if necessary
    ///
    /// Returns successfully if directory does not exist
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.join("bar")?.create_dir_all().await?;
    /// assert!(directory.exists().await?);
    ///
    /// directory.remove_dir_all().await?;
    ///
    /// assert!(!directory.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    #[async_recursion]
    pub async fn remove_dir_all(&self) -> VfsResult<()> {
        if !self.exists().await? {
            return Ok(());
        }
        let mut path_stream = self.read_dir().await?;
        while let Some(child) = path_stream.next().await {
            let metadata = child.metadata().await?;
            match metadata.file_type {
                VfsFileType::File => child.remove_file().await?,
                VfsFileType::Directory => child.remove_dir_all().await?,
            }
        }
        self.remove_dir().await?;
        Ok(())
    }

    /// Returns the file metadata for the file at this path
    ///
    /// ```
    /// use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// use async_std::io::WriteExt;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir().await?;
    ///
    /// assert_eq!(directory.metadata().await?.len, 0);
    /// assert_eq!(directory.metadata().await?.file_type, VfsFileType::Directory);
    ///
    /// let file = path.join("bar.txt")?;
    /// write!(file.create_file().await?, "Hello, world!").await?;
    ///
    /// assert_eq!(file.metadata().await?.len, 13);
    /// assert_eq!(file.metadata().await?.file_type, VfsFileType::File);
    /// # Ok::<(), VfsError>(())
    /// # });
    pub async fn metadata(&self) -> VfsResult<VfsMetadata> {
        self.fs.fs.metadata(&self.path).await.map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not get metadata")
        })
    }

    /// Returns `true` if the path exists and is pointing at a regular file, otherwise returns `false`.
    ///
    /// Note that this call may fail if the file's existence cannot be determined or the metadata can not be retrieved
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir().await?;
    /// let file = path.join("foo.txt")?;
    /// file.create_file().await?;
    ///
    /// assert!(!directory.is_file().await?);
    /// assert!(file.is_file().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    pub async fn is_file(&self) -> VfsResult<bool> {
        if !self.exists().await? {
            return Ok(false);
        }
        let metadata = self.metadata().await?;
        Ok(metadata.file_type == VfsFileType::File)
    }

    /// Returns `true` if the path exists and is pointing at a directory, otherwise returns `false`.
    ///
    /// Note that this call may fail if the directory's existence cannot be determined or the metadata can not be retrieved
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir().await?;
    /// let file = path.join("foo.txt")?;
    /// file.create_file().await?;
    ///
    /// assert!(directory.is_dir().await?);
    /// assert!(!file.is_dir().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    pub async fn is_dir(&self) -> VfsResult<bool> {
        if !self.exists().await? {
            return Ok(false);
        }
        let metadata = self.metadata().await?;
        Ok(metadata.file_type == VfsFileType::Directory)
    }

    /// Returns true if a file or directory exists at this path, false otherwise
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let directory = path.join("foo")?;
    ///
    /// assert!(!directory.exists().await?);
    ///
    /// directory.create_dir().await?;
    ///
    /// assert!(directory.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    pub async fn exists(&self) -> VfsResult<bool> {
        self.fs.fs.exists(&self.path).await
    }

    /// Returns the filename portion of this path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo/bar.txt")?;
    ///
    /// assert_eq!(&file.filename(), "bar.txt");
    ///
    /// # Ok::<(), VfsError>(())
    pub fn filename(&self) -> String {
        self.filename_internal()
    }

    /// Returns the extension portion of this path
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    ///
    /// assert_eq!(path.join("foo/bar.txt")?.extension(), Some("txt".to_string()));
    /// assert_eq!(path.join("foo/bar.txt.zip")?.extension(), Some("zip".to_string()));
    /// assert_eq!(path.join("foo/bar")?.extension(), None);
    ///
    /// # Ok::<(), VfsError>(())
    pub fn extension(&self) -> Option<String> {
        self.extension_internal()
    }

    /// Returns the parent path of this portion of this path
    ///
    /// Root will return itself.
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsFileType, VfsMetadata};
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    ///
    /// assert_eq!(path.parent(), path.root());
    /// assert_eq!(path.join("foo/bar")?.parent(), path.join("foo")?);
    /// assert_eq!(path.join("foo")?.parent(), path);
    ///
    /// # Ok::<(), VfsError>(())
    pub fn parent(&self) -> Self {
        let parent_path = self.parent_internal(&self.path);
        Self {
            path: parent_path,
            fs: self.fs.clone(),
        }
    }

    /// Recursively iterates over all the directories and files at this path
    ///
    /// Directories are visited before their children
    ///
    /// Note that the iterator items can contain errors, usually when directories are removed during the iteration.
    /// The returned paths may also point to non-existant files if there is concurrent removal.
    ///
    /// Also note that loops in the file system hierarchy may cause this iterator to never terminate.
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::{VfsError, VfsResult};
    /// use futures::stream::StreamExt;
    /// # tokio_test::block_on(async {
    /// let root = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// root.join("foo/bar")?.create_dir_all().await?;
    /// root.join("fizz/buzz")?.create_dir_all().await?;
    /// root.join("foo/bar/baz")?.create_file().await?;
    ///
    /// let mut directories = root.walk_dir().await?.map(|res| res.unwrap()).collect::<Vec<_>>().await;
    ///
    /// directories.sort_by_key(|path| path.as_str().to_string());
    /// let expected = vec!["fizz", "fizz/buzz", "foo", "foo/bar", "foo/bar/baz"].iter().map(|path| root.join(path)).collect::<VfsResult<Vec<_>>>()?;
    /// assert_eq!(directories, expected);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn walk_dir(&self) -> VfsResult<WalkDirIterator> {
        Ok(WalkDirIterator {
            inner: self.read_dir().await?,
            todo: vec![],
            prev_result: None,
            metadata_fut: None,
            read_dir_fut: None,
        })
    }

    /// Reads a complete file to a string
    ///
    /// Returns an error if the file does not exist or is not valid UTF-8
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use async_std::io::{ReadExt, WriteExt};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file().await?, "Hello, world!").await?;
    ///
    /// let result = file.read_to_string().await?;
    ///
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn read_to_string(&self) -> VfsResult<String> {
        let metadata = self.metadata().await?;
        if metadata.file_type != VfsFileType::File {
            return Err(
                VfsError::from(VfsErrorKind::Other("Path is a directory".into()))
                    .with_path(&self.path)
                    .with_context(|| "Could not read path"),
            );
        }
        let mut result = String::with_capacity(metadata.len as usize);
        self.open_file()
            .await?
            .read_to_string(&mut result)
            .await
            .map_err(|source| {
                VfsError::from(source)
                    .with_path(&self.path)
                    .with_context(|| "Could not read path")
            })?;
        Ok(result)
    }

    /// Copies a file to a new destination
    ///
    /// The destination must not exist, but its parent directory must
    ///
    /// ```
    /// use async_std::io::{ReadExt, WriteExt};
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let src = path.join("foo.txt")?;
    /// write!(src.create_file().await?, "Hello, world!").await?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.copy_file(&dest).await?;
    ///
    /// assert_eq!(dest.read_to_string().await?, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn copy_file(&self, destination: &AsyncVfsPath) -> VfsResult<()> {
        async {
            if destination.exists().await? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&self.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.copy_file(&self.path, &destination.path).await;
                match result {
                    Err(err) => match err.kind() {
                        VfsErrorKind::NotSupported => {
                            // continue
                        }
                        _ => return Err(err),
                    },
                    other => return other,
                }
            }
            let mut src = self.open_file().await?;
            let mut dest = destination.create_file().await?;
            async_std::io::copy(&mut src, &mut dest)
                .await
                .map_err(|source| {
                    VfsError::from(source)
                        .with_path(&self.path)
                        .with_context(|| "Could not read path")
                })?;
            Ok(())
        }
        .await
        .map_err(|err| {
            err.with_path(&self.path).with_context(|| {
                format!(
                    "Could not copy '{}' to '{}'",
                    self.as_str(),
                    destination.as_str()
                )
            })
        })?;
        Ok(())
    }

    /// Moves or renames a file to a new destination
    ///
    /// The destination must not exist, but its parent directory must
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// use async_std::io::{ReadExt, WriteExt};
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let src = path.join("foo.txt")?;
    /// write!(src.create_file().await?, "Hello, world!").await?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.move_file(&dest).await?;
    ///
    /// assert_eq!(dest.read_to_string().await?, "Hello, world!");
    /// assert!(!src.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn move_file(&self, destination: &AsyncVfsPath) -> VfsResult<()> {
        async {
            if destination.exists().await? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_file(&self.path, &destination.path);
                match result.await {
                    Err(err) => match err.kind() {
                        VfsErrorKind::NotSupported => {
                            // continue
                        }
                        _ => return Err(err),
                    },
                    other => return other,
                }
            }
            let mut src = self.open_file().await?;
            let mut dest = destination.create_file().await?;
            async_std::io::copy(&mut src, &mut dest)
                .await
                .map_err(|source| {
                    VfsError::from(source)
                        .with_path(&self.path)
                        .with_context(|| "Could not read path")
                })?;
            self.remove_file().await?;
            Ok(())
        }
        .await
        .map_err(|err| {
            err.with_path(&self.path).with_context(|| {
                format!(
                    "Could not move '{}' to '{}'",
                    self.as_str(),
                    destination.as_str()
                )
            })
        })?;
        Ok(())
    }

    /// Copies a directory to a new destination, recursively
    ///
    /// The destination must not exist, but the parent directory must
    ///
    /// Returns the number of files copied
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let src = path.join("foo")?;
    /// src.join("dir")?.create_dir_all().await?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.copy_dir(&dest).await?;
    ///
    /// assert!(dest.join("dir")?.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn copy_dir(&self, destination: &AsyncVfsPath) -> VfsResult<u64> {
        let files_copied = async {
            let mut files_copied = 0u64;
            if destination.exists().await? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            destination.create_dir().await?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            let mut path_stream = self.walk_dir().await?;
            while let Some(file) = path_stream.next().await {
                let src_path: AsyncVfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata().await?.file_type {
                    VfsFileType::Directory => dest_path.create_dir().await?,
                    VfsFileType::File => src_path.copy_file(&dest_path).await?,
                }
                files_copied += 1;
            }
            Ok(files_copied)
        }
        .await
        .map_err(|err| {
            err.with_path(&self.path).with_context(|| {
                format!(
                    "Could not copy directory '{}' to '{}'",
                    self.as_str(),
                    destination.as_str()
                )
            })
        })?;
        Ok(files_copied)
    }

    /// Moves a directory to a new destination, including subdirectories and files
    ///
    /// The destination must not exist, but its parent directory must
    ///
    /// ```
    /// # use vfs::async_vfs::{AsyncMemoryFS, AsyncVfsPath};
    /// # use vfs::VfsError;
    /// # tokio_test::block_on(async {
    /// let path = AsyncVfsPath::new(AsyncMemoryFS::new());
    /// let src = path.join("foo")?;
    /// src.join("dir")?.create_dir_all().await?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.move_dir(&dest).await?;
    ///
    /// assert!(dest.join("dir")?.exists().await?);
    /// assert!(!src.join("dir")?.exists().await?);
    /// # Ok::<(), VfsError>(())
    /// # });
    /// ```
    pub async fn move_dir(&self, destination: &AsyncVfsPath) -> VfsResult<()> {
        async {
            if destination.exists().await? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_dir(&self.path, &destination.path).await;
                match result {
                    Err(err) => match err.kind() {
                        VfsErrorKind::NotSupported => {
                            // continue
                        }
                        _ => return Err(err),
                    },
                    other => return other,
                }
            }
            destination.create_dir().await?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            let mut path_stream = self.walk_dir().await?;
            while let Some(file) = path_stream.next().await {
                let src_path: AsyncVfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata().await?.file_type {
                    VfsFileType::Directory => dest_path.create_dir().await?,
                    VfsFileType::File => src_path.copy_file(&dest_path).await?,
                }
            }
            self.remove_dir_all().await?;
            Ok(())
        }
        .await
        .map_err(|err| {
            err.with_path(&self.path).with_context(|| {
                format!(
                    "Could not move directory '{}' to '{}'",
                    self.as_str(),
                    destination.as_str()
                )
            })
        })?;
        Ok(())
    }
}

/// An iterator for recursively walking a file hierarchy
pub struct WalkDirIterator {
    /// the path iterator of the current directory
    inner: Box<dyn Stream<Item = AsyncVfsPath> + Send + Unpin>,
    /// stack of subdirectories still to walk
    todo: Vec<AsyncVfsPath>,
    /// used to store the previous yield of the todo stream,
    /// which would otherwise get dropped if path.metadata() is pending
    prev_result: Option<AsyncVfsPath>,
    // Used to store futures when poll_next returns pending
    // this ensures a new future is not spawned on each poll.
    read_dir_fut: Option<
        BoxFuture<'static, Result<Box<(dyn Stream<Item = AsyncVfsPath> + Send + Unpin)>, VfsError>>,
    >,
    metadata_fut: Option<BoxFuture<'static, Result<VfsMetadata, VfsError>>>,
}

impl std::fmt::Debug for WalkDirIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalkDirIterator")?;
        self.todo.fmt(f)
    }
}

impl Stream for WalkDirIterator {
    type Item = VfsResult<AsyncVfsPath>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // Check if we have a previously stored result from last call
        // that we could not utilize due to pending path.metadata() call
        let result = if this.prev_result.is_none() {
            loop {
                match this.inner.poll_next_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Some(path)) => break Ok(path),
                    Poll::Ready(None) => {
                        let directory = if this.todo.is_empty() {
                            return Poll::Ready(None);
                        } else {
                            this.todo[this.todo.len() - 1].clone()
                        };
                        let mut read_dir_fut = if this.read_dir_fut.is_some() {
                            this.read_dir_fut.take().unwrap()
                        } else {
                            Box::pin(async move { directory.read_dir().await })
                        };
                        match read_dir_fut.poll_unpin(cx) {
                            Poll::Pending => {
                                this.read_dir_fut = Some(read_dir_fut);
                                return Poll::Pending;
                            }
                            Poll::Ready(Err(err)) => {
                                let _ = this.todo.pop();
                                break Err(err);
                            }
                            Poll::Ready(Ok(iterator)) => {
                                let _ = this.todo.pop();
                                this.inner = iterator;
                            }
                        }
                    }
                }
            }
        } else {
            Ok(this.prev_result.take().unwrap())
        };
        if let Ok(path) = &result {
            let mut metadata_fut = if this.metadata_fut.is_some() {
                this.metadata_fut.take().unwrap()
            } else {
                let path_clone = path.clone();
                Box::pin(async move { path_clone.metadata().await })
            };
            match metadata_fut.poll_unpin(cx) {
                Poll::Pending => {
                    this.prev_result = Some(path.clone());
                    this.metadata_fut = Some(metadata_fut);
                    return Poll::Pending;
                }
                Poll::Ready(Ok(meta)) => {
                    if meta.file_type == VfsFileType::Directory {
                        this.todo.push(path.clone());
                    }
                }
                Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
            }
        }
        Poll::Ready(Some(result))
    }
}
