//! Error types for serde_styx.

use std::fmt;

/// Error type for serde_styx operations.
#[derive(Debug)]
pub struct Error {
    msg: String,
}

impl Error {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }

    pub(crate) fn custom(msg: impl fmt::Display) -> Self {
        Self {
            msg: msg.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::custom(msg)
    }
}

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::custom(msg)
    }
}

/// Result type for serde_styx operations.
pub type Result<T> = std::result::Result<T, Error>;
