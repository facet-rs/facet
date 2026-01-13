//! DOM event types for tree-based parsing.

use std::borrow::Cow;

/// Events emitted by a DOM parser.
///
/// These events represent the structure of a tree-based document (HTML/XML).
/// The event stream follows this pattern for each element:
///
/// ```text
/// NodeStart { tag: "div" }
///   Attribute { name: "class", value: "container" }
///   Attribute { name: "id", value: "main" }
///   ChildrenStart
///     Text("Hello ")
///     NodeStart { tag: "strong" }
///       ChildrenStart
///         Text("world")
///       ChildrenEnd
///     NodeEnd
///     Text("!")
///   ChildrenEnd
/// NodeEnd
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum DomEvent<'a> {
    /// Start of an element node.
    ///
    /// Followed by zero or more `Attribute` events, then `ChildrenStart`.
    NodeStart {
        /// The tag name (e.g., "div", "p", "my-component").
        tag: Cow<'a, str>,
        /// Optional namespace URI.
        namespace: Option<Cow<'a, str>>,
    },

    /// An attribute on the current element.
    ///
    /// Only valid between `NodeStart` and `ChildrenStart`.
    Attribute {
        /// Attribute name.
        name: Cow<'a, str>,
        /// Attribute value.
        value: Cow<'a, str>,
        /// Optional namespace URI.
        namespace: Option<Cow<'a, str>>,
    },

    /// Start of the children section.
    ///
    /// After this, expect `Text`, `NodeStart`, or `ChildrenEnd`.
    ChildrenStart,

    /// End of the children section.
    ChildrenEnd,

    /// End of an element node.
    ///
    /// Must be preceded by `ChildrenEnd` (or directly by attributes for void elements).
    NodeEnd,

    /// Text content.
    ///
    /// Only valid between `ChildrenStart` and `ChildrenEnd`.
    Text(Cow<'a, str>),

    /// A comment (usually ignored during deserialization).
    Comment(Cow<'a, str>),

    /// A processing instruction (XML) or DOCTYPE (HTML).
    ProcessingInstruction {
        /// Target (e.g., "xml" for `<?xml ...?>`).
        target: Cow<'a, str>,
        /// Data content.
        data: Cow<'a, str>,
    },

    /// DOCTYPE declaration (HTML5).
    Doctype(Cow<'a, str>),
}

impl<'a> DomEvent<'a> {
    /// Returns true if this is a `NodeStart` event.
    pub fn is_node_start(&self) -> bool {
        matches!(self, DomEvent::NodeStart { .. })
    }

    /// Returns true if this is a `NodeEnd` event.
    pub fn is_node_end(&self) -> bool {
        matches!(self, DomEvent::NodeEnd)
    }

    /// Returns true if this is an `Attribute` event.
    pub fn is_attribute(&self) -> bool {
        matches!(self, DomEvent::Attribute { .. })
    }

    /// Returns true if this is a `Text` event.
    pub fn is_text(&self) -> bool {
        matches!(self, DomEvent::Text(_))
    }

    /// Returns true if this is `ChildrenStart`.
    pub fn is_children_start(&self) -> bool {
        matches!(self, DomEvent::ChildrenStart)
    }

    /// Returns true if this is `ChildrenEnd`.
    pub fn is_children_end(&self) -> bool {
        matches!(self, DomEvent::ChildrenEnd)
    }
}
