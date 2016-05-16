
use ::{VFS, VPath, VMetadata};
use std::io::Result;

pub struct WalkDirIter<P: VPath> {
    todo: Vec<P>
}

pub fn walk_dir<P: VPath>(path: &P) -> WalkDirIter<P> {
    WalkDirIter {todo: vec![path.clone()]}
}

impl <P: VPath> Iterator for WalkDirIter<P> {
    type Item = P;
    // TODO: handle loops
    fn next(&mut self) -> Option<P> {
        let res = self.todo.pop();
        if let Some(ref path) = res {
            if let Ok(metadata) = path.metadata() {
                if metadata.is_dir() {
                    for entry in path.read_dir().unwrap() {
                        if let Ok(child) = entry {
                            self.todo.push(child);
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
    use ::VPath;
    use ::{VFS, VMetadata};
    use ::memory::{MemoryFS, MemoryPath};

    #[test]
    fn mkdir() {
        let fs = MemoryFS::new();
        let path = fs.path("/foo/bar/baz");
        path.mkdir().unwrap();
        let paths: Vec<String> = walk_dir(&fs.path("/foo")).map(|x: MemoryPath| x.path).collect();
        assert_eq!(paths, vec!["/foo", "/foo/bar", "/foo/bar/baz"]);
    }
}




