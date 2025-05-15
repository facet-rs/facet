//! Debug utilities for deserialization formats.

use crate::span::Span;
use alloc::borrow::Cow;

/// Trait for handling input data in error reporting and debugging.
/// Provides methods to convert input slices to human-readable strings
/// and to create byte arrays for error storage.
pub trait InputDebug {
    /// Returns a string representation of the input at the given span.
    /// Used for pretty error messages in diagnostics.
    fn slice(&self, span: Span) -> &str;

    /// Converts the input to a borrowed or owned byte array.
    /// Used when constructing error objects that need to store input data.
    fn as_cow(&self) -> Cow<'_, [u8]>;
}

impl InputDebug for [u8] {
    fn slice(&self, span: Span) -> &str {
        core::str::from_utf8(&self[span.start()..span.end()]).unwrap_or("<invalid utf8>")
    }

    fn as_cow(&self) -> Cow<'_, [u8]> {
        alloc::borrow::Cow::Borrowed(self)
    }
}

impl InputDebug for [&str] {
    fn slice(&self, span: Span) -> &str {
        // Simplified implementation - just return the argument at that position if it exists
        if span.start() < self.len() {
            self[span.start()]
        } else {
            "<out of bounds>"
        }
    }

    fn as_cow(&self) -> Cow<'_, [u8]> {
        // For CLI args, we join them for error reporting
        let joined = self.join(" ");
        Cow::Owned(joined.into_bytes())
    }
}

/// Helper function for error creation
pub fn input_to_cow<'input, I>(input: &'input I) -> Cow<'input, [u8]>
where
    I: ?Sized + 'input + InputDebug,
{
    input.as_cow()
}
