//! Configuration value with span tracking throughout the tree.

use std::string::String;
use std::sync::Arc;
use std::vec::Vec;

use facet::Facet;
use facet_reflect::Span;
use indexmap::IndexMap;

use crate::path::Path;
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

/// Type alias for the object map type used in ConfigValue.
/// Keys are original field names (from target_path / struct definition).
/// The ConfigValueParser translates to effective names when emitting events.
pub type ObjectMap = IndexMap<String, ConfigValue, std::hash::RandomState>;

/// An enum value with variant name and fields.
#[derive(Debug, Clone, Facet)]
pub struct EnumValue {
    /// The variant name (kebab-case for CLI, as provided for config files).
    pub variant: String,
    /// Fields of the variant (empty for unit variants).
    pub fields: ObjectMap,
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
    Object(Sourced<ObjectMap>),
    /// An enum value (subcommand or enum field in config).
    Enum(Sourced<EnumValue>),
}

pub trait ConfigValueVisitor {
    fn enter_value(&mut self, _path: &Path, _value: &ConfigValue) {}
    fn exit_value(&mut self, _path: &Path, _value: &ConfigValue) {}
}

/// Mutable visitor for transforming ConfigValue trees.
pub trait ConfigValueVisitorMut {
    /// Called for each value, allowing mutation.
    /// Called before recursing into children.
    fn visit_value(&mut self, _path: &Path, _value: &mut ConfigValue) {}
}

impl ConfigValue {
    /// Visit all ConfigValue nodes in depth-first order.
    pub fn visit(&self, visitor: &mut impl ConfigValueVisitor, path: &mut Path) {
        visitor.enter_value(path, self);
        match self {
            ConfigValue::Array(arr) => {
                for (i, item) in arr.value.iter().enumerate() {
                    path.push(i.to_string());
                    item.visit(visitor, path);
                    path.pop();
                }
            }
            ConfigValue::Object(obj) => {
                for (key, value) in &obj.value {
                    path.push(key.clone());
                    value.visit(visitor, path);
                    path.pop();
                }
            }
            _ => {}
        }
        visitor.exit_value(path, self);
    }

    /// Visit all ConfigValue nodes mutably in depth-first order.
    pub fn visit_mut(&mut self, visitor: &mut impl ConfigValueVisitorMut, path: &mut Path) {
        visitor.visit_value(path, self);
        match self {
            ConfigValue::Array(arr) => {
                for (i, item) in arr.value.iter_mut().enumerate() {
                    path.push(i.to_string());
                    item.visit_mut(visitor, path);
                    path.pop();
                }
            }
            ConfigValue::Object(obj) => {
                for (key, value) in obj.value.iter_mut() {
                    path.push(key.clone());
                    value.visit_mut(visitor, path);
                    path.pop();
                }
            }
            ConfigValue::Enum(e) => {
                for (key, value) in e.value.fields.iter_mut() {
                    path.push(key.clone());
                    value.visit_mut(visitor, path);
                    path.pop();
                }
            }
            _ => {}
        }
    }

    /// Get the span of this value (regardless of variant).
    pub fn span(&self) -> Option<Span> {
        match self {
            ConfigValue::Null(s) => s.span,
            ConfigValue::Bool(s) => s.span,
            ConfigValue::Integer(s) => s.span,
            ConfigValue::Float(s) => s.span,
            ConfigValue::String(s) => s.span,
            ConfigValue::Array(s) => s.span,
            ConfigValue::Object(s) => s.span,
            ConfigValue::Enum(s) => s.span,
        }
    }

    /// Get a mutable reference to the span of this value.
    pub fn span_mut(&mut self) -> &mut Option<Span> {
        match self {
            ConfigValue::Null(s) => &mut s.span,
            ConfigValue::Bool(s) => &mut s.span,
            ConfigValue::Integer(s) => &mut s.span,
            ConfigValue::Float(s) => &mut s.span,
            ConfigValue::String(s) => &mut s.span,
            ConfigValue::Array(s) => &mut s.span,
            ConfigValue::Object(s) => &mut s.span,
            ConfigValue::Enum(s) => &mut s.span,
        }
    }

    /// Get the provenance of this value (regardless of variant).
    pub fn provenance(&self) -> Option<&Provenance> {
        match self {
            ConfigValue::Null(s) => s.provenance.as_ref(),
            ConfigValue::Bool(s) => s.provenance.as_ref(),
            ConfigValue::Integer(s) => s.provenance.as_ref(),
            ConfigValue::Float(s) => s.provenance.as_ref(),
            ConfigValue::String(s) => s.provenance.as_ref(),
            ConfigValue::Array(s) => s.provenance.as_ref(),
            ConfigValue::Object(s) => s.provenance.as_ref(),
            ConfigValue::Enum(s) => s.provenance.as_ref(),
        }
    }

    /// Navigate to a value by path.
    pub fn get_by_path(&self, path: &Path) -> Option<&ConfigValue> {
        let mut current = self;
        for segment in path {
            match current {
                ConfigValue::Object(obj) => {
                    current = obj.value.get(segment)?;
                }
                ConfigValue::Array(arr) => {
                    let index: usize = segment.parse().ok()?;
                    current = arr.value.get(index)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    /// Navigate to a value by path (mutable).
    pub fn get_by_path_mut(&mut self, path: &Path) -> Option<&mut ConfigValue> {
        let mut current = self;
        for segment in path {
            match current {
                ConfigValue::Object(obj) => {
                    current = obj.value.get_mut(segment)?;
                }
                ConfigValue::Array(arr) => {
                    let index: usize = segment.parse().ok()?;
                    current = arr.value.get_mut(index)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

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
            ConfigValue::Enum(s) => {
                s.set_file_provenance(file.clone(), path);
                for (key, value) in s.value.fields.iter_mut() {
                    let key_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    value.set_file_provenance_recursive(file, &key_path);
                }
            }
        }
    }

    /// Extract the selected subcommand path from a ConfigValue.
    ///
    /// Given a ConfigValue representing parsed CLI args, this extracts the chain of
    /// selected subcommands. For example, if the user ran `myapp repo clone --help`,
    /// this would return `["Repo", "Clone"]` (using variant names, not CLI names).
    ///
    /// The `subcommand_field_name` is the name of the field that holds the subcommand
    /// at the root level (e.g., "command" for `#[facet(args::subcommand)] command: Command`).
    pub fn extract_subcommand_path(&self, subcommand_field_name: &str) -> Vec<String> {
        let mut path = Vec::new();
        self.extract_subcommand_path_recursive(subcommand_field_name, &mut path);
        path
    }

    fn extract_subcommand_path_recursive(&self, field_name: &str, path: &mut Vec<String>) {
        // Look for the subcommand field in this value
        let subcommand_value = match self {
            ConfigValue::Object(obj) => obj.value.get(field_name),
            _ => None,
        };

        if let Some(ConfigValue::Enum(enum_val)) = subcommand_value {
            // Add this variant to the path
            path.push(enum_val.value.variant.clone());

            // Check if this variant has a nested subcommand
            // Look for a field that itself contains an Enum (indicating nested subcommand)
            for (_key, value) in &enum_val.value.fields {
                if let ConfigValue::Enum(nested) = value {
                    // Found a nested subcommand - add it and recurse
                    path.push(nested.value.variant.clone());
                    // Check if this nested enum has further nested subcommands
                    for (_k, v) in &nested.value.fields {
                        if matches!(v, ConfigValue::Enum(_)) {
                            // Recurse with a synthetic object containing this enum
                            let mut synthetic = ObjectMap::default();
                            synthetic.insert("__nested".to_string(), v.clone());
                            let obj = ConfigValue::Object(Sourced::new(synthetic));
                            obj.extract_subcommand_path_recursive("__nested", path);
                        }
                    }
                    break;
                }
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

    #[test]
    fn test_sourced_unit_unwraps_to_scalar() {
        let shape = <Sourced<()> as Facet>::SHAPE;
        assert!(
            shape.is_metadata_container(),
            "Sourced<()> should be a metadata container"
        );

        let inner = facet_reflect::get_metadata_container_value_shape(shape);
        assert!(inner.is_some(), "should get inner shape from Sourced<()>");

        let inner = inner.unwrap();
        assert!(
            inner.scalar_type().is_some(),
            "inner shape should be scalar (unit): {:?}",
            inner.scalar_type()
        );
    }

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

    #[test]
    fn test_sourced_is_metadata_container() {
        let shape = <Sourced<i64> as Facet>::SHAPE;
        assert!(
            shape.is_metadata_container(),
            "Sourced<i64> should be a metadata container"
        );

        let inner = facet_reflect::get_metadata_container_value_shape(shape);
        assert!(inner.is_some(), "should get inner shape");

        let inner = inner.unwrap();
        assert!(
            inner.scalar_type().is_some(),
            "inner shape should be scalar (i64)"
        );
    }

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

    /// Helper to create a test ConfigValue object
    fn cv_object(fields: impl IntoIterator<Item = (&'static str, ConfigValue)>) -> ConfigValue {
        let map: ObjectMap = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Object(Sourced::new(map))
    }

    /// Helper to create a test ConfigValue enum
    fn cv_enum(
        variant: &str,
        fields: impl IntoIterator<Item = (&'static str, ConfigValue)>,
    ) -> ConfigValue {
        let map: ObjectMap = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Enum(Sourced::new(EnumValue {
            variant: variant.to_string(),
            fields: map,
        }))
    }

    fn cv_string(s: &str) -> ConfigValue {
        ConfigValue::String(Sourced::new(s.to_string()))
    }

    fn cv_bool(b: bool) -> ConfigValue {
        ConfigValue::Bool(Sourced::new(b))
    }

    #[test]
    fn test_extract_subcommand_path_single_level() {
        // Simulates: myapp install foo
        // Structure: { command: Enum("Install", { package: "foo" }) }
        let cli_value = cv_object([(
            "command",
            cv_enum("Install", [("package", cv_string("foo"))]),
        )]);

        let path = cli_value.extract_subcommand_path("command");
        assert_eq!(path, vec!["Install"]);
    }

    #[test]
    fn test_extract_subcommand_path_two_levels() {
        // Simulates: myapp repo clone https://example.com
        // Structure: { command: Enum("Repo", { action: Enum("Clone", { url: "..." }) }) }
        let cli_value = cv_object([(
            "command",
            cv_enum(
                "Repo",
                [(
                    "action",
                    cv_enum("Clone", [("url", cv_string("https://example.com"))]),
                )],
            ),
        )]);

        let path = cli_value.extract_subcommand_path("command");
        assert_eq!(path, vec!["Repo", "Clone"]);
    }

    #[test]
    fn test_extract_subcommand_path_three_levels() {
        // Simulates: myapp cloud aws s3 upload
        // Structure: { command: Enum("Cloud", { provider: Enum("Aws", { service: Enum("S3", { action: Enum("Upload", {}) }) }) }) }
        let cli_value = cv_object([(
            "command",
            cv_enum(
                "Cloud",
                [(
                    "provider",
                    cv_enum(
                        "Aws",
                        [(
                            "service",
                            cv_enum("S3", [("action", cv_enum("Upload", []))]),
                        )],
                    ),
                )],
            ),
        )]);

        let path = cli_value.extract_subcommand_path("command");
        assert_eq!(path, vec!["Cloud", "Aws", "S3", "Upload"]);
    }

    #[test]
    fn test_extract_subcommand_path_no_subcommand() {
        // No subcommand field
        let cli_value = cv_object([("verbose", cv_bool(true))]);

        let path = cli_value.extract_subcommand_path("command");
        assert!(path.is_empty());
    }

    #[test]
    fn test_extract_subcommand_path_with_other_fields() {
        // Simulates: myapp --verbose install foo
        // Structure: { verbose: true, command: Enum("Install", { package: "foo" }) }
        let cli_value = cv_object([
            ("verbose", cv_bool(true)),
            (
                "command",
                cv_enum("Install", [("package", cv_string("foo"))]),
            ),
        ]);

        let path = cli_value.extract_subcommand_path("command");
        assert_eq!(path, vec!["Install"]);
    }
}
