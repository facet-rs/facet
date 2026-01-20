//! Span tracking for source locations.

/// A span representing a range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct Span {
    /// Byte offset of the start (inclusive)
    pub start: u32,
    /// Byte offset of the end (exclusive)
    pub end: u32,
}

impl Span {
    /// Create a new span from start and end byte offsets.
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    /// Create an empty span at a position.
    #[inline]
    pub fn empty(pos: u32) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Length of this span in bytes.
    #[inline]
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Whether this span is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Extend this span to include another span.
    #[inline]
    pub fn extend(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Get the source text for this span.
    #[inline]
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }
}

impl From<std::ops::Range<u32>> for Span {
    fn from(range: std::ops::Range<u32>) -> Self {
        Span::new(range.start, range.end)
    }
}

impl From<Span> for std::ops::Range<usize> {
    fn from(span: Span) -> Self {
        span.start as usize..span.end as usize
    }
}
