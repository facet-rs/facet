extern crate alloc;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
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
/// This enum is optimized for the common case (simple named keys) while still
/// supporting rich metadata for formats like Styx.
///
/// - `Name`: Simple string key (24 bytes) - used by JSON, YAML, TOML, etc.
/// - `Full`: Boxed full key with metadata (8 bytes) - used by Styx for doc/tag support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKey<'de> {
    /// Simple named key (common case for JSON/YAML/TOML).
    Name(Cow<'de, str>),
    /// Full key with metadata support (for Styx).
    Full(Box<FullFieldKey<'de>>),
}

/// Full field key with metadata support.
///
/// Used by formats like Styx that support documentation comments and type tags on keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullFieldKey<'de> {
    /// Field name.
    ///
    /// `None` represents a unit key (e.g., `@` in Styx) which can be deserialized as
    /// `None` for `Option<String>` map keys. For struct field deserialization, `None`
    /// is an error since struct fields always have names.
    pub name: Option<Cow<'de, str>>,
    /// Location hint.
    pub location: FieldLocationHint,
    /// Metadata (doc comments, type tags) attached to this field.
    pub meta: ValueMeta<'de>,
}

impl<'de> FieldKey<'de> {
    /// Create a new field key with a name (common case).
    #[inline]
    pub fn new(name: impl Into<Cow<'de, str>>, _location: FieldLocationHint) -> Self {
        FieldKey::Name(name.into())
    }

    /// Create a new field key with a name and documentation.
    pub fn with_doc(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        doc: Vec<Cow<'de, str>>,
    ) -> Self {
        if doc.is_empty() {
            FieldKey::Name(name.into())
        } else {
            FieldKey::Full(Box::new(FullFieldKey {
                name: Some(name.into()),
                location,
                meta: ValueMeta::builder().doc(doc).build(),
            }))
        }
    }

    /// Create a tagged field key (e.g., `@string` in Styx).
    ///
    /// Used for type pattern keys where the key is a tag rather than a bare identifier.
    pub fn tagged(tag: impl Into<Cow<'de, str>>, location: FieldLocationHint) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: None,
            location,
            meta: ValueMeta::builder().tag(tag.into()).build(),
        }))
    }

    /// Create a tagged field key with documentation.
    pub fn tagged_with_doc(
        tag: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        doc: Vec<Cow<'de, str>>,
    ) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: None,
            location,
            meta: ValueMeta::builder()
                .tag(tag.into())
                .maybe_doc(Some(doc))
                .build(),
        }))
    }

    /// Create a tagged field key with a name (e.g., `@string"mykey"` in Styx).
    ///
    /// Used for type pattern keys that also have an associated name/payload.
    pub fn tagged_with_name(
        tag: impl Into<Cow<'de, str>>,
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
    ) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: Some(name.into()),
            location,
            meta: ValueMeta::builder().tag(tag.into()).build(),
        }))
    }

    /// Create a tagged field key with a name and documentation.
    pub fn tagged_with_name_and_doc(
        tag: impl Into<Cow<'de, str>>,
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        doc: Vec<Cow<'de, str>>,
    ) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: Some(name.into()),
            location,
            meta: ValueMeta::builder()
                .tag(tag.into())
                .maybe_doc(Some(doc))
                .build(),
        }))
    }

    /// Create a unit field key (no name).
    ///
    /// Used for formats like Styx where `@` represents a unit key in maps.
    /// This is equivalent to `tagged("")` - a tag with an empty name.
    pub fn unit(location: FieldLocationHint) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: None,
            location,
            meta: ValueMeta::builder().tag(Cow::Borrowed("")).build(),
        }))
    }

    /// Create a unit field key with documentation.
    pub fn unit_with_doc(location: FieldLocationHint, doc: Vec<Cow<'de, str>>) -> Self {
        FieldKey::Full(Box::new(FullFieldKey {
            name: None,
            location,
            meta: ValueMeta::builder()
                .tag(Cow::Borrowed(""))
                .maybe_doc(Some(doc))
                .build(),
        }))
    }

    /// Get the field name, if any.
    #[inline]
    pub fn name(&self) -> Option<&Cow<'de, str>> {
        match self {
            FieldKey::Name(name) => Some(name),
            FieldKey::Full(full) => full.name.as_ref(),
        }
    }

    /// Get the documentation comments, if any.
    #[inline]
    pub fn doc(&self) -> Option<&[Cow<'de, str>]> {
        match self {
            FieldKey::Name(_) => None,
            FieldKey::Full(full) => full.meta.doc(),
        }
    }

    /// Get the tag name, if any.
    #[inline]
    pub fn tag(&self) -> Option<&Cow<'de, str>> {
        match self {
            FieldKey::Name(_) => None,
            FieldKey::Full(full) => full.meta.tag(),
        }
    }

    /// Get the metadata, if any.
    #[inline]
    pub fn meta(&self) -> Option<&ValueMeta<'de>> {
        match self {
            FieldKey::Name(_) => None,
            FieldKey::Full(full) => Some(&full.meta),
        }
    }

    /// Get the location hint.
    #[inline]
    pub fn location(&self) -> FieldLocationHint {
        match self {
            FieldKey::Name(_) => FieldLocationHint::KeyValue,
            FieldKey::Full(full) => full.location,
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

    /// Returns a static string describing the kind of scalar for error messages.
    #[inline]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            ScalarValue::Unit => "unit",
            ScalarValue::Null => "null",
            ScalarValue::Bool(_) => "bool",
            ScalarValue::Char(_) => "char",
            ScalarValue::I64(_) => "i64",
            ScalarValue::U64(_) => "u64",
            ScalarValue::I128(_) => "i128",
            ScalarValue::U128(_) => "u128",
            ScalarValue::F64(_) => "f64",
            ScalarValue::Str(_) => "string",
            ScalarValue::Bytes(_) => "bytes",
        }
    }
}

/// Metadata associated with a value being deserialized.
///
/// This includes documentation comments and type tags from formats that support them
/// (like Styx). For formats that don't provide metadata (like JSON), these will be empty/none.
///
/// Use [`ValueMeta::builder()`] to construct instances.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValueMeta<'a> {
    doc: Option<Vec<Cow<'a, str>>>,
    tag: Option<Cow<'a, str>>,
}

impl<'a> ValueMeta<'a> {
    /// Create a new builder for `ValueMeta`.
    #[inline]
    pub fn builder() -> ValueMetaBuilder<'a> {
        ValueMetaBuilder::default()
    }

    /// Get the documentation comments, if any.
    #[inline]
    pub fn doc(&self) -> Option<&[Cow<'a, str>]> {
        self.doc.as_deref()
    }

    /// Get the type tag, if any (e.g., `@string` in Styx).
    #[inline]
    pub fn tag(&self) -> Option<&Cow<'a, str>> {
        self.tag.as_ref()
    }

    /// Returns `true` if this metadata has no content.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.doc.is_none() && self.tag.is_none()
    }
}

/// Builder for [`ValueMeta`].
#[derive(Debug, Clone, Default)]
pub struct ValueMetaBuilder<'a> {
    doc: Option<Vec<Cow<'a, str>>>,
    tag: Option<Cow<'a, str>>,
}

impl<'a> ValueMetaBuilder<'a> {
    /// Set the documentation comments.
    #[inline]
    pub fn doc(mut self, doc: Vec<Cow<'a, str>>) -> Self {
        if !doc.is_empty() {
            self.doc = Some(doc);
        }
        self
    }

    /// Set the documentation comments if present.
    #[inline]
    pub fn maybe_doc(mut self, doc: Option<Vec<Cow<'a, str>>>) -> Self {
        if let Some(d) = doc
            && !d.is_empty()
        {
            self.doc = Some(d);
        }
        self
    }

    /// Set the type tag.
    #[inline]
    pub fn tag(mut self, tag: Cow<'a, str>) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Set the type tag if present.
    #[inline]
    pub fn maybe_tag(mut self, tag: Option<Cow<'a, str>>) -> Self {
        if tag.is_some() {
            self.tag = tag;
        }
        self
    }

    /// Build the `ValueMeta`.
    #[inline]
    pub fn build(self) -> ValueMeta<'a> {
        ValueMeta {
            doc: self.doc,
            tag: self.tag,
        }
    }
}

/// Event emitted by a format parser while streaming through input.
#[derive(Clone, PartialEq)]
pub struct ParseEvent<'de> {
    /// The kind of event.
    pub kind: ParseEventKind<'de>,
    /// Source span of this event in the input.
    pub span: facet_reflect::Span,
    /// Optional metadata (doc comments, type tags) attached to this value.
    ///
    /// For most formats (JSON, TOML, etc.) this will be `None`. Formats like Styx
    /// that support documentation comments and type tags on values will populate this.
    pub meta: Option<ValueMeta<'de>>,
}

impl<'de> ParseEvent<'de> {
    /// Create a new event with the given kind and span.
    #[inline]
    pub fn new(kind: ParseEventKind<'de>, span: facet_reflect::Span) -> Self {
        Self {
            kind,
            span,
            meta: None,
        }
    }

    /// Attach metadata to this event using a builder.
    ///
    /// # Example
    /// ```ignore
    /// ParseEvent::new(kind, span).with_meta(|m| m.doc(lines).tag(tag))
    /// ```
    #[inline]
    pub fn with_meta(
        mut self,
        f: impl FnOnce(ValueMetaBuilder<'de>) -> ValueMetaBuilder<'de>,
    ) -> Self {
        let meta = f(ValueMetaBuilder::default()).build();
        if !meta.is_empty() {
            self.meta = Some(meta);
        }
        self
    }
}

/// The kind of parse event.
#[derive(Clone, PartialEq)]
pub enum ParseEventKind<'de> {
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
        // Delegate to kind's debug, span is secondary
        write!(f, "{:?}@{}", self.kind, self.span)
    }
}

impl<'de> fmt::Debug for ParseEventKind<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseEventKind::StructStart(kind) => f.debug_tuple("StructStart").field(kind).finish(),
            ParseEventKind::StructEnd => f.write_str("StructEnd"),
            ParseEventKind::FieldKey(key) => f.debug_tuple("FieldKey").field(key).finish(),
            ParseEventKind::OrderedField => f.write_str("OrderedField"),
            ParseEventKind::SequenceStart(kind) => {
                f.debug_tuple("SequenceStart").field(kind).finish()
            }
            ParseEventKind::SequenceEnd => f.write_str("SequenceEnd"),
            ParseEventKind::Scalar(value) => f.debug_tuple("Scalar").field(value).finish(),
            ParseEventKind::VariantTag(tag) => f.debug_tuple("VariantTag").field(tag).finish(),
        }
    }
}

impl ParseEvent<'_> {
    /// Returns a static string describing the kind of event for error messages.
    #[inline]
    pub const fn kind_name(&self) -> &'static str {
        self.kind.kind_name()
    }
}

impl ParseEventKind<'_> {
    /// Returns a static string describing the kind of event for error messages.
    #[inline]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            ParseEventKind::StructStart(_) => "struct start",
            ParseEventKind::StructEnd => "struct end",
            ParseEventKind::FieldKey(_) => "field key",
            ParseEventKind::OrderedField => "ordered field",
            ParseEventKind::SequenceStart(_) => "sequence start",
            ParseEventKind::SequenceEnd => "sequence end",
            ParseEventKind::Scalar(_) => "scalar",
            ParseEventKind::VariantTag(_) => "variant tag",
        }
    }
}
