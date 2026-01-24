//! Parser that converts `ConfigValue` trees into `ParseEvent` streams for deserialization.
//!
//! This allows us to deserialize `ConfigValue` into arbitrary Facet types using the
//! standard `facet-format` deserializer infrastructure.

use alloc::vec::Vec;

use facet_core::{Facet, Shape, Type, UserType};
use facet_format::{
    ContainerKind, FieldKey, FieldLocationHint, FormatDeserializer, FormatParser, ParseEvent,
    ScalarValue,
};
use facet_reflect::Span;
use indexmap::IndexMap;

use crate::config_value::{ConfigValue, Sourced};
use crate::provenance::Provenance;

/// Deserialize a `ConfigValue` into a Facet type.
///
/// This is the main entry point for converting merged configuration values
/// into strongly-typed structs.
///
/// # Example
///
/// ```ignore
/// use facet_args::config_value::ConfigValue;
/// use facet_args::config_value_parser::from_config_value;
///
/// #[derive(facet::Facet)]
/// struct Config {
///     port: u16,
///     host: String,
/// }
///
/// let config_value = /* ... merged from CLI/env/file ... */;
/// let config: Config = from_config_value(&config_value)?;
/// ```
pub fn from_config_value<T>(value: &ConfigValue) -> Result<T, ConfigValueDeserializeError>
where
    T: Facet<'static>,
{
    // First, fill in defaults for missing fields based on the target shape
    let value_with_defaults = fill_defaults_from_shape(value, T::SHAPE);

    let parser = ConfigValueParser::new(&value_with_defaults, T::SHAPE);
    let mut deserializer = FormatDeserializer::new_owned(parser);
    deserializer
        .deserialize()
        .map_err(ConfigValueDeserializeError::Deserialize)
}

/// Walk the shape and insert default values for missing fields in the ConfigValue tree.
/// This allows proper provenance tracking (defaults are marked as coming from Default).
fn fill_defaults_from_shape(value: &ConfigValue, shape: &'static Shape) -> ConfigValue {
    tracing::debug!(
        shape = shape.type_identifier,
        "fill_defaults_from_shape: entering"
    );
    match value {
        ConfigValue::Object(sourced) => {
            // Get struct fields from shape
            let fields = match &shape.ty {
                Type::User(UserType::Struct(s)) => &s.fields,
                _ => return value.clone(),
            };

            let mut new_map = sourced.value.clone();

            // For each field in the struct shape, check if it's missing in the ConfigValue
            for field in fields.iter() {
                if !new_map.contains_key(field.name) {
                    // Field is missing - insert a default
                    if let Some(default_value) = get_default_config_value(field) {
                        tracing::debug!(
                            field = field.name,
                            shape = shape.type_identifier,
                            ?default_value,
                            "fill_defaults_from_shape: inserting default for missing field"
                        );
                        new_map.insert(field.name.to_string(), default_value);
                    }
                }
            }

            // Recursively process nested objects
            for (key, val) in new_map.iter_mut() {
                // Find the corresponding field shape
                if let Some(field) = fields.iter().find(|f| f.name == key) {
                    *val = fill_defaults_from_shape(val, field.shape.get());
                }
            }

            let result = ConfigValue::Object(Sourced {
                value: new_map,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            });
            tracing::debug!(
                shape = shape.type_identifier,
                "fill_defaults_from_shape: completed Object"
            );
            result
        }
        ConfigValue::Array(sourced) => {
            // Recursively process array items
            // TODO: get element shape from array def
            let items: Vec<_> = sourced
                .value
                .iter()
                .map(|item| {
                    // For now, just pass through arrays without adding defaults
                    item.clone()
                })
                .collect();

            ConfigValue::Array(Sourced {
                value: items,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        _ => value.clone(),
    }
}

/// Get a default ConfigValue for a field, if one should be provided.
///
/// Only provides CLI-friendly defaults (false/0) for scalar fields that DON'T have
/// a #[facet(default)] attribute. For struct fields, always creates an empty Object
/// so the deserializer can enter and recursively fill defaults via facet-reflect.
fn get_default_config_value(field: &'static facet_core::Field) -> Option<ConfigValue> {
    use facet_core::ScalarType;

    let shape = field.shape.get();

    // For struct types, create an empty object (even if field has default attribute)
    // The deserializer needs the Object to enter and recursively fill defaults
    if let Type::User(UserType::Struct(_)) = &shape.ty {
        return Some(ConfigValue::Object(Sourced {
            value: IndexMap::new(),
            span: None,
            provenance: Some(Provenance::Default),
        }));
    }

    // For scalar types with explicit defaults, let facet-reflect handle them
    if field.default.is_some() {
        return None;
    }

    // For scalar types without explicit defaults, emit CLI-friendly defaults
    match shape.scalar_type() {
        Some(ScalarType::Bool) => {
            return Some(ConfigValue::Bool(Sourced {
                value: false,
                span: None,
                provenance: Some(Provenance::Default),
            }));
        }
        Some(
            ScalarType::U8
            | ScalarType::U16
            | ScalarType::U32
            | ScalarType::U64
            | ScalarType::U128
            | ScalarType::USize
            | ScalarType::I8
            | ScalarType::I16
            | ScalarType::I32
            | ScalarType::I64
            | ScalarType::I128
            | ScalarType::ISize,
        ) => {
            return Some(ConfigValue::Integer(Sourced {
                value: 0,
                span: None,
                provenance: Some(Provenance::Default),
            }));
        }
        _ => None,
    }
}

/// Errors that can occur during ConfigValue deserialization.
#[derive(Debug)]
pub enum ConfigValueDeserializeError {
    /// Error during deserialization.
    Deserialize(facet_format::DeserializeError<ConfigValueParseError>),
}

impl core::fmt::Display for ConfigValueDeserializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigValueDeserializeError::Deserialize(e) => write!(f, "{}", e),
        }
    }
}

impl core::error::Error for ConfigValueDeserializeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ConfigValueDeserializeError::Deserialize(e) => Some(e),
        }
    }
}

/// Parser that emits events from a `ConfigValue` tree.
pub struct ConfigValueParser<'input> {
    /// Stack of values to process.
    stack: Vec<StackFrame<'input>>,
    /// The most recent span (for error reporting).
    last_span: Option<Span>,
    /// Peeked event (cached for peek_event).
    peeked: Option<ParseEvent<'input>>,
}

/// A frame on the parsing stack.
enum StackFrame<'input> {
    /// Processing an object - emit key-value pairs.
    Object {
        entries: Vec<(&'input str, &'input ConfigValue)>,
        index: usize,
    },
    /// Processing an array - emit items in sequence.
    Array {
        items: &'input [ConfigValue],
        index: usize,
    },
    /// A single value to emit.
    Value(&'input ConfigValue),
}

impl<'input> ConfigValueParser<'input> {
    /// Create a new parser from a `ConfigValue`.
    pub fn new(value: &'input ConfigValue, _target_shape: &'static Shape) -> Self {
        Self {
            stack: alloc::vec![StackFrame::Value(value)],
            last_span: None,
            peeked: None,
        }
    }

    /// Get the most recent span.
    pub fn last_span(&self) -> Option<Span> {
        self.last_span
    }

    /// Update the last span from a `Sourced` wrapper.
    fn update_span<T>(&mut self, sourced: &Sourced<T>) {
        if let Some(span) = sourced.span {
            self.last_span = Some(span);
        }
    }
}

impl<'input> FormatParser<'input> for ConfigValueParser<'input> {
    type Error = ConfigValueParseError;
    type Probe<'a>
        = EmptyProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'input>>, Self::Error> {
        // If we have a peeked event, return it
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }

        loop {
            let frame = match self.stack.pop() {
                Some(f) => f,
                None => return Ok(None), // Done
            };

            match frame {
                StackFrame::Value(value) => {
                    return Ok(Some(self.emit_value(value)?));
                }
                StackFrame::Object { entries, index } => {
                    if index < entries.len() {
                        // Emit the next key-value pair
                        let (key, value) = entries[index];

                        // Push continuation for next entry
                        self.stack.push(StackFrame::Object {
                            entries: entries.clone(),
                            index: index + 1,
                        });

                        // Push value to process after key
                        self.stack.push(StackFrame::Value(value));

                        // Emit key
                        return Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                            key,
                            FieldLocationHint::KeyValue,
                        ))));
                    } else {
                        // Object entries done
                        return Ok(Some(ParseEvent::StructEnd));
                    }
                }
                StackFrame::Array { items, index } => {
                    if index < items.len() {
                        // Push continuation for next item
                        self.stack.push(StackFrame::Array {
                            items,
                            index: index + 1,
                        });

                        // Push item to process
                        self.stack.push(StackFrame::Value(&items[index]));

                        // Continue to process the value
                        continue;
                    } else {
                        // Array is done
                        return Ok(Some(ParseEvent::SequenceEnd));
                    }
                }
            }
        }
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'input>>, Self::Error> {
        if self.peeked.is_none() {
            self.peeked = self.next_event()?;
        }
        Ok(self.peeked.clone())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // Pop and discard the next value
        self.next_event()?;
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // We don't need probing for ConfigValue (it's already parsed)
        Ok(EmptyProbe)
    }
}

/// Empty probe stream for ConfigValueParser (we don't need evidence collection).
pub struct EmptyProbe;

impl<'de> facet_format::ProbeStream<'de> for EmptyProbe {
    type Error = ConfigValueParseError;

    fn next(&mut self) -> Result<Option<facet_format::FieldEvidence<'de>>, Self::Error> {
        Ok(None)
    }
}

/// Get struct fields that are missing from the ConfigValue and need CLI-friendly defaults.

impl<'input> ConfigValueParser<'input> {
    /// Emit an event for a single value.
    fn emit_value(
        &mut self,
        value: &'input ConfigValue,
    ) -> Result<ParseEvent<'input>, ConfigValueParseError> {
        match value {
            ConfigValue::Null(sourced) => {
                self.update_span(sourced);
                Ok(ParseEvent::Scalar(ScalarValue::Null))
            }
            ConfigValue::Bool(sourced) => {
                self.update_span(sourced);
                Ok(ParseEvent::Scalar(ScalarValue::Bool(sourced.value)))
            }
            ConfigValue::Integer(sourced) => {
                self.update_span(sourced);
                Ok(ParseEvent::Scalar(ScalarValue::I64(sourced.value)))
            }
            ConfigValue::Float(sourced) => {
                self.update_span(sourced);
                Ok(ParseEvent::Scalar(ScalarValue::F64(sourced.value)))
            }
            ConfigValue::String(sourced) => {
                self.update_span(sourced);
                Ok(ParseEvent::Scalar(ScalarValue::Str(
                    alloc::borrow::Cow::Borrowed(&sourced.value),
                )))
            }
            ConfigValue::Array(sourced) => {
                self.update_span(sourced);

                // Push array processing
                self.stack.push(StackFrame::Array {
                    items: &sourced.value,
                    index: 0,
                });

                Ok(ParseEvent::SequenceStart(ContainerKind::Array))
            }
            ConfigValue::Object(sourced) => {
                self.update_span(sourced);

                // Collect entries as borrowed slices
                let entries: Vec<(&str, &ConfigValue)> =
                    sourced.value.iter().map(|(k, v)| (k.as_str(), v)).collect();

                // Push object processing
                self.stack.push(StackFrame::Object { entries, index: 0 });

                Ok(ParseEvent::StructStart(ContainerKind::Object))
            }
        }
    }
}

/// Errors that can occur while parsing a `ConfigValue`.
#[derive(Debug)]
pub enum ConfigValueParseError {
    /// Generic error message.
    Message(alloc::string::String),
}

impl core::fmt::Display for ConfigValueParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigValueParseError::Message(msg) => write!(f, "{}", msg),
        }
    }
}

impl core::error::Error for ConfigValueParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_parse_null() {
        let value = ConfigValue::Null(Sourced::new(()));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::Scalar(ScalarValue::Null))));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, None));
    }

    #[test]
    fn test_parse_bool() {
        let value = ConfigValue::Bool(Sourced::new(true));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::Bool(true)))
        ));
    }

    #[test]
    fn test_parse_integer() {
        let value = ConfigValue::Integer(Sourced::new(42));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::I64(42)))
        ));
    }

    #[test]
    fn test_parse_string() {
        let value = ConfigValue::String(Sourced::new("hello".to_string()));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        if let Some(ParseEvent::Scalar(ScalarValue::Str(s))) = event {
            assert_eq!(s.as_ref(), "hello");
        } else {
            panic!("expected string scalar");
        }
    }

    #[test]
    fn test_parse_empty_array() {
        let value = ConfigValue::Array(Sourced::new(alloc::vec![]));
        let mut parser = ConfigValueParser::new(&value);

        // Should emit SequenceStart, then SequenceEnd
        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::SequenceStart(ContainerKind::Array))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::SequenceEnd)));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, None));
    }

    #[test]
    fn test_parse_array_with_items() {
        let value = ConfigValue::Array(Sourced::new(alloc::vec![
            ConfigValue::Integer(Sourced::new(1)),
            ConfigValue::Integer(Sourced::new(2)),
            ConfigValue::Integer(Sourced::new(3)),
        ]));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::SequenceStart(_))));

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::I64(1)))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::I64(2)))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::I64(3)))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::SequenceEnd)));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, None));
    }

    #[test]
    fn test_parse_empty_object() {
        let value = ConfigValue::Object(Sourced::new(indexmap::IndexMap::default()));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::StructStart(ContainerKind::Object))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::StructEnd)));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, None));
    }

    #[test]
    fn test_parse_object_with_fields() {
        let mut map = indexmap::IndexMap::default();
        map.insert(
            "name".to_string(),
            ConfigValue::String(Sourced::new("Alice".to_string())),
        );
        map.insert("age".to_string(), ConfigValue::Integer(Sourced::new(30)));

        let value = ConfigValue::Object(Sourced::new(map));
        let mut parser = ConfigValueParser::new(&value);

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::StructStart(_))));

        // First field
        let event = parser.next_event().unwrap();
        if let Some(ParseEvent::FieldKey(key)) = event {
            assert_eq!(key.name.as_ref().map(|s| s.as_ref()), Some("name"));
        } else {
            panic!("expected FieldKey");
        }

        let event = parser.next_event().unwrap();
        if let Some(ParseEvent::Scalar(ScalarValue::Str(s))) = event {
            assert_eq!(s.as_ref(), "Alice");
        } else {
            panic!("expected string value");
        }

        // Second field
        let event = parser.next_event().unwrap();
        if let Some(ParseEvent::FieldKey(key)) = event {
            assert_eq!(key.name.as_ref().map(|s| s.as_ref()), Some("age"));
        } else {
            panic!("expected FieldKey");
        }

        let event = parser.next_event().unwrap();
        assert!(matches!(
            event,
            Some(ParseEvent::Scalar(ScalarValue::I64(30)))
        ));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, Some(ParseEvent::StructEnd)));

        let event = parser.next_event().unwrap();
        assert!(matches!(event, None));
    }

    #[test]
    fn test_from_config_value_simple() {
        use facet::Facet;

        #[derive(Debug, Facet, PartialEq)]
        struct SimpleConfig {
            port: i64,
            enabled: bool,
        }

        let mut map = indexmap::IndexMap::default();
        map.insert("port".to_string(), ConfigValue::Integer(Sourced::new(8080)));
        map.insert("enabled".to_string(), ConfigValue::Bool(Sourced::new(true)));

        let value = ConfigValue::Object(Sourced::new(map));
        let config: SimpleConfig = from_config_value(&value).expect("should deserialize");

        assert_eq!(config.port, 8080);
        assert_eq!(config.enabled, true);
    }

    #[test]
    fn test_from_config_value_nested() {
        use facet::Facet;

        #[derive(Debug, Facet, PartialEq)]
        struct SmtpConfig {
            host: alloc::string::String,
            port: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct ServerConfig {
            port: i64,
            smtp: SmtpConfig,
        }

        // Build nested config value
        let mut smtp_map = indexmap::IndexMap::default();
        smtp_map.insert(
            "host".to_string(),
            ConfigValue::String(Sourced::new("smtp.example.com".to_string())),
        );
        smtp_map.insert("port".to_string(), ConfigValue::Integer(Sourced::new(587)));

        let mut server_map = indexmap::IndexMap::default();
        server_map.insert("port".to_string(), ConfigValue::Integer(Sourced::new(8080)));
        server_map.insert(
            "smtp".to_string(),
            ConfigValue::Object(Sourced::new(smtp_map)),
        );

        let value = ConfigValue::Object(Sourced::new(server_map));
        let config: ServerConfig = from_config_value(&value).expect("should deserialize");

        assert_eq!(config.port, 8080);
        assert_eq!(config.smtp.host, "smtp.example.com");
        assert_eq!(config.smtp.port, 587);
    }
}
