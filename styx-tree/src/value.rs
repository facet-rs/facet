//! Value types for Styx documents.
//!
//! In Styx, every value has the same structure:
//! - An optional tag (`@name`)
//! - An optional payload (scalar, sequence, or object)
//!
//! This means:
//! - `@` is `Value { tag: None, payload: None }` (unit)
//! - `foo` is `Value { tag: None, payload: Some(Payload::Scalar(...)) }`
//! - `@string` is `Value { tag: Some("string"), payload: None }`
//! - `@seq(a b)` is `Value { tag: Some("seq"), payload: Some(Payload::Sequence(...)) }`
//! - `@object{...}` is `Value { tag: Some("object"), payload: Some(Payload::Object(...)) }`

use styx_parse::{ScalarKind, Span};

/// A Styx value: optional tag + optional payload.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Value {
    /// Optional tag (e.g., `string` for `@string`).
    pub tag: Option<Tag>,
    /// Optional payload.
    pub payload: Option<Payload>,
    /// Source span (None if programmatically constructed).
    pub span: Option<Span>,
}

/// A tag on a value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Tag {
    /// Tag name (without `@`).
    pub name: String,
    /// Source span.
    pub span: Option<Span>,
}

/// The payload of a value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(u8)]
pub enum Payload {
    /// Scalar text.
    Scalar(Scalar),
    /// Sequence `(a b c)`.
    Sequence(Sequence),
    /// Object `{key value, ...}`.
    Object(Object),
}

/// A scalar value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Scalar {
    /// The text content.
    pub text: String,
    /// What kind of scalar syntax was used.
    pub kind: ScalarKind,
    /// Source span (None if programmatically constructed).
    pub span: Option<Span>,
}

/// A sequence of values.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Sequence {
    /// Items in the sequence.
    pub items: Vec<Value>,
    /// Source span.
    pub span: Option<Span>,
}

/// An object (mapping of keys to values).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Object {
    /// Entries in the object.
    pub entries: Vec<Entry>,
    /// Source span.
    pub span: Option<Span>,
}

/// An entry in an object.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "facet", facet(skip_all_unless_truthy))]
pub struct Entry {
    /// The key.
    pub key: Value,
    /// The value.
    pub value: Value,
    /// Doc comment attached to this entry.
    pub doc_comment: Option<String>,
}

impl Value {
    /// Create a unit value (`@`).
    pub fn unit() -> Self {
        Value {
            tag: None,
            payload: None,
            span: None,
        }
    }

    /// Create a scalar value (no tag).
    pub fn scalar(text: impl Into<String>) -> Self {
        Value {
            tag: None,
            payload: Some(Payload::Scalar(Scalar {
                text: text.into(),
                kind: ScalarKind::Bare,
                span: None,
            })),
            span: None,
        }
    }

    /// Create a tagged value with no payload (e.g., `@string`).
    pub fn tag(name: impl Into<String>) -> Self {
        Value {
            tag: Some(Tag {
                name: name.into(),
                span: None,
            }),
            payload: None,
            span: None,
        }
    }

    /// Create a tagged value with a payload.
    pub fn tagged(name: impl Into<String>, payload: Value) -> Self {
        Value {
            tag: Some(Tag {
                name: name.into(),
                span: None,
            }),
            payload: payload.payload,
            span: None,
        }
    }

    /// Create an empty sequence (no tag).
    pub fn sequence() -> Self {
        Value {
            tag: None,
            payload: Some(Payload::Sequence(Sequence {
                items: Vec::new(),
                span: None,
            })),
            span: None,
        }
    }

    /// Create a sequence with items (no tag).
    pub fn seq(items: Vec<Value>) -> Self {
        Value {
            tag: None,
            payload: Some(Payload::Sequence(Sequence { items, span: None })),
            span: None,
        }
    }

    /// Create an empty object (no tag).
    pub fn object() -> Self {
        Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: Vec::new(),

                span: None,
            })),
            span: None,
        }
    }

    /// Check if this is unit (`@` - no tag, no payload).
    pub fn is_unit(&self) -> bool {
        self.tag.is_none() && self.payload.is_none()
    }

    /// Check if this is a `@schema` tag (used for schema declarations).
    pub fn is_schema_tag(&self) -> bool {
        self.tag_name() == Some("schema")
    }

    /// Get the tag name if present.
    pub fn tag_name(&self) -> Option<&str> {
        self.tag.as_ref().map(|t| t.name.as_str())
    }

    /// Get as string (for untagged scalars).
    pub fn as_str(&self) -> Option<&str> {
        if self.tag.is_some() {
            return None;
        }
        match &self.payload {
            Some(Payload::Scalar(s)) => Some(&s.text),
            _ => None,
        }
    }

    /// Get the scalar text regardless of tag.
    pub fn scalar_text(&self) -> Option<&str> {
        match &self.payload {
            Some(Payload::Scalar(s)) => Some(&s.text),
            _ => None,
        }
    }

    /// Get as object (payload only).
    pub fn as_object(&self) -> Option<&Object> {
        match &self.payload {
            Some(Payload::Object(o)) => Some(o),
            _ => None,
        }
    }

    /// Get as mutable object (payload only).
    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match &mut self.payload {
            Some(Payload::Object(o)) => Some(o),
            _ => None,
        }
    }

    /// Get as sequence (payload only).
    pub fn as_sequence(&self) -> Option<&Sequence> {
        match &self.payload {
            Some(Payload::Sequence(s)) => Some(s),
            _ => None,
        }
    }

    /// Get as mutable sequence (payload only).
    pub fn as_sequence_mut(&mut self) -> Option<&mut Sequence> {
        match &mut self.payload {
            Some(Payload::Sequence(s)) => Some(s),
            _ => None,
        }
    }

    /// Add a tag to this value.
    pub fn with_tag(mut self, name: impl Into<String>) -> Self {
        self.tag = Some(Tag {
            name: name.into(),
            span: None,
        });
        self
    }

    /// Get a value by path.
    ///
    /// Path segments are separated by `.`.
    /// Use `[n]` for sequence indexing.
    pub fn get(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return Some(self);
        }

        let (segment, rest) = split_path(path);

        match &self.payload {
            Some(Payload::Object(obj)) => {
                let value = obj.get(segment)?;
                if rest.is_empty() {
                    Some(value)
                } else {
                    value.get(rest)
                }
            }
            Some(Payload::Sequence(seq)) => {
                // Handle [n] indexing
                if segment.starts_with('[') && segment.ends_with(']') {
                    let idx: usize = segment[1..segment.len() - 1].parse().ok()?;
                    let value = seq.get(idx)?;
                    if rest.is_empty() {
                        Some(value)
                    } else {
                        value.get(rest)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get a mutable value by path.
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Value> {
        if path.is_empty() {
            return Some(self);
        }

        let (segment, rest) = split_path(path);

        match &mut self.payload {
            Some(Payload::Object(obj)) => {
                let value = obj.get_mut(segment)?;
                if rest.is_empty() {
                    Some(value)
                } else {
                    value.get_mut(rest)
                }
            }
            Some(Payload::Sequence(seq)) => {
                if segment.starts_with('[') && segment.ends_with(']') {
                    let idx: usize = segment[1..segment.len() - 1].parse().ok()?;
                    let value = seq.get_mut(idx)?;
                    if rest.is_empty() {
                        Some(value)
                    } else {
                        value.get_mut(rest)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Object {
    /// Get entry value by key (for untagged scalar keys).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.key.as_str() == Some(key))
            .map(|e| &e.value)
    }

    /// Get mutable entry value by key.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|e| e.key.as_str() == Some(key))
            .map(|e| &mut e.value)
    }

    /// Get entry by unit key (`@`).
    pub fn get_unit(&self) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.key.is_unit())
            .map(|e| &e.value)
    }

    /// Get mutable entry by unit key.
    pub fn get_unit_mut(&mut self) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|e| e.key.is_unit())
            .map(|e| &mut e.value)
    }

    /// Iterate over entries as (key, value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&Value, &Value)> {
        self.entries.iter().map(|e| (&e.key, &e.value))
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key.as_str() == Some(key))
    }

    /// Check if unit key exists.
    pub fn contains_unit_key(&self) -> bool {
        self.entries.iter().any(|e| e.key.is_unit())
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert or update an entry with a string key.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        let key_str = key.into();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.key.as_str() == Some(&key_str))
        {
            entry.value = value;
        } else {
            self.entries.push(Entry {
                key: Value::scalar(key_str),
                value,
                doc_comment: None,
            });
        }
    }

    /// Insert or update an entry with a unit key.
    pub fn insert_unit(&mut self, value: Value) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.key.is_unit()) {
            entry.value = value;
        } else {
            self.entries.push(Entry {
                key: Value::unit(),
                value,
                doc_comment: None,
            });
        }
    }
}

impl Sequence {
    /// Get item by index.
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.items.get(index)
    }

    /// Get mutable item by index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.items.get_mut(index)
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate over items.
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.items.iter()
    }

    /// Push an item.
    pub fn push(&mut self, value: Value) {
        self.items.push(value);
    }
}

/// Split path at first `.` or `[`.
fn split_path(path: &str) -> (&str, &str) {
    // Handle [n] at start
    if path.starts_with('[')
        && let Some(end) = path.find(']')
    {
        let segment = &path[..=end];
        let rest = &path[end + 1..];
        // Skip leading `.` in rest
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        return (segment, rest);
    }

    // Find first `.` or `[`
    let dot_pos = path.find('.');
    let bracket_pos = path.find('[');

    match (dot_pos, bracket_pos) {
        (Some(d), Some(b)) if b < d => (&path[..b], &path[b..]),
        (Some(d), _) => (&path[..d], &path[d + 1..]),
        (None, Some(b)) => (&path[..b], &path[b..]),
        (None, None) => (path, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_path() {
        assert_eq!(split_path("foo"), ("foo", ""));
        assert_eq!(split_path("foo.bar"), ("foo", "bar"));
        assert_eq!(split_path("foo.bar.baz"), ("foo", "bar.baz"));
        assert_eq!(split_path("[0]"), ("[0]", ""));
        assert_eq!(split_path("[0].foo"), ("[0]", "foo"));
        assert_eq!(split_path("foo[0]"), ("foo", "[0]"));
        assert_eq!(split_path("foo[0].bar"), ("foo", "[0].bar"));
    }

    #[test]
    fn test_unit_value() {
        let v = Value::unit();
        assert!(v.is_unit());
        assert!(v.tag.is_none());
        assert!(v.payload.is_none());
    }

    #[test]
    fn test_scalar_value() {
        let v = Value::scalar("hello");
        assert!(!v.is_unit());
        assert!(v.tag.is_none());
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_tagged_value() {
        let v = Value::tag("string");
        assert!(!v.is_unit());
        assert_eq!(v.tag_name(), Some("string"));
        assert!(v.payload.is_none());
    }

    #[test]
    fn test_object_get() {
        let mut obj = Object {
            entries: vec![Entry {
                key: Value::scalar("name"),
                value: Value::scalar("Alice"),
                doc_comment: None,
            }],

            span: None,
        };

        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(obj.get("missing"), None);

        obj.insert("age", Value::scalar("30"));
        assert_eq!(obj.get("age").and_then(|v| v.as_str()), Some("30"));
    }

    #[test]
    fn test_object_unit_key() {
        let mut obj = Object {
            entries: vec![],

            span: None,
        };

        obj.insert_unit(Value::scalar("root"));
        assert!(obj.contains_unit_key());
        assert_eq!(obj.get_unit().and_then(|v| v.as_str()), Some("root"));
    }

    #[test]
    fn test_value_path_access() {
        let value = Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: vec![
                    Entry {
                        key: Value::scalar("user"),
                        value: Value {
                            tag: None,
                            payload: Some(Payload::Object(Object {
                                entries: vec![Entry {
                                    key: Value::scalar("name"),
                                    value: Value::scalar("Alice"),
                                    doc_comment: None,
                                }],

                                span: None,
                            })),
                            span: None,
                        },
                        doc_comment: None,
                    },
                    Entry {
                        key: Value::scalar("items"),
                        value: Value {
                            tag: None,
                            payload: Some(Payload::Sequence(Sequence {
                                items: vec![
                                    Value::scalar("a"),
                                    Value::scalar("b"),
                                    Value::scalar("c"),
                                ],
                                span: None,
                            })),
                            span: None,
                        },
                        doc_comment: None,
                    },
                ],

                span: None,
            })),
            span: None,
        };

        assert_eq!(
            value.get("user.name").and_then(|v| v.as_str()),
            Some("Alice")
        );
        assert_eq!(value.get("items[0]").and_then(|v| v.as_str()), Some("a"));
        assert_eq!(value.get("items[2]").and_then(|v| v.as_str()), Some("c"));
        assert_eq!(value.get("missing"), None);
    }

    /// Test that Value can roundtrip through JSON via Facet.
    #[test]
    #[cfg(feature = "facet")]
    fn test_value_json_roundtrip() {
        // Build a complicated Value
        let value = Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: vec![
                    // Schema declaration
                    Entry {
                        key: Value::tag("schema"),
                        value: Value::scalar("my-schema.styx"),
                        doc_comment: Some("Schema for this config".to_string()),
                    },
                    // Simple scalar
                    Entry {
                        key: Value::scalar("name"),
                        value: Value::scalar("my-app"),
                        doc_comment: None,
                    },
                    // Tagged value
                    Entry {
                        key: Value::scalar("port"),
                        value: Value::tagged("int", Value::scalar("8080")),
                        doc_comment: None,
                    },
                    // Nested object
                    Entry {
                        key: Value::scalar("server"),
                        value: Value {
                            tag: None,
                            payload: Some(Payload::Object(Object {
                                entries: vec![
                                    Entry {
                                        key: Value::scalar("host"),
                                        value: Value::scalar("localhost"),
                                        doc_comment: None,
                                    },
                                    Entry {
                                        key: Value::scalar("tls"),
                                        value: Value {
                                            tag: Some(Tag {
                                                name: "object".to_string(),
                                                span: None,
                                            }),
                                            payload: Some(Payload::Object(Object {
                                                entries: vec![
                                                    Entry {
                                                        key: Value::scalar("cert"),
                                                        value: Value::scalar("/path/to/cert.pem"),
                                                        doc_comment: None,
                                                    },
                                                    Entry {
                                                        key: Value::scalar("key"),
                                                        value: Value::scalar("/path/to/key.pem"),
                                                        doc_comment: None,
                                                    },
                                                ],

                                                span: None,
                                            })),
                                            span: None,
                                        },
                                        doc_comment: Some("TLS configuration".to_string()),
                                    },
                                ],

                                span: None,
                            })),
                            span: None,
                        },
                        doc_comment: Some("Server settings".to_string()),
                    },
                    // Sequence
                    Entry {
                        key: Value::scalar("tags"),
                        value: Value {
                            tag: None,
                            payload: Some(Payload::Sequence(Sequence {
                                items: vec![
                                    Value::scalar("production"),
                                    Value::scalar("web"),
                                    Value::tagged("important", Value::unit()),
                                ],
                                span: None,
                            })),
                            span: None,
                        },
                        doc_comment: None,
                    },
                    // Unit value
                    Entry {
                        key: Value::scalar("debug"),
                        value: Value::unit(),
                        doc_comment: None,
                    },
                ],

                span: Some(Span::new(0, 100)),
            })),
            span: Some(Span::new(0, 100)),
        };

        // Serialize to JSON
        let json = facet_json::to_string(&value).expect("should serialize");
        eprintln!("JSON representation:\n{json}");

        // Deserialize back
        let roundtripped: Value = facet_json::from_str(&json).expect("should deserialize");

        // Verify equality
        assert_eq!(value, roundtripped, "Value should survive JSON roundtrip");
    }

    #[test]
    #[cfg(feature = "facet")]
    fn test_value_postcard_roundtrip() {
        // Simple scalar
        let v = Value::scalar("hello");
        let bytes = facet_postcard::to_vec(&v).expect("serialize scalar");
        let v2: Value = facet_postcard::from_slice(&bytes).expect("deserialize scalar");
        assert_eq!(v, v2);

        // Tagged value
        let v = Value::tag("string");
        let bytes = facet_postcard::to_vec(&v).expect("serialize tagged");
        let v2: Value = facet_postcard::from_slice(&bytes).expect("deserialize tagged");
        assert_eq!(v, v2);

        // Nested object (recursive structure)
        let v = Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: vec![
                    Entry {
                        key: Value::scalar("name"),
                        value: Value::scalar("Alice"),
                        doc_comment: None,
                    },
                    Entry {
                        key: Value::scalar("nested"),
                        value: Value {
                            tag: None,
                            payload: Some(Payload::Object(Object {
                                entries: vec![Entry {
                                    key: Value::scalar("inner"),
                                    value: Value::scalar("value"),
                                    doc_comment: None,
                                }],

                                span: None,
                            })),
                            span: None,
                        },
                        doc_comment: Some("A nested object".to_string()),
                    },
                ],

                span: None,
            })),
            span: None,
        };
        let bytes = facet_postcard::to_vec(&v).expect("serialize nested");
        let v2: Value = facet_postcard::from_slice(&bytes).expect("deserialize nested");
        assert_eq!(v, v2);

        // Sequence with values
        let v = Value::seq(vec![
            Value::scalar("a"),
            Value::scalar("b"),
            Value::tagged("important", Value::unit()),
        ]);
        let bytes = facet_postcard::to_vec(&v).expect("serialize sequence");
        let v2: Value = facet_postcard::from_slice(&bytes).expect("deserialize sequence");
        assert_eq!(v, v2);
    }
}
