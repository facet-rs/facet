//! Configuration value with span tracking throughout the tree.

use alloc::string::String;
use alloc::vec::Vec;

use facet::Facet;
use facet_reflect::Span;
use indexmap::IndexMap;

/// A value with source span information.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct Spanned<T> {
    /// The wrapped value.
    pub value: T,
    /// The source span (offset and length), if available.
    #[facet(metadata = "span")]
    pub span: Option<Span>,
}

/// A configuration value with span tracking at every level.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
#[facet(untagged)]
pub enum ConfigValue {
    /// A null value.
    Null(Spanned<()>),
    /// A boolean value.
    Bool(Spanned<bool>),
    /// An integer value.
    Integer(Spanned<i64>),
    /// A floating-point value.
    Float(Spanned<f64>),
    /// A string value.
    String(Spanned<String>),
    /// An array of values.
    Array(Spanned<Vec<ConfigValue>>),
    /// An object/map of key-value pairs.
    Object(Spanned<IndexMap<String, ConfigValue, std::hash::RandomState>>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;

    #[test]
    fn test_spanned_is_metadata_container() {
        let shape = <Spanned<i64> as Facet>::SHAPE;
        assert!(
            shape.is_metadata_container(),
            "Spanned<i64> should be a metadata container"
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
}
