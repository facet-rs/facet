//! Errors from parsing YAML documents.

use alloc::{
    format,
    string::{String, ToString},
};

/// Any error
#[derive(Debug, Clone)]
pub struct AnyErr(pub String);

impl core::fmt::Display for AnyErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl core::error::Error for AnyErr {}

impl From<String> for AnyErr {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AnyErr {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<facet_reflect::ReflectError> for AnyErr {
    fn from(value: facet_reflect::ReflectError) -> Self {
        Self(format!("Reflection error: {value}"))
    }
}
