use core::fmt;

/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Starting position of the span in bytes
    pub start: Pos,
    /// Length of the span in bytes
    pub len: usize,
}

/// Trait for types that can be annotated with a Span.
pub trait Spannable: Sized {
    /// Annotate this value with a span, wrapping it in `Spanned<Self>`
    fn with_span(self, span: Span) -> Spanned<Self>;
}

impl<T> Spannable for T {
    fn with_span(self, span: Span) -> Spanned<Self> {
        Spanned { node: self, span }
    }
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

impl<T: fmt::Display> fmt::Display for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}-{}",
            self.node,
            self.span.start(),
            self.span.end()
        )
    }
}
