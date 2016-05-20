
use {VFS, VPath, VMetadata};
use std::io::Result;

pub struct WalkDirIter {
    todo: Vec<Box<VPath>>,
}

pub fn walk_dir(path: &Box<VPath>) -> WalkDirIter {
    WalkDirIter { todo: vec![path.clone()] }
}

impl Iterator for WalkDirIter {
    type Item = Box<VPath>;
    // TODO: handle loops
    fn next(&mut self) -> Option<Box<VPath>> {
        let res = self.todo.pop();
        if let Some(ref path) = res {
            if let Ok(metadata) = path.metadata() {
                if metadata.is_dir() {
                    if let Ok(entries) = path.read_dir() {
                        for entry in entries {
                            if let Ok(child) = entry {
                                self.todo.push(child);
                            }
                        }
                    }
                }
            }
        }
        res
    }
}








    #[cfg(test)]
mod tests {
    use std::io::{Read, Write, Seek, SeekFrom, Result};

    use super::*;
    use VPath;
    use {VFS, VMetadata};
    use memory::{MemoryFS, MemoryPath};

    #[test]
    fn mkdir() {
        let fs = MemoryFS::new();
        let path = fs.path("/foo/bar/baz");
        path.mkdir().unwrap();
        let paths: Vec<String> = walk_dir(&(Box::new(fs.path("/foo")) as Box<VPath>))
                                     .map(|x: Box<VPath>| x.to_string().into_owned())
                                     .collect();
        assert_eq!(paths, vec!["/foo", "/foo/bar", "/foo/bar/baz"]);
    }
}
