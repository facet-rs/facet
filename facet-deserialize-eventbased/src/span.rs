use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::str;

/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Starting position of the span in bytes
    start: Pos,
    /// Length of the span in bytes
    len: usize,
}

impl Span {
    /// Creates a new span with the given start position and length
    pub fn new(start: Pos, len: usize) -> Self {
        Span { start, len }
    }
    /// Start position of the span
    pub fn start(&self) -> Pos {
        self.start
    }
    /// Length of the span
    pub fn len(&self) -> usize {
        self.len
    }
    /// Returns `true` if this span has zero length
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// End position (start + length)
    pub fn end(&self) -> Pos {
        self.start + self.len
    }
}

/// A value of type `T` annotated with its `Span`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The actual data/value being wrapped
    pub node: T,
    /// The span information indicating the position and length in the source
    pub span: Span,
}
