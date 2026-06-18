use core::fmt;

/// Errors that can occur during CBOR serialization or deserialization.
#[derive(Debug)]
pub enum CborError {
    ReflectError(String),
    UnsupportedType(String),
    UnexpectedEof,
    InvalidCbor(String),
    TypeMismatch { expected: String, got: String },
}

impl fmt::Display for CborError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CborError::ReflectError(msg) => write!(f, "reflect error: {msg}"),
            CborError::UnsupportedType(msg) => write!(f, "unsupported type: {msg}"),
            CborError::UnexpectedEof => write!(f, "unexpected end of input"),
            CborError::InvalidCbor(msg) => write!(f, "invalid CBOR: {msg}"),
            CborError::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for CborError {}
