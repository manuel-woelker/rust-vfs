//! Error and Result definitions

use std::{error, fmt, io};

/// The error type of this crate
#[derive(Debug)]
pub struct VfsError {
    /// The path this error was encountered in
    path: String,
    /// The kind of error
    kind: VfsErrorKind,
    /// An optional human-readable string describing the context for this error
    ///
    /// If not provided, a generic context message is used
    context: String,
    /// The underlying error
    cause: Option<Box<VfsError>>,
}

/// The only way to create a VfsError is via a VfsErrorKind
///
/// This conversion implements certain normalizations
impl From<VfsErrorKind> for VfsError {
    fn from(kind: VfsErrorKind) -> Self {
        // Normalize the error here before we return it
        let kind = match kind {
            VfsErrorKind::IoError(io) => match io.kind() {
                io::ErrorKind::NotFound => VfsErrorKind::FileNotFound,
                // TODO: If MSRV changes to 1.53, enable this. Alternatively,
                //      if it's possible to #[cfg] just this line, try that
                // io::ErrorKind::Unsupported => VfsErrorKind::NotSupported,
                _ => VfsErrorKind::IoError(io),
            },
            // Remaining kinda are passed through as-is
            other => other,
        };

        Self {
            // TODO (Techno): See if this could be checked at compile-time to make sure the VFS abstraction
            //              never forgets to add a path. Might need a separate error type for FS impls vs VFS
            path: "PATH NOT FILLED BY VFS LAYER".into(),
            kind,
            context: "An error occured".into(),
            cause: None,
        }
    }
}

impl From<io::Error> for VfsError {
    fn from(err: io::Error) -> Self {
        Self::from(VfsErrorKind::IoError(err))
    }
}

impl VfsError {
    // Path filled by the VFS crate rather than the implementations
    pub(crate) fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn with_context<C, F>(mut self, context: F) -> Self
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.context = context().to_string();
        self
    }

    pub fn with_cause(mut self, cause: VfsError) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    pub fn kind(&self) -> &VfsErrorKind {
        &self.kind
    }

    pub fn path(&self) -> &String {
        &self.path
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} for '{}': {}", self.context, self.path, self.kind())
    }
}

impl error::Error for VfsError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        if let Some(cause) = &self.cause {
            Some(cause)
        } else {
            None
        }
    }
}

/// The kinds of errors that can occur
#[derive(Debug)]
pub enum VfsErrorKind {
    /// A generic I/O error
    ///
    /// Certain standard I/O errors are normalized to their VfsErrorKind counterparts
    IoError(io::Error),

    #[cfg(feature = "async-fs")]
    /// A generic async I/O error
    AsyncIoError(io::Error),

    /// The file or directory at the given path could not be found
    FileNotFound,

    /// The given path is invalid, e.g. because contains '.' or '..'
    InvalidPath,

    /// Generic error variant
    Other(String),

    /// There is already a directory at the given path
    DirectoryExists,

    /// There is already a file at the given path
    FileExists,

    /// Functionality not supported by this filesystem
    NotSupported,
}

impl fmt::Display for VfsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsErrorKind::IoError(cause) => {
                write!(f, "IO error: {}", cause)
            }
            #[cfg(feature = "async-fs")]
            VfsErrorKind::AsyncIoError(cause) => {
                write!(f, "Async IO error: {}", cause)
            }
            VfsErrorKind::FileNotFound => {
                write!(f, "The file or directory could not be found")
            }
            VfsErrorKind::InvalidPath => {
                write!(f, "The path is invalid")
            }
            VfsErrorKind::Other(message) => {
                write!(f, "FileSystem error: {}", message)
            }
            VfsErrorKind::NotSupported => {
                write!(f, "Functionality not supported by this filesystem")
            }
            VfsErrorKind::DirectoryExists => {
                write!(f, "Directory already exists")
            }
            VfsErrorKind::FileExists => {
                write!(f, "File already exists")
            }
        }
    }
}

/// The result type of this crate
pub type VfsResult<T> = std::result::Result<T, VfsError>;

#[cfg(test)]
mod tests {
    use crate::error::VfsErrorKind;
    use crate::{VfsError, VfsResult};

    fn produce_vfs_result() -> VfsResult<()> {
        Err(VfsError::from(VfsErrorKind::Other("Not a file".into())).with_path("foo"))
    }

    fn produce_anyhow_result() -> anyhow::Result<()> {
        Ok(produce_vfs_result()?)
    }

    #[test]
    fn anyhow_compatibility() {
        let result = produce_anyhow_result().unwrap_err();
        assert_eq!(
            result.to_string(),
            "An error occured for 'foo': FileSystem error: Not a file"
        )
    }
}
