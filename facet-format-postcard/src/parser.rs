//! Postcard parser implementing FormatParser and FormatJitParser.
//!
//! This is a Tier-2 only parser. The FormatParser methods return errors
//! because only JIT deserialization is supported.

use crate::error::{PostcardError, codes};
use facet_format::{FieldEvidence, FormatParser, ParseEvent, ProbeStream};

/// Postcard parser for Tier-2 JIT deserialization.
///
/// This parser only supports JIT mode. Calling non-JIT methods will return errors.
pub struct PostcardParser<'de> {
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    input: &'de [u8],
    pos: usize,
}

impl<'de> PostcardParser<'de> {
    /// Create a new postcard parser from input bytes.
    pub fn new(input: &'de [u8]) -> Self {
        Self { input, pos: 0 }
    }

    /// Create an "unsupported" error for non-JIT methods.
    fn unsupported_error(&self) -> PostcardError {
        PostcardError {
            code: codes::UNSUPPORTED,
            pos: self.pos,
            message: "PostcardParser is Tier-2 JIT only - FormatParser methods are not supported"
                .to_string(),
        }
    }
}

/// Stub probe stream for PostcardParser.
///
/// This is never actually used since we don't support non-JIT parsing.
pub struct PostcardProbe;

impl<'de> ProbeStream<'de> for PostcardProbe {
    type Error = PostcardError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        Err(PostcardError {
            code: codes::UNSUPPORTED,
            pos: 0,
            message: "PostcardParser is Tier-2 JIT only - ProbeStream methods are not supported"
                .to_string(),
        })
    }
}

impl<'de> FormatParser<'de> for PostcardParser<'de> {
    type Error = PostcardError;
    type Probe<'a>
        = PostcardProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        Err(self.unsupported_error())
    }

    fn peek_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        Err(self.unsupported_error())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        Err(self.unsupported_error())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        Err(self.unsupported_error())
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
