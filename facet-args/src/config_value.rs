//! Configuration value with span tracking throughout the tree.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use facet::Facet;
use facet_reflect::Span;
use indexmap::IndexMap;

use crate::provenance::{ConfigFile, Provenance};

/// A value with full provenance tracking
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct Sourced<T> {
    /// The wrapped value.
    pub value: T,
    /// The source span (offset and length), populated during deserialization.
    #[facet(metadata = "span")]
    pub span: Option<Span>,
    /// Full provenance information (user-managed metadata, not filled by deserializer).
    #[facet(metadata = "other")]
    pub provenance: Option<Provenance>,
}

impl<T> Sourced<T> {
    /// Create a new Sourced value with no provenance.
    pub fn new(value: T) -> Self {
        Self {
            value,
            span: None,
            provenance: None,
        }
    }

    /// Create a new Sourced value with provenance.
    pub fn with_provenance(value: T, provenance: Provenance) -> Self {
        let span = match &provenance {
            Provenance::File { offset, len, .. } => Some(Span::new(*offset, *len)),
            _ => None,
        };
        Self {
            value,
            span,
            provenance: Some(provenance),
        }
    }

    /// Set the provenance from a config file, using the span if available.
    pub fn set_file_provenance(&mut self, file: Arc<ConfigFile>, key_path: impl Into<String>) {
        if let Some(span) = self.span {
            self.provenance = Some(Provenance::file(file, key_path, span.offset, span.len));
        }
    }
}

/// Information about a missing required field.
#[derive(Debug, Clone, Facet)]
pub struct MissingFieldInfo {
    /// Field name (e.g., "email" or "host")
    pub field_name: String,
    /// Full path (e.g., "config.server.host")
    pub field_path: String,
    /// Type name
    pub type_name: String,
    /// Documentation comment if available
    pub doc_comment: Option<String>,
}

/// A configuration value with full provenance tracking at every level.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
#[facet(untagged)]
pub enum ConfigValue {
    /// A null value.
    Null(Sourced<()>),
    /// A boolean value.
    Bool(Sourced<bool>),
    /// An integer value.
    Integer(Sourced<i64>),
    /// A floating-point value.
    Float(Sourced<f64>),
    /// A string value.
    String(Sourced<String>),
    /// An array of values.
    Array(Sourced<Vec<ConfigValue>>),
    /// An object/map of key-value pairs.
    Object(Sourced<IndexMap<String, ConfigValue, std::hash::RandomState>>),
    /// A missing required field (used for error reporting)
    Missing(MissingFieldInfo),
}

impl ConfigValue {
    /// Recursively set file provenance on this value and all nested values.
    ///
    /// This should be called after parsing a config file to populate provenance
    /// on the entire tree.
    pub fn set_file_provenance_recursive(&mut self, file: &Arc<ConfigFile>, path: &str) {
        match self {
            ConfigValue::Null(s) => s.set_file_provenance(file.clone(), path),
            ConfigValue::Bool(s) => s.set_file_provenance(file.clone(), path),
            ConfigValue::Integer(s) => s.set_file_provenance(file.clone(), path),
            ConfigValue::Float(s) => s.set_file_provenance(file.clone(), path),
            ConfigValue::String(s) => s.set_file_provenance(file.clone(), path),
            ConfigValue::Array(s) => {
                s.set_file_provenance(file.clone(), path);
                for (i, item) in s.value.iter_mut().enumerate() {
                    let item_path = if path.is_empty() {
                        format!("{i}")
                    } else {
                        format!("{path}[{i}]")
                    };
                    item.set_file_provenance_recursive(file, &item_path);
                }
            }
            ConfigValue::Object(s) => {
                s.set_file_provenance(file.clone(), path);
                for (key, value) in s.value.iter_mut() {
                    let key_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    value.set_file_provenance_recursive(file, &key_path);
                }
            }
            ConfigValue::Missing(_) => {
                // Missing values don't have file provenance - they're synthetic markers
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;

    #[test]
    fn test_unit_is_scalar() {
        let shape = <() as Facet>::SHAPE;
        assert!(
            shape.scalar_type().is_some(),
            "() should have a scalar type: {:?}",
            shape.scalar_type()
        );
    }

    // TODO: Spanned type is not defined in this module
    // #[test]
    // fn test_spanned_unit_unwraps_to_scalar() {
    //     let shape = <Spanned<()> as Facet>::SHAPE;
    //     assert!(
    //         shape.is_metadata_container(),
    //         "Spanned<()> should be a metadata container"
    //     );

    //     let inner = facet_reflect::get_metadata_container_value_shape(shape);
    //     assert!(inner.is_some(), "should get inner shape from Spanned<()>");

    //     let inner = inner.unwrap();
    //     assert!(
    //         inner.scalar_type().is_some(),
    //         "inner shape should be scalar (unit): {:?}",
    //         inner.scalar_type()
    //     );
    // }

    #[test]
    fn test_null_variant_classification() {
        use facet_core::Facet;
        use facet_solver::VariantsByFormat;

        let shape = <ConfigValue as Facet>::SHAPE;
        let variants = VariantsByFormat::from_shape(shape).expect("should get variants");

        // Check that we have a scalar variant for Null
        let null_variant = variants
            .scalar_variants
            .iter()
            .find(|(v, _)| v.name == "Null");
        assert!(
            null_variant.is_some(),
            "Null should be in scalar_variants. Found: {:?}",
            variants
                .scalar_variants
                .iter()
                .map(|(v, _)| v.name)
                .collect::<Vec<_>>()
        );

        let (_, inner_shape) = null_variant.unwrap();
        assert!(
            inner_shape.scalar_type().is_some(),
            "Null's inner_shape should have a scalar type: {:?}",
            inner_shape.scalar_type()
        );
        assert_eq!(
            inner_shape.scalar_type(),
            Some(facet_core::ScalarType::Unit),
            "Null's inner_shape should be Unit type"
        );
    }

    // TODO: Spanned type is not defined in this module
    // #[test]
    // fn test_spanned_is_metadata_container() {
    //     let shape = <Spanned<i64> as Facet>::SHAPE;
    //     assert!(
    //         shape.is_metadata_container(),
    //         "Spanned<i64> should be a metadata container"
    //     );

    //     let inner = facet_reflect::get_metadata_container_value_shape(shape);
    //     assert!(inner.is_some(), "should get inner shape");

    //     let inner = inner.unwrap();
    //     assert!(
    //         inner.scalar_type().is_some(),
    //         "inner shape should be scalar (i64)"
    //     );
    // }

    #[test]
    fn test_parse_null() {
        let json = "null";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse null");
        assert!(matches!(value, ConfigValue::Null(_)));
    }

    #[test]
    fn test_parse_bool_true() {
        let json = "true";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse true");
        assert!(matches!(value, ConfigValue::Bool(ref s) if s.value));
    }

    #[test]
    fn test_parse_bool_false() {
        let json = "false";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse false");
        assert!(matches!(value, ConfigValue::Bool(ref s) if !s.value));
    }

    #[test]
    fn test_parse_integer() {
        let json = "42";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse integer");
        assert!(matches!(value, ConfigValue::Integer(ref s) if s.value == 42));
    }

    #[test]
    fn test_parse_negative_integer() {
        let json = "-123";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse negative integer");
        assert!(matches!(value, ConfigValue::Integer(ref s) if s.value == -123));
    }

    #[test]
    fn test_parse_float() {
        let json = "3.5";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse float");
        assert!(matches!(value, ConfigValue::Float(ref s) if (s.value - 3.5).abs() < 0.001));
    }

    #[test]
    fn test_parse_string() {
        let json = r#""hello""#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse string");
        assert!(matches!(value, ConfigValue::String(ref s) if s.value == "hello"));
    }

    #[test]
    fn test_parse_empty_string() {
        let json = r#""""#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse empty string");
        assert!(matches!(value, ConfigValue::String(ref s) if s.value.is_empty()));
    }

    #[test]
    fn test_parse_array() {
        let json = r#"[1, 2, 3]"#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse array");
        assert!(matches!(value, ConfigValue::Array(ref s) if s.value.len() == 3));
    }

    #[test]
    fn test_parse_empty_array() {
        let json = "[]";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse empty array");
        assert!(matches!(value, ConfigValue::Array(ref s) if s.value.is_empty()));
    }

    #[test]
    fn test_parse_object() {
        let json = r#"{"name": "hello", "count": 42}"#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse object");
        assert!(matches!(value, ConfigValue::Object(_)));
    }

    #[test]
    fn test_parse_empty_object() {
        let json = "{}";
        let value: ConfigValue = facet_json::from_str(json).expect("should parse empty object");
        assert!(matches!(value, ConfigValue::Object(ref s) if s.value.is_empty()));
    }

    #[test]
    fn test_parse_nested_object() {
        let json = r#"{"outer": {"inner": 42}}"#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse nested object");
        assert!(matches!(value, ConfigValue::Object(_)));
    }

    #[test]
    fn test_parse_mixed_array() {
        let json = r#"[1, "two", true, null]"#;
        let value: ConfigValue = facet_json::from_str(json).expect("should parse mixed array");
        if let ConfigValue::Array(arr) = value {
            assert_eq!(arr.value.len(), 4);
            assert!(matches!(arr.value[0], ConfigValue::Integer(_)));
            assert!(matches!(arr.value[1], ConfigValue::String(_)));
            assert!(matches!(arr.value[2], ConfigValue::Bool(_)));
            assert!(matches!(arr.value[3], ConfigValue::Null(_)));
        } else {
            panic!("expected array");
        }
    }

    // === Sourced<T> tests (experimental provenance tracking) ===

    #[test]
    fn test_sourced_deserialize_integer() {
        // Test that Sourced<i64> deserializes correctly with #[facet(skip)] on provenance
        let json = "42";
        let result: Result<Sourced<i64>, _> = facet_json::from_str(json);
        assert!(
            result.is_ok(),
            "Sourced<i64> should deserialize: {:?}",
            result.err()
        );
        let sourced = result.unwrap();
        assert_eq!(sourced.value, 42);
        assert!(
            sourced.provenance.is_none(),
            "provenance should be None after deserialization"
        );
    }

    #[test]
    fn test_sourced_deserialize_string() {
        let json = r#""hello""#;
        let result: Result<Sourced<String>, _> = facet_json::from_str(json);
        assert!(
            result.is_ok(),
            "Sourced<String> should deserialize: {:?}",
            result.err()
        );
        let sourced = result.unwrap();
        assert_eq!(sourced.value, "hello");
        assert!(sourced.provenance.is_none());
    }

    #[test]
    fn test_sourced_with_provenance() {
        let file = Arc::new(ConfigFile::new("config.json", r#"{"port": 8080}"#));
        let sourced = Sourced::with_provenance(8080i64, Provenance::file(file, "port", 9, 4));

        assert_eq!(sourced.value, 8080);
        assert!(sourced.provenance.is_some());
        assert!(sourced.provenance.as_ref().unwrap().is_file());
        // Span should be derived from provenance
        assert_eq!(sourced.span, Some(Span::new(9, 4)));
    }

    #[test]
    fn test_sourced_set_file_provenance() {
        // Simulate what happens after deserialization: we have a span, then add file provenance
        let mut sourced = Sourced {
            value: 8080i64,
            span: Some(Span::new(9, 4)),
            provenance: None,
        };

        let file = Arc::new(ConfigFile::new("config.json", r#"{"port": 8080}"#));
        sourced.set_file_provenance(file.clone(), "port");

        assert!(sourced.provenance.is_some());
        if let Some(Provenance::File {
            file: f,
            key_path,
            offset,
            len,
        }) = &sourced.provenance
        {
            assert_eq!(f.path.as_str(), "config.json");
            assert_eq!(key_path, "port");
            assert_eq!(*offset, 9);
            assert_eq!(*len, 4);
        } else {
            panic!("expected File provenance");
        }
    }

    #[test]
    fn test_set_file_provenance_recursive() {
        // Parse a nested JSON object
        let json = r#"{"port": 8080, "smtp": {"host": "mail.example.com", "port": 587}}"#;
        let mut value: ConfigValue = facet_json::from_str(json).expect("should parse");

        // Create a config file and set provenance recursively
        let file = Arc::new(ConfigFile::new("config.json", json));
        value.set_file_provenance_recursive(&file, "");

        // Check root object has provenance
        if let ConfigValue::Object(ref obj) = value {
            assert!(obj.provenance.is_some());

            // Check "port" has provenance with correct key_path
            if let Some(ConfigValue::Integer(port)) = obj.value.get("port") {
                assert!(port.provenance.is_some());
                if let Some(Provenance::File { key_path, .. }) = &port.provenance {
                    assert_eq!(key_path, "port");
                } else {
                    panic!("expected File provenance for port");
                }
            } else {
                panic!("expected port field");
            }

            // Check nested "smtp.host" has correct key_path
            if let Some(ConfigValue::Object(smtp)) = obj.value.get("smtp") {
                assert!(smtp.provenance.is_some());
                if let Some(Provenance::File { key_path, .. }) = &smtp.provenance {
                    assert_eq!(key_path, "smtp");
                }

                if let Some(ConfigValue::String(host)) = smtp.value.get("host") {
                    assert!(host.provenance.is_some());
                    if let Some(Provenance::File { key_path, .. }) = &host.provenance {
                        assert_eq!(key_path, "smtp.host");
                    } else {
                        panic!("expected File provenance for smtp.host");
                    }
                } else {
                    panic!("expected smtp.host field");
                }
            } else {
                panic!("expected smtp field");
            }
        } else {
            panic!("expected object");
        }
    }
}
