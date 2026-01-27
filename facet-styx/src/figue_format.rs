//! Figue ConfigFormat implementation for Styx.
//!
//! This module provides [`StyxFormat`], which implements figue's [`ConfigFormat`] trait
//! for parsing Styx configuration files.
//!
//! # Example
//!
//! ```rust,ignore
//! use figue::{builder, FormatRegistry};
//! use facet_styx::StyxFormat;
//!
//! let config = builder::<MyConfig>()
//!     .unwrap()
//!     .file(|f| f.formats(FormatRegistry::new().with(StyxFormat)))
//!     .build();
//! ```

use figue::{ConfigFormat, ConfigFormatError, ConfigValue};

/// Styx config file format.
///
/// Parses `.styx` files using `facet-styx`, preserving span information
/// for error reporting.
#[derive(Debug, Clone, Copy, Default)]
pub struct StyxFormat;

impl ConfigFormat for StyxFormat {
    fn extensions(&self) -> &[&str] {
        &["styx"]
    }

    fn parse(&self, contents: &str) -> Result<ConfigValue, ConfigFormatError> {
        crate::from_str(contents).map_err(|e| ConfigFormatError::new(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_styx_format_extensions() {
        let format = StyxFormat;
        assert_eq!(format.extensions(), &["styx"]);
    }

    #[test]
    fn test_styx_format_parse_object() {
        let format = StyxFormat;
        let result = format.parse("port 8080\nhost localhost");
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let value = result.unwrap();
        assert!(matches!(value, ConfigValue::Object(_)));
    }

    #[test]
    fn test_styx_format_parse_nested() {
        let format = StyxFormat;
        let result = format.parse("smtp {\n  host mail.example.com\n  port 587\n}");
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
    }

    #[test]
    fn test_styx_format_parse_array() {
        let format = StyxFormat;
        let result = format.parse("items (one two three)");
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
    }

    #[test]
    fn test_styx_format_parse_error() {
        let format = StyxFormat;
        // Invalid syntax: unmatched closing brace
        let result = format.parse("}");
        assert!(result.is_err(), "expected error, got {:?}", result);
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
