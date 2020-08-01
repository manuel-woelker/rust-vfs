//! Utility functions for working with the virtual file systems

use {FileSystem, VfsPath, VfsMetadata};
use std::io::VfsResult;

pub struct WalkDirIter {
    todo: Vec<Box<VfsPath>>,
}

/// Recursively traverse the file system at the given path
pub fn walk_dir(path: &VfsPath) -> WalkDirIter {
    WalkDirIter { todo: vec![path.box_clone()] }
}

impl Iterator for WalkDirIter {
    type Item = Box<VfsPath>;
    // TODO: handle loops
    fn next(&mut self) -> Option<Box<VfsPath>> {
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
    use std::io::{Read, Write, Seek, SeekFrom, VfsResult};

    use super::*;
    use VfsPath;
    use {FileSystem, VfsMetadata};
    use memory::{MemoryFS, MemoryPath};

    #[test]
    fn mkdir() {
        let fs = MemoryFS::new();
        let path = fs.path("/foo/bar/baz");
        path.mkdir().unwrap();
        let paths: Vec<String> = walk_dir(&fs.path("/foo"))
                                     .map(|x: Box<VfsPath>| x.to_string().into_owned())
                                     .collect();
        assert_eq!(paths, vec!["/foo", "/foo/bar", "/foo/bar/baz"]);
    }
}
