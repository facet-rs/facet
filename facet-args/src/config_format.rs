//! Config file format abstraction for layered configuration.
//!
//! This module provides the [`ConfigFormat`] trait for pluggable config file parsing,
//! along with a built-in [`JsonFormat`] implementation.
//!
//! # Example
//!
//! ```rust,ignore
//! use facet_args::config_format::{ConfigFormat, JsonFormat};
//!
//! let format = JsonFormat;
//! let config = format.parse(r#"{"port": 8080}"#)?;
//! ```

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use camino::Utf8Path;

use crate::config_value::ConfigValue;
use crate::provenance::ConfigFile;

/// Error returned when parsing a config file fails.
#[derive(Debug)]
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
/// use facet_args::config_format::{ConfigFormat, ConfigFormatError};
/// use facet_args::config_value::ConfigValue;
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

/// A registry of config file formats.
///
/// This allows registering multiple formats and selecting the appropriate
/// one based on file extension.
#[derive(Default)]
pub struct FormatRegistry {
    formats: Vec<Box<dyn ConfigFormat>>,
}

impl FormatRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            formats: Vec::new(),
        }
    }

    /// Create a registry with the default JSON format.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(JsonFormat);
        registry
    }

    /// Register a new format.
    pub fn register<F: ConfigFormat + 'static>(&mut self, format: F) {
        self.formats.push(Box::new(format));
    }

    /// Find a format that handles the given file extension.
    ///
    /// The extension should not include the leading dot.
    pub fn find_by_extension(&self, extension: &str) -> Option<&dyn ConfigFormat> {
        let ext_lower = extension.to_lowercase();
        self.formats
            .iter()
            .find(|f| {
                f.extensions()
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&ext_lower))
            })
            .map(|f| f.as_ref())
    }

    /// Parse a config file, automatically selecting the format based on extension.
    pub fn parse(&self, contents: &str, extension: &str) -> Result<ConfigValue, ConfigFormatError> {
        let format = self.find_by_extension(extension).ok_or_else(|| {
            ConfigFormatError::new(format!("unsupported file extension: .{extension}"))
        })?;
        format.parse(contents)
    }

    /// Parse a config file and set provenance on all values.
    ///
    /// This is the preferred method for loading config files, as it ensures
    /// all values have proper provenance tracking for error messages.
    pub fn parse_file(
        &self,
        path: &Utf8Path,
        contents: &str,
    ) -> Result<ConfigValue, ConfigFormatError> {
        let extension = path.extension().unwrap_or("");
        let mut value = self.parse(contents, extension)?;

        // Create config file and set provenance recursively
        let file = Arc::new(ConfigFile::new(path, contents));
        value.set_file_provenance_recursive(&file, "");

        Ok(value)
    }

    /// Get all registered extensions.
    pub fn extensions(&self) -> Vec<&str> {
        self.formats
            .iter()
            .flat_map(|f| f.extensions().iter().copied())
            .collect()
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
    fn test_format_registry_new() {
        let registry = FormatRegistry::new();
        assert!(registry.find_by_extension("json").is_none());
    }

    #[test]
    fn test_format_registry_with_defaults() {
        let registry = FormatRegistry::with_defaults();
        assert!(registry.find_by_extension("json").is_some());
        assert!(registry.find_by_extension("JSON").is_some()); // case insensitive
        assert!(registry.find_by_extension("toml").is_none());
    }

    #[test]
    fn test_format_registry_register() {
        let mut registry = FormatRegistry::new();
        registry.register(JsonFormat);
        assert!(registry.find_by_extension("json").is_some());
    }

    #[test]
    fn test_format_registry_parse() {
        let registry = FormatRegistry::with_defaults();
        let result = registry.parse(r#"{"key": "value"}"#, "json");
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_registry_parse_unsupported() {
        let registry = FormatRegistry::with_defaults();
        let result = registry.parse("key = value", "toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unsupported"));
    }

    #[test]
    fn test_format_registry_extensions() {
        let registry = FormatRegistry::with_defaults();
        let extensions = registry.extensions();
        assert!(extensions.contains(&"json"));
    }

    #[test]
    fn test_config_format_error_display() {
        let err = ConfigFormatError::new("something went wrong");
        assert_eq!(err.to_string(), "something went wrong");

        let err = ConfigFormatError::with_offset("unexpected token", 42);
        assert_eq!(err.to_string(), "at byte 42: unexpected token");
    }

    #[test]
    fn test_format_registry_parse_file_with_provenance() {
        use crate::provenance::Provenance;

        let registry = FormatRegistry::with_defaults();
        let contents = r#"{"port": 8080, "host": "localhost"}"#;
        let path = Utf8Path::new("config.json");

        let result = registry.parse_file(path, contents);
        assert!(result.is_ok(), "parse_file failed: {:?}", result.err());

        let value = result.unwrap();
        if let ConfigValue::Object(obj) = value {
            // Root object should have provenance
            assert!(obj.provenance.is_some());
            if let Some(Provenance::File { file, key_path, .. }) = &obj.provenance {
                assert_eq!(file.path.as_str(), "config.json");
                assert_eq!(key_path, "");
            } else {
                panic!("expected File provenance");
            }

            // "port" field should have provenance with key_path
            if let Some(ConfigValue::Integer(port)) = obj.value.get("port") {
                assert!(port.provenance.is_some());
                if let Some(Provenance::File { key_path, .. }) = &port.provenance {
                    assert_eq!(key_path, "port");
                }
            } else {
                panic!("expected port field");
            }
        } else {
            panic!("expected object");
        }
    }
}
