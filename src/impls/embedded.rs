use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::io::{Cursor, Write};
use std::marker::PhantomData;

use rust_embed::RustEmbed;

use crate::{FileSystem, SeekAndRead, VfsError, VfsFileType, VfsMetadata, VfsResult};

type EmbeddedPath = Cow<'static, str>;

#[derive(Debug)]
/// a read-only file system embedded in the executable
/// see [rust-embed](https://docs.rs/rust-embed/) for how to create a `RustEmbed`
pub struct EmbeddedFS<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    p: PhantomData<T>,
    directory_map: HashMap<EmbeddedPath, HashSet<EmbeddedPath>>,
    files: HashMap<EmbeddedPath, u64>,
}

impl<T> EmbeddedFS<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    pub fn new() -> Self {
        let mut directory_map: HashMap<EmbeddedPath, HashSet<EmbeddedPath>> = Default::default();
        let mut files: HashMap<EmbeddedPath, u64> = Default::default();
        for file in T::iter() {
            let mut path = file.clone();
            files.insert(
                file.clone(),
                T::get(&path).expect("Path should exist").data.len() as u64,
            );
            while let Some((prefix, suffix)) = rsplit_once_cow(&path, "/") {
                let children = directory_map.entry(prefix.clone()).or_default();
                children.insert(suffix);
                path = prefix;
            }
            let children = directory_map.entry("".into()).or_default();
            children.insert(path);
        }
        EmbeddedFS {
            p: PhantomData::default(),
            directory_map,
            files,
        }
    }
}

fn rsplit_once_cow(input: &EmbeddedPath, delimiter: &str) -> Option<(EmbeddedPath, EmbeddedPath)> {
    let mut result: Vec<_> = match input {
        EmbeddedPath::Borrowed(s) => s.rsplitn(2, delimiter).map(|a| Cow::Borrowed(a)).collect(),
        EmbeddedPath::Owned(s) => s
            .rsplitn(2, delimiter)
            .map(|a| Cow::Owned(a.to_string()))
            .collect(),
    };
    if result.len() == 2 {
        Some((result.remove(1), result.remove(0)))
    } else {
        None
    }
}

impl<T> FileSystem for EmbeddedFS<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>> {
        let normalized_path = normalize_path(path)?;
        if let Some(children) = self.directory_map.get(normalized_path) {
            return Ok(Box::new(
                children.clone().into_iter().map(|path| path.into_owned()),
            ));
        } else {
            if self.files.contains_key(normalized_path) {
                // Actually a file
                return Err(VfsError::Other {
                    message: format!("{} is not a directory", path),
                });
            }
            return Err(VfsError::FileNotFound {
                path: path.to_string(),
            });
        }
    }

    fn create_dir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>> {
        match T::get(path.split_at(1).1) {
            None => {
                return Err(VfsError::FileNotFound {
                    path: path.to_string(),
                });
            }
            Some(file) => Ok(Box::new(Cursor::new(file.data))),
        }
    }

    fn create_file(&self, _path: &str) -> VfsResult<Box<dyn Write>> {
        Err(VfsError::NotSupported)
    }

    fn append_file(&self, _path: &str) -> VfsResult<Box<dyn Write>> {
        Err(VfsError::NotSupported)
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let normalized_path = normalize_path(path)?;
        if let Some(len) = self.files.get(normalized_path) {
            return Ok(VfsMetadata {
                file_type: VfsFileType::File,
                len: *len,
            });
        }
        if self.directory_map.contains_key(normalized_path) {
            return Ok(VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
            });
        }
        return Err(VfsError::FileNotFound {
            path: path.to_string(),
        });
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        let path = normalize_path(path)?;
        if self.files.contains_key(path) {
            return Ok(true);
        }
        if self.directory_map.contains_key(path) {
            return Ok(true);
        }
        if path == "" {
            // Root always exists
            return Ok(true);
        }
        return Ok(false);
    }

    fn remove_file(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn remove_dir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
}

fn normalize_path(path: &str) -> VfsResult<&str> {
    if path.len() == 0 {
        return Ok("");
    }
    let path = &path[1..];
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::Read;

    use crate::{FileSystem, VfsError, VfsFileType, VfsPath};

    use super::*;

    #[derive(RustEmbed, Debug)]
    #[folder = "test/test_directory"]
    struct TestEmbed;

    fn get_test_fs() -> EmbeddedFS<TestEmbed> {
        EmbeddedFS::new()
    }

    test_vfs_readonly!({ get_test_fs() });
    #[test]
    fn read_dir_lists_directory() {
        let fs = get_test_fs();
        assert_eq!(
            fs.read_dir("/").unwrap().collect::<HashSet<_>>(),
            vec!["a", "a.txt.dir", "c", "a.txt", "b.txt"]
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            fs.read_dir("/a").unwrap().collect::<HashSet<_>>(),
            vec!["d.txt", "x"]
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            fs.read_dir("/a.txt.dir").unwrap().collect::<HashSet<_>>(),
            vec!["g.txt"]
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn read_dir_no_directory_err() {
        let fs = get_test_fs();
        assert!(match fs.read_dir("/c/f") {
            Err(VfsError::FileNotFound { .. }) => true,
            _ => false,
        });
        assert!(match fs.read_dir("/a.txt.") {
            Err(VfsError::FileNotFound { .. }) => true,
            _ => false,
        });
        assert!(match fs.read_dir("/abc/def/ghi") {
            Err(VfsError::FileNotFound { .. }) => true,
            _ => false,
        });
    }

    #[test]
    fn read_dir_on_file_err() {
        let fs = get_test_fs();
        assert!(match fs.read_dir("/a.txt") {
            Err(VfsError::Other { message }) => &message == "/a.txt is not a directory",
            _ => false,
        });
        assert!(match fs.read_dir("/a/d.txt") {
            Err(VfsError::Other { message }) => &message == "/a/d.txt is not a directory",
            _ => false,
        });
    }

    #[test]
    fn create_dir_not_supported() {
        let fs = get_test_fs();
        assert!(match fs.create_dir("/abc") {
            Err(VfsError::NotSupported) => true,
            _ => false,
        })
    }

    #[test]
    fn open_file() {
        let fs = get_test_fs();
        let mut text = String::new();
        fs.open_file("/a.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "a");
    }

    #[test]
    fn open_empty_file() {
        let fs = get_test_fs();
        let mut text = String::new();
        fs.open_file("/a.txt.dir/g.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "");
    }

    #[test]
    fn open_file_not_found() {
        let fs = get_test_fs();
        assert!(match fs.open_file("/") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "/",
            _ => false,
        });
        assert!(match fs.open_file("/abc.txt") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "/abc.txt",
            _ => false,
        });
        assert!(match fs.open_file("/c/f.txt") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "/c/f.txt",
            _ => false,
        });
    }

    #[test]
    fn create_file_not_supported() {
        let fs = get_test_fs();
        assert!(match fs.create_file("/abc.txt") {
            Err(VfsError::NotSupported) => true,
            _ => false,
        });
    }

    #[test]
    fn append_file_not_supported() {
        let fs = get_test_fs();
        assert!(match fs.append_file("/abc.txt") {
            Err(VfsError::NotSupported) => true,
            _ => false,
        });
    }

    #[test]
    fn metadata_file() {
        let fs = get_test_fs();
        let d = fs.metadata("/a/d.txt").unwrap();
        assert_eq!(d.len, 1);
        assert_eq!(d.file_type, VfsFileType::File);

        let g = fs.metadata("/a.txt.dir/g.txt").unwrap();
        assert_eq!(g.len, 0);
        assert_eq!(g.file_type, VfsFileType::File);
    }

    #[test]
    fn metadata_directory() {
        let fs = get_test_fs();
        let root = fs.metadata("/").unwrap();
        assert_eq!(root.len, 0);
        assert_eq!(root.file_type, VfsFileType::Directory);

        // The empty path is treated as root
        let root = fs.metadata("").unwrap();
        assert_eq!(root.len, 0);
        assert_eq!(root.file_type, VfsFileType::Directory);

        let a = fs.metadata("/a").unwrap();
        assert_eq!(a.len, 0);
        assert_eq!(a.file_type, VfsFileType::Directory);
    }

    #[test]
    fn metadata_not_found() {
        let fs = get_test_fs();
        assert!(match fs.metadata("/abc.txt") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "/abc.txt",
            _ => false,
        });
    }

    #[test]
    fn exists() {
        let fs = get_test_fs();
        assert!(fs.exists("").unwrap());
        assert!(fs.exists("/a").unwrap());
        assert!(fs.exists("/a/d.txt").unwrap());
        assert!(fs.exists("/a.txt.dir").unwrap());
        assert!(fs.exists("/a.txt.dir/g.txt").unwrap());
        assert!(fs.exists("/c").unwrap());
        assert!(fs.exists("/c/e.txt").unwrap());
        assert!(fs.exists("/a.txt").unwrap());
        assert!(fs.exists("/b.txt").unwrap());

        assert!(!fs.exists("/abc").unwrap());
        assert!(!fs.exists("/a.txt.").unwrap());
    }

    #[test]
    fn remove_file_not_supported() {
        let fs = get_test_fs();
        assert!(match fs.remove_file("/abc.txt") {
            Err(VfsError::NotSupported) => true,
            _ => false,
        });
    }

    #[test]
    fn remove_dir_not_supported() {
        let fs = get_test_fs();
        assert!(match fs.remove_dir("/abc.txt") {
            Err(VfsError::NotSupported) => true,
            _ => false,
        });
    }

    #[test]
    fn integration() {
        let root: VfsPath = get_test_fs().into();
        let a_file = root.join("a.txt").unwrap();
        assert!(a_file.exists().unwrap());
        let mut text = String::new();
        a_file
            .open_file()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text.as_str(), "a");
        assert_eq!(a_file.filename(), String::from("a.txt"));

        text.clear();
        root.join("a")
            .unwrap()
            .join("d.txt")
            .unwrap()
            .open_file()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, String::from("d"));

        assert!(root.join("a.txt.dir").unwrap().exists().unwrap());
        assert!(!root.join("g").unwrap().exists().unwrap());
    }
}
