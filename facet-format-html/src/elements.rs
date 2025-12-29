//! Typed HTML element definitions.
//!
//! This module provides Facet-derived types for all standard HTML5 elements.
//! Each element type includes its valid attributes and can contain child elements.
//!
//! # Organization
//!
//! Elements are organized by category:
//! - **Document**: `Html`, `Head`, `Body`
//! - **Metadata**: `Title`, `Base`, `Link`, `Meta`, `Style`
//! - **Sections**: `Header`, `Footer`, `Main`, `Article`, `Section`, `Nav`, `Aside`
//! - **Headings**: `H1`, `H2`, `H3`, `H4`, `H5`, `H6`
//! - **Grouping**: `P`, `Div`, `Span`, `Pre`, `Blockquote`, `Ol`, `Ul`, `Li`, etc.
//! - **Text-level**: `A`, `Em`, `Strong`, `Code`, `Br`, `Wbr`, etc.
//! - **Embedded**: `Img`, `Iframe`, `Video`, `Audio`, `Source`, `Picture`
//! - **Tables**: `Table`, `Thead`, `Tbody`, `Tr`, `Th`, `Td`, etc.
//! - **Forms**: `Form`, `Input`, `Button`, `Select`, `Option`, `Textarea`, `Label`
//! - **Interactive**: `Details`, `Summary`, `Dialog`
//! - **Scripting**: `Script`, `Noscript`, `Template`, `Canvas`

use facet::Facet;
// Note: We use xml::text here because Rust doesn't allow referencing macro-generated
// attributes from the same crate. The deserializer's is_text() helper handles both
// xml::text and xml::text equivalently.
use facet_format_xml as xml;

// =============================================================================
// Global Attributes (common to all HTML elements)
// =============================================================================

/// Global attributes that can appear on any HTML element.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(default)]
pub struct GlobalAttrs {
    /// Unique identifier for the element.
    #[facet(default)]
    pub id: Option<String>,
    /// CSS class names.
    #[facet(default)]
    pub class: Option<String>,
    /// Inline CSS styles.
    #[facet(default)]
    pub style: Option<String>,
    /// Advisory title/tooltip.
    #[facet(default)]
    pub title: Option<String>,
    /// Language of the element's content.
    #[facet(default)]
    pub lang: Option<String>,
    /// Text directionality (ltr, rtl, auto).
    #[facet(default)]
    pub dir: Option<String>,
    /// Whether the element is hidden.
    #[facet(default)]
    pub hidden: Option<String>,
    /// Tab order of the element.
    #[facet(default)]
    pub tabindex: Option<String>,
    /// Access key for the element.
    #[facet(default)]
    pub accesskey: Option<String>,
    /// Whether the element is draggable.
    #[facet(default)]
    pub draggable: Option<String>,
    /// Whether the element is editable.
    #[facet(default)]
    pub contenteditable: Option<String>,
    /// Whether spellchecking is enabled.
    #[facet(default)]
    pub spellcheck: Option<String>,
    /// Whether the element should be translated.
    #[facet(default)]
    pub translate: Option<String>,
    /// ARIA role.
    #[facet(default)]
    pub role: Option<String>,
}

// =============================================================================
// Document Structure
// =============================================================================

/// The root HTML document element.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "html")]
pub struct Html {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Document head.
    #[facet(default)]
    pub head: Option<Head>,
    /// Document body.
    #[facet(default)]
    pub body: Option<Body>,
}

/// The document head containing metadata.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "head")]
pub struct Head {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Document title.
    #[facet(default)]
    pub title: Option<Title>,
    /// Base URL element.
    #[facet(default)]
    pub base: Option<Base>,
    /// Linked resources (stylesheets, icons, etc.).
    #[facet(default)]
    pub link: Vec<Link>,
    /// Metadata elements.
    #[facet(default)]
    pub meta: Vec<Meta>,
    /// Inline styles.
    #[facet(default)]
    pub style: Vec<Style>,
    /// Scripts.
    #[facet(default)]
    pub script: Vec<Script>,
}

/// The document body containing visible content.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "body")]
pub struct Body {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements (mixed content).
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Metadata Elements
// =============================================================================

/// The document title.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "title")]
pub struct Title {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Text content of the title.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Base URL for relative URLs.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "base")]
pub struct Base {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Base URL.
    #[facet(default)]
    pub href: Option<String>,
    /// Default browsing context.
    #[facet(default)]
    pub target: Option<String>,
}

/// External resource link.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "link")]
pub struct Link {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL of the linked resource.
    #[facet(default)]
    pub href: Option<String>,
    /// Relationship type.
    #[facet(default)]
    pub rel: Option<String>,
    /// MIME type of the resource.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Media query for the resource.
    #[facet(default)]
    pub media: Option<String>,
    /// Integrity hash.
    #[facet(default)]
    pub integrity: Option<String>,
    /// Crossorigin attribute.
    #[facet(default)]
    pub crossorigin: Option<String>,
    /// Resource sizes (for icons).
    #[facet(default)]
    pub sizes: Option<String>,
    /// Alternative stylesheet title.
    #[facet(default, rename = "as")]
    pub as_: Option<String>,
}

/// Document metadata.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "meta")]
pub struct Meta {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Metadata name.
    #[facet(default)]
    pub name: Option<String>,
    /// Metadata content.
    #[facet(default)]
    pub content: Option<String>,
    /// Character encoding.
    #[facet(default)]
    pub charset: Option<String>,
    /// Pragma directive.
    #[facet(default, rename = "http-equiv")]
    pub http_equiv: Option<String>,
    /// Property (for Open Graph, etc.).
    #[facet(default)]
    pub property: Option<String>,
}

/// Inline stylesheet.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "style")]
pub struct Style {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Media query.
    #[facet(default)]
    pub media: Option<String>,
    /// MIME type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// CSS content.
    #[facet(default)]
    pub text: String,
}

// =============================================================================
// Section Elements
// =============================================================================

/// Page header.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "header")]
pub struct Header {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Page or section footer.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "footer")]
pub struct Footer {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Main content area.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "main")]
pub struct Main {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Self-contained article.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "article")]
pub struct Article {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Generic section.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "section")]
pub struct Section {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Navigation section.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "nav")]
pub struct Nav {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Sidebar content.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "aside")]
pub struct Aside {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Address/contact information.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "address")]
pub struct Address {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Heading Elements
// =============================================================================

/// Level 1 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h1")]
pub struct H1 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Level 2 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h2")]
pub struct H2 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Level 3 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h3")]
pub struct H3 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Level 4 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h4")]
pub struct H4 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Level 5 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h5")]
pub struct H5 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Level 6 heading.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "h6")]
pub struct H6 {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

// =============================================================================
// Grouping Content
// =============================================================================

/// Paragraph.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "p")]
pub struct P {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Generic container (block).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "div")]
pub struct Div {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Generic container (inline).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "span")]
pub struct Span {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Preformatted text.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "pre")]
pub struct Pre {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Block quotation.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "blockquote")]
pub struct Blockquote {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Citation URL.
    #[facet(default)]
    pub cite: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Ordered list.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "ol")]
pub struct Ol {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Starting number.
    #[facet(default)]
    pub start: Option<String>,
    /// Numbering type (1, a, A, i, I).
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Reversed order.
    #[facet(default)]
    pub reversed: Option<String>,
    /// List items.
    #[facet(default)]
    pub li: Vec<Li>,
}

/// Unordered list.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "ul")]
pub struct Ul {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// List items.
    #[facet(default)]
    pub li: Vec<Li>,
}

/// List item.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "li")]
pub struct Li {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Value (for ol).
    #[facet(default)]
    pub value: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Description list.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "dl")]
pub struct Dl {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Terms and descriptions (mixed dt/dd).
    #[facet(default)]
    pub dt: Vec<Dt>,
    /// Descriptions.
    #[facet(default)]
    pub dd: Vec<Dd>,
}

/// Description term.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "dt")]
pub struct Dt {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Description details.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "dd")]
pub struct Dd {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Figure with optional caption.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "figure")]
pub struct Figure {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Figure caption.
    #[facet(default)]
    pub figcaption: Option<Figcaption>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Figure caption.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "figcaption")]
pub struct Figcaption {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Horizontal rule (thematic break).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
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
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "a")]
pub struct A {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(default)]
    pub href: Option<String>,
    /// Target browsing context.
    #[facet(default)]
    pub target: Option<String>,
    /// Relationship.
    #[facet(default)]
    pub rel: Option<String>,
    /// Download filename.
    #[facet(default)]
    pub download: Option<String>,
    /// MIME type hint.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Language of linked resource.
    #[facet(default)]
    pub hreflang: Option<String>,
    /// Referrer policy.
    #[facet(default)]
    pub referrerpolicy: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Emphasis.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "em")]
pub struct Em {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Strong importance.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "strong")]
pub struct Strong {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Small print.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "small")]
pub struct Small {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Strikethrough (no longer accurate).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "s")]
pub struct S {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Citation.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "cite")]
pub struct Cite {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Inline quotation.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "q")]
pub struct Q {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Citation URL.
    #[facet(default)]
    pub cite: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Definition term.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "dfn")]
pub struct Dfn {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Abbreviation.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "abbr")]
pub struct Abbr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Ruby annotation (for East Asian typography).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "ruby")]
pub struct Ruby {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Data with machine-readable value.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "data")]
pub struct Data {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Machine-readable value.
    #[facet(default)]
    pub value: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Time.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "time")]
pub struct Time {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Machine-readable datetime.
    #[facet(default)]
    pub datetime: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Code fragment.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "code")]
pub struct Code {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Variable.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "var")]
pub struct Var {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Sample output.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "samp")]
pub struct Samp {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Keyboard input.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "kbd")]
pub struct Kbd {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Subscript.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "sub")]
pub struct Sub {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Superscript.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "sup")]
pub struct Sup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Italic.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "i")]
pub struct I {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Bold.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "b")]
pub struct B {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Underline.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "u")]
pub struct U {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Highlighted text.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "mark")]
pub struct Mark {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Bidirectional isolation.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "bdi")]
pub struct Bdi {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Bidirectional override.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "bdo")]
pub struct Bdo {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Line break.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "br")]
pub struct Br {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
}

/// Word break opportunity.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
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
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "img")]
pub struct Img {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Image URL.
    #[facet(default)]
    pub src: Option<String>,
    /// Alternative text.
    #[facet(default)]
    pub alt: Option<String>,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// Srcset for responsive images.
    #[facet(default)]
    pub srcset: Option<String>,
    /// Sizes attribute.
    #[facet(default)]
    pub sizes: Option<String>,
    /// Loading behavior.
    #[facet(default)]
    pub loading: Option<String>,
    /// Decoding hint.
    #[facet(default)]
    pub decoding: Option<String>,
    /// Crossorigin.
    #[facet(default)]
    pub crossorigin: Option<String>,
    /// Referrer policy.
    #[facet(default)]
    pub referrerpolicy: Option<String>,
    /// Usemap reference.
    #[facet(default)]
    pub usemap: Option<String>,
    /// Whether this is a server-side image map.
    #[facet(default)]
    pub ismap: Option<String>,
}

/// Inline frame.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "iframe")]
pub struct Iframe {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(default)]
    pub src: Option<String>,
    /// Srcdoc content.
    #[facet(default)]
    pub srcdoc: Option<String>,
    /// Frame name.
    #[facet(default)]
    pub name: Option<String>,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// Sandbox restrictions.
    #[facet(default)]
    pub sandbox: Option<String>,
    /// Feature policy.
    #[facet(default)]
    pub allow: Option<String>,
    /// Fullscreen allowed.
    #[facet(default)]
    pub allowfullscreen: Option<String>,
    /// Loading behavior.
    #[facet(default)]
    pub loading: Option<String>,
    /// Referrer policy.
    #[facet(default)]
    pub referrerpolicy: Option<String>,
}

/// Embedded object.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "object")]
pub struct Object {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Data URL.
    #[facet(default)]
    pub data: Option<String>,
    /// MIME type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// Usemap reference.
    #[facet(default)]
    pub usemap: Option<String>,
    /// Fallback content.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Video player.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "video")]
pub struct Video {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Video URL.
    #[facet(default)]
    pub src: Option<String>,
    /// Poster image.
    #[facet(default)]
    pub poster: Option<String>,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// Show controls.
    #[facet(default)]
    pub controls: Option<String>,
    /// Autoplay.
    #[facet(default)]
    pub autoplay: Option<String>,
    /// Loop playback.
    #[facet(default, rename = "loop")]
    pub loop_: Option<String>,
    /// Muted by default.
    #[facet(default)]
    pub muted: Option<String>,
    /// Preload behavior.
    #[facet(default)]
    pub preload: Option<String>,
    /// Plays inline (iOS).
    #[facet(default)]
    pub playsinline: Option<String>,
    /// Crossorigin.
    #[facet(default)]
    pub crossorigin: Option<String>,
    /// Source elements.
    #[facet(default)]
    pub source: Vec<Source>,
    /// Track elements.
    #[facet(default)]
    pub track: Vec<Track>,
}

/// Audio player.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "audio")]
pub struct Audio {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Audio URL.
    #[facet(default)]
    pub src: Option<String>,
    /// Show controls.
    #[facet(default)]
    pub controls: Option<String>,
    /// Autoplay.
    #[facet(default)]
    pub autoplay: Option<String>,
    /// Loop playback.
    #[facet(default, rename = "loop")]
    pub loop_: Option<String>,
    /// Muted by default.
    #[facet(default)]
    pub muted: Option<String>,
    /// Preload behavior.
    #[facet(default)]
    pub preload: Option<String>,
    /// Crossorigin.
    #[facet(default)]
    pub crossorigin: Option<String>,
    /// Source elements.
    #[facet(default)]
    pub source: Vec<Source>,
}

/// Media source.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "source")]
pub struct Source {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(default)]
    pub src: Option<String>,
    /// MIME type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Srcset (for picture).
    #[facet(default)]
    pub srcset: Option<String>,
    /// Sizes.
    #[facet(default)]
    pub sizes: Option<String>,
    /// Media query.
    #[facet(default)]
    pub media: Option<String>,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
}

/// Text track for video/audio.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "track")]
pub struct Track {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// URL.
    #[facet(default)]
    pub src: Option<String>,
    /// Track kind.
    #[facet(default)]
    pub kind: Option<String>,
    /// Language.
    #[facet(default)]
    pub srclang: Option<String>,
    /// Label.
    #[facet(default)]
    pub label: Option<String>,
    /// Default track.
    #[facet(default)]
    pub default: Option<String>,
}

/// Picture element for art direction.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "picture")]
pub struct Picture {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Source elements.
    #[facet(default)]
    pub source: Vec<Source>,
    /// Fallback image.
    #[facet(default)]
    pub img: Option<Img>,
}

/// Canvas for graphics.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "canvas")]
pub struct Canvas {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// Fallback content.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// SVG root element (simplified).
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "svg")]
pub struct Svg {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Width.
    #[facet(default)]
    pub width: Option<String>,
    /// Height.
    #[facet(default)]
    pub height: Option<String>,
    /// ViewBox.
    #[facet(default, rename = "viewBox")]
    pub view_box: Option<String>,
    /// Xmlns.
    #[facet(default)]
    pub xmlns: Option<String>,
    /// Preserve aspect ratio.
    #[facet(default, rename = "preserveAspectRatio")]
    pub preserve_aspect_ratio: Option<String>,
}

// =============================================================================
// Table Elements
// =============================================================================

/// Table.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "table")]
pub struct Table {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Caption.
    #[facet(default)]
    pub caption: Option<Caption>,
    /// Column groups.
    #[facet(default)]
    pub colgroup: Vec<Colgroup>,
    /// Table head.
    #[facet(default)]
    pub thead: Option<Thead>,
    /// Table body sections.
    #[facet(default)]
    pub tbody: Vec<Tbody>,
    /// Table foot.
    #[facet(default)]
    pub tfoot: Option<Tfoot>,
    /// Direct rows (when no thead/tbody/tfoot).
    #[facet(default)]
    pub tr: Vec<Tr>,
}

/// Table caption.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "caption")]
pub struct Caption {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Column group.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "colgroup")]
pub struct Colgroup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(default)]
    pub span: Option<String>,
    /// Column definitions.
    #[facet(default)]
    pub col: Vec<Col>,
}

/// Table column.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "col")]
pub struct Col {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(default)]
    pub span: Option<String>,
}

/// Table head.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "thead")]
pub struct Thead {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(default)]
    pub tr: Vec<Tr>,
}

/// Table body.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "tbody")]
pub struct Tbody {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(default)]
    pub tr: Vec<Tr>,
}

/// Table foot.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "tfoot")]
pub struct Tfoot {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Rows.
    #[facet(default)]
    pub tr: Vec<Tr>,
}

/// Table row.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "tr")]
pub struct Tr {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Header cells.
    #[facet(default)]
    pub th: Vec<Th>,
    /// Data cells.
    #[facet(default)]
    pub td: Vec<Td>,
}

/// Table header cell.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "th")]
pub struct Th {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(default)]
    pub colspan: Option<String>,
    /// Number of rows spanned.
    #[facet(default)]
    pub rowspan: Option<String>,
    /// Header scope.
    #[facet(default)]
    pub scope: Option<String>,
    /// Headers this cell relates to.
    #[facet(default)]
    pub headers: Option<String>,
    /// Abbreviation.
    #[facet(default)]
    pub abbr: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Table data cell.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "td")]
pub struct Td {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Number of columns spanned.
    #[facet(default)]
    pub colspan: Option<String>,
    /// Number of rows spanned.
    #[facet(default)]
    pub rowspan: Option<String>,
    /// Headers this cell relates to.
    #[facet(default)]
    pub headers: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

// =============================================================================
// Form Elements
// =============================================================================

/// Form.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "form")]
pub struct Form {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Form action URL.
    #[facet(default)]
    pub action: Option<String>,
    /// HTTP method.
    #[facet(default)]
    pub method: Option<String>,
    /// Encoding type.
    #[facet(default)]
    pub enctype: Option<String>,
    /// Target.
    #[facet(default)]
    pub target: Option<String>,
    /// Form name.
    #[facet(default)]
    pub name: Option<String>,
    /// Autocomplete.
    #[facet(default)]
    pub autocomplete: Option<String>,
    /// Disable validation.
    #[facet(default)]
    pub novalidate: Option<String>,
    /// Accept-charset.
    #[facet(default, rename = "accept-charset")]
    pub accept_charset: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Input control.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "input")]
pub struct Input {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Input type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Value.
    #[facet(default)]
    pub value: Option<String>,
    /// Placeholder.
    #[facet(default)]
    pub placeholder: Option<String>,
    /// Required.
    #[facet(default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Readonly.
    #[facet(default)]
    pub readonly: Option<String>,
    /// Checked (for checkboxes/radios).
    #[facet(default)]
    pub checked: Option<String>,
    /// Autocomplete.
    #[facet(default)]
    pub autocomplete: Option<String>,
    /// Autofocus.
    #[facet(default)]
    pub autofocus: Option<String>,
    /// Min value.
    #[facet(default)]
    pub min: Option<String>,
    /// Max value.
    #[facet(default)]
    pub max: Option<String>,
    /// Step.
    #[facet(default)]
    pub step: Option<String>,
    /// Pattern.
    #[facet(default)]
    pub pattern: Option<String>,
    /// Size.
    #[facet(default)]
    pub size: Option<String>,
    /// Maxlength.
    #[facet(default)]
    pub maxlength: Option<String>,
    /// Minlength.
    #[facet(default)]
    pub minlength: Option<String>,
    /// Multiple values allowed.
    #[facet(default)]
    pub multiple: Option<String>,
    /// Accept (for file inputs).
    #[facet(default)]
    pub accept: Option<String>,
    /// Alt text (for image inputs).
    #[facet(default)]
    pub alt: Option<String>,
    /// Src (for image inputs).
    #[facet(default)]
    pub src: Option<String>,
    /// Width (for image inputs).
    #[facet(default)]
    pub width: Option<String>,
    /// Height (for image inputs).
    #[facet(default)]
    pub height: Option<String>,
    /// List datalist reference.
    #[facet(default)]
    pub list: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Form action override.
    #[facet(default)]
    pub formaction: Option<String>,
    /// Form method override.
    #[facet(default)]
    pub formmethod: Option<String>,
    /// Form enctype override.
    #[facet(default)]
    pub formenctype: Option<String>,
    /// Form target override.
    #[facet(default)]
    pub formtarget: Option<String>,
    /// Form novalidate override.
    #[facet(default)]
    pub formnovalidate: Option<String>,
}

/// Button.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "button")]
pub struct Button {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Button type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Value.
    #[facet(default)]
    pub value: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Autofocus.
    #[facet(default)]
    pub autofocus: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Form action override.
    #[facet(default)]
    pub formaction: Option<String>,
    /// Form method override.
    #[facet(default)]
    pub formmethod: Option<String>,
    /// Form enctype override.
    #[facet(default)]
    pub formenctype: Option<String>,
    /// Form target override.
    #[facet(default)]
    pub formtarget: Option<String>,
    /// Form novalidate override.
    #[facet(default)]
    pub formnovalidate: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Select dropdown.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "select")]
pub struct Select {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Multiple selection.
    #[facet(default)]
    pub multiple: Option<String>,
    /// Size (visible options).
    #[facet(default)]
    pub size: Option<String>,
    /// Required.
    #[facet(default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Autofocus.
    #[facet(default)]
    pub autofocus: Option<String>,
    /// Autocomplete.
    #[facet(default)]
    pub autocomplete: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Options.
    #[facet(default)]
    pub option: Vec<OptionElement>,
    /// Option groups.
    #[facet(default)]
    pub optgroup: Vec<Optgroup>,
}

/// Option in a select.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "option")]
pub struct OptionElement {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Value.
    #[facet(default)]
    pub value: Option<String>,
    /// Selected.
    #[facet(default)]
    pub selected: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Label.
    #[facet(default)]
    pub label: Option<String>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Option group.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "optgroup")]
pub struct Optgroup {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Label.
    #[facet(default)]
    pub label: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Options.
    #[facet(default)]
    pub option: Vec<OptionElement>,
}

/// Textarea.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "textarea")]
pub struct Textarea {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Rows.
    #[facet(default)]
    pub rows: Option<String>,
    /// Cols.
    #[facet(default)]
    pub cols: Option<String>,
    /// Placeholder.
    #[facet(default)]
    pub placeholder: Option<String>,
    /// Required.
    #[facet(default)]
    pub required: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Readonly.
    #[facet(default)]
    pub readonly: Option<String>,
    /// Autofocus.
    #[facet(default)]
    pub autofocus: Option<String>,
    /// Autocomplete.
    #[facet(default)]
    pub autocomplete: Option<String>,
    /// Maxlength.
    #[facet(default)]
    pub maxlength: Option<String>,
    /// Minlength.
    #[facet(default)]
    pub minlength: Option<String>,
    /// Wrap.
    #[facet(default)]
    pub wrap: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Form label.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "label")]
pub struct Label {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Associated control ID.
    #[facet(default, rename = "for")]
    pub for_: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Fieldset grouping.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "fieldset")]
pub struct Fieldset {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Disabled.
    #[facet(default)]
    pub disabled: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Legend.
    #[facet(default)]
    pub legend: Option<Legend>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Fieldset legend.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "legend")]
pub struct Legend {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Datalist.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "datalist")]
pub struct Datalist {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Options.
    #[facet(default)]
    pub option: Vec<OptionElement>,
}

/// Output.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "output")]
pub struct Output {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Associated controls.
    #[facet(default, rename = "for")]
    pub for_: Option<String>,
    /// Name.
    #[facet(default)]
    pub name: Option<String>,
    /// Form override.
    #[facet(default)]
    pub form: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Progress indicator.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "progress")]
pub struct Progress {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Current value.
    #[facet(default)]
    pub value: Option<String>,
    /// Maximum value.
    #[facet(default)]
    pub max: Option<String>,
    /// Fallback content.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

/// Meter/gauge.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "meter")]
pub struct Meter {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Current value.
    #[facet(default)]
    pub value: Option<String>,
    /// Minimum value.
    #[facet(default)]
    pub min: Option<String>,
    /// Maximum value.
    #[facet(default)]
    pub max: Option<String>,
    /// Low threshold.
    #[facet(default)]
    pub low: Option<String>,
    /// High threshold.
    #[facet(default)]
    pub high: Option<String>,
    /// Optimum value.
    #[facet(default)]
    pub optimum: Option<String>,
    /// Fallback content.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
}

// =============================================================================
// Interactive Elements
// =============================================================================

/// Details disclosure widget.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "details")]
pub struct Details {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Open state.
    #[facet(default)]
    pub open: Option<String>,
    /// Summary.
    #[facet(default)]
    pub summary: Option<Summary>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Details summary.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "summary")]
pub struct Summary {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<PhrasingContent>,
    /// Text content.
    #[facet(xml::text, default)]
    pub text: String,
}

/// Dialog box.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "dialog")]
pub struct Dialog {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Open state.
    #[facet(default)]
    pub open: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Scripting Elements
// =============================================================================

/// Script.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "script")]
pub struct Script {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Script URL.
    #[facet(default)]
    pub src: Option<String>,
    /// MIME type.
    #[facet(default, rename = "type")]
    pub type_: Option<String>,
    /// Async loading.
    #[facet(default, rename = "async")]
    pub async_: Option<String>,
    /// Defer loading.
    #[facet(default)]
    pub defer: Option<String>,
    /// Crossorigin.
    #[facet(default)]
    pub crossorigin: Option<String>,
    /// Integrity hash.
    #[facet(default)]
    pub integrity: Option<String>,
    /// Referrer policy.
    #[facet(default)]
    pub referrerpolicy: Option<String>,
    /// Nomodule flag.
    #[facet(default)]
    pub nomodule: Option<String>,
    /// Inline script content.
    #[facet(default)]
    pub text: String,
}

/// Noscript fallback.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "noscript")]
pub struct Noscript {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Template.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "template")]
pub struct Template {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

/// Slot for web components.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(rename = "slot")]
pub struct Slot {
    /// Global attributes.
    #[facet(flatten, default)]
    pub attrs: GlobalAttrs,
    /// Slot name.
    #[facet(default)]
    pub name: Option<String>,
    /// Child elements.
    #[facet(default)]
    #[facet(recursive_type)]
    pub children: Vec<FlowContent>,
}

// =============================================================================
// Content Categories (Enums for mixed content)
// =============================================================================

/// Flow content - most block and inline elements.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
#[allow(clippy::large_enum_variant)] // DOM-like structures naturally have large variants
pub enum FlowContent {
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
}

/// Phrasing content - inline elements and text.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
#[allow(clippy::large_enum_variant)] // DOM-like structures naturally have large variants
pub enum PhrasingContent {
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
}
