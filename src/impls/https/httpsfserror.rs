use crate::VfsError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpsFSError {
    /// A generic IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// The file or directory at the given path could not be found
    #[error("The file or directory `{path}` could not be found")]
    FileNotFound {
        /// The path of the file not found
        path: String,
    },

    /// The given path is invalid, e.g. because contains '.' or '..'
    #[error("The path `{path}` is invalid")]
    InvalidPath {
        /// The invalid path
        path: String,
    },

    /// Generic error variant
    #[error("FileSystem error: {message}")]
    Other {
        /// The generic error message
        message: String,
    },

    /// Generic error context, used for adding context to an error (like a path)
    #[error("{context}, cause: {cause}")]
    WithContext {
        /// The context error message
        context: String,
        /// The underlying error
        #[source]
        cause: Box<HttpsFSError>,
    },

    /// Functionality not supported by this filesystem
    #[error("Functionality not supported by this filesystem")]
    NotSupported,

    #[error("Serialization/Deserialization error: {0}")]
    SerDe(serde_json::Error),

    #[error("Network error: {0}")]
    Network(reqwest::Error),

    #[error("Authentification Error: {0}")]
    Auth(AuthError),

    #[error("Error while parsing a http header: {0}")]
    InvalidHeader(String),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Server didn't specified a authentification method.")]
    NoMethodSpecified,
    #[error("Authentification method, requested by server, is not supported.")]
    MethodNotSupported,
    #[error("No credential source set. (Use HttpsFS::builder().set_credential_provider()).")]
    NoCredentialSource,
    #[error("Faild. (Password or username wrong?)")]
    Failed,
}

impl From<serde_json::Error> for HttpsFSError {
    fn from(error: serde_json::Error) -> Self {
        HttpsFSError::SerDe(error)
    }
}

impl From<reqwest::Error> for HttpsFSError {
    fn from(error: reqwest::Error) -> Self {
        HttpsFSError::Network(error)
    }
}

impl From<HttpsFSError> for VfsError {
    fn from(error: HttpsFSError) -> Self {
        let cause = Box::new(match error {
            HttpsFSError::SerDe(_) => VfsError::Other {
                message: format!("{}", error),
            },
            HttpsFSError::Network(_) => VfsError::Other {
                message: format!("{}", error),
            },
            HttpsFSError::Auth(_) => VfsError::Other {
                message: format!("{}", error),
            },
            HttpsFSError::InvalidHeader(_) => VfsError::Other {
                message: format!("{}", error),
            },
            HttpsFSError::IoError(io) => {
                return VfsError::IoError(io);
            }
            HttpsFSError::FileNotFound { path } => {
                return VfsError::FileNotFound { path };
            }
            HttpsFSError::InvalidPath { path } => {
                return VfsError::InvalidPath { path };
            }
            HttpsFSError::Other { message } => {
                return VfsError::Other { message };
            }
            HttpsFSError::WithContext { context, cause } => {
                return VfsError::WithContext {
                    context,
                    cause: Box::new(VfsError::from(*cause)),
                };
            }
            HttpsFSError::NotSupported => return VfsError::NotSupported,
        });
        VfsError::WithContext {
            context: String::from("HttpsFS"),
            cause,
        }
    }
}

impl From<VfsError> for HttpsFSError {
    fn from(error: VfsError) -> Self {
        match error {
            VfsError::IoError(io) => HttpsFSError::IoError(io),
            VfsError::FileNotFound { path } => HttpsFSError::FileNotFound { path },
            VfsError::InvalidPath { path } => HttpsFSError::InvalidPath { path },
            VfsError::Other { message } => HttpsFSError::Other { message },
            VfsError::WithContext { context, cause } => HttpsFSError::WithContext {
                context,
                cause: Box::new(HttpsFSError::from(*cause)),
            },
            VfsError::NotSupported => HttpsFSError::NotSupported,
        }
    }
}
