use facet_reflect::Span;

/// Opaque token returned by [`FormatParser::save`].
///
/// This token can be passed to [`FormatParser::restore`] to replay
/// all events that were consumed since the save point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavePoint(pub u64);

impl SavePoint {
    /// Create a new save point with the given ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Streaming parser for a specific wire format.
pub trait FormatParser<'de> {
    /// Parser-specific error type.
    type Error: core::fmt::Debug;

    /// Read the next parse event, or `None` if the input is exhausted.
    ///
    /// Returns `Ok(None)` at end-of-input (EOF). For formats like TOML where
    /// structs can be "reopened" (fields added after the struct was previously
    /// exited), callers should continue processing until EOF rather than
    /// stopping at `StructEnd`.
    ///
    /// If [`restore`](Self::restore) was called, events are first replayed
    /// from the internal buffer before reading new events from the input.
    fn next_event(&mut self) -> Result<Option<crate::ParseEvent<'de>>, Self::Error>;

    /// Peek at the next event without consuming it, or `None` if at EOF.
    fn peek_event(&mut self) -> Result<Option<crate::ParseEvent<'de>>, Self::Error>;

    /// Skip the current value (for unknown fields, etc.).
    fn skip_value(&mut self) -> Result<(), Self::Error>;

    /// Save the current parser position and start recording events.
    ///
    /// Returns a [`SavePoint`] token. All events returned by [`next_event`](Self::next_event)
    /// after this call are recorded internally. Call [`restore`](Self::restore) with this
    /// token to replay all recorded events.
    ///
    /// This is used for untagged enum resolution: save, read ahead to determine
    /// the variant, then restore and parse with the correct type.
    fn save(&mut self) -> SavePoint;

    /// Restore to a previous save point, replaying recorded events.
    ///
    /// After calling this, subsequent calls to [`next_event`](Self::next_event) will
    /// first return all events that were recorded since the save point, then
    /// continue reading from the input.
    ///
    /// The save point is consumed - to save again, call [`save`](Self::save).
    fn restore(&mut self, save_point: SavePoint);

    /// Capture the raw representation of the current value without parsing it.
    ///
    /// This is used for types like `RawJson` that want to defer parsing.
    /// The parser should skip the value and return the raw bytes/string
    /// from the input.
    ///
    /// Returns `Ok(None)` if raw capture is not supported (e.g., streaming mode
    /// or formats where raw capture doesn't make sense).
    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        // Default: not supported
        self.skip_value()?;
        Ok(None)
    }

    /// Returns the shape of the format's raw capture type (e.g., `RawJson::SHAPE`).
    ///
    /// When the deserializer encounters a shape that matches this, it will use
    /// `capture_raw` to capture the raw representation and store it in a
    /// `Cow<str>` (the raw type must be a newtype over `Cow<str>`).
    ///
    /// Returns `None` if this format doesn't support raw capture types.
    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        None
    }

    /// Returns true if this format is self-describing.
    ///
    /// Self-describing formats (like JSON, YAML) include type information in the wire format
    /// and emit `FieldKey` events for struct fields.
    ///
    /// Non-self-describing formats (like postcard, bincode) don't include type markers
    /// and use `OrderedField` events, relying on the driver to provide schema information
    /// via `hint_struct_fields`.
    fn is_self_describing(&self) -> bool {
        true // Default: most formats are self-describing
    }

    /// Hint to the parser that a struct with the given number of fields is expected.
    ///
    /// For non-self-describing formats, this allows the parser to emit the correct
    /// number of `OrderedField` events followed by `StructEnd`.
    ///
    /// Self-describing formats can ignore this hint.
    fn hint_struct_fields(&mut self, _num_fields: usize) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser what scalar type is expected next.
    ///
    /// For non-self-describing formats, this allows the parser to correctly
    /// decode the next value and emit an appropriate `Scalar` event.
    ///
    /// Self-describing formats can ignore this hint (they determine the type
    /// from the wire format).
    fn hint_scalar_type(&mut self, _hint: ScalarTypeHint) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that a sequence (array/Vec) is expected.
    ///
    /// For non-self-describing formats, this triggers reading the length prefix
    /// and setting up sequence state.
    ///
    /// Self-describing formats can ignore this hint.
    fn hint_sequence(&mut self) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that a byte sequence (`Vec<u8>`, `&[u8]`, etc.) is expected.
    ///
    /// For binary formats like postcard that store `Vec<u8>` as raw bytes (varint length
    /// followed by raw data), this allows bulk reading instead of element-by-element
    /// deserialization.
    ///
    /// If the parser handles this hint, it should emit `Scalar(Bytes(...))` directly.
    /// If it doesn't support this optimization, it should return `false` and the
    /// deserializer will fall back to element-by-element deserialization via `hint_sequence`.
    ///
    /// Returns `true` if the hint is handled (parser will emit `Scalar(Bytes(...))`),
    /// `false` otherwise.
    fn hint_byte_sequence(&mut self) -> bool {
        // Default: not supported, fall back to element-by-element
        false
    }

    /// Hint to the parser that a fixed-size array is expected.
    ///
    /// For non-self-describing formats, this tells the parser the array length
    /// is known at compile time (from the type), so no length prefix is read.
    /// This differs from `hint_sequence` which reads a length prefix for Vec/slices.
    ///
    /// Self-describing formats can ignore this hint.
    fn hint_array(&mut self, _len: usize) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that an `Option<T>` is expected.
    ///
    /// For non-self-describing formats (like postcard), this allows the parser
    /// to read the discriminant byte and emit either:
    /// - `Scalar(Null)` for None (discriminant 0x00)
    /// - Set up state to parse the inner value for Some (discriminant 0x01)
    ///
    /// Self-describing formats can ignore this hint (they determine `Option`
    /// presence from the wire format, e.g., null vs value in JSON).
    fn hint_option(&mut self) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that a map is expected.
    ///
    /// For non-self-describing formats (like postcard), this allows the parser
    /// to read the length prefix and set up map state. The parser should then
    /// emit `SequenceStart` (representing the map entries) followed by pairs of
    /// key and value events, and finally `SequenceEnd`.
    ///
    /// Self-describing formats can ignore this hint (they determine map structure
    /// from the wire format, e.g., `{...}` in JSON).
    fn hint_map(&mut self) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that a dynamic value is expected.
    ///
    /// Non-self-describing formats can use this to switch to a self-describing
    /// encoding for dynamic values (e.g., tagged scalar/array/object).
    /// Self-describing formats can ignore this hint.
    fn hint_dynamic_value(&mut self) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that an enum is expected, providing variant information.
    ///
    /// For non-self-describing formats (like postcard), this allows the parser
    /// to read the variant discriminant (varint) and map it to the variant name,
    /// and to emit appropriate wrapper events for multi-field variants.
    ///
    /// The `variants` slice contains metadata for each variant in declaration order,
    /// matching the indices used in the wire format.
    ///
    /// Self-describing formats can ignore this hint (they include variant names
    /// in the wire format).
    fn hint_enum(&mut self, _variants: &[EnumVariantHint]) {
        // Default: ignore (self-describing formats don't need this)
    }

    /// Hint to the parser that an opaque scalar type is expected.
    ///
    /// For non-self-describing binary formats (like postcard), this allows the parser
    /// to use format-specific encoding for types like UUID (16 raw bytes), ULID,
    /// OrderedFloat, etc. that have a more efficient binary representation than
    /// their string form.
    ///
    /// The `type_identifier` is the type's identifier string (e.g., "Uuid", "Ulid",
    /// "OrderedFloat", `DateTime<Utc>`). The `shape` provides access to inner type
    /// information (e.g., whether OrderedFloat wraps f32 or f64).
    ///
    /// Returns `true` if the parser will handle this type specially (caller should
    /// expect format-specific `ScalarValue`), or `false` to fall back to standard
    /// handling (e.g., `hint_scalar_type(String)` for `FromStr` types).
    ///
    /// Self-describing formats can ignore this and return `false`.
    fn hint_opaque_scalar(
        &mut self,
        _type_identifier: &'static str,
        _shape: &'static facet_core::Shape,
    ) -> bool {
        // Default: not handled, fall back to standard behavior
        false
    }

    /// Returns the source span of the most recently consumed event.
    ///
    /// This is used for error reporting - when a deserialization error occurs,
    /// the span of the last consumed event helps locate the problem in the input.
    ///
    /// Parsers that track source positions should override this to return
    /// meaningful span information. The default implementation returns `None`.
    fn current_span(&self) -> Option<Span> {
        None
    }

    /// Returns the format namespace for format-specific proxy resolution.
    ///
    /// When a field or container has format-specific proxies (e.g., `#[facet(xml::proxy = XmlProxy)]`),
    /// this namespace is used to look up the appropriate proxy. If no namespace is returned,
    /// only the format-agnostic proxy (`#[facet(proxy = ...)]`) is considered.
    ///
    /// Examples:
    /// - XML parser should return `Some("xml")`
    /// - JSON parser should return `Some("json")`
    ///
    /// Default: returns `None` (only format-agnostic proxies are used).
    fn format_namespace(&self) -> Option<&'static str> {
        None
    }
}

/// Metadata about an enum variant for use with `hint_enum`.
///
/// Provides the information needed by non-self-describing formats to correctly
/// parse enum variants, including the variant's structure kind and field count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumVariantHint {
    /// Name of the variant (e.g., "Some", "Pair", "Named")
    pub name: &'static str,
    /// The kind of struct this variant represents (Unit, Tuple, TupleStruct, or Struct)
    pub kind: facet_core::StructKind,
    /// Number of fields in this variant
    pub field_count: usize,
}

/// Hint for what scalar type is expected next.
///
/// Used by non-self-describing formats to know how to decode the next value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarTypeHint {
    /// Boolean (postcard: 0 or 1 byte)
    Bool,
    /// Unsigned 8-bit integer (postcard: raw byte)
    U8,
    /// Unsigned 16-bit integer (postcard: varint)
    U16,
    /// Unsigned 32-bit integer (postcard: varint)
    U32,
    /// Unsigned 64-bit integer (postcard: varint)
    U64,
    /// Unsigned 128-bit integer (postcard: varint)
    U128,
    /// Platform-sized unsigned integer (postcard: varint)
    Usize,
    /// Signed 8-bit integer (postcard: zigzag varint)
    I8,
    /// Signed 16-bit integer (postcard: zigzag varint)
    I16,
    /// Signed 32-bit integer (postcard: zigzag varint)
    I32,
    /// Signed 64-bit integer (postcard: zigzag varint)
    I64,
    /// Signed 128-bit integer (postcard: zigzag varint)
    I128,
    /// Platform-sized signed integer (postcard: zigzag varint)
    Isize,
    /// 32-bit float (postcard: 4 bytes little-endian)
    F32,
    /// 64-bit float (postcard: 8 bytes little-endian)
    F64,
    /// UTF-8 string (postcard: varint length + bytes)
    String,
    /// Raw bytes (postcard: varint length + bytes)
    Bytes,
    /// Character (postcard: UTF-8 encoded)
    Char,
}

/// Extension trait for parsers that support format-specific JIT (Tier 2).
///
/// Parsers implement this trait to enable the Tier 2 fast path, which
/// generates Cranelift IR that parses bytes directly instead of going
/// through the event abstraction.
///
/// # Requirements
///
/// Tier 2 requires:
/// - The full input slice must be available upfront
/// - The parser must be able to report and update its cursor position
/// - The parser must reset internal state when `jit_set_pos` is called
#[cfg(feature = "jit")]
pub trait FormatJitParser<'de>: FormatParser<'de> {
    /// The format-specific JIT emitter type.
    type FormatJit: crate::jit::JitFormat;

    /// Return the full input slice.
    fn jit_input(&self) -> &'de [u8];

    /// Return the current byte offset (cursor position).
    ///
    /// Returns `None` if there is buffered state (e.g., a peeked event)
    /// that makes the position ambiguous.
    fn jit_pos(&self) -> Option<usize>;

    /// Commit a new cursor position after Tier 2 execution succeeds.
    ///
    /// Must also invalidate/reset any internal scanning/tokenizer state
    /// so that subsequent parsing continues from `pos` consistently.
    fn jit_set_pos(&mut self, pos: usize);

    /// Return a format JIT emitter instance (usually a ZST).
    fn jit_format(&self) -> Self::FormatJit;

    /// Convert a Tier 2 error (code + position) into `Self::Error`.
    fn jit_error(&self, input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error;
}
