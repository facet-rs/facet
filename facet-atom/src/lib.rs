//! Atom Syndication Format (RFC 4287) types for `facet-xml`.
//!
//! This crate provides strongly-typed Rust representations of Atom feed elements,
//! enabling parsing and generation of Atom feeds using `facet-xml`.
//!
//! # Example
//!
//! ```rust
//! use facet_atom::{Feed, Entry, Person, Link, TextContent, TextType};
//!
//! let atom_xml = r#"<?xml version="1.0" encoding="utf-8"?>
//! <feed xmlns="http://www.w3.org/2005/Atom">
//!     <title>Example Feed</title>
//!     <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
//!     <updated>2003-12-13T18:30:02Z</updated>
//!     <author>
//!         <name>John Doe</name>
//!     </author>
//!     <link href="http://example.org/"/>
//!     <entry>
//!         <title>Atom-Powered Robots Run Amok</title>
//!         <id>urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a</id>
//!         <updated>2003-12-13T18:30:02Z</updated>
//!         <link href="http://example.org/2003/12/13/atom03"/>
//!         <summary>Some text.</summary>
//!     </entry>
//! </feed>"#;
//!
//! let feed: Feed = facet_atom::from_str(atom_xml).unwrap();
//! assert_eq!(feed.title.as_ref().unwrap().content.as_deref(), Some("Example Feed"));
//! assert_eq!(feed.entries.len(), 1);
//! ```
//!
//! # Atom Namespace
//!
//! All types use the Atom namespace `http://www.w3.org/2005/Atom` as specified in RFC 4287.

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_xml as xml;
use facet_xml::{XmlParser, to_vec};

/// Atom namespace URI as defined in RFC 4287
pub const ATOM_NS: &str = "http://www.w3.org/2005/Atom";

/// Error type for Atom parsing
pub type Error = facet_format::DeserializeError<facet_xml::XmlError>;

/// Error type for Atom serialization
pub type SerializeError = facet_format::SerializeError<facet_xml::XmlSerializeError>;

/// Deserialize an Atom document from a string.
pub fn from_str<'input, T>(xml: &'input str) -> Result<T, Error>
where
    T: Facet<'input>,
{
    let parser = XmlParser::new(xml.as_bytes());
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Deserialize an Atom document from bytes.
pub fn from_slice<'input, T>(xml: &'input [u8]) -> Result<T, Error>
where
    T: Facet<'input>,
{
    let parser = XmlParser::new(xml);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Serialize an Atom value to a string.
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    Ok(String::from_utf8(bytes).expect("XmlSerializer produces valid UTF-8"))
}

// =============================================================================
// Container Elements
// =============================================================================

/// The top-level Atom feed document (`<feed>`).
///
/// A feed contains metadata about the feed itself and zero or more entries.
///
/// # Required Elements (per RFC 4287)
/// - `id`: Permanent, universally unique identifier
/// - `title`: Human-readable title
/// - `updated`: Most recent modification time
///
/// # Optional Elements
/// - `author`: One or more feed authors (required if entries lack authors)
/// - `link`: Links to related resources
/// - `category`: Categories for the feed
/// - `contributor`: Contributors to the feed
/// - `generator`: Software that generated the feed
/// - `icon`: Small image for the feed (1:1 aspect ratio)
/// - `logo`: Larger image for the feed (2:1 aspect ratio)
/// - `rights`: Copyright/usage rights
/// - `subtitle`: Human-readable description
/// - `entry`: Individual content entries
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2005/Atom",
    rename = "feed",
    skip_all_unless_truthy
)]
pub struct Feed {
    /// Permanent, universally unique identifier for the feed.
    /// Must be an IRI (Internationalized Resource Identifier).
    #[facet(xml::element)]
    pub id: Option<String>,

    /// Human-readable title for the feed.
    #[facet(xml::element)]
    pub title: Option<TextContent>,

    /// Most recent time the feed was modified in a significant way.
    /// Format: RFC 3339 timestamp (e.g., "2003-12-13T18:30:02Z")
    #[facet(xml::element)]
    pub updated: Option<String>,

    /// Authors of the feed.
    #[facet(xml::elements, rename = "author")]
    pub authors: Vec<Person>,

    /// Links to related resources.
    #[facet(xml::elements, rename = "link")]
    pub links: Vec<Link>,

    /// Categories that the feed belongs to.
    #[facet(xml::elements, rename = "category")]
    pub categories: Vec<Category>,

    /// Contributors to the feed.
    #[facet(xml::elements, rename = "contributor")]
    pub contributors: Vec<Person>,

    /// Software agent used to generate the feed.
    #[facet(xml::element)]
    pub generator: Option<Generator>,

    /// IRI reference to a small image (favicon-style, 1:1 aspect ratio).
    #[facet(xml::element)]
    pub icon: Option<String>,

    /// IRI reference to a larger image (banner-style, 2:1 aspect ratio).
    #[facet(xml::element)]
    pub logo: Option<String>,

    /// Copyright/usage rights information.
    #[facet(xml::element)]
    pub rights: Option<TextContent>,

    /// Human-readable description or subtitle.
    #[facet(xml::element)]
    pub subtitle: Option<TextContent>,

    /// Individual entries in the feed.
    #[facet(xml::elements, rename = "entry")]
    pub entries: Vec<Entry>,
}

/// An individual entry in an Atom feed (`<entry>`).
///
/// # Required Elements (per RFC 4287)
/// - `id`: Permanent, universally unique identifier
/// - `title`: Human-readable title
/// - `updated`: Most recent modification time
///
/// # Conditionally Required
/// - `author`: Required unless the feed or source provides one
/// - `link` with `rel="alternate"`: Required if no `content` element
/// - `summary`: Required if content has `src` attribute or is non-text
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2005/Atom",
    rename = "entry",
    skip_all_unless_truthy
)]
pub struct Entry {
    /// Permanent, universally unique identifier for the entry.
    #[facet(xml::element)]
    pub id: Option<String>,

    /// Human-readable title for the entry.
    #[facet(xml::element)]
    pub title: Option<TextContent>,

    /// Most recent time the entry was modified in a significant way.
    #[facet(xml::element)]
    pub updated: Option<String>,

    /// Authors of the entry.
    #[facet(xml::elements, rename = "author")]
    pub authors: Vec<Person>,

    /// Links to related resources.
    #[facet(xml::elements, rename = "link")]
    pub links: Vec<Link>,

    /// Categories that the entry belongs to.
    #[facet(xml::elements, rename = "category")]
    pub categories: Vec<Category>,

    /// Contributors to the entry.
    #[facet(xml::elements, rename = "contributor")]
    pub contributors: Vec<Person>,

    /// The entry content.
    #[facet(xml::element)]
    pub content: Option<Content>,

    /// Time when the entry was first created or published.
    #[facet(xml::element)]
    pub published: Option<String>,

    /// Copyright/usage rights information.
    #[facet(xml::element)]
    pub rights: Option<TextContent>,

    /// Brief summary or excerpt of the entry.
    #[facet(xml::element)]
    pub summary: Option<TextContent>,

    /// Metadata from the original feed if this entry was copied.
    #[facet(xml::element)]
    pub source: Option<Source>,
}

/// Metadata about the original feed when an entry is copied (`<source>`).
///
/// Contains a subset of feed metadata to preserve attribution
/// when entries are aggregated from multiple sources.
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2005/Atom",
    rename = "source",
    skip_all_unless_truthy
)]
pub struct Source {
    /// Identifier of the original feed.
    #[facet(xml::element)]
    pub id: Option<String>,

    /// Title of the original feed.
    #[facet(xml::element)]
    pub title: Option<TextContent>,

    /// Last update time of the original feed.
    #[facet(xml::element)]
    pub updated: Option<String>,

    /// Authors of the original feed.
    #[facet(xml::elements, rename = "author")]
    pub authors: Vec<Person>,

    /// Links from the original feed.
    #[facet(xml::elements, rename = "link")]
    pub links: Vec<Link>,

    /// Categories from the original feed.
    #[facet(xml::elements, rename = "category")]
    pub categories: Vec<Category>,

    /// Contributors from the original feed.
    #[facet(xml::elements, rename = "contributor")]
    pub contributors: Vec<Person>,

    /// Generator of the original feed.
    #[facet(xml::element)]
    pub generator: Option<Generator>,

    /// Icon from the original feed.
    #[facet(xml::element)]
    pub icon: Option<String>,

    /// Logo from the original feed.
    #[facet(xml::element)]
    pub logo: Option<String>,

    /// Rights from the original feed.
    #[facet(xml::element)]
    pub rights: Option<TextContent>,

    /// Subtitle from the original feed.
    #[facet(xml::element)]
    pub subtitle: Option<TextContent>,
}

// =============================================================================
// Person Construct
// =============================================================================

/// A person (author or contributor) in an Atom feed.
///
/// Used for both `<author>` and `<contributor>` elements.
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct Person {
    /// Human-readable name for the person (required).
    #[facet(xml::element)]
    pub name: Option<String>,

    /// IRI associated with the person (e.g., homepage).
    #[facet(xml::element)]
    pub uri: Option<String>,

    /// Email address for the person (RFC 2822 format).
    #[facet(xml::element)]
    pub email: Option<String>,
}

// =============================================================================
// Text Construct
// =============================================================================

/// Content type for text constructs.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum TextType {
    /// Plain text (default). Content should be displayed as-is.
    #[default]
    Text,
    /// HTML content. Markup should be escaped in the XML.
    Html,
    /// XHTML content. Markup is embedded as child elements.
    Xhtml,
}

/// A text construct used for title, subtitle, summary, and rights.
///
/// Per RFC 4287, text constructs can contain:
/// - Plain text (`type="text"`, default)
/// - Escaped HTML (`type="html"`)
/// - Inline XHTML (`type="xhtml"`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct TextContent {
    /// The content type. Defaults to "text" if not specified.
    #[facet(xml::attribute, rename = "type")]
    pub content_type: Option<TextType>,

    /// The text content (for type="text" or type="html").
    /// For type="xhtml", the content is within a div element.
    #[facet(xml::text)]
    pub content: Option<String>,
}

// =============================================================================
// Link Element
// =============================================================================

/// A link to a related resource (`<link>`).
///
/// Links define relationships between the feed/entry and external resources.
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct Link {
    /// The URI of the referenced resource (required).
    #[facet(xml::attribute)]
    pub href: Option<String>,

    /// The link relation type.
    /// Common values: "alternate", "self", "enclosure", "related", "via"
    #[facet(xml::attribute)]
    pub rel: Option<String>,

    /// Advisory media type of the resource.
    #[facet(xml::attribute, rename = "type")]
    pub media_type: Option<String>,

    /// Language of the referenced resource (RFC 3066 tag).
    #[facet(xml::attribute)]
    pub hreflang: Option<String>,

    /// Human-readable description of the link.
    #[facet(xml::attribute)]
    pub title: Option<String>,

    /// Advisory length of the resource in bytes.
    #[facet(xml::attribute)]
    pub length: Option<u64>,
}

// =============================================================================
// Category Element
// =============================================================================

/// A category for the feed or entry (`<category>`).
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct Category {
    /// The category identifier (required).
    #[facet(xml::attribute)]
    pub term: Option<String>,

    /// IRI identifying the categorization scheme.
    #[facet(xml::attribute)]
    pub scheme: Option<String>,

    /// Human-readable label for display.
    #[facet(xml::attribute)]
    pub label: Option<String>,
}

// =============================================================================
// Generator Element
// =============================================================================

/// Information about the software that generated the feed (`<generator>`).
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct Generator {
    /// IRI reference to the generator's website.
    #[facet(xml::attribute)]
    pub uri: Option<String>,

    /// Version of the generating software.
    #[facet(xml::attribute)]
    pub version: Option<String>,

    /// Human-readable name of the generator.
    #[facet(xml::text)]
    pub name: Option<String>,
}

// =============================================================================
// Content Element
// =============================================================================

/// The content of an entry (`<content>`).
///
/// Content can be inline (text, HTML, XHTML, or other XML) or referenced
/// via a `src` attribute for external content.
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2005/Atom", skip_all_unless_truthy)]
pub struct Content {
    /// The content type. For inline content: "text", "html", "xhtml", or a MIME type.
    /// For external content: a MIME type hint.
    #[facet(xml::attribute, rename = "type")]
    pub content_type: Option<String>,

    /// IRI reference to external content. If present, the element should be empty.
    #[facet(xml::attribute)]
    pub src: Option<String>,

    /// The inline content (when `src` is not present).
    /// For non-XML MIME types, this is Base64-encoded.
    #[facet(xml::text)]
    pub body: Option<String>,
}

// Re-export XML utilities for convenience
pub use facet_xml;

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_parse_basic_feed() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Example Feed</title>
                <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
                <updated>2003-12-13T18:30:02Z</updated>
                <author>
                    <name>John Doe</name>
                </author>
                <link href="http://example.org/"/>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        assert_eq!(
            feed.id.as_deref(),
            Some("urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6")
        );
        assert_eq!(
            feed.title.as_ref().and_then(|t| t.content.as_deref()),
            Some("Example Feed")
        );
        assert_eq!(feed.updated.as_deref(), Some("2003-12-13T18:30:02Z"));
        assert_eq!(feed.authors.len(), 1);
        assert_eq!(
            feed.authors.first().and_then(|a| a.name.as_deref()),
            Some("John Doe")
        );
        assert_eq!(feed.links.len(), 1);
        assert_eq!(
            feed.links.first().and_then(|l| l.href.as_deref()),
            Some("http://example.org/")
        );
    }

    #[test]
    fn test_parse_feed_with_entries() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Example Feed</title>
                <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
                <updated>2003-12-13T18:30:02Z</updated>
                <entry>
                    <title>Atom-Powered Robots Run Amok</title>
                    <id>urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a</id>
                    <updated>2003-12-13T18:30:02Z</updated>
                    <link href="http://example.org/2003/12/13/atom03"/>
                    <summary>Some text.</summary>
                </entry>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        assert_eq!(feed.entries.len(), 1);
        let entry = &feed.entries[0];
        assert_eq!(
            entry.title.as_ref().and_then(|t| t.content.as_deref()),
            Some("Atom-Powered Robots Run Amok")
        );
        assert_eq!(
            entry.id.as_deref(),
            Some("urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a")
        );
        assert_eq!(
            entry.summary.as_ref().and_then(|s| s.content.as_deref()),
            Some("Some text.")
        );
    }

    #[test]
    fn test_parse_entry_with_content() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <entry>
                    <title>Test Entry</title>
                    <id>test:entry:1</id>
                    <updated>2024-01-01T00:00:00Z</updated>
                    <content type="html">&lt;p&gt;Hello, World!&lt;/p&gt;</content>
                </entry>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();
        let entry = &feed.entries[0];
        let content = entry.content.as_ref().unwrap();

        assert_eq!(content.content_type.as_deref(), Some("html"));
        assert_eq!(content.body.as_deref(), Some("<p>Hello, World!</p>"));
    }

    #[test]
    fn test_parse_link_attributes() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <link href="http://example.org/" rel="alternate" type="text/html" hreflang="en" title="Example"/>
                <link href="http://example.org/feed.atom" rel="self" type="application/atom+xml"/>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        assert_eq!(feed.links.len(), 2);

        let alternate = &feed.links[0];
        assert_eq!(alternate.href.as_deref(), Some("http://example.org/"));
        assert_eq!(alternate.rel.as_deref(), Some("alternate"));
        assert_eq!(alternate.media_type.as_deref(), Some("text/html"));
        assert_eq!(alternate.hreflang.as_deref(), Some("en"));
        assert_eq!(alternate.title.as_deref(), Some("Example"));

        let self_link = &feed.links[1];
        assert_eq!(
            self_link.href.as_deref(),
            Some("http://example.org/feed.atom")
        );
        assert_eq!(self_link.rel.as_deref(), Some("self"));
    }

    #[test]
    fn test_parse_category() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <category term="technology" scheme="http://example.org/categories" label="Technology"/>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        assert_eq!(feed.categories.len(), 1);
        let cat = &feed.categories[0];
        assert_eq!(cat.term.as_deref(), Some("technology"));
        assert_eq!(cat.scheme.as_deref(), Some("http://example.org/categories"));
        assert_eq!(cat.label.as_deref(), Some("Technology"));
    }

    #[test]
    fn test_parse_generator() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <generator uri="http://example.org/generator" version="1.0">Example Generator</generator>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        let generator = feed.generator.as_ref().unwrap();
        assert_eq!(generator.name.as_deref(), Some("Example Generator"));
        assert_eq!(
            generator.uri.as_deref(),
            Some("http://example.org/generator")
        );
        assert_eq!(generator.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn test_parse_person_full() {
        let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <author>
                    <name>John Doe</name>
                    <uri>http://example.org/johndoe</uri>
                    <email>john@example.org</email>
                </author>
                <contributor>
                    <name>Jane Smith</name>
                </contributor>
            </feed>
        "#};

        let feed: Feed = from_str(xml).unwrap();

        assert_eq!(feed.authors.len(), 1);
        let author = &feed.authors[0];
        assert_eq!(author.name.as_deref(), Some("John Doe"));
        assert_eq!(author.uri.as_deref(), Some("http://example.org/johndoe"));
        assert_eq!(author.email.as_deref(), Some("john@example.org"));

        assert_eq!(feed.contributors.len(), 1);
        assert_eq!(feed.contributors[0].name.as_deref(), Some("Jane Smith"));
    }

    #[test]
    fn test_roundtrip_simple_feed() {
        let feed = Feed {
            id: Some("urn:uuid:test".to_string()),
            title: Some(TextContent {
                content_type: None,
                content: Some("Test Feed".to_string()),
            }),
            updated: Some("2024-01-01T00:00:00Z".to_string()),
            authors: vec![Person {
                name: Some("Test Author".to_string()),
                uri: None,
                email: None,
            }],
            links: vec![Link {
                href: Some("http://example.org/".to_string()),
                rel: Some("alternate".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let xml = to_string(&feed).unwrap();
        let parsed: Feed = from_str(&xml).unwrap();

        assert_eq!(parsed.id, feed.id);
        assert_eq!(
            parsed.title.as_ref().and_then(|t| t.content.as_ref()),
            feed.title.as_ref().and_then(|t| t.content.as_ref())
        );
        assert_eq!(parsed.updated, feed.updated);
        assert_eq!(parsed.authors.len(), feed.authors.len());
    }
}
