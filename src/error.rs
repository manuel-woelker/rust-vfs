//! Error and Result definitions

use std::fmt::{Display, Formatter};
use std::io::Error;

/// The error type of this crate
#[derive(Debug)]
pub enum VfsError {
    /// A generic IO error
    IoError(std::io::Error),

    /// The file or directory at the given path could not be found
    FileNotFound {
        /// The path of the file not found
        path: String,
    },

    /// The given path is invalid, e.g. because contains '.' or '..'
    InvalidPath {
        /// The invalid path
        path: String,
    },

    /// Generic error variant
    Other {
        /// The generic error message
        message: String,
    },

    /// Generic error context, used for adding context to an error (like a path)
    WithContext {
        /// The context error message
        context: String,
        /// The underlying error
        cause: Box<VfsError>,
    },

    /// Functionality not supported by this filesystem
    NotSupported,
}

impl From<String> for VfsError {
    fn from(message: String) -> Self {
        VfsError::Other { message }
    }
}

impl From<std::io::Error> for VfsError {
    fn from(cause: Error) -> Self {
        VfsError::IoError(cause)
    }
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsError::IoError(cause) => {
                write!(f, "IO error: {}", cause)
            }
            VfsError::FileNotFound { path } => {
                write!(f, "The file or directory `{}` could not be found", path)
            }
            VfsError::InvalidPath { path } => {
                write!(f, "The path `{}` is invalid", path)
            }
            VfsError::Other { message } => {
                write!(f, "FileSystem error: {}", message)
            }
            VfsError::WithContext { context, cause } => {
                write!(f, "{}, cause: {}", context, cause)
            }
            VfsError::NotSupported => {
                write!(f, "Functionality not supported by this filesystem")
            }
        }
    }
}

/// The result type of this crate
pub type VfsResult<T> = std::result::Result<T, VfsError>;

/// Result extension trait to add context information
pub(crate) trait VfsResultExt<T> {
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
