use core::fmt;

/// Errors that can occur during CBOR serialization.
#[derive(Debug)]
pub enum CborError {
    ReflectError(String),
    UnsupportedType(String),
}

impl fmt::Display for CborError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CborError::ReflectError(msg) => write!(f, "reflect error: {msg}"),
            CborError::UnsupportedType(msg) => write!(f, "unsupported type: {msg}"),
        }
    }
}

impl std::error::Error for CborError {}
