//! Config file format abstraction for layered configuration.
//!
//! This module is under active development and not yet wired into the main API.
//!
//! This module provides the [`ConfigFormat`] trait for pluggable config file parsing,
//! along with a built-in [`JsonFormat`] implementation.
//!
//! The [`FormatRegistry`](crate::layers::file::FormatRegistry) that manages multiple
//! formats is in the [`layers::file`](crate::layers::file) module.
//!
//! # Example
//!
//! ```rust,ignore
//! use figue::config_format::{ConfigFormat, JsonFormat};
//!
//! let format = JsonFormat;
//! let config = format.parse(r#"{"port": 8080}"#)?;
//! ```

use std::string::String;

use facet::Facet;

use crate::config_value::ConfigValue;

/// Error returned when parsing a config file fails.
#[derive(Facet, Debug)]
pub struct ConfigFormatError {
    /// Human-readable error message.
    pub message: String,

    /// Byte offset in the source where the error occurred, if known.
    pub offset: Option<usize>,
}

impl ConfigFormatError {
    /// Create a new error with just a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
        }
    }

    /// Create a new error with a message and source offset.
    pub fn with_offset(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
        }
    }
}

impl core::fmt::Display for ConfigFormatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(offset) = self.offset {
            write!(f, "at byte {}: {}", offset, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl core::error::Error for ConfigFormatError {}

/// Trait for config file format parsers.
///
/// Implementations of this trait can parse configuration files into [`ConfigValue`],
/// preserving source span information for rich error messages.
///
/// # Built-in Formats
///
/// - [`JsonFormat`] - JSON files (`.json`)
///
/// # Custom Formats
///
/// To support additional formats (TOML, YAML, etc.), implement this trait:
///
/// ```rust,ignore
/// use figue::config_format::{ConfigFormat, ConfigFormatError};
/// use figue::config_value::ConfigValue;
///
/// pub struct TomlFormat;
///
/// impl ConfigFormat for TomlFormat {
///     fn extensions(&self) -> &[&str] {
///         &["toml"]
///     }
///
///     fn parse(&self, contents: &str) -> Result<ConfigValue, ConfigFormatError> {
///         // Parse TOML and convert to ConfigValue with spans...
///         todo!()
///     }
/// }
/// ```
pub trait ConfigFormat: Send + Sync {
    /// File extensions this format handles (without the leading dot).
    ///
    /// For example, `["json"]` or `["yaml", "yml"]`.
    fn extensions(&self) -> &[&str];

    /// Parse file contents into a [`ConfigValue`] with span tracking.
    ///
    /// The implementation should preserve source locations in the returned
    /// `ConfigValue` tree so that error messages can point to the exact
    /// location in the config file.
    fn parse(&self, contents: &str) -> Result<ConfigValue, ConfigFormatError>;
}

/// JSON config file format.
///
/// Parses `.json` files using `facet-json`, preserving span information
/// for error reporting.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonFormat;

impl ConfigFormat for JsonFormat {
    fn extensions(&self) -> &[&str] {
        &["json"]
    }

    fn parse(&self, contents: &str) -> Result<ConfigValue, ConfigFormatError> {
        facet_json::from_str(contents).map_err(|e| ConfigFormatError::new(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_format_extensions() {
        let format = JsonFormat;
        assert_eq!(format.extensions(), &["json"]);
    }

    #[test]
    fn test_json_format_parse_object() {
        let format = JsonFormat;
        let result = format.parse(r#"{"port": 8080, "host": "localhost"}"#);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let value = result.unwrap();
        assert!(matches!(value, ConfigValue::Object(_)));
    }

    #[test]
    fn test_json_format_parse_nested() {
        let format = JsonFormat;
        let result = format.parse(r#"{"smtp": {"host": "mail.example.com", "port": 587}}"#);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
    }

    #[test]
    fn test_json_format_parse_array() {
        let format = JsonFormat;
        let result = format.parse(r#"["one", "two", "three"]"#);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let value = result.unwrap();
        assert!(matches!(value, ConfigValue::Array(_)));
    }

    #[test]
    fn test_json_format_parse_error() {
        let format = JsonFormat;
        let result = format.parse(r#"{"port": invalid}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn test_config_format_error_display() {
        let err = ConfigFormatError::new("something went wrong");
        assert_eq!(err.to_string(), "something went wrong");

        let err = ConfigFormatError::with_offset("unexpected token", 42);
        assert_eq!(err.to_string(), "at byte 42: unexpected token");
    }
}
