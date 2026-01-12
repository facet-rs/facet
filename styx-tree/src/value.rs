//! Value types for Styx documents.

use styx_parse::{ScalarKind, Separator, Span};

/// A Styx value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Scalar text (from any scalar syntax).
    Scalar(Scalar),

    /// Unit value (`@`).
    Unit,

    /// Tagged value (`@name` or `@name payload`).
    Tagged(Tagged),

    /// Sequence `(a b c)`.
    Sequence(Sequence),

    /// Object `{key value, ...}`.
    Object(Object),
}

/// A scalar value.
#[derive(Debug, Clone, PartialEq)]
pub struct Scalar {
    /// The text content.
    pub text: String,
    /// What kind of scalar syntax was used.
    pub kind: ScalarKind,
    /// Source span (None if programmatically constructed).
    pub span: Option<Span>,
}

/// A tagged value.
#[derive(Debug, Clone, PartialEq)]
pub struct Tagged {
    /// Tag name (without `@`).
    pub tag: String,
    /// Optional payload value.
    pub payload: Option<Box<Value>>,
    /// Source span.
    pub span: Option<Span>,
}

/// A sequence of values.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    /// Items in the sequence.
    pub items: Vec<Value>,
    /// Source span.
    pub span: Option<Span>,
}

/// An object (mapping of keys to values).
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Entries in the object.
    pub entries: Vec<Entry>,
    /// Separator style used.
    pub separator: Separator,
    /// Source span.
    pub span: Option<Span>,
}

/// An entry in an object.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key (usually a Scalar, but can be Unit or Tagged).
    pub key: Value,
    /// The value.
    pub value: Value,
    /// Doc comment attached to this entry.
    pub doc_comment: Option<String>,
}

impl Value {
    /// Create a scalar value.
    pub fn scalar(text: impl Into<String>) -> Self {
        Value::Scalar(Scalar {
            text: text.into(),
            kind: ScalarKind::Bare,
            span: None,
        })
    }

    /// Create a unit value.
    pub fn unit() -> Self {
        Value::Unit
    }

    /// Create an empty object.
    pub fn object() -> Self {
        Value::Object(Object {
            entries: Vec::new(),
            separator: Separator::Newline,
            span: None,
        })
    }

    /// Create an empty sequence.
    pub fn sequence() -> Self {
        Value::Sequence(Sequence {
            items: Vec::new(),
            span: None,
        })
    }

    /// Get as string (for scalars).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Scalar(s) => Some(&s.text),
            _ => None,
        }
    }

    /// Get as object.
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Get as mutable object.
    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Get as sequence.
    pub fn as_sequence(&self) -> Option<&Sequence> {
        match self {
            Value::Sequence(s) => Some(s),
            _ => None,
        }
    }

    /// Get as mutable sequence.
    pub fn as_sequence_mut(&mut self) -> Option<&mut Sequence> {
        match self {
            Value::Sequence(s) => Some(s),
            _ => None,
        }
    }

    /// Check if unit.
    pub fn is_unit(&self) -> bool {
        matches!(self, Value::Unit)
    }

    /// Get tag name if tagged.
    pub fn tag(&self) -> Option<&str> {
        match self {
            Value::Tagged(t) => Some(&t.tag),
            _ => None,
        }
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

        match self {
            Value::Object(obj) => {
                let value = obj.get(segment)?;
                if rest.is_empty() {
                    Some(value)
                } else {
                    value.get(rest)
                }
            }
            Value::Sequence(seq) => {
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
            Value::Tagged(t) => {
                if let Some(payload) = &t.payload {
                    payload.get(path)
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

        match self {
            Value::Object(obj) => {
                let value = obj.get_mut(segment)?;
                if rest.is_empty() {
                    Some(value)
                } else {
                    value.get_mut(rest)
                }
            }
            Value::Sequence(seq) => {
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
            Value::Tagged(t) => {
                if let Some(payload) = &mut t.payload {
                    payload.get_mut(path)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Object {
    /// Get entry value by key.
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

    /// Iterate over entries as (key, value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&Value, &Value)> {
        self.entries.iter().map(|e| (&e.key, &e.value))
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key.as_str() == Some(key))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert or update an entry.
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
    if path.starts_with('[') {
        if let Some(end) = path.find(']') {
            let segment = &path[..=end];
            let rest = &path[end + 1..];
            // Skip leading `.` in rest
            let rest = rest.strip_prefix('.').unwrap_or(rest);
            return (segment, rest);
        }
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
    fn test_object_get() {
        let mut obj = Object {
            entries: vec![Entry {
                key: Value::scalar("name"),
                value: Value::scalar("Alice"),
                doc_comment: None,
            }],
            separator: Separator::Newline,
            span: None,
        };

        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(obj.get("missing"), None);

        obj.insert("age", Value::scalar("30"));
        assert_eq!(obj.get("age").and_then(|v| v.as_str()), Some("30"));
    }

    #[test]
    fn test_value_path_access() {
        let value = Value::Object(Object {
            entries: vec![
                Entry {
                    key: Value::scalar("user"),
                    value: Value::Object(Object {
                        entries: vec![Entry {
                            key: Value::scalar("name"),
                            value: Value::scalar("Alice"),
                            doc_comment: None,
                        }],
                        separator: Separator::Newline,
                        span: None,
                    }),
                    doc_comment: None,
                },
                Entry {
                    key: Value::scalar("items"),
                    value: Value::Sequence(Sequence {
                        items: vec![Value::scalar("a"), Value::scalar("b"), Value::scalar("c")],
                        span: None,
                    }),
                    doc_comment: None,
                },
            ],
            separator: Separator::Newline,
            span: None,
        });

        assert_eq!(
            value.get("user.name").and_then(|v| v.as_str()),
            Some("Alice")
        );
        assert_eq!(value.get("items[0]").and_then(|v| v.as_str()), Some("a"));
        assert_eq!(value.get("items[2]").and_then(|v| v.as_str()), Some("c"));
        assert_eq!(value.get("missing"), None);
    }
}
