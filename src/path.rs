//! Virtual filesystem path
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)

use std::io::{Read, Seek, Write};
use std::sync::Arc;

use crate::error::VfsErrorKind;
use crate::{FileSystem, VfsError, VfsResult};

/// Trait combining Seek and Read, return value for opening files
pub trait SeekAndRead: Seek + Read {}

impl<T> SeekAndRead for T where T: Seek + Read {}

/// A trait for common non-async behaviour of both sync and async paths
pub(crate) trait PathLike: Clone {
    fn get_path(&self) -> String;
    fn filename_internal(&self) -> String {
        let path = self.get_path();
        let index = path.rfind('/').map(|x| x + 1).unwrap_or(0);
        path[index..].to_string()
    }

    fn extension_internal(&self) -> Option<String> {
        let filename = self.filename_internal();
        let mut parts = filename.rsplitn(2, '.');
        let after = parts.next();
        let before = parts.next();
        match before {
            None | Some("") => None,
            _ => after.map(|x| x.to_string()),
        }
    }

    fn parent_internal(&self, path: &str) -> String {
        let index = path.rfind('/');
        index
            .map(|idx| path[..idx].to_string())
            .unwrap_or_else(|| "".to_string())
    }

    fn join_internal(&self, in_path: &str, path: &str) -> VfsResult<String> {
        if path.is_empty() {
            return Ok(in_path.to_string());
        }
        let mut new_components: Vec<&str> = vec![];
        let mut base_path = if path.starts_with('/') {
            "".to_string()
        } else {
            in_path.to_string()
        };
        // Prevent paths from ending in slashes unless this is just the root directory.
        if path.len() > 1 && path.ends_with('/') {
            return Err(VfsError::from(VfsErrorKind::InvalidPath).with_path(path));
        }
        for component in path.split('/') {
            if component == "." || component.is_empty() {
                continue;
            }
            if component == ".." {
                if !new_components.is_empty() {
                    new_components.truncate(new_components.len() - 1);
                } else {
                    base_path = self.parent_internal(&base_path);
                }
            } else {
                new_components.push(component);
            }
        }
        let mut path = base_path;
        for component in new_components {
            path += "/";
            path += component
        }
        Ok(path)
    }
}

/// Type of file
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VfsFileType {
    /// A plain file
    File,
    /// A Directory
    Directory,
}

/// File metadata information
#[derive(Debug)]
pub struct VfsMetadata {
    /// The type of file
    pub file_type: VfsFileType,
    /// Length of the file in bytes, 0 for directories
    pub len: u64,
}

#[derive(Debug)]
struct VFS {
    fs: Box<dyn FileSystem>,
}

/// A virtual filesystem path, identifying a single file or directory in this virtual filesystem
#[derive(Clone, Debug)]
pub struct VfsPath {
    path: String,
    fs: Arc<VFS>,
}

impl PathLike for VfsPath {
    fn get_path(&self) -> String {
        self.path.to_string()
    }
}

impl PartialEq for VfsPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && Arc::ptr_eq(&self.fs, &other.fs)
    }
}

impl Eq for VfsPath {}

impl VfsPath {
    /// Creates a root path for the given filesystem
    ///
    /// ```
    /// # use vfs::{PhysicalFS, VfsPath};
    /// let path = VfsPath::new(PhysicalFS::new("."));
    /// ````
    pub fn new<T: FileSystem>(filesystem: T) -> Self {
        VfsPath {
            path: "".to_string(),
            fs: Arc::new(VFS {
                fs: Box::new(filesystem),
            }),
        }
    }

    /// Returns the string representation of this path
    ///
    /// ```
    /// # use vfs::{PhysicalFS, VfsError, VfsPath};
    /// let path = VfsPath::new(PhysicalFS::new("."));
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
    /// # use vfs::{PhysicalFS, VfsError, VfsPath};
    /// let path = VfsPath::new(PhysicalFS::new("."));
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
    /// # use vfs::{MemoryFS, VfsError, VfsFileType, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo/bar")?;
    ///
    /// assert_eq!(directory.root(), path);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn root(&self) -> VfsPath {
        VfsPath {
            path: "".to_string(),
            fs: self.fs.clone(),
        }
    }

    /// Returns true if this is the root path
    ///
    /// ```
    /// # use vfs::{MemoryFS, VfsError, VfsFileType, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
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
    /// # use vfs::{MemoryFS, VfsError, VfsFileType, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    ///
    /// directory.create_dir()?;
    ///
    /// assert!(directory.exists()?);
    /// assert_eq!(directory.metadata()?.file_type, VfsFileType::Directory);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn create_dir(&self) -> VfsResult<()> {
        self.get_parent("create directory")?;
        self.fs.fs.create_dir(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not create directory")
        })
    }

    /// Creates the directory at this path, also creating parent directories as necessary
    ///
    /// ```
    /// # use vfs::{MemoryFS, VfsError, VfsFileType, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo/bar")?;
    ///
    /// directory.create_dir_all()?;
    ///
    /// assert!(directory.exists()?);
    /// assert_eq!(directory.metadata()?.file_type, VfsFileType::Directory);
    /// let parent = path.join("foo")?;
    /// assert!(parent.exists()?);
    /// assert_eq!(parent.metadata()?.file_type, VfsFileType::Directory);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn create_dir_all(&self) -> VfsResult<()> {
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
            if let Err(error) = self.fs.fs.create_dir(directory) {
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
    /// # use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// path.join("foo")?.create_dir()?;
    /// path.join("bar")?.create_dir()?;
    ///
    /// let mut directories: Vec<_> = path.read_dir()?.collect();
    ///
    /// directories.sort_by_key(|path| path.as_str().to_string());
    /// assert_eq!(directories, vec![path.join("bar")?, path.join("foo")?]);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn read_dir(&self) -> VfsResult<Box<dyn Iterator<Item = VfsPath> + Send>> {
        let parent = self.path.clone();
        let fs = self.fs.clone();
        Ok(Box::new(
            self.fs
                .fs
                .read_dir(&self.path)
                .map_err(|err| {
                    err.with_path(&self.path)
                        .with_context(|| "Could not read directory")
                })?
                .map(move |path| VfsPath {
                    path: format!("{}/{}", parent, path),
                    fs: fs.clone(),
                }),
        ))
    }

    /// Creates a file at this path for writing, overwriting any existing file
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let file = path.join("foo.txt")?;
    ///
    /// write!(file.create_file()?, "Hello, world!")?;
    ///
    /// let mut result = String::new();
    /// file.open_file()?.read_to_string(&mut result)?;
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn create_file(&self) -> VfsResult<Box<dyn Write + Send>> {
        self.get_parent("create file")?;
        self.fs.fs.create_file(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not create file")
        })
    }

    /// Opens the file at this path for reading
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file()?, "Hello, world!")?;
    /// let mut result = String::new();
    ///
    /// file.open_file()?.read_to_string(&mut result)?;
    ///
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn open_file(&self) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.fs.fs.open_file(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not open file")
        })
    }

    /// Checks whether parent is a directory
    fn get_parent(&self, action: &str) -> VfsResult<()> {
        let parent = self.parent();
        if !parent.exists()? {
            return Err(VfsError::from(VfsErrorKind::Other(format!(
                "Could not {}, parent directory does not exist",
                action
            )))
            .with_path(&self.path));
        }
        let metadata = parent.metadata()?;
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file()?, "Hello, ")?;
    /// write!(file.append_file()?, "world!")?;
    /// let mut result = String::new();
    /// file.open_file()?.read_to_string(&mut result)?;
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn append_file(&self) -> VfsResult<Box<dyn Write + Send>> {
        self.fs.fs.append_file(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not open file for appending")
        })
    }

    /// Removes the file at this path
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file()?, "Hello, ")?;
    /// assert!(file.exists()?);
    ///
    /// file.remove_file()?;
    ///
    /// assert!(!file.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn remove_file(&self) -> VfsResult<()> {
        self.fs.fs.remove_file(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not remove file")
        })
    }

    /// Removes the directory at this path
    ///
    /// The directory must be empty.
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir();
    /// assert!(directory.exists()?);
    ///
    /// directory.remove_dir()?;
    ///
    /// assert!(!directory.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn remove_dir(&self) -> VfsResult<()> {
        self.fs.fs.remove_dir(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not remove directory")
        })
    }

    /// Ensures that the directory at this path is removed, recursively deleting all contents if necessary
    ///
    /// Returns successfully if directory does not exist
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.join("bar")?.create_dir_all();
    /// assert!(directory.exists()?);
    ///
    /// directory.remove_dir_all()?;
    ///
    /// assert!(!directory.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn remove_dir_all(&self) -> VfsResult<()> {
        if !self.exists()? {
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
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir();
    ///
    /// assert_eq!(directory.metadata()?.len, 0);
    /// assert_eq!(directory.metadata()?.file_type, VfsFileType::Directory);
    ///
    /// let file = path.join("bar.txt")?;
    /// write!(file.create_file()?, "Hello, world!")?;
    ///
    /// assert_eq!(file.metadata()?.len, 13);
    /// assert_eq!(file.metadata()?.file_type, VfsFileType::File);
    /// # Ok::<(), VfsError>(())
    pub fn metadata(&self) -> VfsResult<VfsMetadata> {
        self.fs.fs.metadata(&self.path).map_err(|err| {
            err.with_path(&self.path)
                .with_context(|| "Could not get metadata")
        })
    }

    /// Returns `true` if the path exists and is pointing at a regular file, otherwise returns `false`.
    ///
    /// Note that this call may fail if the file's existence cannot be determined or the metadata can not be retrieved
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir()?;
    /// let file = path.join("foo.txt")?;
    /// file.create_file()?;
    ///
    /// assert!(!directory.is_file()?);
    /// assert!(file.is_file()?);
    /// # Ok::<(), VfsError>(())
    pub fn is_file(&self) -> VfsResult<bool> {
        if !self.exists()? {
            return Ok(false);
        }
        let metadata = self.metadata()?;
        Ok(metadata.file_type == VfsFileType::File)
    }

    /// Returns `true` if the path exists and is pointing at a directory, otherwise returns `false`.
    ///
    /// Note that this call may fail if the directory's existence cannot be determined or the metadata can not be retrieved
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    /// directory.create_dir()?;
    /// let file = path.join("foo.txt")?;
    /// file.create_file()?;
    ///
    /// assert!(directory.is_dir()?);
    /// assert!(!file.is_dir()?);
    /// # Ok::<(), VfsError>(())
    pub fn is_dir(&self) -> VfsResult<bool> {
        if !self.exists()? {
            return Ok(false);
        }
        let metadata = self.metadata()?;
        Ok(metadata.file_type == VfsFileType::Directory)
    }

    /// Returns true if a file or directory exists at this path, false otherwise
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let directory = path.join("foo")?;
    ///
    /// assert!(!directory.exists()?);
    ///
    /// directory.create_dir();
    ///
    /// assert!(directory.exists()?);
    /// # Ok::<(), VfsError>(())
    pub fn exists(&self) -> VfsResult<bool> {
        self.fs.fs.exists(&self.path)
    }

    /// Returns the filename portion of this path
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsFileType, VfsMetadata, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
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
    /// # use vfs::{MemoryFS, VfsError, VfsPath, VfsResult};
    /// let root = VfsPath::new(MemoryFS::new());
    /// root.join("foo/bar")?.create_dir_all()?;
    /// root.join("fizz/buzz")?.create_dir_all()?;
    /// root.join("foo/bar/baz")?.create_file()?;
    ///
    /// let mut directories = root.walk_dir()?.collect::<VfsResult<Vec<_>>>()?;
    ///
    /// directories.sort_by_key(|path| path.as_str().to_string());
    /// let expected = vec!["fizz", "fizz/buzz", "foo", "foo/bar", "foo/bar/baz"].iter().map(|path| root.join(path)).collect::<VfsResult<Vec<_>>>()?;
    /// assert_eq!(directories, expected);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn walk_dir(&self) -> VfsResult<WalkDirIterator> {
        Ok(WalkDirIterator {
            inner: Box::new(self.read_dir()?),
            todo: vec![],
        })
    }

    /// Reads a complete file to a string
    ///
    /// Returns an error if the file does not exist or is not valid UTF-8
    ///
    /// ```
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let file = path.join("foo.txt")?;
    /// write!(file.create_file()?, "Hello, world!")?;
    ///
    /// let result = file.read_to_string()?;
    ///
    /// assert_eq!(&result, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn read_to_string(&self) -> VfsResult<String> {
        let metadata = self.metadata()?;
        if metadata.file_type != VfsFileType::File {
            return Err(
                VfsError::from(VfsErrorKind::Other("Path is a directory".into()))
                    .with_path(&self.path)
                    .with_context(|| "Could not read path"),
            );
        }
        let mut result = String::with_capacity(metadata.len as usize);
        self.open_file()?
            .read_to_string(&mut result)
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let src = path.join("foo.txt")?;
    /// write!(src.create_file()?, "Hello, world!")?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.copy_file(&dest)?;
    ///
    /// assert_eq!(dest.read_to_string()?, "Hello, world!");
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn copy_file(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&self.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.copy_file(&self.path, &destination.path);
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
            let mut src = self.open_file()?;
            let mut dest = destination.create_file()?;
            std::io::copy(&mut src, &mut dest).map_err(|source| {
                VfsError::from(source)
                    .with_path(&self.path)
                    .with_context(|| "Could not read path")
            })?;
            Ok(())
        }()
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let src = path.join("foo.txt")?;
    /// write!(src.create_file()?, "Hello, world!")?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.move_file(&dest)?;
    ///
    /// assert_eq!(dest.read_to_string()?, "Hello, world!");
    /// assert!(!src.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn move_file(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_file(&self.path, &destination.path);
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
            let mut src = self.open_file()?;
            let mut dest = destination.create_file()?;
            std::io::copy(&mut src, &mut dest).map_err(|source| {
                VfsError::from(source)
                    .with_path(&self.path)
                    .with_context(|| "Could not read path")
            })?;
            self.remove_file()?;
            Ok(())
        }()
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let src = path.join("foo")?;
    /// src.join("dir")?.create_dir_all()?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.copy_dir(&dest)?;
    ///
    /// assert!(dest.join("dir")?.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn copy_dir(&self, destination: &VfsPath) -> VfsResult<u64> {
        let mut files_copied = 0u64;
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            destination.create_dir()?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            for file in self.walk_dir()? {
                let src_path: VfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata()?.file_type {
                    VfsFileType::Directory => dest_path.create_dir()?,
                    VfsFileType::File => src_path.copy_file(&dest_path)?,
                }
                files_copied += 1;
            }
            Ok(())
        }()
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
    /// # use std::io::Read;
    /// use vfs::{MemoryFS, VfsError, VfsPath};
    /// let path = VfsPath::new(MemoryFS::new());
    /// let src = path.join("foo")?;
    /// src.join("dir")?.create_dir_all()?;
    /// let dest = path.join("bar.txt")?;
    ///
    /// src.move_dir(&dest)?;
    ///
    /// assert!(dest.join("dir")?.exists()?);
    /// assert!(!src.join("dir")?.exists()?);
    /// # Ok::<(), VfsError>(())
    /// ```
    pub fn move_dir(&self, destination: &VfsPath) -> VfsResult<()> {
        || -> VfsResult<()> {
            if destination.exists()? {
                return Err(VfsError::from(VfsErrorKind::Other(
                    "Destination exists already".into(),
                ))
                .with_path(&destination.path));
            }
            if Arc::ptr_eq(&self.fs, &destination.fs) {
                let result = self.fs.fs.move_dir(&self.path, &destination.path);
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
            destination.create_dir()?;
            let prefix = self.path.as_str();
            let prefix_len = prefix.len();
            for file in self.walk_dir()? {
                let src_path: VfsPath = file?;
                let dest_path = destination.join(&src_path.as_str()[prefix_len + 1..])?;
                match src_path.metadata()?.file_type {
                    VfsFileType::Directory => dest_path.create_dir()?,
                    VfsFileType::File => src_path.copy_file(&dest_path)?,
                }
            }
            self.remove_dir_all()?;
            Ok(())
        }()
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
    inner: Box<dyn Iterator<Item = VfsPath> + Send>,
    /// stack of subdirectories still to walk
    todo: Vec<VfsPath>,
}

impl std::fmt::Debug for WalkDirIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalkDirIterator")?;
        self.todo.fmt(f)
    }
}

impl Iterator for WalkDirIterator {
    type Item = VfsResult<VfsPath>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = loop {
            match self.inner.next() {
                Some(path) => break Some(Ok(path)),
                None => {
                    match self.todo.pop() {
                        None => return None, // all done!
                        Some(directory) => match directory.read_dir() {
                            Ok(iterator) => self.inner = iterator,
                            Err(err) => break Some(Err(err)),
                        },
                    }
                }
            }
        };
        if let Some(Ok(path)) = &result {
            let metadata = path.metadata();
            match metadata {
                Ok(metadata) => {
                    if metadata.file_type == VfsFileType::Directory {
                        self.todo.push(path.clone());
                    }
                }
                Err(err) => return Some(Err(err)),
            }
        }
        result
    }
}
