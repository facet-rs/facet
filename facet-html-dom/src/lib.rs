//! Typed HTML element definitions for use with `facet-html`.
//!
//! This crate provides Facet-derived types for all standard HTML5 elements,
//! allowing you to parse and serialize HTML documents with full type safety.
//!
//! # Quick Start
//!
//! ```rust
//! use facet_html_dom::{Html, Body, Div, P, FlowContent};
//!
//! // Parse an HTML document
//! let html_source = r#"
//!     <!DOCTYPE html>
//!     <html>
//!         <body>
//!             <div class="container">
//!                 <p>Hello, world!</p>
//!             </div>
//!         </body>
//!     </html>
//! "#;
//!
//! let doc: Html = facet_html::from_str(html_source).unwrap();
//!
//! // Access the parsed structure
//! if let Some(body) = &doc.body {
//!     for child in &body.children {
//!         if let FlowContent::Div(div) = child {
//!             println!("Found div with class: {:?}", div.attrs.class);
//!         }
//!     }
//! }
//!
//! // Serialize back to HTML
//! let output = facet_html::to_string_pretty(&doc).unwrap();
//! ```
//!
//! # Content Models
//!
//! HTML elements are organized by their content model:
//!
//! - [`FlowContent`] - Block and inline elements that can appear in `<body>`, `<div>`, etc.
//! - [`PhrasingContent`] - Inline elements that can appear in `<p>`, `<span>`, `<a>`, etc.
//!
//! These enums allow mixed content with proper nesting validation at the type level.
//!
//! # Global Attributes
//!
//! All elements include [`GlobalAttrs`] via the `attrs` field, which provides:
//! - Standard attributes: `id`, `class`, `style`, `lang`, `dir`, etc.
//! - Common event handlers: `onclick`, `onchange`, `onfocus`, etc.
//! - An `extra` field that captures unknown attributes like `data-*` and `aria-*`
//!
//! # Custom Elements
//!
//! Unknown HTML elements (like `<my-component>` or syntax highlighting tags like `<a-k>`)
//! are captured via [`CustomElement`] and [`CustomPhrasingElement`], preserving their
//! tag names, attributes, and children during parse/serialize roundtrips.
//!
//! # Element Categories
//!
//! Elements are organized by category:
//! - **Document**: [`Html`], [`Head`], [`Body`]
//! - **Metadata**: [`Title`], [`Base`], [`Link`], [`Meta`], [`Style`]
//! - **Sections**: [`Header`], [`Footer`], [`Main`], [`Article`], [`Section`], [`Nav`], [`Aside`]
//! - **Headings**: [`H1`], [`H2`], [`H3`], [`H4`], [`H5`], [`H6`]
//! - **Grouping**: [`P`], [`Div`], [`Span`], [`Pre`], [`Blockquote`], [`Ol`], [`Ul`], [`Li`], etc.
//! - **Text-level**: [`A`], [`Em`], [`Strong`], [`Code`], [`Br`], [`Wbr`], etc.
//! - **Embedded**: [`Img`], [`Iframe`], [`Video`], [`Audio`], [`Source`], [`Picture`]
//! - **Tables**: [`Table`], [`Thead`], [`Tbody`], [`Tr`], [`Th`], [`Td`], etc.
//! - **Forms**: [`Form`], [`Input`], [`Button`], [`Select`], [`OptionElement`], [`Textarea`], [`Label`]
//! - **Interactive**: [`Details`], [`Summary`], [`Dialog`]
//! - **Scripting**: [`Script`], [`Noscript`], [`Template`], [`Canvas`]

use facet::Facet;
use facet_html as html;

// =============================================================================
// Global Attributes (common to all HTML elements)
// =============================================================================

/// Global attributes that can appear on any HTML element.
///
/// This includes standard HTML global attributes and common event handlers.
/// Unknown attributes (like data-*, aria-*, and less common event handlers)
/// are captured in the `extra` field.
#[derive(Default, Facet)]
#[facet(default, skip_all_unless_truthy)]
pub struct GlobalAttrs {
    // Standard global attributes
    /// Unique identifier for the element.
    #[facet(html::attribute, default)]
    pub id: Option<String>,
    /// CSS class names.
    #[facet(html::attribute, default)]
    pub class: Option<String>,
    /// Inline CSS styles.
    #[facet(html::attribute, default)]
    pub style: Option<String>,
    /// Advisory title/tooltip.
    /// Note: Named `tooltip` in Rust to avoid collision with `<title>` child element in Head.
    /// Serializes as the `title` HTML attribute.
    #[facet(html::attribute, default, rename = "title")]
    pub tooltip: Option<String>,
    /// Language of the element's content.
    #[facet(html::attribute, default)]
    pub lang: Option<String>,
    /// Text directionality (ltr, rtl, auto).
    #[facet(html::attribute, default)]
    pub dir: Option<String>,
    /// Whether the element is hidden.
    #[facet(html::attribute, default)]
    pub hidden: Option<String>,
    /// Tab order of the element.
    #[facet(html::attribute, default)]
    pub tabindex: Option<String>,
    /// Access key for the element.
    #[facet(html::attribute, default)]
    pub accesskey: Option<String>,
    /// Whether the element is draggable.
    #[facet(html::attribute, default)]
    pub draggable: Option<String>,
    /// Whether the element is editable.
    #[facet(html::attribute, default)]
    pub contenteditable: Option<String>,
    /// Whether spellchecking is enabled.
    #[facet(html::attribute, default)]
    pub spellcheck: Option<String>,
    /// Whether the element should be translated.
    #[facet(html::attribute, default)]
    pub translate: Option<String>,
    /// ARIA role.
    #[facet(html::attribute, default)]
    pub role: Option<String>,

    // Common event handlers (most frequently used)
    /// Script to run on mouse click.
    #[facet(html::attribute, default)]
    pub onclick: Option<String>,
    /// Script to run on mouse double-click.
    #[facet(html::attribute, default)]
    pub ondblclick: Option<String>,
    /// Script to run when mouse button is pressed.
    #[facet(html::attribute, default)]
    pub onmousedown: Option<String>,
    /// Script to run when mouse pointer moves over element.
    #[facet(html::attribute, default)]
    pub onmouseover: Option<String>,
    /// Script to run when mouse pointer moves out of element.
    #[facet(html::attribute, default)]
    pub onmouseout: Option<String>,
    /// Script to run when mouse button is released.
    #[facet(html::attribute, default)]
    pub onmouseup: Option<String>,
    /// Script to run when mouse enters element.
    #[facet(html::attribute, default)]
    pub onmouseenter: Option<String>,
    /// Script to run when mouse leaves element.
    #[facet(html::attribute, default)]
    pub onmouseleave: Option<String>,
    /// Script to run when key is pressed down.
    #[facet(html::attribute, default)]
    pub onkeydown: Option<String>,
    /// Script to run when key is released.
    #[facet(html::attribute, default)]
    pub onkeyup: Option<String>,
    /// Script to run when element receives focus.
    #[facet(html::attribute, default)]
    pub onfocus: Option<String>,
    /// Script to run when element loses focus.
    #[facet(html::attribute, default)]
    pub onblur: Option<String>,
    /// Script to run when value changes.
    #[facet(html::attribute, default)]
    pub onchange: Option<String>,
    /// Script to run on input.
    #[facet(html::attribute, default)]
    pub oninput: Option<String>,
    /// Script to run when form is submitted.
    #[facet(html::attribute, default)]
    pub onsubmit: Option<String>,
    /// Script to run when resource is loaded.
    #[facet(html::attribute, default)]
    pub onload: Option<String>,
    /// Script to run when error occurs.
    #[facet(html::attribute, default)]
    pub onerror: Option<String>,
    /// Script to run when element is scrolled.
    #[facet(html::attribute, default)]
    pub onscroll: Option<String>,
    /// Script to run on context menu (right-click).
    #[facet(html::attribute, default)]
    pub oncontextmenu: Option<String>,

    // Catch-all for unknown attributes (data-*, aria-*, less common events, etc.)
    /// Extra attributes not explicitly modeled.
    /// Includes data-* attributes, aria-* attributes, and less common event handlers.
    /// Keys are the full attribute names as they appear in HTML.
    /// Uses BTreeMap for deterministic serialization order.
    #[facet(flatten, default)]
    pub extra: std::collections::BTreeMap<String, String>,
}

// =============================================================================
// Document Structure
// =============================================================================

/// The root HTML document element.
#[derive(Default, Facet)]
#[facet(rename = "html")]
pub struct Html {
    /// DOCTYPE declaration name (e.g., "html" for `<!DOCTYPE html>`).
    /// When present, the serializer will emit `<!DOCTYPE {name}>` before the html element.
    /// Set to `Some("html".to_string())` for standard HTML5 documents.
    /// This is handled specially by the HTML parser/serializer using the "doctype" pseudo-attribute.
    #[facet(html::attribute, default)]
    pub doctype: Option<String>,
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Document head.
    #[facet(default)]
    pub head: Option<Head>,
    /// Document body.
    ///
    /// When marked with `other`, this field acts as a fallback: if the root element
    /// is not `<html>`, the content is deserialized into this Body field instead.
    /// This enables parsing HTML fragments (like `<div>...</div>`) into the `Html` type.
    #[facet(other, default)]
    pub body: Option<Body>,
}

/// The document head containing metadata.
#[derive(Default, Facet)]
#[facet(rename = "head")]
pub struct Head {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements (metadata content).
    #[facet(flatten, default)]
    pub children: Vec<MetadataContent>,
}

impl Head {
    /// Get the title element if present.
    pub fn title(&self) -> Option<&Title> {
        self.children.iter().find_map(|c| match c {
            MetadataContent::Title(t) => Some(t),
            _ => None,
        })
    }

    /// Get all meta elements.
    pub fn meta(&self) -> impl Iterator<Item = &Meta> {
        self.children.iter().filter_map(|c| match c {
            MetadataContent::Meta(m) => Some(m),
            _ => None,
        })
    }

    /// Get all link elements.
    pub fn links(&self) -> impl Iterator<Item = &Link> {
        self.children.iter().filter_map(|c| match c {
            MetadataContent::Link(l) => Some(l),
            _ => None,
        })
    }

    /// Get all style elements.
    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.children.iter().filter_map(|c| match c {
            MetadataContent::Style(s) => Some(s),
            _ => None,
        })
    }

    /// Get all script elements.
    pub fn scripts(&self) -> impl Iterator<Item = &Script> {
        self.children.iter().filter_map(|c| match c {
            MetadataContent::Script(s) => Some(s),
            _ => None,
        })
    }
}

/// The document body containing visible content.
#[derive(Default, Facet)]
#[facet(rename = "body")]
pub struct Body {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements (mixed content).
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Metadata Elements
// =============================================================================

/// The document title.
#[derive(Default, Facet)]
#[facet(rename = "title")]
pub struct Title {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Text content of the title.
    #[facet(html::text, default)]
    pub text: String,
}

/// Base URL for relative URLs.
#[derive(Default, Facet)]
#[facet(rename = "base")]
pub struct Base {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Base URL.
    #[facet(html::attribute, default)]
    pub href: Option<String>,
    /// Default browsing context.
    #[facet(html::attribute, default)]
    pub target: Option<String>,
}

/// External resource link.
#[derive(Default, Facet)]
#[facet(rename = "link")]
pub struct Link {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL of the linked resource.
    #[facet(html::attribute, default)]
    pub href: Option<String>,
    /// Relationship type.
    #[facet(html::attribute, default)]
    pub rel: Option<String>,
    /// MIME type of the resource.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Media query for the resource.
    #[facet(html::attribute, default)]
    pub media: Option<String>,
    /// Integrity hash.
    #[facet(html::attribute, default)]
    pub integrity: Option<String>,
    /// Crossorigin attribute.
    #[facet(html::attribute, default)]
    pub crossorigin: Option<String>,
    /// Resource sizes (for icons).
    #[facet(html::attribute, default)]
    pub sizes: Option<String>,
    /// Alternative stylesheet title.
    #[facet(html::attribute, default, rename = "as")]
    pub as_: Option<String>,
}

/// Document metadata.
#[derive(Default, Facet)]
#[facet(rename = "meta")]
pub struct Meta {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Metadata name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Metadata content.
    #[facet(html::attribute, default)]
    pub content: Option<String>,
    /// Character encoding.
    #[facet(html::attribute, default)]
    pub charset: Option<String>,
    /// Pragma directive.
    #[facet(html::attribute, default, rename = "http-equiv")]
    pub http_equiv: Option<String>,
    /// Property (for Open Graph, etc.).
    #[facet(html::attribute, default)]
    pub property: Option<String>,
}

/// Inline stylesheet.
#[derive(Default, Facet)]
#[facet(rename = "style")]
pub struct Style {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Media query.
    #[facet(html::attribute, default)]
    pub media: Option<String>,
    /// MIME type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// CSS content.
    #[facet(html::text, default)]
    pub text: String,
}

// =============================================================================
// Section Elements
// =============================================================================

/// Page header.
#[derive(Default, Facet)]
#[facet(rename = "header")]
pub struct Header {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Page or section footer.
#[derive(Default, Facet)]
#[facet(rename = "footer")]
pub struct Footer {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Main content area.
#[derive(Default, Facet)]
#[facet(rename = "main")]
pub struct Main {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Self-contained article.
#[derive(Default, Facet)]
#[facet(rename = "article")]
pub struct Article {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Generic section.
#[derive(Default, Facet)]
#[facet(rename = "section")]
pub struct Section {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Navigation section.
#[derive(Default, Facet)]
#[facet(rename = "nav")]
pub struct Nav {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Sidebar content.
#[derive(Default, Facet)]
#[facet(rename = "aside")]
pub struct Aside {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Address/contact information.
#[derive(Default, Facet)]
#[facet(rename = "address")]
pub struct Address {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Heading Elements
// =============================================================================

/// Level 1 heading.
#[derive(Default, Facet)]
#[facet(rename = "h1")]
pub struct H1 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Level 2 heading.
#[derive(Default, Facet)]
#[facet(rename = "h2")]
pub struct H2 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Level 3 heading.
#[derive(Default, Facet)]
#[facet(rename = "h3")]
pub struct H3 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Level 4 heading.
#[derive(Default, Facet)]
#[facet(rename = "h4")]
pub struct H4 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Level 5 heading.
#[derive(Default, Facet)]
#[facet(rename = "h5")]
pub struct H5 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Level 6 heading.
#[derive(Default, Facet)]
#[facet(rename = "h6")]
pub struct H6 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

// =============================================================================
// Grouping Content
// =============================================================================

/// Paragraph.
#[derive(Default, Facet)]
#[facet(rename = "p")]
pub struct P {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Generic container (block).
#[derive(Default, Facet)]
#[facet(rename = "div")]
pub struct Div {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Generic container (inline).
#[derive(Default, Facet)]
#[facet(rename = "span")]
pub struct Span {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Preformatted text.
#[derive(Default, Facet)]
#[facet(rename = "pre")]
pub struct Pre {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Block quotation.
#[derive(Default, Facet)]
#[facet(rename = "blockquote")]
pub struct Blockquote {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Citation URL.
    #[facet(html::attribute, default)]
    pub cite: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Ordered list.
#[derive(Default, Facet)]
#[facet(rename = "ol")]
pub struct Ol {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Starting number.
    #[facet(html::attribute, default)]
    pub start: Option<String>,
    /// Numbering type (1, a, A, i, I).
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Reversed order.
    #[facet(html::attribute, default)]
    pub reversed: Option<String>,
    /// List items.
    #[facet(html::elements, default)]
    pub li: Vec<Li>,
}

/// Unordered list.
#[derive(Default, Facet)]
#[facet(rename = "ul")]
pub struct Ul {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// List items.
    #[facet(html::elements, default)]
    pub li: Vec<Li>,
}

/// List item.
#[derive(Default, Facet)]
#[facet(rename = "li")]
pub struct Li {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Value (for ol).
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Description list.
#[derive(Default, Facet)]
#[facet(rename = "dl")]
pub struct Dl {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Terms and descriptions (mixed dt/dd in order).
    #[facet(flatten, default)]
    pub children: Vec<DlContent>,
}

/// Content types that can appear inside a description list.
///
/// Note: Text content is silently discarded (per HTML spec, `<dl>` only contains `<dt>`/`<dd>`).
#[derive(Facet)]
#[repr(u8)]
pub enum DlContent {
    /// Description term.
    #[facet(rename = "dt")]
    Dt(Dt),
    /// Description details.
    #[facet(rename = "dd")]
    Dd(Dd),
}

/// Description term.
#[derive(Default, Facet)]
#[facet(rename = "dt")]
pub struct Dt {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Description details.
#[derive(Default, Facet)]
#[facet(rename = "dd")]
pub struct Dd {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Figure with optional caption.
#[derive(Default, Facet)]
#[facet(rename = "figure")]
pub struct Figure {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Figure caption.
    #[facet(default)]
    pub figcaption: Option<Figcaption>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Figure caption.
#[derive(Default, Facet)]
#[facet(rename = "figcaption")]
pub struct Figcaption {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Horizontal rule (thematic break).
#[derive(Default, Facet)]
#[facet(rename = "hr")]
pub struct Hr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
}

// =============================================================================
// Text-level Semantics
// =============================================================================

/// Hyperlink.
#[derive(Default, Facet)]
#[facet(rename = "a")]
pub struct A {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(html::attribute, default)]
    pub href: Option<String>,
    /// Target browsing context.
    #[facet(html::attribute, default)]
    pub target: Option<String>,
    /// Relationship.
    #[facet(html::attribute, default)]
    pub rel: Option<String>,
    /// Download filename.
    #[facet(html::attribute, default)]
    pub download: Option<String>,
    /// MIME type hint.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Language of linked resource.
    #[facet(html::attribute, default)]
    pub hreflang: Option<String>,
    /// Referrer policy.
    #[facet(html::attribute, default)]
    pub referrerpolicy: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Emphasis.
#[derive(Default, Facet)]
#[facet(rename = "em")]
pub struct Em {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Strong importance.
#[derive(Default, Facet)]
#[facet(rename = "strong")]
pub struct Strong {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Small print.
#[derive(Default, Facet)]
#[facet(rename = "small")]
pub struct Small {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Strikethrough (no longer accurate).
#[derive(Default, Facet)]
#[facet(rename = "s")]
pub struct S {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Citation.
#[derive(Default, Facet)]
#[facet(rename = "cite")]
pub struct Cite {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Inline quotation.
#[derive(Default, Facet)]
#[facet(rename = "q")]
pub struct Q {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Citation URL.
    #[facet(html::attribute, default)]
    pub cite: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Definition term.
#[derive(Default, Facet)]
#[facet(rename = "dfn")]
pub struct Dfn {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Abbreviation.
#[derive(Default, Facet)]
#[facet(rename = "abbr")]
pub struct Abbr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Ruby annotation (for East Asian typography).
#[derive(Default, Facet)]
#[facet(rename = "ruby")]
pub struct Ruby {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Data with machine-readable value.
#[derive(Default, Facet)]
#[facet(rename = "data")]
pub struct Data {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Machine-readable value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Time.
#[derive(Default, Facet)]
#[facet(rename = "time")]
pub struct Time {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Machine-readable datetime.
    #[facet(html::attribute, default)]
    pub datetime: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Code fragment.
#[derive(Default, Facet)]
#[facet(rename = "code")]
pub struct Code {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Variable.
#[derive(Default, Facet)]
#[facet(rename = "var")]
pub struct Var {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Sample output.
#[derive(Default, Facet)]
#[facet(rename = "samp")]
pub struct Samp {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Keyboard input.
#[derive(Default, Facet)]
#[facet(rename = "kbd")]
pub struct Kbd {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Subscript.
#[derive(Default, Facet)]
#[facet(rename = "sub")]
pub struct Sub {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Superscript.
#[derive(Default, Facet)]
#[facet(rename = "sup")]
pub struct Sup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Italic.
#[derive(Default, Facet)]
#[facet(rename = "i")]
pub struct I {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Bold.
#[derive(Default, Facet)]
#[facet(rename = "b")]
pub struct B {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Underline.
#[derive(Default, Facet)]
#[facet(rename = "u")]
pub struct U {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Highlighted text.
#[derive(Default, Facet)]
#[facet(rename = "mark")]
pub struct Mark {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Bidirectional isolation.
#[derive(Default, Facet)]
#[facet(rename = "bdi")]
pub struct Bdi {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Bidirectional override.
#[derive(Default, Facet)]
#[facet(rename = "bdo")]
pub struct Bdo {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Line break.
#[derive(Default, Facet)]
#[facet(rename = "br")]
pub struct Br {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
}

/// Word break opportunity.
#[derive(Default, Facet)]
#[facet(rename = "wbr")]
pub struct Wbr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
}

// =============================================================================
// Embedded Content
// =============================================================================

/// Image.
#[derive(Default, Facet)]
#[facet(rename = "img")]
pub struct Img {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Image URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Alternative text.
    #[facet(html::attribute, default)]
    pub alt: Option<String>,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// Srcset for responsive images.
    #[facet(html::attribute, default)]
    pub srcset: Option<String>,
    /// Sizes attribute.
    #[facet(html::attribute, default)]
    pub sizes: Option<String>,
    /// Loading behavior.
    #[facet(html::attribute, default)]
    pub loading: Option<String>,
    /// Decoding hint.
    #[facet(html::attribute, default)]
    pub decoding: Option<String>,
    /// Crossorigin.
    #[facet(html::attribute, default)]
    pub crossorigin: Option<String>,
    /// Referrer policy.
    #[facet(html::attribute, default)]
    pub referrerpolicy: Option<String>,
    /// Usemap reference.
    #[facet(html::attribute, default)]
    pub usemap: Option<String>,
    /// Whether this is a server-side image map.
    #[facet(html::attribute, default)]
    pub ismap: Option<String>,
}

/// Inline frame.
#[derive(Default, Facet)]
#[facet(rename = "iframe")]
pub struct Iframe {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Srcdoc content.
    #[facet(html::attribute, default)]
    pub srcdoc: Option<String>,
    /// Frame name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// Sandbox restrictions.
    #[facet(html::attribute, default)]
    pub sandbox: Option<String>,
    /// Feature policy.
    #[facet(html::attribute, default)]
    pub allow: Option<String>,
    /// Fullscreen allowed.
    #[facet(html::attribute, default)]
    pub allowfullscreen: Option<String>,
    /// Loading behavior.
    #[facet(html::attribute, default)]
    pub loading: Option<String>,
    /// Referrer policy.
    #[facet(html::attribute, default)]
    pub referrerpolicy: Option<String>,
}

/// Embedded object.
#[derive(Default, Facet)]
#[facet(rename = "object")]
pub struct Object {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Data URL.
    #[facet(html::attribute, default)]
    pub data: Option<String>,
    /// MIME type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// Usemap reference.
    #[facet(html::attribute, default)]
    pub usemap: Option<String>,
    /// Fallback content.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Video player.
#[derive(Default, Facet)]
#[facet(rename = "video")]
pub struct Video {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Video URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Poster image.
    #[facet(html::attribute, default)]
    pub poster: Option<String>,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// Show controls.
    #[facet(html::attribute, default)]
    pub controls: Option<String>,
    /// Autoplay.
    #[facet(html::attribute, default)]
    pub autoplay: Option<String>,
    /// Loop playback.
    #[facet(html::attribute, default, rename = "loop")]
    pub loop_: Option<String>,
    /// Muted by default.
    #[facet(html::attribute, default)]
    pub muted: Option<String>,
    /// Preload behavior.
    #[facet(html::attribute, default)]
    pub preload: Option<String>,
    /// Plays inline (iOS).
    #[facet(html::attribute, default)]
    pub playsinline: Option<String>,
    /// Crossorigin.
    #[facet(html::attribute, default)]
    pub crossorigin: Option<String>,
    /// Source elements.
    #[facet(html::elements, default)]
    pub source: Vec<Source>,
    /// Track elements.
    #[facet(html::elements, default)]
    pub track: Vec<Track>,
}

/// Audio player.
#[derive(Default, Facet)]
#[facet(rename = "audio")]
pub struct Audio {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Audio URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Show controls.
    #[facet(html::attribute, default)]
    pub controls: Option<String>,
    /// Autoplay.
    #[facet(html::attribute, default)]
    pub autoplay: Option<String>,
    /// Loop playback.
    #[facet(html::attribute, default, rename = "loop")]
    pub loop_: Option<String>,
    /// Muted by default.
    #[facet(html::attribute, default)]
    pub muted: Option<String>,
    /// Preload behavior.
    #[facet(html::attribute, default)]
    pub preload: Option<String>,
    /// Crossorigin.
    #[facet(html::attribute, default)]
    pub crossorigin: Option<String>,
    /// Source elements.
    #[facet(html::elements, default)]
    pub source: Vec<Source>,
}

/// Media source.
#[derive(Default, Facet)]
#[facet(rename = "source")]
pub struct Source {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// MIME type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Srcset (for picture).
    #[facet(html::attribute, default)]
    pub srcset: Option<String>,
    /// Sizes.
    #[facet(html::attribute, default)]
    pub sizes: Option<String>,
    /// Media query.
    #[facet(html::attribute, default)]
    pub media: Option<String>,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
}

/// Text track for video/audio.
#[derive(Default, Facet)]
#[facet(rename = "track")]
pub struct Track {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Track kind.
    #[facet(html::attribute, default)]
    pub kind: Option<String>,
    /// Language.
    #[facet(html::attribute, default)]
    pub srclang: Option<String>,
    /// Label.
    #[facet(html::attribute, default)]
    pub label: Option<String>,
    /// Default track.
    #[facet(html::attribute, default)]
    pub default: Option<String>,
}

/// Picture element for art direction.
#[derive(Default, Facet)]
#[facet(rename = "picture")]
pub struct Picture {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Source elements.
    #[facet(html::elements, default)]
    pub source: Vec<Source>,
    /// Fallback image.
    #[facet(default)]
    pub img: Option<Img>,
}

/// Canvas for graphics.
#[derive(Default, Facet)]
#[facet(rename = "canvas")]
pub struct Canvas {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// Fallback content.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// SVG root element (simplified).
#[derive(Default, Facet)]
#[facet(rename = "svg")]
pub struct Svg {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Width.
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height.
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// ViewBox.
    #[facet(html::attribute, default, rename = "viewBox")]
    pub view_box: Option<String>,
    /// Xmlns.
    #[facet(html::attribute, default)]
    pub xmlns: Option<String>,
    /// Preserve aspect ratio.
    #[facet(html::attribute, default, rename = "preserveAspectRatio")]
    pub preserve_aspect_ratio: Option<String>,
    /// Child elements (SVG content).
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<SvgContent>,
}

/// A custom SVG element with a dynamic tag name.
///
/// This captures any SVG element that isn't explicitly modeled, preserving
/// its tag name, attributes, and children during parse/serialize roundtrips.
#[derive(Default, Facet)]
pub struct CustomSvgElement {
    /// The tag name of the SVG element (e.g., "rect", "path", "g").
    #[facet(html::tag, default)]
    pub tag: String,
    /// Global attributes (id, class, style, data-*, etc.).
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<SvgContent>,
}

/// SVG content - elements and text that can appear inside an SVG.
#[derive(Facet)]
#[repr(u8)]
pub enum SvgContent {
    /// Text node (named to avoid collision with SVG `<text>` element).
    #[facet(html::text)]
    TextNode(String),
    /// Any SVG element (catch-all).
    #[facet(html::custom_element)]
    Element(CustomSvgElement),
}

// =============================================================================
// Table Elements
// =============================================================================

/// Table.
#[derive(Default, Facet)]
#[facet(rename = "table")]
pub struct Table {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Caption.
    #[facet(default)]
    pub caption: Option<Caption>,
    /// Column groups.
    #[facet(html::elements, default)]
    pub colgroup: Vec<Colgroup>,
    /// Table head.
    #[facet(default)]
    pub thead: Option<Thead>,
    /// Table body sections.
    #[facet(html::elements, default)]
    pub tbody: Vec<Tbody>,
    /// Table foot.
    #[facet(default)]
    pub tfoot: Option<Tfoot>,
    /// Direct rows (when no thead/tbody/tfoot).
    #[facet(html::elements, default)]
    pub tr: Vec<Tr>,
}

/// Table caption.
#[derive(Default, Facet)]
#[facet(rename = "caption")]
pub struct Caption {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Column group.
#[derive(Default, Facet)]
#[facet(rename = "colgroup")]
pub struct Colgroup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(html::attribute, default)]
    pub span: Option<String>,
    /// Column definitions.
    #[facet(html::elements, default)]
    pub col: Vec<Col>,
}

/// Table column.
#[derive(Default, Facet)]
#[facet(rename = "col")]
pub struct Col {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(html::attribute, default)]
    pub span: Option<String>,
}

/// Table head.
#[derive(Default, Facet)]
#[facet(rename = "thead")]
pub struct Thead {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(html::elements, default)]
    pub tr: Vec<Tr>,
}

/// Table body.
#[derive(Default, Facet)]
#[facet(rename = "tbody")]
pub struct Tbody {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(html::elements, default)]
    pub tr: Vec<Tr>,
}

/// Table foot.
#[derive(Default, Facet)]
#[facet(rename = "tfoot")]
pub struct Tfoot {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(html::elements, default)]
    pub tr: Vec<Tr>,
}

/// Table row.
#[derive(Default, Facet)]
#[facet(rename = "tr")]
pub struct Tr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Header cells.
    #[facet(html::elements, default)]
    pub th: Vec<Th>,
    /// Data cells.
    #[facet(html::elements, default)]
    pub td: Vec<Td>,
}

/// Table header cell.
#[derive(Default, Facet)]
#[facet(rename = "th")]
pub struct Th {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(html::attribute, default)]
    pub colspan: Option<String>,
    /// Number of rows spanned.
    #[facet(html::attribute, default)]
    pub rowspan: Option<String>,
    /// Header scope.
    #[facet(html::attribute, default)]
    pub scope: Option<String>,
    /// Headers this cell relates to.
    #[facet(html::attribute, default)]
    pub headers: Option<String>,
    /// Abbreviation.
    #[facet(html::attribute, default)]
    pub abbr: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Table data cell.
#[derive(Default, Facet)]
#[facet(rename = "td")]
pub struct Td {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(html::attribute, default)]
    pub colspan: Option<String>,
    /// Number of rows spanned.
    #[facet(html::attribute, default)]
    pub rowspan: Option<String>,
    /// Headers this cell relates to.
    #[facet(html::attribute, default)]
    pub headers: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Form Elements
// =============================================================================

/// Form.
#[derive(Default, Facet)]
#[facet(rename = "form")]
pub struct Form {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Form action URL.
    #[facet(html::attribute, default)]
    pub action: Option<String>,
    /// HTTP method.
    #[facet(html::attribute, default)]
    pub method: Option<String>,
    /// Encoding type.
    #[facet(html::attribute, default)]
    pub enctype: Option<String>,
    /// Target.
    #[facet(html::attribute, default)]
    pub target: Option<String>,
    /// Form name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Autocomplete.
    #[facet(html::attribute, default)]
    pub autocomplete: Option<String>,
    /// Disable validation.
    #[facet(html::attribute, default)]
    pub novalidate: Option<String>,
    /// Accept-charset.
    #[facet(html::attribute, default, rename = "accept-charset")]
    pub accept_charset: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Input control.
#[derive(Default, Facet)]
#[facet(rename = "input")]
pub struct Input {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Input type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Placeholder.
    #[facet(html::attribute, default)]
    pub placeholder: Option<String>,
    /// Required.
    #[facet(html::attribute, default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Readonly.
    #[facet(html::attribute, default)]
    pub readonly: Option<String>,
    /// Checked (for checkboxes/radios).
    #[facet(html::attribute, default)]
    pub checked: Option<String>,
    /// Autocomplete.
    #[facet(html::attribute, default)]
    pub autocomplete: Option<String>,
    /// Autofocus.
    #[facet(html::attribute, default)]
    pub autofocus: Option<String>,
    /// Min value.
    #[facet(html::attribute, default)]
    pub min: Option<String>,
    /// Max value.
    #[facet(html::attribute, default)]
    pub max: Option<String>,
    /// Step.
    #[facet(html::attribute, default)]
    pub step: Option<String>,
    /// Pattern.
    #[facet(html::attribute, default)]
    pub pattern: Option<String>,
    /// Size.
    #[facet(html::attribute, default)]
    pub size: Option<String>,
    /// Maxlength.
    #[facet(html::attribute, default)]
    pub maxlength: Option<String>,
    /// Minlength.
    #[facet(html::attribute, default)]
    pub minlength: Option<String>,
    /// Multiple values allowed.
    #[facet(html::attribute, default)]
    pub multiple: Option<String>,
    /// Accept (for file inputs).
    #[facet(html::attribute, default)]
    pub accept: Option<String>,
    /// Alt text (for image inputs).
    #[facet(html::attribute, default)]
    pub alt: Option<String>,
    /// Src (for image inputs).
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// Width (for image inputs).
    #[facet(html::attribute, default)]
    pub width: Option<String>,
    /// Height (for image inputs).
    #[facet(html::attribute, default)]
    pub height: Option<String>,
    /// List datalist reference.
    #[facet(html::attribute, default)]
    pub list: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Form action override.
    #[facet(html::attribute, default)]
    pub formaction: Option<String>,
    /// Form method override.
    #[facet(html::attribute, default)]
    pub formmethod: Option<String>,
    /// Form enctype override.
    #[facet(html::attribute, default)]
    pub formenctype: Option<String>,
    /// Form target override.
    #[facet(html::attribute, default)]
    pub formtarget: Option<String>,
    /// Form novalidate override.
    #[facet(html::attribute, default)]
    pub formnovalidate: Option<String>,
}

/// Button.
#[derive(Default, Facet)]
#[facet(rename = "button")]
pub struct Button {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Button type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Autofocus.
    #[facet(html::attribute, default)]
    pub autofocus: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Form action override.
    #[facet(html::attribute, default)]
    pub formaction: Option<String>,
    /// Form method override.
    #[facet(html::attribute, default)]
    pub formmethod: Option<String>,
    /// Form enctype override.
    #[facet(html::attribute, default)]
    pub formenctype: Option<String>,
    /// Form target override.
    #[facet(html::attribute, default)]
    pub formtarget: Option<String>,
    /// Form novalidate override.
    #[facet(html::attribute, default)]
    pub formnovalidate: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Select dropdown.
#[derive(Default, Facet)]
#[facet(rename = "select")]
pub struct Select {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Multiple selection.
    #[facet(html::attribute, default)]
    pub multiple: Option<String>,
    /// Size (visible options).
    #[facet(html::attribute, default)]
    pub size: Option<String>,
    /// Required.
    #[facet(html::attribute, default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Autofocus.
    #[facet(html::attribute, default)]
    pub autofocus: Option<String>,
    /// Autocomplete.
    #[facet(html::attribute, default)]
    pub autocomplete: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Options.
    #[facet(html::elements, default)]
    pub option: Vec<OptionElement>,
    /// Option groups.
    #[facet(html::elements, default)]
    pub optgroup: Vec<Optgroup>,
}

/// Option in a select.
#[derive(Default, Facet)]
#[facet(rename = "option")]
pub struct OptionElement {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Selected.
    #[facet(html::attribute, default)]
    pub selected: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Label.
    #[facet(html::attribute, default)]
    pub label: Option<String>,
    /// Text content.
    #[facet(html::text, default)]
    pub text: String,
}

/// Option group.
#[derive(Default, Facet)]
#[facet(rename = "optgroup")]
pub struct Optgroup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Label.
    #[facet(html::attribute, default)]
    pub label: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Options.
    #[facet(html::elements, default)]
    pub option: Vec<OptionElement>,
}

/// Textarea.
#[derive(Default, Facet)]
#[facet(rename = "textarea")]
pub struct Textarea {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Rows.
    #[facet(html::attribute, default)]
    pub rows: Option<String>,
    /// Cols.
    #[facet(html::attribute, default)]
    pub cols: Option<String>,
    /// Placeholder.
    #[facet(html::attribute, default)]
    pub placeholder: Option<String>,
    /// Required.
    #[facet(html::attribute, default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Readonly.
    #[facet(html::attribute, default)]
    pub readonly: Option<String>,
    /// Autofocus.
    #[facet(html::attribute, default)]
    pub autofocus: Option<String>,
    /// Autocomplete.
    #[facet(html::attribute, default)]
    pub autocomplete: Option<String>,
    /// Maxlength.
    #[facet(html::attribute, default)]
    pub maxlength: Option<String>,
    /// Minlength.
    #[facet(html::attribute, default)]
    pub minlength: Option<String>,
    /// Wrap.
    #[facet(html::attribute, default)]
    pub wrap: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Text content.
    #[facet(html::text, default)]
    pub text: String,
}

/// Form label.
#[derive(Default, Facet)]
#[facet(rename = "label")]
pub struct Label {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Associated control ID.
    #[facet(html::attribute, default, rename = "for")]
    pub for_: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Fieldset grouping.
#[derive(Default, Facet)]
#[facet(rename = "fieldset")]
pub struct Fieldset {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Disabled.
    #[facet(html::attribute, default)]
    pub disabled: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Legend.
    #[facet(default)]
    pub legend: Option<Legend>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Fieldset legend.
#[derive(Default, Facet)]
#[facet(rename = "legend")]
pub struct Legend {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Datalist.
#[derive(Default, Facet)]
#[facet(rename = "datalist")]
pub struct Datalist {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Options.
    #[facet(html::elements, default)]
    pub option: Vec<OptionElement>,
}

/// Output.
#[derive(Default, Facet)]
#[facet(rename = "output")]
pub struct Output {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Associated controls.
    #[facet(html::attribute, default, rename = "for")]
    pub for_: Option<String>,
    /// Name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Form override.
    #[facet(html::attribute, default)]
    pub form: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Progress indicator.
#[derive(Default, Facet)]
#[facet(rename = "progress")]
pub struct Progress {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Current value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Maximum value.
    #[facet(html::attribute, default)]
    pub max: Option<String>,
    /// Fallback content.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Meter/gauge.
#[derive(Default, Facet)]
#[facet(rename = "meter")]
pub struct Meter {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Current value.
    #[facet(html::attribute, default)]
    pub value: Option<String>,
    /// Minimum value.
    #[facet(html::attribute, default)]
    pub min: Option<String>,
    /// Maximum value.
    #[facet(html::attribute, default)]
    pub max: Option<String>,
    /// Low threshold.
    #[facet(html::attribute, default)]
    pub low: Option<String>,
    /// High threshold.
    #[facet(html::attribute, default)]
    pub high: Option<String>,
    /// Optimum value.
    #[facet(html::attribute, default)]
    pub optimum: Option<String>,
    /// Fallback content.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

// =============================================================================
// Interactive Elements
// =============================================================================

/// Details disclosure widget.
#[derive(Default, Facet)]
#[facet(rename = "details")]
pub struct Details {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Open state.
    #[facet(html::attribute, default)]
    pub open: Option<String>,
    /// Summary.
    #[facet(default)]
    pub summary: Option<Summary>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Details summary.
#[derive(Default, Facet)]
#[facet(rename = "summary")]
pub struct Summary {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Dialog box.
#[derive(Default, Facet)]
#[facet(rename = "dialog")]
pub struct Dialog {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Open state.
    #[facet(html::attribute, default)]
    pub open: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Scripting Elements
// =============================================================================

/// Script.
#[derive(Default, Facet)]
#[facet(rename = "script")]
pub struct Script {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Script URL.
    #[facet(html::attribute, default)]
    pub src: Option<String>,
    /// MIME type.
    #[facet(html::attribute, default, rename = "type")]
    pub type_: Option<String>,
    /// Async loading.
    #[facet(html::attribute, default, rename = "async")]
    pub async_: Option<String>,
    /// Defer loading.
    #[facet(html::attribute, default)]
    pub defer: Option<String>,
    /// Crossorigin.
    #[facet(html::attribute, default)]
    pub crossorigin: Option<String>,
    /// Integrity hash.
    #[facet(html::attribute, default)]
    pub integrity: Option<String>,
    /// Referrer policy.
    #[facet(html::attribute, default)]
    pub referrerpolicy: Option<String>,
    /// Nomodule flag.
    #[facet(html::attribute, default)]
    pub nomodule: Option<String>,
    /// Inline script content.
    #[facet(html::text, default)]
    pub text: String,
}

/// Noscript fallback.
#[derive(Default, Facet)]
#[facet(rename = "noscript")]
pub struct Noscript {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Template.
#[derive(Default, Facet)]
#[facet(rename = "template")]
pub struct Template {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Slot for web components.
#[derive(Default, Facet)]
#[facet(rename = "slot")]
pub struct Slot {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Slot name.
    #[facet(html::attribute, default)]
    pub name: Option<String>,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Custom Elements
// =============================================================================

/// A custom HTML element with a dynamic tag name.
///
/// This type is used as a catch-all for unknown elements during HTML parsing.
/// Custom elements (like `<a-k>`, `<a-f>` from arborium syntax highlighting)
/// are preserved with their tag name, attributes, and children.
///
/// # Example
///
/// ```ignore
/// // Input: <a-k>fn</a-k>
/// // Parses as:
/// CustomElement {
///     tag: "a-k".to_string(),
///     attrs: GlobalAttrs::default(),
///     children: vec![FlowContent::Text("fn".to_string())],
/// }
/// ```
#[derive(Default, Facet)]
pub struct CustomElement {
    /// The tag name of the custom element (e.g., "a-k", "my-component").
    ///
    /// This field is marked with `#[facet(html::tag)]` to indicate it should
    /// receive the element's tag name during deserialization.
    #[facet(html::tag, default)]
    pub tag: String,
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// A custom phrasing element with a dynamic tag name.
///
/// Similar to [`CustomElement`] but for inline/phrasing content contexts.
/// This allows custom elements to appear inside paragraphs, spans, etc.
#[derive(Default, Facet)]
pub struct CustomPhrasingElement {
    /// The tag name of the custom element.
    #[facet(html::tag, default)]
    pub tag: String,
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

// =============================================================================
// Content Categories (Enums for mixed content)
// =============================================================================

/// Metadata content - elements that can appear in `<head>`.
#[derive(Facet)]
#[repr(u8)]
pub enum MetadataContent {
    /// Text node (for whitespace between elements).
    #[facet(html::text)]
    Text(String),
    /// Document title.
    #[facet(rename = "title")]
    Title(Title),
    /// Base URL element.
    #[facet(rename = "base")]
    Base(Base),
    /// Linked resources (stylesheets, icons, etc.).
    #[facet(rename = "link")]
    Link(Link),
    /// Metadata elements.
    #[facet(rename = "meta")]
    Meta(Meta),
    /// Inline styles.
    #[facet(rename = "style")]
    Style(Style),
    /// Scripts.
    #[facet(rename = "script")]
    Script(Script),
    /// Noscript element.
    #[facet(rename = "noscript")]
    Noscript(Noscript),
    /// Template element.
    #[facet(rename = "template")]
    Template(Template),
}

/// Flow content - most block and inline elements.
#[derive(Facet)]
#[repr(u8)]
#[allow(clippy::large_enum_variant)] // DOM-like structures naturally have large variants
pub enum FlowContent {
    /// Text node (for mixed content).
    #[facet(html::text)]
    Text(String),

    // Sections
    /// Header element.
    #[facet(rename = "header")]
    Header(Header),
    /// Footer element.
    #[facet(rename = "footer")]
    Footer(Footer),
    /// Main element.
    #[facet(rename = "main")]
    Main(Main),
    /// Article element.
    #[facet(rename = "article")]
    Article(Article),
    /// Section element.
    #[facet(rename = "section")]
    Section(Section),
    /// Nav element.
    #[facet(rename = "nav")]
    Nav(Nav),
    /// Aside element.
    #[facet(rename = "aside")]
    Aside(Aside),
    /// Address element.
    #[facet(rename = "address")]
    Address(Address),

    // Headings
    /// H1 element.
    #[facet(rename = "h1")]
    H1(H1),
    /// H2 element.
    #[facet(rename = "h2")]
    H2(H2),
    /// H3 element.
    #[facet(rename = "h3")]
    H3(H3),
    /// H4 element.
    #[facet(rename = "h4")]
    H4(H4),
    /// H5 element.
    #[facet(rename = "h5")]
    H5(H5),
    /// H6 element.
    #[facet(rename = "h6")]
    H6(H6),

    // Grouping
    /// P element.
    #[facet(rename = "p")]
    P(P),
    /// Div element.
    #[facet(rename = "div")]
    Div(Div),
    /// Pre element.
    #[facet(rename = "pre")]
    Pre(Pre),
    /// Blockquote element.
    #[facet(rename = "blockquote")]
    Blockquote(Blockquote),
    /// Ol element.
    #[facet(rename = "ol")]
    Ol(Ol),
    /// Ul element.
    #[facet(rename = "ul")]
    Ul(Ul),
    /// Dl element.
    #[facet(rename = "dl")]
    Dl(Dl),
    /// Figure element.
    #[facet(rename = "figure")]
    Figure(Figure),
    /// Hr element.
    #[facet(rename = "hr")]
    Hr(Hr),

    // Phrasing (inline)
    /// A element.
    #[facet(rename = "a")]
    A(A),
    /// Span element.
    #[facet(rename = "span")]
    Span(Span),
    /// Em element.
    #[facet(rename = "em")]
    Em(Em),
    /// Strong element.
    #[facet(rename = "strong")]
    Strong(Strong),
    /// Code element.
    #[facet(rename = "code")]
    Code(Code),
    /// Img element.
    #[facet(rename = "img")]
    Img(Img),
    /// Br element.
    #[facet(rename = "br")]
    Br(Br),

    // Tables
    /// Table element.
    #[facet(rename = "table")]
    Table(Table),

    // Forms
    /// Form element.
    #[facet(rename = "form")]
    Form(Form),
    /// Input element.
    #[facet(rename = "input")]
    Input(Input),
    /// Button element.
    #[facet(rename = "button")]
    Button(Button),
    /// Select element.
    #[facet(rename = "select")]
    Select(Select),
    /// Textarea element.
    #[facet(rename = "textarea")]
    Textarea(Textarea),
    /// Label element.
    #[facet(rename = "label")]
    Label(Label),
    /// Fieldset element.
    #[facet(rename = "fieldset")]
    Fieldset(Fieldset),

    // Interactive
    /// Details element.
    #[facet(rename = "details")]
    Details(Details),
    /// Dialog element.
    #[facet(rename = "dialog")]
    Dialog(Dialog),

    // Embedded
    /// Iframe element.
    #[facet(rename = "iframe")]
    Iframe(Iframe),
    /// Video element.
    #[facet(rename = "video")]
    Video(Video),
    /// Audio element.
    #[facet(rename = "audio")]
    Audio(Audio),
    /// Picture element.
    #[facet(rename = "picture")]
    Picture(Picture),
    /// Canvas element.
    #[facet(rename = "canvas")]
    Canvas(Canvas),
    /// Svg element.
    #[facet(rename = "svg")]
    Svg(Svg),

    // Scripting
    /// Script element.
    #[facet(rename = "script")]
    Script(Script),
    /// Noscript element.
    #[facet(rename = "noscript")]
    Noscript(Noscript),
    /// Template element.
    #[facet(rename = "template")]
    Template(Template),

    // Custom elements (catch-all for unknown elements)
    /// Custom element (catch-all for unknown elements like `<a-k>`, `<my-component>`).
    #[facet(html::custom_element)]
    Custom(CustomElement),
}

/// Phrasing content - inline elements and text.
#[derive(Facet)]
#[repr(u8)]
#[allow(clippy::large_enum_variant)] // DOM-like structures naturally have large variants
pub enum PhrasingContent {
    /// Text node (for mixed content).
    #[facet(html::text)]
    Text(String),
    /// A element.
    #[facet(rename = "a")]
    A(A),
    /// Span element.
    #[facet(rename = "span")]
    Span(Span),
    /// Em element.
    #[facet(rename = "em")]
    Em(Em),
    /// Strong element.
    #[facet(rename = "strong")]
    Strong(Strong),
    /// Small element.
    #[facet(rename = "small")]
    Small(Small),
    /// S element.
    #[facet(rename = "s")]
    S(S),
    /// Cite element.
    #[facet(rename = "cite")]
    Cite(Cite),
    /// Q element.
    #[facet(rename = "q")]
    Q(Q),
    /// Dfn element.
    #[facet(rename = "dfn")]
    Dfn(Dfn),
    /// Abbr element.
    #[facet(rename = "abbr")]
    Abbr(Abbr),
    /// Data element.
    #[facet(rename = "data")]
    Data(Data),
    /// Time element.
    #[facet(rename = "time")]
    Time(Time),
    /// Code element.
    #[facet(rename = "code")]
    Code(Code),
    /// Var element.
    #[facet(rename = "var")]
    Var(Var),
    /// Samp element.
    #[facet(rename = "samp")]
    Samp(Samp),
    /// Kbd element.
    #[facet(rename = "kbd")]
    Kbd(Kbd),
    /// Sub element.
    #[facet(rename = "sub")]
    Sub(Sub),
    /// Sup element.
    #[facet(rename = "sup")]
    Sup(Sup),
    /// I element.
    #[facet(rename = "i")]
    I(I),
    /// B element.
    #[facet(rename = "b")]
    B(B),
    /// U element.
    #[facet(rename = "u")]
    U(U),
    /// Mark element.
    #[facet(rename = "mark")]
    Mark(Mark),
    /// Bdi element.
    #[facet(rename = "bdi")]
    Bdi(Bdi),
    /// Bdo element.
    #[facet(rename = "bdo")]
    Bdo(Bdo),
    /// Br element.
    #[facet(rename = "br")]
    Br(Br),
    /// Wbr element.
    #[facet(rename = "wbr")]
    Wbr(Wbr),
    /// Img element.
    #[facet(rename = "img")]
    Img(Img),
    /// Input element.
    #[facet(rename = "input")]
    Input(Input),
    /// Button element.
    #[facet(rename = "button")]
    Button(Button),
    /// Select element.
    #[facet(rename = "select")]
    Select(Select),
    /// Textarea element.
    #[facet(rename = "textarea")]
    Textarea(Textarea),
    /// Label element.
    #[facet(rename = "label")]
    Label(Label),
    /// Output element.
    #[facet(rename = "output")]
    Output(Output),
    /// Progress element.
    #[facet(rename = "progress")]
    Progress(Progress),
    /// Meter element.
    #[facet(rename = "meter")]
    Meter(Meter),
    /// Script element.
    #[facet(rename = "script")]
    Script(Script),

    // Custom elements (catch-all for unknown elements)
    /// Custom element (catch-all for unknown elements like `<a-k>`, `<my-component>`).
    #[facet(html::custom_element)]
    Custom(CustomPhrasingElement),
}
