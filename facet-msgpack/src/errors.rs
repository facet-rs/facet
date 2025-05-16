use core::fmt;

use facet_reflect::ReflectError;

#[derive(Debug)]
#[non_exhaustive]
/// Errors that can occur during MessagePack encoding/decoding operations
pub enum Error<'shape> {
    /// Encountered a MessagePack type that doesn't match the expected type
    UnexpectedType,
    /// Not enough data available to decode a complete MessagePack value
    InsufficientData,
    /// The MessagePack data is malformed or corrupted
    InvalidData,
    /// Encountered a field name that isn't recognized
    UnknownField(String),
    /// Required field is missing from the input
    MissingField(String),
    /// Integer value is too large for the target type
    IntegerOverflow,
    /// Shape is not supported for deserialization
    UnsupportedShape(String),
    /// Type is not supported for deserialization
    UnsupportedType(String),
    /// Feature not yet implemented
    NotImplemented(String),
    /// Reflection error
    ReflectError(ReflectError<'shape>),
    /// Invalid enum variant
    InvalidEnum(String),
}

impl<'shape> From<ReflectError<'shape>> for Error<'shape> {
    fn from(err: ReflectError<'shape>) -> Self {
        Self::ReflectError(err)
    }
}

impl fmt::Display for Error<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnexpectedType => write!(f, "Unexpected MessagePack type"),
            Error::InsufficientData => write!(f, "Insufficient data to decode"),
            Error::InvalidData => write!(f, "Invalid MessagePack data"),
            Error::UnknownField(field) => write!(f, "Unknown field: {}", field),
            Error::MissingField(field) => write!(f, "Missing required field: {}", field),
            Error::IntegerOverflow => write!(f, "Integer value too large for target type"),
            Error::UnsupportedShape(shape) => {
                write!(f, "Unsupported shape for deserialization: {}", shape)
            }
            Error::UnsupportedType(typ) => {
                write!(f, "Unsupported type for deserialization: {}", typ)
            }
            Error::NotImplemented(feature) => {
                write!(f, "Feature not yet implemented: {}", feature)
            }
            Error::ReflectError(err) => {
                write!(f, "Reflection error: {}", err)
            }
            Error::InvalidEnum(message) => {
                write!(f, "Invalid enum variant: {}", message)
            }
        }
    }
}

impl<'shape> std::error::Error for Error<'shape> {}
