/// Run basic read/write vfs test to check for conformance
/// If an Filesystem implementation is read-only use [test_vfs_readonly!] instead
#[macro_export]
macro_rules! test_vfs {
    ($root:expr) => {
        #[cfg(test)]
        mod vfs_tests {
            use super::*;
            use $crate::VfsFileType;
            use $crate::VfsPath;
            use $crate::VfsResult;

            fn create_root() -> VfsPath {
                $root.into()
            }

            #[test]
            fn vfs_can_be_created() {
                create_root();
            }

            #[test]
            fn write_and_read_file()  -> VfsResult<()>{
                let root = create_root();
                let path = root.join("foobar.txt").unwrap();
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
                assert!(path.exists()?);
                assert!(!root.join("foo").unwrap().exists()?);
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.len, 12);
                assert_eq!(metadata.file_type, VfsFileType::File);
                Ok(())
            }

            #[test]
            fn append_file() {
                let root = create_root();
                let path = root.join("test_append.txt").unwrap();
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
            fn append_non_existing_file() {
                let root = create_root();
                let path = root.join("test_append.txt").unwrap();
                let result = path.append_file();
                match result {
                    Ok(_) => {panic!("Expected error");}
                    Err(err) => {
                        let error_message = format!("{}", err);
                        assert!(
                            error_message.starts_with("Could not open file for appending"),
                            "Actual message: {}",
                            error_message);
                    }
                }
            }

            #[test]
            fn create_dir() {
                let root = create_root();
                let _string = String::new();
                let path = root.join("foo").unwrap();
                path.create_dir().unwrap();
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VfsFileType::Directory);
                assert_eq!(metadata.len, 0);
            }

            #[test]
            fn create_dir_with_camino() {
                let root = create_root();
                let _string = String::new();
                let path = root.join(camino::Utf8Path::new("foo")).unwrap();
                path.create_dir().unwrap();
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VfsFileType::Directory);
                assert_eq!(metadata.len, 0);
            }

            #[test]
            fn create_dir_all() -> VfsResult<()>{
                let root = create_root();
                let _string = String::new();
                let path = root.join("foo").unwrap();
                path.create_dir().unwrap();
                let path = root.join("foo/bar/baz").unwrap();
                path.create_dir_all().unwrap();
                assert!(path.exists()?);
                assert!(root.join("foo/bar").unwrap().exists()?);
                let metadata = path.metadata().unwrap();
                assert_eq!(metadata.file_type, VfsFileType::Directory);
                assert_eq!(metadata.len, 0);
                path.create_dir_all().unwrap();
                root.create_dir_all().unwrap();
                Ok(())
            }

            #[test]
            fn create_dir_all_should_fail_for_existing_file() -> VfsResult<()>{
                let root = create_root();
                let _string = String::new();
                let path = root.join("foo").unwrap();
                let path2 = root.join("foo/bar").unwrap();
                path.create_file().unwrap();
                let result = path2.create_dir_all();
                match result {
                    Ok(_) => {panic!("Expected error");}
                    Err(err) => {
                        let error_message = format!("{}", err);
                        if let VfsErrorKind::FileExists = err.kind() {

                        } else {
                            panic!("Expected file exists error")
                        }
                        assert!(
                            error_message.eq("Could not create directories at '/foo/bar' for '/foo': File already exists"),
                            "Actual message: {}",
                            error_message);
                    }
                }
                Ok(())
            }

            #[test]
            fn read_dir() {
                let root = create_root();
                let _string = String::new();
                root.join("foo/bar/biz").unwrap().create_dir_all().unwrap();
                root.join("baz").unwrap().create_file().unwrap();
                root.join("foo/fizz").unwrap().create_file().unwrap();
                let mut files: Vec<_> = root
                    .read_dir()
                    .unwrap()
                    .map(|path| path.as_str().to_string())
                    .collect();
                files.sort();
                assert_eq!(files, vec!["/baz".to_string(), "/foo".to_string()]);
                let mut files: Vec<_> = root
                    .join("foo")
                    .unwrap()
                    .read_dir()
                    .unwrap()
                    .map(|path| path.as_str().to_string())
                    .collect();
                files.sort();
                assert_eq!(files, vec!["/foo/bar".to_string(), "/foo/fizz".to_string()]);
            }

            #[test]
            fn remove_file() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("baz").unwrap();
                assert!(!path.exists()?);
                path.create_file().unwrap();
                assert!(path.exists()?);
                path.remove_file().unwrap();
                assert!(!path.exists()?);
                Ok(())
            }

            #[test]
            fn remove_file_nonexisting() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("baz").unwrap();
                assert!(!path.exists()?);
                assert!(path.remove_file().is_err());
                Ok(())
            }

            #[test]
            fn remove_dir() -> VfsResult<()>{
                let root = create_root();
                let path = root.join("baz").unwrap();
                assert!(!path.exists()?);
                path.create_dir().unwrap();
                assert!(path.exists()?);
                path.remove_dir().unwrap();
                assert!(!path.exists()?);
                Ok(())
            }

            #[test]
            fn remove_dir_nonexisting() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("baz").unwrap();
                assert!(!path.exists()?);
                assert!(path.remove_dir().is_err());
                Ok(())
            }

            #[test]
            fn remove_dir_notempty() {
                let root = create_root();
                let path = root.join("bar").unwrap();
                root.join("bar/baz/fizz").unwrap().create_dir_all().unwrap();
                assert!(path.remove_dir().is_err());
            }

            #[test]
            fn remove_dir_all() -> VfsResult<()>{
                let root = create_root();
                let path = root.join("foo").unwrap();
                assert!(!path.exists()?);
                path.join("bar/baz/fizz").unwrap().create_dir_all().unwrap();
                path.join("bar/buzz").unwrap().create_file().unwrap();
                assert!(path.exists()?);
                assert!(path.remove_dir_all().is_ok());
                assert!(!path.exists()?);
                Ok(())
            }

            #[test]
            fn remove_dir_all_nonexisting() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("baz").unwrap();
                assert!(!path.exists()?);
                assert!(path.remove_dir_all().is_ok());
                Ok(())
            }

            #[test]
            fn filename() {
                let root = create_root();
                assert_eq!(root.filename(), "");
                assert_eq!(
                    root.join("name.foo.bar").unwrap().filename(),
                    "name.foo.bar"
                );
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").unwrap().filename(),
                    "name.foo.bar"
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").unwrap().filename(),
                    ".name.foo.bar"
                );
                assert_eq!(root.join("fizz.buzz/foo.").unwrap().filename(), "foo.");
            }

            #[test]
            fn extension() {
                let root = create_root();
                assert_eq!(root.extension(), None, "root");
                assert_eq!(root.join("name").unwrap().extension(), None, "name");
                assert_eq!(
                    root.join("name.bar").unwrap().extension(),
                    Some("bar".to_string()),
                    "name.bar"
                );
                assert_eq!(
                    root.join("name.").unwrap().extension(),
                    Some("".to_string()),
                    "name."
                );
                assert_eq!(root.join(".name").unwrap().extension(), None, ".name");
                assert_eq!(
                    root.join(".name.bar").unwrap().extension(),
                    Some("bar".to_string()),
                    ".name.bar"
                );
                assert_eq!(
                    root.join(".name.").unwrap().extension(),
                    Some("".to_string()),
                    ".name."
                );
                assert_eq!(
                    root.join("name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/foo.").unwrap().extension(),
                    Some("".to_string())
                );
            }

            #[test]
            fn parent() {
                let root = create_root();
                assert_eq!(root.parent(), root.clone(), "root");
                assert_eq!(
                    root.join("foo").unwrap().parent(),
                    root.clone(),
                    "foo"
                );
                assert_eq!(
                    root.join("foo/bar").unwrap().parent(),
                    root.join("foo").unwrap(),
                    "foo/bar"
                );
                assert_eq!(
                    root.join("foo/bar/baz").unwrap().parent(),
                    root.join("foo/bar").unwrap(),
                    "foo/bar/baz"
                );
            }

            #[test]
            fn eq() {
                let root = create_root();

                assert_eq!(root, root);
                assert_eq!(root.join("foo").unwrap(), root.join("foo").unwrap());
                assert_eq!(
                    root.join("foo").unwrap(),
                    root.join("foo/bar").unwrap().parent()
                );
                assert_eq!(root, root.join("foo").unwrap().parent());

                assert_ne!(root, root.join("foo").unwrap());
                assert_ne!(root.join("bar").unwrap(), root.join("foo").unwrap());

                let root2 = create_root();
                assert_ne!(root, root2);
                assert_ne!(root.join("foo").unwrap(), root2.join("foo").unwrap());
            }

            #[test]
            fn join() {
                let root = create_root();
                assert_eq!(root.join("").unwrap().as_str(), "");
                assert_eq!(root.join("foo").unwrap().join("").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo/bar").unwrap().as_str(), "/foo/bar");
                assert_eq!(root.join("foo/////bar").unwrap().as_str(), "/foo/bar");
                assert_eq!(root.join("foo/bar/baz").unwrap().as_str(), "/foo/bar/baz");
                assert_eq!(
                    root.join("foo").unwrap().join("bar").unwrap().as_str(),
                    "/foo/bar"
                );
                assert_eq!(root.join(".foo").unwrap().as_str(), "/.foo");
                assert_eq!(root.join("..foo").unwrap().as_str(), "/..foo");
                assert_eq!(root.join("foo.").unwrap().as_str(), "/foo.");
                assert_eq!(root.join("foo..").unwrap().as_str(), "/foo..");

                assert_eq!(root.join(".").unwrap().as_str(), "");
                assert_eq!(root.join("./foo").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo/.").unwrap().as_str(), "/foo");

                assert_eq!(root.join("foo/..").unwrap().as_str(), "");
                assert_eq!(root.join("foo").unwrap().join("..").unwrap().as_str(), "");
                assert_eq!(
                    root.join("foo/bar").unwrap().join("..").unwrap().as_str(),
                    "/foo"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("../baz")
                        .unwrap()
                        .as_str(),
                    "/foo/baz"
                );
                assert_eq!(root.join("foo/bar/../..").unwrap().as_str(), "");
                assert_eq!(root.join("foo/bar/../..").unwrap().as_str(), "");
                assert_eq!(root.join("foo/bar/baz/../..").unwrap().as_str(), "/foo");
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../..")
                        .unwrap()
                        .as_str(),
                    "/foo"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../../fizz")
                        .unwrap()
                        .as_str(),
                    "/foo/fizz"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../../fizz/..")
                        .unwrap()
                        .as_str(),
                    "/foo"
                );
                assert_eq!(
                    root.join("..").unwrap(),
                    root
                );
                assert_eq!(
                    root.join("../foo").unwrap(),
                    root.join("foo").unwrap()
                );

                assert_eq!(root.join("/").unwrap(), root);
                assert_eq!(root.join("foo/bar").unwrap().join("/baz").unwrap(), root.join("baz").unwrap());

                assert_eq!(
                    root.join("/foo/bar/baz").unwrap().join("../../..").unwrap(),
                    root
                );

                /// Utility function for templating the same error message
                fn invalid_path_message(path: &str) -> String {
                    format!("An error occured for '{}': The path is invalid", path)
                }

                assert_eq!(
                    root.join("foo/").unwrap_err().to_string(),
                    invalid_path_message("foo/"),
                    "foo/"
                );
            }

            #[test]
            fn walk_dir_empty() -> VfsResult<()> {
                let root = create_root();

                assert_entries(&root, vec![])
            }

            fn assert_entries(path: &VfsPath, expected: Vec<&str>) -> VfsResult<()> {
                let entries: Vec<VfsPath> = path.walk_dir()?.map(|path| path.unwrap()).collect();
                let mut paths = entries.iter().map(|x| x.as_str()).collect::<Vec<&str>>();
                paths.sort();
                assert_eq!(paths, expected);
                Ok(())
            }

            #[test]
            fn walk_dir_single_file() -> VfsResult<()> {
                let root = create_root();
                root.join("baz").unwrap().create_file().unwrap();
                assert_entries(&root, vec!["/baz"])
            }

            #[test]
            fn walk_dir_single_directory() -> VfsResult<()> {
                let root = create_root();
                root.join("baz")?.create_dir()?;
                assert_entries(&root, vec!["/baz"])
            }

            #[test]
            fn walk_dir_deep_directory() -> VfsResult<()> {
                let root = create_root();
                root.join("foo/bar/fizz/buzz")?.create_dir_all()?;
                assert_entries(
                    &root,
                    vec!["/foo", "/foo/bar", "/foo/bar/fizz", "/foo/bar/fizz/buzz"],
                )?;
                assert_entries(
                    &root.join("foo")?,
                    vec!["/foo/bar", "/foo/bar/fizz", "/foo/bar/fizz/buzz"],
                )
            }

            #[test]
            fn walk_dir_flat() -> VfsResult<()> {
                let root = create_root();
                root.join("foo/bar/foobar")?.create_dir_all()?;
                root.join("foo/baz")?.create_dir_all()?;
                root.join("foo/fizz")?.create_dir_all()?;
                root.join("foo/buzz")?.create_dir_all()?;
                root.join("foobar")?.create_dir_all()?;
                assert_entries(
                    &root,
                    vec![
                        "/foo",
                        "/foo/bar",
                        "/foo/bar/foobar",
                        "/foo/baz",
                        "/foo/buzz",
                        "/foo/fizz",
                        "/foobar",
                    ],
                )?;
                assert_entries(
                    &root.join("foo")?,
                    vec![
                        "/foo/bar",
                        "/foo/bar/foobar",
                        "/foo/baz",
                        "/foo/buzz",
                        "/foo/fizz",
                    ],
                )
            }

            #[test]
            fn walk_dir_file_in_dir() -> VfsResult<()> {
                let root = create_root();
                root.join("foo/bar")?.create_dir_all()?;
                root.join("foo/bar/foobar")?.create_file()?;
                assert_entries(&root, vec!["/foo", "/foo/bar", "/foo/bar/foobar"])?;
                assert_entries(&root.join("foo")?, vec!["/foo/bar", "/foo/bar/foobar"])
            }

            #[test]
            fn walk_dir_missing_path() -> VfsResult<()> {
                let root = create_root();
                let error_message = root
                    .join("foo")?
                    .walk_dir()
                    .expect_err("walk_dir")
                    .to_string();
                assert!(
                    error_message.starts_with("Could not read directory for '/foo'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn walk_dir_remove_directory_while_walking() -> VfsResult<()> {
                let root = create_root();
                root.join("foo")?.create_dir_all()?;
                let mut walker = root.walk_dir()?;
                assert_eq!(format!("{:?}", &walker), "WalkDirIterator[]");

                assert_eq!(walker.next().expect("foo")?.as_str(), "/foo");
                root.join("foo")?.remove_dir()?;
                let error_message = walker
                    .next()
                    .expect("no next")
                    .expect_err("walk_dir")
                    .to_string();
                assert!(
                    error_message.starts_with("Could not read directory for '/foo'"),
                    "Actual message: {}",
                    error_message
                );
                let next = walker.next();
                assert!(next.is_none(), "Got next: {:?}", next);
                Ok(())
            }

            #[test]
            fn read_to_string() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("foobar.txt")?;
                path.create_file()?.write_all(b"Hello World")?;
                assert_eq!(path.read_to_string()?, "Hello World");
                Ok(())
            }

            #[test]
            fn read_to_string_missing() -> VfsResult<()> {
                let root = create_root();
                let error_message = root.join("foobar.txt")?.read_to_string().expect_err("read_to_string").to_string();
                assert!(
                    error_message.starts_with("Could not get metadata for '/foobar.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn read_to_string_directory() -> VfsResult<()> {
                let root = create_root();
                root.join("foobar.txt")?.create_dir()?;
                let error_message = root.join("foobar.txt")?.read_to_string().expect_err("read_to_string").to_string();
                assert!(
                    error_message.starts_with("Could not read path for '/foobar.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn read_to_string_nonutf8() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("foobar.txt")?;
                path.create_file()?.write_all(&vec![0, 159, 146, 150])?;
                let error_message = path.read_to_string().expect_err("read_to_string").to_string();
                assert_eq!(
                    &error_message,
                    "Could not read path for '/foobar.txt': IO error: stream did not contain valid UTF-8"
                );
                Ok(())
            }

            #[test]
            fn copy_file() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;
                src.copy_file(&dest)?;
                assert_eq!(&dest.read_to_string()?, "Hello World");
                Ok(())
            }

            #[test]
            fn copy_file_not_exist() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;

                let error_message = src.copy_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not copy '/a.txt' to '/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn copy_file_dest_already_exist() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;
                dest.create_file()?.write_all(b"Hello World")?;

                let error_message = src.copy_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not copy '/a.txt' to '/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn copy_file_parent_directory_missing() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("x/b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.copy_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not copy '/a.txt' to '/x/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn copy_file_parent_directory_is_file() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("a.txt/b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.copy_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not copy '/a.txt' to '/a.txt/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn copy_file_to_root() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.copy_file(&root).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not copy '/a.txt' to ''"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn move_file() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;
                src.move_file(&dest)?;
                assert_eq!(&dest.read_to_string()?, "Hello World");
                assert!(!src.exists()?, "Source should not exist anymore");
                Ok(())
            }

            #[test]
            fn move_file_not_exist() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;

                let error_message = src.move_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not move '/a.txt' to '/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn move_file_dest_already_exist() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;
                dest.create_file()?.write_all(b"Hello World")?;

                let error_message = src.move_file(&dest).expect_err("move_file").to_string();
                assert!(
                    error_message.starts_with("Could not move '/a.txt' to '/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }
            #[test]
            fn move_file_parent_directory_missing() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("x/b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.move_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not move '/a.txt' to '/x/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn move_file_parent_directory_is_file() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                let dest = root.join("a.txt/b.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.move_file(&dest).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not move '/a.txt' to '/a.txt/b.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn move_file_to_root() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("a.txt")?;
                src.create_file()?.write_all(b"Hello World")?;

                let error_message = src.move_file(&root).expect_err("copy_file").to_string();
                assert!(
                    error_message.starts_with("Could not move '/a.txt' to ''"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn copy_dir() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.join("bar/biz/fizz/buzz")?.create_dir_all()?;
                src.join("bar/baz.txt")?.create_file()?.write_all(b"Hello World")?;

                let dest = root.join("foo2")?;
                assert_eq!(5, src.copy_dir(&dest)?);
                assert_eq!(&dest.join("bar/baz.txt")?.read_to_string()?, "Hello World");
                assert!(&dest.join("bar/biz/fizz/buzz")?.exists()?, "directory should exist");
                Ok(())
            }

            #[test]
            fn copy_dir_to_root() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.create_dir_all()?;
                let error_message = src.copy_dir(&root).expect_err("copy_dir").to_string();
                assert!(
                    error_message.starts_with("Could not copy directory '/foo' to ''"),
                    "Actual message: {}",
                    error_message
                );
               Ok(())
            }

            #[test]
            fn copy_dir_to_existing() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.create_dir_all()?;
                let dest = root.join("foo2")?;
                dest.create_dir_all()?;

                let error_message = src.copy_dir(&dest).expect_err("copy_dir").to_string();
                assert!(
                    error_message.starts_with("Could not copy directory '/foo' to '/foo2'"),
                    "Actual message: {}",
                    error_message
                );
               Ok(())
            }

            #[test]
            fn move_dir() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.join("bar/biz/fizz/buzz")?.create_dir_all()?;
                src.join("bar/baz.txt")?.create_file()?.write_all(b"Hello World")?;

                let dest = root.join("foo2")?;
                src.move_dir(&dest)?;
                assert_eq!(&dest.join("bar/baz.txt")?.read_to_string()?, "Hello World");
                assert!(&dest.join("bar/biz/fizz/buzz")?.exists()?, "directory should exist");
                assert!(!src.exists()?, "source directory should not exist");
                Ok(())
            }

            #[test]
            fn move_dir_to_root() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.create_dir_all()?;
                let error_message = src.move_dir(&root).expect_err("move_dir").to_string();
                assert!(
                    error_message.starts_with("Could not move directory '/foo' to ''"),
                    "Actual message: {}",
                    error_message
                );
               Ok(())
            }

            #[test]
            fn move_dir_to_existing() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;
                src.create_dir_all()?;
                let dest = root.join("foo2")?;
                dest.create_dir_all()?;

                let error_message = src.move_dir(&dest).expect_err("move_dir").to_string();
                assert!(
                    error_message.starts_with("Could not move directory '/foo' to '/foo2'"),
                    "Actual message: {}",
                    error_message
                );
               Ok(())
            }

            #[test]
            fn is_file_is_dir() -> VfsResult<()> {
                let root = create_root();
                let src = root.join("foo")?;

                assert!(!root.is_file()?);
                assert!(root.is_dir()?);

                assert!(!src.is_file()?);
                assert!(!src.is_dir()?);

                src.create_dir_all()?;
                assert!(!src.is_file()?);
                assert!(src.is_dir()?);

                src.remove_dir()?;
                assert!(!src.is_file()?);
                assert!(!src.is_dir()?);

                src.create_file()?;
                assert!(src.is_file()?);
                assert!(!src.is_dir()?);

                src.remove_file()?;
                assert!(!src.is_file()?);
                assert!(!src.is_dir()?);
               Ok(())
            }

        }
    };
}

/// Run readonly vfs test to check for conformance
#[macro_export]
macro_rules! test_vfs_readonly {
    ($root:expr) => {
        #[cfg(test)]
        mod vfs_tests_readonly {
            use super::*;
            use $crate::VfsFileType;
            use $crate::VfsPath;
            use $crate::VfsResult;

            fn create_root() -> VfsPath {
                $root.into()
            }

            #[test]
            fn vfs_can_be_created() {
                create_root();
            }

            #[test]
            fn read_file() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("a.txt").unwrap();
                {
                    let mut file = path.open_file().unwrap();
                    let mut string: String = String::new();
                    file.read_to_string(&mut string).unwrap();
                    assert_eq!(string, "a");
                }
                assert!(path.exists()?);
                let metadata = path.metadata()?;
                assert_eq!(metadata.len, 1);
                assert_eq!(metadata.file_type, VfsFileType::File);
                Ok(())
            }

            #[test]
            fn read_dir() {
                let root = create_root();
                let mut files: Vec<_> = root
                    .read_dir()
                    .unwrap()
                    .map(|path| path.as_str().to_string())
                    .collect();
                files.sort();
                assert_eq!(
                    files,
                    vec!["/a", "/a.txt", "/a.txt.dir", "/b.txt", "/c"]
                        .into_iter()
                        .map(String::from)
                        .collect::<Vec<_>>()
                );

                let mut files: Vec<_> = root
                    .join("a")
                    .unwrap()
                    .read_dir()
                    .unwrap()
                    .map(|path| path.as_str().to_string())
                    .collect();
                files.sort();
                assert_eq!(files, vec!["/a/d.txt".to_string(), "/a/x".to_string()]);
            }

            #[test]
            fn filename() {
                let root = create_root();
                assert_eq!(root.filename(), "");
                assert_eq!(
                    root.join("name.foo.bar").unwrap().filename(),
                    "name.foo.bar"
                );
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").unwrap().filename(),
                    "name.foo.bar"
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").unwrap().filename(),
                    ".name.foo.bar"
                );
                assert_eq!(root.join("fizz.buzz/foo.").unwrap().filename(), "foo.");
            }

            #[test]
            fn extension() {
                let root = create_root();
                assert_eq!(root.extension(), None, "root");
                assert_eq!(root.join("name").unwrap().extension(), None, "name");
                assert_eq!(
                    root.join("name.bar").unwrap().extension(),
                    Some("bar".to_string()),
                    "name.bar"
                );
                assert_eq!(
                    root.join("name.").unwrap().extension(),
                    Some("".to_string()),
                    "name."
                );
                assert_eq!(root.join(".name").unwrap().extension(), None, ".name");
                assert_eq!(
                    root.join(".name.bar").unwrap().extension(),
                    Some("bar".to_string()),
                    ".name.bar"
                );
                assert_eq!(
                    root.join(".name.").unwrap().extension(),
                    Some("".to_string()),
                    ".name."
                );
                assert_eq!(
                    root.join("name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/.name.foo.bar").unwrap().extension(),
                    Some("bar".to_string())
                );
                assert_eq!(
                    root.join("fizz.buzz/foo.").unwrap().extension(),
                    Some("".to_string())
                );
            }

            #[test]
            fn parent() {
                let root = create_root();
                assert_eq!(root.parent(), root.clone(), "root");
                assert_eq!(root.join("foo").unwrap().parent(), root.clone(), "foo");
                assert_eq!(
                    root.join("foo/bar").unwrap().parent(),
                    root.join("foo").unwrap(),
                    "foo/bar"
                );
                assert_eq!(
                    root.join("foo/bar/baz").unwrap().parent(),
                    root.join("foo/bar").unwrap(),
                    "foo/bar/baz"
                );
            }

            #[test]
            fn root() {
                let root = create_root();
                assert_eq!(root, root.root());
                assert_eq!(root.join("foo/bar").unwrap().root(), root.root());
            }

            #[test]
            fn eq() {
                let root = create_root();

                assert_eq!(root, root);
                assert_eq!(root.join("foo").unwrap(), root.join("foo").unwrap());
                assert_eq!(
                    root.join("foo").unwrap(),
                    root.join("foo/bar").unwrap().parent()
                );
                assert_eq!(root, root.join("foo").unwrap().parent());

                assert_ne!(root, root.join("foo").unwrap());
                assert_ne!(root.join("bar").unwrap(), root.join("foo").unwrap());

                let root2 = create_root();
                assert_ne!(root, root2);
                assert_ne!(root.join("foo").unwrap(), root2.join("foo").unwrap());
            }

            #[test]
            fn join() {
                let root = create_root();
                assert_eq!(root.join("").unwrap().as_str(), "");
                assert_eq!(root.join("foo").unwrap().join("").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo/bar").unwrap().as_str(), "/foo/bar");
                assert_eq!(root.join("foo/bar/baz").unwrap().as_str(), "/foo/bar/baz");
                assert_eq!(
                    root.join("foo").unwrap().join("bar").unwrap().as_str(),
                    "/foo/bar"
                );
                assert_eq!(root.join(".foo").unwrap().as_str(), "/.foo");
                assert_eq!(root.join("..foo").unwrap().as_str(), "/..foo");
                assert_eq!(root.join("foo.").unwrap().as_str(), "/foo.");
                assert_eq!(root.join("foo..").unwrap().as_str(), "/foo..");

                assert_eq!(root.join(".").unwrap().as_str(), "");
                assert_eq!(root.join("./foo").unwrap().as_str(), "/foo");
                assert_eq!(root.join("foo/.").unwrap().as_str(), "/foo");

                assert_eq!(root.join("foo/..").unwrap().as_str(), "");
                assert_eq!(root.join("foo").unwrap().join("..").unwrap().as_str(), "");
                assert_eq!(
                    root.join("foo/bar").unwrap().join("..").unwrap().as_str(),
                    "/foo"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("../baz")
                        .unwrap()
                        .as_str(),
                    "/foo/baz"
                );
                assert_eq!(root.join("foo/bar/../..").unwrap().as_str(), "");
                assert_eq!(root.join("foo/bar/../..").unwrap().as_str(), "");
                assert_eq!(root.join("foo/bar/baz/../..").unwrap().as_str(), "/foo");
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../..")
                        .unwrap()
                        .as_str(),
                    "/foo"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../../fizz")
                        .unwrap()
                        .as_str(),
                    "/foo/fizz"
                );
                assert_eq!(
                    root.join("foo/bar")
                        .unwrap()
                        .join("baz/../../fizz/..")
                        .unwrap()
                        .as_str(),
                    "/foo"
                );
                assert_eq!(root.join("..").unwrap(), root);
                assert_eq!(root.join("../foo").unwrap(), root.join("foo").unwrap());

                assert_eq!(root.join("/").unwrap(), root);
                assert_eq!(
                    root.join("foo/bar").unwrap().join("/baz").unwrap(),
                    root.join("baz").unwrap()
                );

                assert_eq!(
                    root.join("/foo/bar/baz").unwrap().join("../../..").unwrap(),
                    root
                );

                /// Utility function for templating the same error message
                // TODO: Maybe deduplicate this function
                fn invalid_path_message(path: &str) -> String {
                    format!("An error occured for '{}': The path is invalid", path)
                }

                assert_eq!(
                    root.join("foo/").unwrap_err().to_string(),
                    invalid_path_message("foo/"),
                    "foo/"
                );
            }

            #[test]
            fn walk_dir_root() -> VfsResult<()> {
                let root = create_root();

                assert_entries(
                    &root,
                    vec![
                        "/a",
                        "/a.txt",
                        "/a.txt.dir",
                        "/a.txt.dir/g.txt",
                        "/a/d.txt",
                        "/a/x",
                        "/a/x/y",
                        "/a/x/y/z",
                        "/b.txt",
                        "/c",
                        "/c/e.txt",
                    ],
                )
            }

            #[test]
            fn walk_dir_folder() -> VfsResult<()> {
                let root = create_root();

                assert_entries(
                    &root.join("a")?,
                    vec!["/a/d.txt", "/a/x", "/a/x/y", "/a/x/y/z"],
                )
            }

            #[test]
            fn walk_dir_nested() -> VfsResult<()> {
                let root = create_root();

                assert_entries(&root.join("a/x/y")?, vec!["/a/x/y/z"])
            }

            fn assert_entries(path: &VfsPath, expected: Vec<&str>) -> VfsResult<()> {
                let entries: Vec<VfsPath> = path.walk_dir()?.map(|path| path.unwrap()).collect();
                let mut paths = entries.iter().map(|x| x.as_str()).collect::<Vec<&str>>();
                paths.sort();
                assert_eq!(paths, expected);
                Ok(())
            }

            #[test]
            fn walk_dir_missing_path() -> VfsResult<()> {
                let root = create_root();
                let error_message = root
                    .join("foo")?
                    .walk_dir()
                    .expect_err("walk_dir")
                    .to_string();
                assert!(
                    error_message.starts_with("Could not read directory for '/foo'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn read_to_string() -> VfsResult<()> {
                let root = create_root();
                let path = root.join("a.txt")?;
                assert_eq!(path.read_to_string()?, "a");
                Ok(())
            }

            #[test]
            fn read_to_string_missing() -> VfsResult<()> {
                let root = create_root();
                let error_message = root
                    .join("foobar.txt")?
                    .read_to_string()
                    .expect_err("read_to_string")
                    .to_string();
                assert!(
                    error_message.starts_with("Could not get metadata for '/foobar.txt'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn read_to_string_directory() -> VfsResult<()> {
                let root = create_root();
                let error_message = root
                    .join("a")?
                    .read_to_string()
                    .expect_err("read_to_string")
                    .to_string();
                assert!(
                    error_message.starts_with("Could not read path for '/a'"),
                    "Actual message: {}",
                    error_message
                );
                Ok(())
            }

            #[test]
            fn is_file_is_dir() -> VfsResult<()> {
                let root = create_root();

                assert!(!root.is_file()?);
                assert!(root.is_dir()?);

                let missing = root.join("foo")?;

                assert!(!missing.is_file()?);
                assert!(!missing.is_dir()?);

                let a = root.join("a")?;
                assert!(!a.is_file()?);
                assert!(a.is_dir()?);

                let atxt = root.join("a.txt")?;
                assert!(atxt.is_file()?);
                assert!(!atxt.is_dir()?);

                Ok(())
            }
        }
    };
}
