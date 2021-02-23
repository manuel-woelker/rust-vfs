use crate::VfsError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpsFSError {
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
        });
        VfsError::WithContext {
            context: String::from("HttpsFS"),
            cause: cause,
        }
    }
}
