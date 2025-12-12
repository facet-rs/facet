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
}
