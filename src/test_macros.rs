#[macro_export]
macro_rules! test_vfs {
    // Run basic vfs test to check for conformance
    ($root:expr) => {
        #[cfg(test)]
        mod vfs_tests {
            use super::*;
            use crate::VfsPath;
            fn create_root() -> VfsPath {
                VfsPath::create($root).unwrap()
            }

            #[test]
            fn vfs_can_be_created() {
                create_root();
            }

            #[test]
            fn write_and_read_file() {
                let root = create_root();
                let path = root.join("foobar.txt");
                let _send = &path as &dyn Send;
                {
                    let mut file = path.create_file().unwrap();
                    write!(file, "Hello world").unwrap();
                    write!(file, "!").unwrap();
                }
                {
                    let mut file = path.open_file().unwrap();
                    let mut string: String = String::new();
                    file.read_to_string(&mut string).unwrap();
                    assert_eq!(string, "Hello world!");
                }
                assert!(path.exists());
                assert!(!root.join("foo").exists());
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.len, 12);
                assert_eq!(metadata.file_type, VfsFileType::File);
            }

            #[test]
            fn append_file() {
                let root = create_root();
                let path = root.join("test_append.txt");
                path.create_file().unwrap().write_all(b"Testing 1").unwrap();
                path.append_file().unwrap().write_all(b"Testing 2").unwrap();
                {
                    let mut file = path.open_file().unwrap();
                    let mut string: String = String::new();
                    file.read_to_string(&mut string).unwrap();
                    assert_eq!(string, "Testing 1Testing 2");
                }
            }

            #[test]
            fn create_dir() {
                let root = create_root();
                let _string = String::new();
                let path = root.join("foo");
                path.create_dir().unwrap();
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VfsFileType::Directory);
                assert_eq!(metadata.len, 0);
            }

            #[test]
            fn create_dir_all() {
                let root = create_root();
                let _string = String::new();
                let path = root.join("foo");
                path.create_dir().unwrap();
                let path = root.join("foo/bar/baz");
                path.create_dir_all().unwrap();
                assert!(path.exists());
                assert!(root.join("foo/bar").exists());
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VfsFileType::Directory);
                assert_eq!(metadata.len, 0);
            }

            #[test]
            fn read_dir() {
                let root = create_root();
                let _string = String::new();
                root.join("foo/bar/biz").create_dir_all().unwrap();
                root.join("baz").create_file().unwrap();
                root.join("foo/fizz").create_file().unwrap();
                let mut files: Vec<_> = root
                    .read_dir()
                    .unwrap()
                    .map(|path| path.path().to_string())
                    .collect();
                files.sort();
                assert_eq!(files, vec!["/baz".to_string(), "/foo".to_string()]);
                let mut files: Vec<_> = root
                    .join("foo")
                    .read_dir()
                    .unwrap()
                    .map(|path| path.path().to_string())
                    .collect();
                files.sort();
                assert_eq!(files, vec!["/foo/bar".to_string(), "/foo/fizz".to_string()]);
            }

            #[test]
            fn remove_file() {
                let root = create_root();
                let path = root.join("baz");
                assert!(!path.exists());
                path.create_file().unwrap();
                assert!(path.exists());
                path.remove_file().unwrap();
                assert!(!path.exists());
            }

            #[test]
            fn remove_file_nonexisting() {
                let root = create_root();
                let path = root.join("baz");
                assert!(!path.exists());
                assert!(path.remove_file().is_err());
            }

            #[test]
            fn remove_dir() {
                let root = create_root();
                let path = root.join("baz");
                assert!(!path.exists());
                path.create_dir().unwrap();
                assert!(path.exists());
                path.remove_dir().unwrap();
                assert!(!path.exists());
            }

            #[test]
            fn remove_dir_nonexisting() {
                let root = create_root();
                let path = root.join("baz");
                assert!(!path.exists());
                assert!(path.remove_dir().is_err());
            }

            #[test]
            fn remove_dir_notempty() {
                let root = create_root();
                let path = root.join("bar");
                root.join("bar/baz/fizz").create_dir_all().unwrap();
                assert!(path.remove_dir().is_err());
            }

            #[test]
            fn remove_dir_all() {
                let root = create_root();
                let path = root.join("foo");
                assert!(!path.exists());
                path.join("bar/baz/fizz").create_dir_all().unwrap();
                path.join("bar/buzz").create_file().unwrap();
                assert!(path.exists());
                assert!(path.remove_dir_all().is_ok());
                assert!(!path.exists());
            }

            #[test]
            fn remove_dir_all_nonexisting() {
                let root = create_root();
                let path = root.join("baz");
                assert!(!path.exists());
                assert!(path.remove_dir_all().is_ok());
            }

            #[test]
            fn filename() {
                let root = create_root();
                assert_eq!(root.filename(), "");
                assert_eq!(root.join("name.foo.bar").filename(), "name.foo.bar");
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").filename(),
                    "name.foo.bar"
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").filename(),
                    ".name.foo.bar"
                );
                assert_eq!(root.join("fizz.buzz/foo.").filename(), "foo.");
            }

            #[test]
            fn extension() {
                let root = create_root();
                assert_eq!(root.extension(), None, "root");
                assert_eq!(root.join("name").extension(), None, "name");
                assert_eq!(
                    root.join("name.bar").extension(),
                    Some("bar".to_string()),
                    "name.bar"
                );
                assert_eq!(
                    root.join("name.").extension(),
                    Some("".to_string()),
                    "name."
                );
                assert_eq!(root.join(".name").extension(), None, ".name");
                assert_eq!(
                    root.join(".name.bar").extension(),
                    Some("bar".to_string()),
                    ".name.bar"
                );
                assert_eq!(
                    root.join(".name.").extension(),
                    Some("".to_string()),
                    ".name."
                );
                assert_eq!(
                    root.join("name.foo.bar").extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/foo.").extension(),
                    Some("".to_string())
                );
            }

            #[test]
            fn parent() {
                let root = create_root();
                assert_eq!(root.parent(), None, "root");
                assert_eq!(root.join("foo").parent(), Some(root.clone()), "foo");
                assert_eq!(
                    root.join("foo/bar").parent(),
                    Some(root.join("foo")),
                    "foo/bar"
                );
                assert_eq!(
                    root.join("foo/bar/baz").parent(),
                    Some(root.join("foo/bar")),
                    "foo/bar/baz"
                );
            }

            #[test]
            fn eq() {
                let root = create_root();

                assert_eq!(root, root);
                assert_eq!(root.join("foo"), root.join("foo"));
                assert_eq!(root.join("foo"), root.join("foo/bar").parent().unwrap());
                assert_eq!(root, root.join("foo").parent().unwrap());

                assert_ne!(root, root.join("foo"));
                assert_ne!(root.join("bar"), root.join("foo"));

                let root2 = create_root();
                assert_ne!(root, root2);
                assert_ne!(root.join("foo"), root2.join("foo"));
            }
        }
    };
}
