extern crate alloc;

use alloc::borrow::Cow;
use core::fmt;

/// Location hint for a serialized field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldLocationHint {
    /// Key/value entry (JSON/YAML/TOML).
    KeyValue,
    /// XML attribute.
    Attribute,
    /// XML/KDL text node.
    Text,
    /// XML/KDL child element/node.
    Child,
    /// KDL property.
    Property,
    /// KDL positional argument.
    Argument,
}

/// Field key with optional namespace (for XML).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldKey<'de> {
    /// Field name.
    pub name: Cow<'de, str>,
    /// Location hint.
    pub location: FieldLocationHint,
    /// Optional namespace URI (for XML namespace support).
    pub namespace: Option<Cow<'de, str>>,
}

impl<'de> FieldKey<'de> {
    /// Create a new field key without namespace.
    pub fn new(name: impl Into<Cow<'de, str>>, location: FieldLocationHint) -> Self {
        Self {
            name: name.into(),
            location,
            namespace: None,
        }
    }

    /// Add a namespace to this field key (builder pattern).
    pub fn with_namespace(mut self, namespace: impl Into<Cow<'de, str>>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }
}

/// Value classification hint for evidence gathering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueTypeHint {
    /// Null-like values.
    Null,
    /// Boolean.
    Bool,
    /// Numeric primitive.
    Number,
    /// Text string.
    String,
    /// Raw bytes (e.g., base64 segments).
    Bytes,
    /// Sequence (array/list/tuple).
    Sequence,
    /// Map/struct/object.
    Map,
}

/// Scalar data extracted from the wire format.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue<'de> {
    /// Null literal.
    Null,
    /// Boolean literal.
    Bool(bool),
    /// Signed integer literal.
    I64(i64),
    /// Unsigned integer literal.
    U64(u64),
    /// Signed 128-bit integer literal.
    I128(i128),
    /// Unsigned 128-bit integer literal.
    U128(u128),
    /// Floating-point literal.
    F64(f64),
    /// UTF-8 string literal.
    Str(Cow<'de, str>),
    /// Binary literal.
    Bytes(Cow<'de, [u8]>),
}

/// Event emitted by a format parser while streaming through input.
#[derive(Clone, PartialEq)]
pub enum ParseEvent<'de> {
    /// Beginning of a struct/object/node.
    StructStart,
    /// End of a struct/object/node.
    StructEnd,
    /// Encountered a field key.
    FieldKey(FieldKey<'de>),
    /// Beginning of a sequence/array/tuple.
    SequenceStart,
    /// End of a sequence/array/tuple.
    SequenceEnd,
    /// Scalar literal.
    Scalar(ScalarValue<'de>),
    /// Variant discriminant that needs to be propagated to the solver.
    VariantTag(&'de str),
}

impl<'de> fmt::Debug for ParseEvent<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseEvent::StructStart => f.write_str("StructStart"),
            ParseEvent::StructEnd => f.write_str("StructEnd"),
            ParseEvent::FieldKey(key) => f.debug_tuple("FieldKey").field(key).finish(),
            ParseEvent::SequenceStart => f.write_str("SequenceStart"),
            ParseEvent::SequenceEnd => f.write_str("SequenceEnd"),
            ParseEvent::Scalar(value) => f.debug_tuple("Scalar").field(value).finish(),
            ParseEvent::VariantTag(tag) => f.debug_tuple("VariantTag").field(tag).finish(),
        }
    }
}
