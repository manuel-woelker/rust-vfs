use crate::{SeekAndRead, VfsMetadata, VfsResult};
use std::fmt::Debug;
use std::io::Write;

pub trait FileSystem: Debug + Sync + Send {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>>;
    fn create_dir(&self, path: &str) -> VfsResult<()>;
    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>>;
    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>>;
    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;
    fn exists(&self, path: &str) -> bool;
    fn remove_file(&self, path: &str) -> VfsResult<()>;
    fn remove_dir(&self, path: &str) -> VfsResult<()>;
}
