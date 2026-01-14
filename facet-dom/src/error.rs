//! Error types for DOM deserialization.

use std::fmt;

/// Error type for DOM deserialization.
#[derive(Debug)]
pub enum DomDeserializeError<E> {
    /// Parser error.
    Parser(E),

    /// Reflection error.
    Reflect(facet_reflect::ReflectError),

    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected.
        expected: &'static str,
    },

    /// Type mismatch.
    TypeMismatch {
        /// What was expected.
        expected: &'static str,
        /// What was found.
        got: String,
    },

    /// Unknown element.
    UnknownElement {
        /// The element tag name.
        tag: String,
    },

    /// Unknown attribute (when deny_unknown_fields is set).
    UnknownAttribute {
        /// The attribute name.
        name: String,
    },

    /// Missing required attribute.
    MissingAttribute {
        /// The attribute name.
        name: &'static str,
    },

    /// Unsupported type.
    Unsupported(String),
}

impl<E> From<facet_reflect::ReflectError> for DomDeserializeError<E> {
    fn from(e: facet_reflect::ReflectError) -> Self {
        Self::Reflect(e)
    }
}

impl<E> From<facet_dessert::DessertError> for DomDeserializeError<E> {
    fn from(e: facet_dessert::DessertError) -> Self {
        match e {
            facet_dessert::DessertError::Reflect { error, .. } => Self::Reflect(error),
            facet_dessert::DessertError::CannotBorrow { message } => {
                Self::Unsupported(message.into_owned())
            }
        }
    }
}

impl<E: std::error::Error> fmt::Display for DomDeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parser(e) => write!(f, "parser error: {e}"),
            Self::Reflect(e) => write!(f, "reflection error: {e}"),
            Self::UnexpectedEof { expected } => write!(f, "unexpected EOF, expected {expected}"),
            Self::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            Self::UnknownElement { tag } => write!(f, "unknown element: <{tag}>"),
            Self::UnknownAttribute { name } => write!(f, "unknown attribute: {name}"),
            Self::MissingAttribute { name } => write!(f, "missing required attribute: {name}"),
            Self::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DomDeserializeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parser(e) => Some(e),
            Self::Reflect(e) => Some(e),
            _ => None,
        }
    }
}
