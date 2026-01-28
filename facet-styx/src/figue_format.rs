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
        let mut value: ConfigValue =
            crate::from_str(contents).map_err(|e| ConfigFormatError::new(e.to_string()))?;

        // Remove @-prefixed keys (like @schema) which are metadata directives, not config values
        if let ConfigValue::Object(ref mut obj) = value {
            obj.value.retain(|key, _| !key.starts_with('@'));
        }

        Ok(value)
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

    #[test]
    fn test_styx_format_filters_schema_directive() {
        // @schema directives should be filtered out when parsing via ConfigFormat
        // because they are metadata, not configuration values
        let format = StyxFormat;
        let result = format.parse(
            r#"@schema {id crate:dibs@1, cli dibs}

db {
    crate reef-db
}
"#,
        );
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let value = result.unwrap();

        if let ConfigValue::Object(obj) = value {
            assert!(
                !obj.value.contains_key("@schema"),
                "@schema should be filtered out, got keys: {:?}",
                obj.value.keys().collect::<Vec<_>>()
            );
            assert!(
                obj.value.contains_key("db"),
                "Expected 'db' key, got: {:?}",
                obj.value.keys().collect::<Vec<_>>()
            );
        } else {
            panic!("Expected ConfigValue::Object, got: {:?}", value);
        }
    }
}
