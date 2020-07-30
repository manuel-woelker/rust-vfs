
#[macro_export]
macro_rules! test_vfs {
    // Run basic vfs test to check for conformance
    ($root:expr) => {

        #[cfg(test)]
        mod vfs_tests {
            use super::*;
            use crate::VPath;
            fn create_root() -> VPath {
                VPath::create($root).unwrap()
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
                assert_eq!(metadata.file_type, VFileType::File);
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
                assert_eq!(metadata.file_type, VFileType::Directory);
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
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VFileType::Directory);
                assert_eq!(metadata.len, 0);
            }

            #[test]
            fn read_dir() {
                let root = create_root();
                let _string = String::new();
                root.join("foo/bar/biz").create_dir_all().unwrap();
                root.join("baz").create_file().unwrap();
                root.join("foo/fizz").create_file().unwrap();
                let mut files: Vec<_> = root.read_dir().unwrap().map(|path| path.path().to_string()).collect();
                files.sort();
                assert_eq!(files, vec!["/baz".to_string(), "/foo".to_string()]);
                let mut files: Vec<_> = root.join("foo").read_dir().unwrap().map(|path| path.path().to_string()).collect();
                files.sort();
                assert_eq!(files, vec!["/foo/bar".to_string(), "/foo/fizz".to_string()]);
            }

        }

    };
}
