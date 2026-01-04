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
    /// Element tag name (for custom elements in XML/HTML).
    Tag,
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

/// The kind of container being parsed.
///
/// This distinguishes between format-specific container types to enable
/// better error messages and type checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// JSON/YAML/TOML object: definitely struct-like with key-value pairs.
    /// Type mismatches (e.g., object where array expected) should produce errors.
    Object,
    /// JSON/YAML array: definitely sequence-like.
    /// Type mismatches (e.g., array where object expected) should produce errors.
    Array,
    /// XML/KDL element: semantically ambiguous.
    /// Could be interpreted as struct, sequence, or scalar wrapper depending on target type.
    /// The deserializer decides based on what type it's deserializing into.
    Element,
}

impl ContainerKind {
    /// Returns true if this container kind is ambiguous (can be struct or sequence).
    pub fn is_ambiguous(self) -> bool {
        matches!(self, ContainerKind::Element)
    }

    /// Human-readable name for error messages.
    pub fn name(self) -> &'static str {
        match self {
            ContainerKind::Object => "object",
            ContainerKind::Array => "array",
            ContainerKind::Element => "element",
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
    /// UTF-8 string literal (definitely a string, not a number).
    Str(Cow<'de, str>),
    /// Binary literal.
    Bytes(Cow<'de, [u8]>),
    /// Stringly-typed value from formats like XML where all values are text.
    ///
    /// Unlike `Str`, this value's type is ambiguous - it could be a number,
    /// boolean, or actual string depending on the target type. The deserializer
    /// will attempt to parse it according to the expected type.
    ///
    /// Examples:
    /// - XML `<value>42</value>` → StringlyTyped("42") → parses as i32, u64, String, etc.
    /// - XML `<value>2.5</value>` → StringlyTyped("2.5") → parses as f64, Decimal, String, etc.
    /// - JSON `"42"` → Str("42") → definitely a string, not a number
    StringlyTyped(Cow<'de, str>),
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
    /// Variant discriminant that needs to be propagated to the solver.
    VariantTag(&'de str),
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
