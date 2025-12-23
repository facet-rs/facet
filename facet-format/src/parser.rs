use crate::FieldEvidence;

/// Streaming cursor that yields serialized fields for solver probing.
pub trait ProbeStream<'de> {
    /// Parser-specific error type.
    type Error;

    /// Produce the next field evidence entry. Returning `Ok(None)` indicates
    /// the parser ran out of evidence or the format does not need additional
    /// passes.
    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error>;
}

/// Streaming parser for a specific wire format.
pub trait FormatParser<'de> {
    /// Parser-specific error type.
    type Error;

    /// Evidence cursor type produced by [`FormatParser::begin_probe`].
    type Probe<'a>: ProbeStream<'de, Error = Self::Error>
    where
        Self: 'a;

    /// Read the next parse event.
    fn next_event(&mut self) -> Result<crate::ParseEvent<'de>, Self::Error>;

    /// Peek at the next event without consuming it.
    fn peek_event(&mut self) -> Result<crate::ParseEvent<'de>, Self::Error>;

    /// Skip the current value (for unknown fields, etc.).
    fn skip_value(&mut self) -> Result<(), Self::Error>;

    /// Begin evidence collection for untagged-enum resolution.
    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error>;

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

    /// Hint to the parser that an enum is expected, providing variant information.
    ///
    /// For non-self-describing formats (like postcard), this allows the parser
    /// to read the variant discriminant (varint) and map it to the variant name.
    ///
    /// The `variant_names` slice contains the variant names in declaration order,
    /// matching the indices used in the wire format.
    ///
    /// Self-describing formats can ignore this hint (they include variant names
    /// in the wire format).
    fn hint_enum(&mut self, _variant_names: &[&str]) {
        // Default: ignore (self-describing formats don't need this)
    }
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
    /// Signed 8-bit integer (postcard: zigzag varint)
    I8,
    /// Signed 16-bit integer (postcard: zigzag varint)
    I16,
    /// Signed 32-bit integer (postcard: zigzag varint)
    I32,
    /// Signed 64-bit integer (postcard: zigzag varint)
    I64,
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
