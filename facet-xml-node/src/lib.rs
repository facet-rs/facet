//! Raw XML element types and deserialization from Element trees.

mod parser;

use facet_xml as xml;
use std::collections::HashMap;

pub use parser::{ElementParseError, ElementParser, from_element};

/// Content that can appear inside an XML element - either child elements or text.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum Content {
    /// Text content.
    #[facet(xml::text)]
    Text(String),
    /// A child element (catch-all for any tag name).
    #[facet(xml::custom_element)]
    Element(Element),
}

impl Content {
    /// Returns `Some(&str)` if this is text content.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Returns `Some(&Element)` if this is an element.
    pub fn as_element(&self) -> Option<&Element> {
        match self {
            Content::Element(e) => Some(e),
            _ => None,
        }
    }
}

/// An XML element that captures any tag name, attributes, and children.
///
/// This type can represent arbitrary XML structure without needing
/// a predefined schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, facet::Facet)]
pub struct Element {
    /// The element's tag name (captured dynamically).
    #[facet(xml::tag, default)]
    pub tag: String,

    /// All attributes as key-value pairs.
    #[facet(flatten, default)]
    pub attrs: HashMap<String, String>,

    /// Child content (elements and text).
    #[facet(flatten, default)]
    #[facet(recursive_type)]
    pub children: Vec<Content>,
}

impl Element {
    /// Create a new element with just a tag name.
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            attrs: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Add an attribute.
    pub fn with_attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.insert(name.into(), value.into());
        self
    }

    /// Add a child element.
    pub fn with_child(mut self, child: Element) -> Self {
        self.children.push(Content::Element(child));
        self
    }

    /// Add text content.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.children.push(Content::Text(text.into()));
        self
    }

    /// Get an attribute value by name.
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(|s| s.as_str())
    }

    /// Iterate over child elements (skipping text nodes).
    pub fn child_elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| c.as_element())
    }

    /// Get the combined text content (concatenated from all text children).
    pub fn text_content(&self) -> String {
        let mut result = String::new();
        for child in &self.children {
            match child {
                Content::Text(t) => result.push_str(t),
                Content::Element(e) => result.push_str(&e.text_content()),
            }
        }
        result
    }

    /// Get a mutable reference to a descendant element by path.
    /// Path is a sequence of child indices.
    pub fn get_mut(&mut self, path: &[usize]) -> Option<&mut Element> {
        if path.is_empty() {
            return Some(self);
        }

        let idx = path[0];
        let child = self.children.get_mut(idx)?;
        match child {
            Content::Element(e) => e.get_mut(&path[1..]),
            Content::Text(_) => {
                // Text nodes can't have children, so we can only
                // reach them if this is the final index
                if path.len() == 1 {
                    None // Can't return &mut Element for a text node
                } else {
                    None
                }
            }
        }
    }

    /// Get a mutable reference to the children vec at a path.
    pub fn children_mut(&mut self, path: &[usize]) -> Option<&mut Vec<Content>> {
        let node = self.get_mut(path)?;
        Some(&mut node.children)
    }

    /// Get a mutable reference to the attrs at a path.
    pub fn attrs_mut(&mut self, path: &[usize]) -> Option<&mut HashMap<String, String>> {
        let node = self.get_mut(path)?;
        Some(&mut node.attrs)
    }

    /// Serialize to HTML string.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        self.write_html(&mut out);
        out
    }

    /// Write HTML to a string buffer.
    pub fn write_html(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.tag);
        // Sort attrs for deterministic output
        let mut attr_list: Vec<_> = self.attrs.iter().collect();
        attr_list.sort_by_key(|(k, _)| *k);
        for (k, v) in attr_list {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&html_escape(v));
            out.push('"');
        }
        out.push('>');
        for child in &self.children {
            match child {
                Content::Text(s) => out.push_str(s),
                Content::Element(e) => e.write_html(out),
            }
        }
        out.push_str("</");
        out.push_str(&self.tag);
        out.push('>');
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl From<Element> for Content {
    fn from(e: Element) -> Self {
        Content::Element(e)
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Content::Text(s)
    }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Content::Text(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_builder_api() {
        let elem = Element::new("root")
            .with_attr("id", "123")
            .with_child(Element::new("child").with_text("hello world"));

        assert_eq!(elem.tag, "root");
        assert_eq!(elem.get_attr("id"), Some("123"));
        assert_eq!(elem.children.len(), 1);

        let child = elem.child_elements().next().unwrap();
        assert_eq!(child.tag, "child");
        assert_eq!(child.text_content(), "hello world");
    }

    #[test]
    fn parse_simple_xml() {
        let xml = r#"<root><child>hello</child></root>"#;
        let elem: Element = facet_xml::from_str(xml).unwrap();

        assert_eq!(elem.tag, "root");
        assert_eq!(elem.children.len(), 1);

        let child = elem.child_elements().next().unwrap();
        assert_eq!(child.tag, "child");
        assert_eq!(child.text_content(), "hello");
    }

    #[test]
    fn parse_with_attributes() {
        let xml = r#"<root id="123" class="test"><child name="foo">bar</child></root>"#;
        let elem: Element = facet_xml::from_str(xml).unwrap();

        assert_eq!(elem.tag, "root");
        assert_eq!(elem.get_attr("id"), Some("123"));
        assert_eq!(elem.get_attr("class"), Some("test"));

        let child = elem.child_elements().next().unwrap();
        assert_eq!(child.get_attr("name"), Some("foo"));
        assert_eq!(child.text_content(), "bar");
    }

    #[test]
    fn parse_mixed_content() {
        let xml = r#"<p>Hello <b>world</b>!</p>"#;
        let elem: Element = facet_xml::from_str(xml).unwrap();

        assert_eq!(elem.tag, "p");
        assert_eq!(elem.children.len(), 3);
        // Note: trailing whitespace is trimmed by XML parser
        assert_eq!(elem.children[0].as_text(), Some("Hello"));
        assert_eq!(elem.children[1].as_element().unwrap().tag, "b");
        assert_eq!(elem.children[2].as_text(), Some("!"));
        assert_eq!(elem.text_content(), "Helloworld!");
    }

    #[test]
    fn from_element_to_struct() {
        #[derive(facet::Facet, Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
        }

        let elem = Element::new("person")
            .with_child(Element::new("name").with_text("Alice"))
            .with_child(Element::new("age").with_text("30"));

        let person: Person = from_element(&elem).unwrap();
        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 30);
    }

    #[test]
    fn from_element_with_attrs() {
        #[derive(facet::Facet, Debug, PartialEq)]
        struct Item {
            #[facet(xml::attribute)]
            id: String,
            value: String,
        }

        let elem = Element::new("item")
            .with_attr("id", "123")
            .with_child(Element::new("value").with_text("hello"));

        let item: Item = from_element(&elem).unwrap();
        assert_eq!(item.id, "123");
        assert_eq!(item.value, "hello");
    }

    #[test]
    fn parse_html_into_element() {
        // Test if we can parse HTML into the generic Element type
        // This would be useful for apply_patches in facet-html-diff
        let html = r#"<html><body><div><p>Hello</p></div></body></html>"#;
        let elem: Element = facet_html::from_str(html).unwrap();

        assert_eq!(elem.tag, "html");

        // Navigate to body
        let body = elem.child_elements().next().unwrap();
        assert_eq!(body.tag, "body");

        // Navigate to div
        let div = body.child_elements().next().unwrap();
        assert_eq!(div.tag, "div");

        // Navigate to p
        let p = div.child_elements().next().unwrap();
        assert_eq!(p.tag, "p");
        assert_eq!(p.text_content(), "Hello");
    }
}
