use core::fmt;

use facet_core::{ScalarType, Shape};
use facet_path::Path;
use facet_reflect::ReflectError;

/// Errors that can occur during postcard serialization
#[derive(Debug)]
pub enum SerializeError {
    /// The output buffer is too small to hold the serialized data
    BufferTooSmall,
    /// Encountered a type that cannot be serialized to postcard format
    UnsupportedType(&'static str),
    /// Encountered a scalar type that postcard doesn't support
    UnsupportedScalar {
        /// The scalar type that was encountered
        scalar_type: ScalarType,
        /// The path in the data structure where this occurred
        path: Path,
        /// The root shape for formatting the path
        root_shape: &'static Shape,
    },
    /// Encountered an unknown scalar (scalar_type() returned None)
    UnknownScalar {
        /// The type name from the shape
        type_name: &'static str,
        /// The path in the data structure where this occurred
        path: Path,
        /// The root shape for formatting the path
        root_shape: &'static Shape,
    },
}

// Re-export for convenience
pub use facet_path::{Path as ErrorPath, PathStep as ErrorPathStep};

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::BufferTooSmall => write!(f, "Buffer too small for serialized data"),
            SerializeError::UnsupportedType(ty) => {
                write!(f, "Unsupported type for postcard serialization: {ty}")
            }
            SerializeError::UnsupportedScalar {
                scalar_type,
                path,
                root_shape,
            } => {
                let path_str = path.format_with_shape(root_shape);
                write!(
                    f,
                    "Unsupported scalar type {:?} at path: {}",
                    scalar_type, path_str
                )
            }
            SerializeError::UnknownScalar {
                type_name,
                path,
                root_shape,
            } => {
                let path_str = path.format_with_shape(root_shape);
                write!(
                    f,
                    "Unknown scalar type '{}' at path: {}",
                    type_name, path_str
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SerializeError {}

/// Errors that can occur during postcard deserialization
#[derive(Debug)]
pub enum DeserializeError {
    /// Not enough data available to decode a complete value
    UnexpectedEnd,
    /// The data is malformed or corrupted
    InvalidData,
    /// Integer value is too large for the target type
    IntegerOverflow,
    /// Encountered a field name that isn't recognized
    UnknownField,
    /// Required field is missing from the input
    MissingField(&'static str),
    /// Shape is not supported for deserialization
    UnsupportedShape,
    /// Type is not supported for deserialization
    UnsupportedType(&'static str),
    /// Invalid enum variant index
    InvalidVariant,
    /// Invalid boolean value (not 0 or 1)
    InvalidBool,
    /// Invalid UTF-8 in string
    InvalidUtf8,
    /// Reflection error from facet-reflect
    ReflectError(ReflectError),
    /// Sequence length mismatch
    LengthMismatch,
}

impl From<ReflectError> for DeserializeError {
    fn from(err: ReflectError) -> Self {
        Self::ReflectError(err)
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::UnexpectedEnd => write!(f, "Unexpected end of input"),
            DeserializeError::InvalidData => write!(f, "Invalid postcard data"),
            DeserializeError::IntegerOverflow => {
                write!(f, "Integer value too large for target type")
            }
            DeserializeError::UnknownField => write!(f, "Unknown field encountered"),
            DeserializeError::MissingField(field) => {
                write!(f, "Missing required field: {field}")
            }
            DeserializeError::UnsupportedShape => {
                write!(f, "Unsupported shape for deserialization")
            }
            DeserializeError::UnsupportedType(ty) => {
                write!(f, "Unsupported type for deserialization: {ty}")
            }
            DeserializeError::InvalidVariant => write!(f, "Invalid enum variant index"),
            DeserializeError::InvalidBool => write!(f, "Invalid boolean value (expected 0 or 1)"),
            DeserializeError::InvalidUtf8 => write!(f, "Invalid UTF-8 in string data"),
            DeserializeError::ReflectError(err) => write!(f, "Reflection error: {err}"),
            DeserializeError::LengthMismatch => write!(f, "Sequence length mismatch"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DeserializeError {}
