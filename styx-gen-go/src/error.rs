//! Error types for Go code generation.

use std::fmt;

/// Errors that can occur during code generation.
#[derive(Debug)]
pub enum GenError {
    /// I/O error
    Io(String),
    /// Type mapping error
    TypeMapping(String),
    /// Formatting error
    Format(String),
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::Io(msg) => write!(f, "I/O error: {}", msg),
            GenError::TypeMapping(msg) => write!(f, "type mapping error: {}", msg),
            GenError::Format(msg) => write!(f, "formatting error: {}", msg),
        }
    }
}

impl std::error::Error for GenError {}

impl From<GenError> for std::io::Error {
    fn from(err: GenError) -> Self {
        std::io::Error::new(std::io::ErrorKind::Other, err.to_string())
    }
}
