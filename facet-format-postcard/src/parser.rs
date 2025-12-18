//! Postcard parser implementing FormatParser and FormatJitParser.
//!
//! This is a Tier-2 only parser - the FormatParser methods will panic
//! as they are not implemented. Use JIT deserialization only.

use crate::error::PostcardError;
use facet_format::{FieldEvidence, FormatParser, ParseEvent, ProbeStream};

/// Postcard parser for Tier-2 JIT deserialization.
///
/// This parser only supports JIT mode. Calling non-JIT methods will panic.
pub struct PostcardParser<'de> {
    input: &'de [u8],
    pos: usize,
}

impl<'de> PostcardParser<'de> {
    /// Create a new postcard parser from input bytes.
    pub fn new(input: &'de [u8]) -> Self {
        Self { input, pos: 0 }
    }
}

/// Stub probe stream for PostcardParser.
///
/// This is never actually used since we don't support non-JIT parsing.
pub struct PostcardProbe;

impl<'de> ProbeStream<'de> for PostcardProbe {
    type Error = PostcardError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        panic!("PostcardParser is Tier-2 JIT only - FormatParser methods are not implemented")
    }
}

impl<'de> FormatParser<'de> for PostcardParser<'de> {
    type Error = PostcardError;
    type Probe<'a>
        = PostcardProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        panic!("PostcardParser is Tier-2 JIT only - FormatParser methods are not implemented")
    }

    fn peek_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        panic!("PostcardParser is Tier-2 JIT only - FormatParser methods are not implemented")
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        panic!("PostcardParser is Tier-2 JIT only - FormatParser methods are not implemented")
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        panic!("PostcardParser is Tier-2 JIT only - FormatParser methods are not implemented")
    }
}

#[cfg(feature = "jit")]
impl<'de> facet_format::FormatJitParser<'de> for PostcardParser<'de> {
    type FormatJit = crate::jit::PostcardJitFormat;

    fn jit_input(&self) -> &'de [u8] {
        self.input
    }

    fn jit_pos(&self) -> Option<usize> {
        // Postcard parser is always in a clean state for JIT
        // (no peeked events, no stack, etc.)
        Some(self.pos)
    }

    fn jit_set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn jit_format(&self) -> Self::FormatJit {
        crate::jit::PostcardJitFormat
    }

    fn jit_error(&self, _input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error {
        PostcardError::from_code(error_code, error_pos)
    }
}
