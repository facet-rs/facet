extern crate alloc;

use alloc::borrow::Cow;
use core::fmt;

/// Location hint for a serialized field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldLocationHint {
    /// Key/value entry (JSON/YAML/TOML/etc).
    #[default]
    KeyValue,
}

/// Field key for a serialized field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldKey<'de> {
    /// Field name.
    pub name: Cow<'de, str>,
    /// Location hint.
    pub location: FieldLocationHint,
}

impl<'de> FieldKey<'de> {
    /// Create a new field key.
    pub fn new(name: impl Into<Cow<'de, str>>, location: FieldLocationHint) -> Self {
        Self {
            name: name.into(),
            location,
        }
    }
}

/// The kind of container being parsed.
///
/// This distinguishes between format-specific container types to enable
/// better error messages and type checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// Object: struct-like with key-value pairs.
    /// Type mismatches (e.g., object where array expected) should produce errors.
    Object,
    /// Array: sequence-like.
    /// Type mismatches (e.g., array where object expected) should produce errors.
    Array,
}

impl ContainerKind {
    /// Human-readable name for error messages.
    pub const fn name(self) -> &'static str {
        match self {
            ContainerKind::Object => "object",
            ContainerKind::Array => "array",
        }
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
    /// Unit type (Rust's `()`).
    Unit,
    /// Null literal.
    Null,
    /// Boolean literal.
    Bool(bool),
    /// Character literal.
    Char(char),
    /// Signed integer literal (fits in i64).
    I64(i64),
    /// Unsigned integer literal (fits in u64).
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
    StructStart(ContainerKind),
    /// End of a struct/object/node.
    StructEnd,
    /// Encountered a field key (for self-describing formats like JSON/YAML).
    FieldKey(FieldKey<'de>),
    /// Next field value in struct field order (for non-self-describing formats like postcard).
    ///
    /// The driver tracks the current field index and uses the schema to determine
    /// which field this value belongs to. This allows formats without field names
    /// in the wire format to still support Tier-0 deserialization.
    OrderedField,
    /// Beginning of a sequence/array/tuple.
    SequenceStart(ContainerKind),
    /// End of a sequence/array/tuple.
    SequenceEnd,
    /// Scalar literal.
    Scalar(ScalarValue<'de>),
    /// Tagged value from a self-describing format with native tagged union syntax.
    ///
    /// This is used by formats like Styx that have explicit tag syntax (e.g., `@tag(value)`).
    /// Most formats (JSON, TOML, etc.) don't need this - they represent enums as
    /// `{"variant_name": value}` which goes through the struct/field path instead.
    ///
    /// `None` represents a unit tag (bare `@` in Styx) with no name.
    VariantTag(Option<&'de str>),
}

impl<'de> fmt::Debug for ParseEvent<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseEvent::StructStart(kind) => f.debug_tuple("StructStart").field(kind).finish(),
            ParseEvent::StructEnd => f.write_str("StructEnd"),
            ParseEvent::FieldKey(key) => f.debug_tuple("FieldKey").field(key).finish(),
            ParseEvent::OrderedField => f.write_str("OrderedField"),
            ParseEvent::SequenceStart(kind) => f.debug_tuple("SequenceStart").field(kind).finish(),
            ParseEvent::SequenceEnd => f.write_str("SequenceEnd"),
            ParseEvent::Scalar(value) => f.debug_tuple("Scalar").field(value).finish(),
            ParseEvent::VariantTag(tag) => f.debug_tuple("VariantTag").field(tag).finish(),
        }
    }
}
