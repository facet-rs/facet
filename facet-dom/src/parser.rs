//! DOM parser trait.

use crate::DomEvent;

/// A parser that emits DOM events from a tree-structured document.
///
/// Implementations exist for HTML (using html5gum) and XML parsers.
pub trait DomParser<'de> {
    /// The error type for parsing failures.
    type Error: std::error::Error + 'static;

    /// Get the next event from the document.
    ///
    /// Returns `Ok(None)` when the document is fully parsed.
    fn next_event(&mut self) -> Result<Option<DomEvent<'de>>, Self::Error>;

    /// Peek at the next event without consuming it.
    fn peek_event(&mut self) -> Result<Option<&DomEvent<'de>>, Self::Error>;

    /// Skip the current node and all its descendants.
    ///
    /// This is used when encountering unknown elements that should be ignored.
    /// After calling this, the parser should be positioned after the matching `NodeEnd`.
    fn skip_node(&mut self) -> Result<(), Self::Error>;

    /// Get the current span in the source document, if available.
    fn current_span(&self) -> Option<facet_reflect::Span> {
        None
    }

    /// Whether this parser is lenient about text in unexpected places.
    ///
    /// HTML parsers return `true` - text without a corresponding field is silently discarded.
    /// XML parsers return `false` - text without a corresponding field is an error.
    fn is_lenient(&self) -> bool {
        false
    }

    /// Returns the format namespace for this parser (e.g., "xml", "html").
    ///
    /// This is used to select format-specific proxy types when a field has
    /// `#[facet(xml::proxy = XmlProxy)]` or similar format-namespaced proxies.
    ///
    /// Returns `None` by default, which falls back to format-agnostic proxies.
    fn format_namespace(&self) -> Option<&'static str> {
        None
    }
}
