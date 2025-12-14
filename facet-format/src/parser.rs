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
}
