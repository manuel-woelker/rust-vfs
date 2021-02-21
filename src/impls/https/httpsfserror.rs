use crate::VfsError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpsFSError {
    #[error("Serialization/Deserialization error: {0}")]
    SerDe(serde_json::Error),

    #[error("Network error: {0}")]
    Network(reqwest::Error),
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
        });
        VfsError::WithContext {
            context: String::from("HttpsFS"),
            cause: cause,
        }
    }
}
