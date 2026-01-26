/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Starting position of the span in bytes
    pub start: Pos,
    /// Length of the span in bytes
    pub len: usize,
}

impl Span {
    /// Creates a new span with the given start position and length
    pub const fn new(start: Pos, len: usize) -> Self {
        Span { start, len }
    }

    /// Length of the span
    pub const fn len(&self) -> usize {
        self.len
    }
}
