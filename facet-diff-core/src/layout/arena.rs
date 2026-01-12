//! Format arena for string storage.

use std::fmt;
use unicode_width::UnicodeWidthStr;

/// A span into the format arena's buffer.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// Start byte offset
    pub start: u32,
    /// End byte offset (exclusive)
    pub end: u32,
}

impl Span {
    /// Byte length of the span
    #[inline]
    pub const fn len(self) -> usize {
        (self.end - self.start) as usize
    }

    /// Check if span is empty
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Arena for formatted strings.
///
/// All scalar values are formatted once into this buffer and referenced by [`Span`].
/// This avoids per-value allocations and allows measuring display width at format time.
pub struct FormatArena {
    buf: String,
}

impl FormatArena {
    /// Create a new arena with pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: String::with_capacity(cap),
        }
    }

    /// Create a new arena with default capacity.
    pub fn new() -> Self {
        // 4KB default - enough for most diffs
        Self::with_capacity(4096)
    }

    /// Format something into the arena, returning span and display width.
    ///
    /// The closure receives a `&mut String` to write into. The span returned
    /// covers exactly what was written. Display width is calculated using
    /// `unicode-width` for proper handling of CJK characters, emoji, etc.
    pub fn format<F>(&mut self, f: F) -> (Span, usize)
    where
        F: FnOnce(&mut String) -> fmt::Result,
    {
        let start = self.buf.len();
        f(&mut self.buf).expect("formatting to String cannot fail");
        let end = self.buf.len();
        let span = Span {
            start: start as u32,
            end: end as u32,
        };
        let width = self.buf[start..end].width();
        (span, width)
    }

    /// Push a string directly, returning span and display width.
    pub fn push_str(&mut self, s: &str) -> (Span, usize) {
        let start = self.buf.len();
        self.buf.push_str(s);
        let span = Span {
            start: start as u32,
            end: self.buf.len() as u32,
        };
        let width = s.width();
        (span, width)
    }

    /// Retrieve the string for a span.
    #[inline]
    pub fn get(&self, span: Span) -> &str {
        &self.buf[span.start as usize..span.end as usize]
    }

    /// Current size of the buffer in bytes.
    pub const fn len(&self) -> usize {
        self.buf.len()
    }

    /// Check if the arena is empty.
    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for FormatArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

    #[test]
    fn test_format_simple() {
        let mut arena = FormatArena::new();
        let (span, width) = arena.format(|w| write!(w, "hello"));
        assert_eq!(arena.get(span), "hello");
        assert_eq!(width, 5);
    }

    #[test]
    fn test_format_multiple() {
        let mut arena = FormatArena::new();

        let (s1, w1) = arena.format(|w| write!(w, "hello"));
        let (s2, w2) = arena.format(|w| write!(w, "world"));

        assert_eq!(arena.get(s1), "hello");
        assert_eq!(arena.get(s2), "world");
        assert_eq!(w1, 5);
        assert_eq!(w2, 5);

        // Spans don't overlap
        assert_eq!(s1.end, s2.start);
    }

    #[test]
    fn test_format_unicode() {
        let mut arena = FormatArena::new();

        // CJK characters are typically 2 columns wide
        let (span, width) = arena.format(|w| write!(w, "日本語"));
        assert_eq!(arena.get(span), "日本語");
        assert_eq!(width, 6); // 3 chars * 2 columns each

        // Emoji
        let (span, width) = arena.format(|w| write!(w, "🦀"));
        assert_eq!(arena.get(span), "🦀");
        assert_eq!(width, 2); // emoji is typically 2 columns
    }

    #[test]
    fn test_push_str() {
        let mut arena = FormatArena::new();
        let (span, width) = arena.push_str("test");
        assert_eq!(arena.get(span), "test");
        assert_eq!(width, 4);
    }

    #[test]
    fn test_span_len() {
        let span = Span { start: 10, end: 20 };
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());

        let empty = Span { start: 5, end: 5 };
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }
}
