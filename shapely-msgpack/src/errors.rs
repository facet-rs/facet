use std::fmt;

#[derive(Debug)]
pub enum Error {
    UnexpectedType,
    InsufficientData,
    InvalidData,
    UnknownField(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnexpectedType => write!(f, "Unexpected MessagePack type"),
            Error::InsufficientData => write!(f, "Insufficient data to decode"),
            Error::InvalidData => write!(f, "Invalid MessagePack data"),
            Error::UnknownField(field) => write!(f, "Unknown field: {}", field),
        }
    }
}

impl std::error::Error for Error {}
