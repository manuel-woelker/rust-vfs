use std::fmt::Display;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VfsError {
    #[error("data store disconnected")]
    IoError(#[from] std::io::Error),
    #[error("the file or directory `{path}` could not be found")]
    FileNotFound { path: String },
    #[error("other FileSystem error: {message}")]
    Other { message: String },
    #[error("{context}, cause: {cause}")]
    WithContext {
        context: String,
        #[source]
        cause: Box<VfsError>,
    },
}

pub type VfsResult<T> = std::result::Result<T, VfsError>;

pub trait VfsResultExt<T> {
    fn with_context<C, F>(self, f: F) -> VfsResult<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T> VfsResultExt<T> for VfsResult<T> {
    fn with_context<C, F>(self, context: F) -> VfsResult<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|error| VfsError::WithContext {
            context: context().to_string(),
            cause: Box::new(error),
        })
    }
}
