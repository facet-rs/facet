use alloc::vec::Vec;
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

/// A Subspan variant of a Span
#[derive(Clone, Debug, PartialEq)]
pub struct Subspan {
    /// Offset from parent span's start
    pub offset: usize,
    /// Length of the subspan
    pub len: usize,
    /// Optional metadata (like delimiter information)
    pub meta: Option<SubspanMeta>,
}

/// Metadata about a subspan, providing context for how the subspan relates
/// to the parent span or other subspans.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SubspanMeta {
    /// Indicates the subspan is part of a delimited sequence,
    /// storing the delimiter character (e.g., ',' in "1,2,3")
    Delimiter(char),

    /// Indicates the subspan represents one side of a key-value pair
    /// (e.g., in "--key=value" or "-k=val")
    KeyValue,
    // Other metadata cases as needed...
}

/// Container for subspans based on span type
pub struct Substack<C> {
    spans: Option<Vec<Subspan>>,
    _marker: PhantomData<C>,
}

impl<C> Substack<C> {
    /// Initialise subspan stack as None
    pub fn new() -> Self {
        Substack {
            spans: None,
            _marker: PhantomData,
        }
    }

    /// Get all stored spans
    pub fn get(&self) -> &[Subspan] {
        match &self.spans {
            Some(spans) => spans,
            None => &[], // Return empty slice if no spans are stored
        }
    }

    /// Pop the most recently added subspan
    pub fn pop(&mut self) -> Option<Subspan> {
        if let Some(spans) = &mut self.spans {
            spans.pop()
        } else {
            None
        }
    }

    /// Clear all subspans
    pub fn clear(&mut self) {
        if let Some(spans) = &mut self.spans {
            spans.clear();
        }
    }
}

impl<C> Default for Substack<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> From<Vec<Subspan>> for Substack<C> {
    fn from(subspans: Vec<Subspan>) -> Self {
        Substack {
            spans: Some(subspans),
            _marker: PhantomData,
        }
    }
}

impl Substack<Raw> {
    /// Add a subspan for Raw spans
    pub fn add(&mut self, offset: usize, len: usize, meta: Option<SubspanMeta>) {
        if self.spans.is_none() {
            self.spans = Some(Vec::new());
        }

        if let Some(spans) = &mut self.spans {
            spans.push(Subspan { offset, len, meta });
        }
    }

    /// Add a simple subspan with just offset and length
    pub fn add_simple(&mut self, offset: usize, len: usize) {
        self.add(offset, len, None);
    }

    /// Add a delimiter subspan
    pub fn add_delimiter(&mut self, offset: usize, len: usize, delimiter: char) {
        self.add(offset, len, Some(SubspanMeta::Delimiter(delimiter)));
    }

    /// Add a key-value subspan
    pub fn add_key_value(&mut self, offset: usize, len: usize) {
        self.add(offset, len, Some(SubspanMeta::KeyValue));
    }
}

impl Substack<Cooked> {
    /// Add a span for Cooked spans (does nothing)
    pub fn add(&mut self, _offset: usize, _len: usize, _meta: Option<SubspanMeta>) {}

    /// Add a simple subspan (does nothing for Cooked)
    pub fn add_simple(&mut self, _offset: usize, _len: usize) {}

    /// Add a delimiter subspan (does nothing for Cooked)
    pub fn add_delimiter(&mut self, _offset: usize, _len: usize, _delimiter: char) {}

    /// Add a key-value subspan (does nothing for Cooked)
    pub fn add_key_value(&mut self, _offset: usize, _len: usize) {}
}

/// This trait allows the compiler to optimize away `Substack`-related code
/// for formats with span types that don't use subspans, making it zero-cost.
pub trait SubstackBehavior {
    /// Whether to use subspans in the `deserialize_wip` instruction stack loop.
    const USES_SUBSTACK: bool;
}

impl SubstackBehavior for Raw {
    const USES_SUBSTACK: bool = true;
}

impl SubstackBehavior for Cooked {
    const USES_SUBSTACK: bool = false;
}
