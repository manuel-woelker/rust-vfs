use crate::{FileSystem, SeekAndRead, VfsError, VfsFileType, VfsMetadata, VfsResult};
use radix_trie::{SubTrie, Trie, TrieCommon};
use rust_embed::RustEmbed;
use std::fmt::Debug;
use std::io::{Cursor, Write};
use std::iter;
use std::marker::PhantomData;
use std::path::Path;

#[derive(Debug)]
/// a read-only file system embedded in the executable
/// see [rust-embed](https://docs.rs/rust-embed/) for how to create a `RustEmbed`
pub struct EmbeddedFs<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    p: PhantomData<T>,
    path_trie: Trie<String, bool>,
}

impl<T> EmbeddedFs<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    pub fn new() -> Self {
        EmbeddedFs {
            p: PhantomData::default(),
            path_trie: T::iter()
                .map(|p| {
                    Path::new(p.as_ref())
                        .ancestors()
                        .map(Path::to_str)
                        .map(Option::unwrap)
                        .map(|p| format!("/{}", p))
                        .zip(iter::once(true).chain(iter::repeat(false)))
                        .collect::<Vec<_>>()
                        .into_iter()
                })
                .flatten()
                .collect(),
        }
    }
}

impl<T> FileSystem for EmbeddedFs<T>
where
    T: RustEmbed + Send + Sync + Debug + 'static,
{
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>> {
        match self.path_trie.get(path) {
            None => {
                return Err(VfsError::FileNotFound {
                    path: path.to_string(),
                });
            }
            Some(&true) => {
                return Err(VfsError::Other {
                    message: format!("{} is not a directory", path),
                });
            }
            Some(&false) => (),
        }

        fn child_elements<'a>(
            d: &'a Path,
            t: SubTrie<'a, String, bool>,
        ) -> Box<dyn Iterator<Item = String> + 'a> {
            let children = t.children().flat_map(move |c| child_elements(d, c));

            if let Some(k) = t.key() {
                match Path::new(k).parent() {
                    None => Box::new(children),
                    Some(p) if p == d => Box::new(iter::once(k.clone()).chain(children)),
                    Some(_) => Box::new(iter::empty()),
                }
            } else {
                Box::new(children)
            }
        }

        let sub_trie: SubTrie<String, bool> = match self.path_trie.subtrie(path) {
            None => return Ok(Box::new(iter::empty())),
            Some(trie) => trie,
        };
        let d = Path::new(path);

        Ok(Box::new(
            sub_trie
                .children()
                .flat_map(|t| child_elements(d, t))
                .collect::<Vec<_>>()
                .into_iter(),
        ))
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
            Some(data) => Ok(Box::new(Cursor::new(data))),
        }
    }

    fn create_file(&self, _path: &str) -> VfsResult<Box<dyn Write>> {
        Err(VfsError::NotSupported)
    }

    fn append_file(&self, _path: &str) -> VfsResult<Box<dyn Write>> {
        Err(VfsError::NotSupported)
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        match self.path_trie.get(path) {
            None => Err(VfsError::FileNotFound {
                path: path.to_string(),
            }),
            Some(&false) => Ok(VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
            }),
            Some(&true) => Ok(VfsMetadata {
                file_type: VfsFileType::File,
                len: T::get(path.split_at(1).1).unwrap().len() as u64,
            }),
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.path_trie.get(path).is_some()
    }

    fn remove_file(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn remove_dir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileSystem, VfsError, VfsFileType, VfsPath};
    use std::collections::HashSet;
    use std::io::Read;

    #[derive(RustEmbed, Debug)]
    #[folder = "test/test_embedded_directory"]
    struct TestEmbed;

    fn get_test_fs() -> EmbeddedFs<TestEmbed> {
        EmbeddedFs::new()
    }

    #[test]
    fn new() {
        let fs = get_test_fs();
        assert_eq!(fs.path_trie.get(""), None);
        assert_eq!(fs.path_trie.get("/"), Some(&false));
        assert_eq!(fs.path_trie.get("/a.txt"), Some(&true));
        assert_eq!(fs.path_trie.get("/b.txt"), Some(&true));
        assert_eq!(fs.path_trie.get("/a"), Some(&false));
        assert_eq!(fs.path_trie.get("/a/d.txt"), Some(&true));
        assert_eq!(fs.path_trie.get("/c"), Some(&false));
        assert_eq!(fs.path_trie.get("/c/e.txt"), Some(&true));
        assert_eq!(fs.path_trie.get("/c/f"), None);
    }

    #[test]
    fn read_dir_lists_directory() {
        let fs = get_test_fs();
        assert_eq!(
            fs.read_dir("/").unwrap().collect::<HashSet<_>>(),
            vec!["/a", "/a.txt.dir", "/c", "/a.txt", "/b.txt"]
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            fs.read_dir("/a").unwrap().collect::<HashSet<_>>(),
            vec!["/a/d.txt"]
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            fs.read_dir("/a.txt.dir").unwrap().collect::<HashSet<_>>(),
            vec!["/a.txt.dir/g.txt"]
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
            Err(VfsError::Other { message }) => message.as_str() == "/a.txt is not a directory",
            _ => false,
        });
        assert!(match fs.read_dir("/a/d.txt") {
            Err(VfsError::Other { message }) => message.as_str() == "/a/d.txt is not a directory",
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

        let a = fs.metadata("/a").unwrap();
        assert_eq!(a.len, 0);
        assert_eq!(a.file_type, VfsFileType::Directory);
    }

    #[test]
    fn metadata_not_found() {
        let fs = get_test_fs();
        assert!(match fs.metadata("") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "",
            _ => false,
        });
        assert!(match fs.metadata("/abc.txt") {
            Err(VfsError::FileNotFound { path }) => path.as_str() == "/abc.txt",
            _ => false,
        });
    }

    #[test]
    fn exists() {
        let fs = get_test_fs();
        assert!(fs.exists("/"));
        assert!(fs.exists("/a"));
        assert!(fs.exists("/a/d.txt"));
        assert!(fs.exists("/a.txt.dir"));
        assert!(fs.exists("/a.txt.dir/g.txt"));
        assert!(fs.exists("/c"));
        assert!(fs.exists("/c/e.txt"));
        assert!(fs.exists("/a.txt"));
        assert!(fs.exists("/b.txt"));

        assert!(!fs.exists("/abc"));
        assert!(!fs.exists("/a.txt."));
        assert!(!fs.exists(""));
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
        assert!(a_file.exists());
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

        assert!(root.join("a.txt.dir").unwrap().exists());
        assert!(!root.join("g").unwrap().exists());
    }
}
