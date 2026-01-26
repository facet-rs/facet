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
///
/// For self-describing formats, this represents either:
/// - A named key (struct field or map key with string name)
/// - A tagged key (e.g., `@string` in Styx for type pattern keys)
/// - A unit key (map key with no name, e.g., `@` in Styx representing `None` in `Option<String>` keys)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldKey<'de> {
    /// Field name.
    ///
    /// `None` represents a unit key (e.g., `@` in Styx) which can be deserialized as
    /// `None` for `Option<String>` map keys. For struct field deserialization, `None`
    /// is an error since struct fields always have names.
    pub name: Option<Cow<'de, str>>,
    /// Location hint.
    pub location: FieldLocationHint,
    /// Documentation comments attached to this field (for formats that support them).
    ///
    /// Used by formats like Styx where `/// comment` before a field is preserved.
    /// When deserializing into a `metadata_container` type like `Documented<T>`,
    /// these doc lines are used to populate the metadata.
    pub doc: Option<Vec<Cow<'de, str>>>,
    /// Tag name for tagged keys (for formats that support them).
    ///
    /// Used by formats like Styx where `@string` in key position represents a type pattern.
    /// When deserializing into a `metadata_container` type with `#[facet(metadata = "tag")]`,
    /// this tag name is used to populate the metadata.
    ///
    /// - `None`: not a tagged key (bare identifier like `name`)
    /// - `Some("")`: unit tag (`@` alone)
    /// - `Some("string")`: named tag (`@string`)
    pub tag: Option<Cow<'de, str>>,
}

impl<'de> FieldKey<'de> {
    /// Create a new field key with a name.
    pub fn new(name: impl Into<Cow<'de, str>>, location: FieldLocationHint) -> Self {
        Self {
            name: Some(name.into()),
            location,
            doc: None,
            tag: None,
        }
    }

    /// Create a new field key with a name and documentation.
    pub fn with_doc(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        doc: Vec<Cow<'de, str>>,
    ) -> Self {
        Self {
            name: Some(name.into()),
            location,
            doc: if doc.is_empty() { None } else { Some(doc) },
            tag: None,
        }
    }

    /// Create a tagged field key (e.g., `@string` in Styx).
    ///
    /// Used for type pattern keys where the key is a tag rather than a bare identifier.
    pub fn tagged(tag: impl Into<Cow<'de, str>>, location: FieldLocationHint) -> Self {
        Self {
            name: None,
            location,
            doc: None,
            tag: Some(tag.into()),
        }
    }

    /// Create a tagged field key with documentation.
    pub fn tagged_with_doc(
        tag: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        doc: Vec<Cow<'de, str>>,
    ) -> Self {
        Self {
            name: None,
            location,
            doc: if doc.is_empty() { None } else { Some(doc) },
            tag: Some(tag.into()),
        }
    }

    /// Create a unit field key (no name).
    ///
    /// Used for formats like Styx where `@` represents a unit key in maps.
    /// This is equivalent to `tagged("")` - a tag with an empty name.
    pub fn unit(location: FieldLocationHint) -> Self {
        Self {
            name: None,
            location,
            doc: None,
            tag: Some(Cow::Borrowed("")),
        }
    }

    /// Create a unit field key with documentation.
    pub fn unit_with_doc(location: FieldLocationHint, doc: Vec<Cow<'de, str>>) -> Self {
        Self {
            name: None,
            location,
            doc: if doc.is_empty() { None } else { Some(doc) },
            tag: Some(Cow::Borrowed("")),
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

impl<'de> ScalarValue<'de> {
    /// Convert scalar value to a string representation.
    ///
    /// This is a non-generic helper extracted to reduce monomorphization bloat.
    /// Returns `None` for `Bytes` since that conversion is context-dependent.
    pub fn to_string_value(&self) -> Option<alloc::string::String> {
        match self {
            ScalarValue::Str(s) => Some(s.to_string()),
            ScalarValue::Bool(b) => Some(b.to_string()),
            ScalarValue::I64(i) => Some(i.to_string()),
            ScalarValue::U64(u) => Some(u.to_string()),
            ScalarValue::I128(i) => Some(i.to_string()),
            ScalarValue::U128(u) => Some(u.to_string()),
            ScalarValue::F64(f) => Some(f.to_string()),
            ScalarValue::Char(c) => Some(c.to_string()),
            ScalarValue::Null => Some("null".to_string()),
            ScalarValue::Unit => Some(alloc::string::String::new()),
            ScalarValue::Bytes(_) => None,
        }
    }

    /// Convert scalar value to a display string for error messages.
    ///
    /// This is a non-generic helper extracted to reduce monomorphization bloat.
    pub fn to_display_string(&self) -> alloc::string::String {
        match self {
            ScalarValue::Str(s) => s.to_string(),
            ScalarValue::Bool(b) => alloc::format!("bool({})", b),
            ScalarValue::I64(i) => alloc::format!("i64({})", i),
            ScalarValue::U64(u) => alloc::format!("u64({})", u),
            ScalarValue::I128(i) => alloc::format!("i128({})", i),
            ScalarValue::U128(u) => alloc::format!("u128({})", u),
            ScalarValue::F64(f) => alloc::format!("f64({})", f),
            ScalarValue::Char(c) => alloc::format!("char({})", c),
            ScalarValue::Bytes(_) => "bytes".to_string(),
            ScalarValue::Null => "null".to_string(),
            ScalarValue::Unit => "unit".to_string(),
        }
    }
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
