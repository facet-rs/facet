use core::fmt;
use core::marker::PhantomData;

/// A Cooked variant of a Span (byte indexed)
#[derive(Debug, PartialEq)]
pub enum Cooked {}

/// A Raw variant of a Span (format-specific index)
#[derive(Debug, PartialEq)]
pub enum Raw {}

/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Debug, PartialEq, Eq)]
pub struct Span<C = Cooked> {
    /// Starting position of the span in bytes
    pub start: Pos,
    /// Length of the span in bytes
    pub len: usize,
    /// Hold on to C
    _p: PhantomData<C>,
}

/// Trait for types that can be annotated with a Span.
pub trait Spannable<C = Cooked>: Sized {
    /// Annotate this value with a span, wrapping it in `Spanned<Self, C>`
    fn with_span(self, span: Span<C>) -> Spanned<Self, C>;
}

impl<T, C> Spannable<C> for T {
    fn with_span(self, span: Span<C>) -> Spanned<Self, C> {
        Spanned { node: self, span }
    }
}

impl<C> Span<C> {
    /// Creates a new span with the given start position and length
    pub fn new(start: Pos, len: usize) -> Self {
        Span {
            start,
            len,
            _p: PhantomData,
        }
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

impl<C> Default for Span<C> {
    fn default() -> Self {
        Span {
            start: 0,
            len: 0,
            _p: PhantomData,
        }
    }
}

/// A value of type `T` annotated with its `Span`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T, C = Cooked> {
    /// The actual data/value being wrapped
    pub node: T,
    /// The span information indicating the position and length in the source
    pub span: Span<C>,
}

impl<T: fmt::Display, C> fmt::Display for Spanned<T, C> {
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

// Copy + Clone not auto-derived for PhantomData
// https://stackoverflow.com/a/31371094/2668831

impl<C> Clone for Span<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for Span<C> {}
