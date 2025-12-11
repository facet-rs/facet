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
    FieldKey(Cow<'de, str>, FieldLocationHint),
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
            ParseEvent::FieldKey(name, loc) => {
                f.debug_tuple("FieldKey").field(name).field(loc).finish()
            }
            ParseEvent::SequenceStart => f.write_str("SequenceStart"),
            ParseEvent::SequenceEnd => f.write_str("SequenceEnd"),
            ParseEvent::Scalar(value) => f.debug_tuple("Scalar").field(value).finish(),
            ParseEvent::VariantTag(tag) => f.debug_tuple("VariantTag").field(tag).finish(),
        }
    }
}
