//! Typed AST wrappers over CST nodes.
//!
//! These provide a more ergonomic API for navigating the syntax tree
//! while still preserving access to the underlying CST for source locations.

use crate::syntax_kind::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Trait for AST nodes that wrap CST nodes.
pub trait AstNode: Sized {
    /// Try to cast a syntax node to this AST type.
    fn cast(node: SyntaxNode) -> Option<Self>;

    /// Get the underlying syntax node.
    fn syntax(&self) -> &SyntaxNode;

    /// Get the source text of this node.
    fn text(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Owned(self.syntax().to_string())
    }
}

/// Macro for defining simple AST node wrappers.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident, $kind:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == $kind {
                    Some(Self(node))
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

ast_node!(
    /// The root document node.
    Document,
    SyntaxKind::DOCUMENT
);

ast_node!(
    /// An entry (key-value pair or sequence element).
    Entry,
    SyntaxKind::ENTRY
);

ast_node!(
    /// An explicit object `{ ... }`.
    Object,
    SyntaxKind::OBJECT
);

ast_node!(
    /// A sequence `( ... )`.
    Sequence,
    SyntaxKind::SEQUENCE
);

ast_node!(
    /// A scalar value.
    Scalar,
    SyntaxKind::SCALAR
);

ast_node!(
    /// A unit value `@`.
    Unit,
    SyntaxKind::UNIT
);

ast_node!(
    /// A tag `@name` with optional payload.
    Tag,
    SyntaxKind::TAG
);

ast_node!(
    /// A heredoc value.
    Heredoc,
    SyntaxKind::HEREDOC
);

ast_node!(
    /// The key part of an entry.
    Key,
    SyntaxKind::KEY
);

ast_node!(
    /// The value part of an entry.
    Value,
    SyntaxKind::VALUE
);

// === Document ===

impl Document {
    /// Iterate over top-level entries.
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.0.children().filter_map(Entry::cast)
    }
}

// === Entry ===

impl Entry {
    /// Get the key of this entry (if it has one).
    pub fn key(&self) -> Option<Key> {
        self.0.children().find_map(Key::cast)
    }

    /// Get the value of this entry (if it has one).
    pub fn value(&self) -> Option<Value> {
        self.0.children().find_map(Value::cast)
    }

    /// Get the key text.
    pub fn key_text(&self) -> Option<String> {
        self.key().map(|k| k.text_content())
    }

    /// Get preceding doc comments.
    pub fn doc_comments(&self) -> impl Iterator<Item = SyntaxToken> {
        // Look for DOC_COMMENT tokens before this entry in the parent
        self.0
            .siblings_with_tokens(rowan::Direction::Prev)
            .skip(1) // Skip self
            .take_while(|el| {
                el.kind() == SyntaxKind::WHITESPACE
                    || el.kind() == SyntaxKind::NEWLINE
                    || el.kind() == SyntaxKind::DOC_COMMENT
            })
            .filter_map(|el| el.into_token())
            .filter(|t| t.kind() == SyntaxKind::DOC_COMMENT)
    }
}

// === Key ===

impl Key {
    /// Get the text content of this key, processing escapes if quoted.
    pub fn text_content(&self) -> String {
        // Get the first meaningful token
        for child in self.0.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Token(token) => {
                    return match token.kind() {
                        SyntaxKind::BARE_SCALAR => token.text().to_string(),
                        SyntaxKind::QUOTED_SCALAR => unescape_quoted(token.text()),
                        SyntaxKind::RAW_SCALAR => token.text().to_string(),
                        _ => continue,
                    };
                }
                rowan::NodeOrToken::Node(node) => {
                    // Recurse into SCALAR node
                    if node.kind() == SyntaxKind::SCALAR
                        && let Some(scalar) = Scalar::cast(node)
                    {
                        return scalar.text_content();
                    }
                }
            }
        }
        String::new()
    }

    /// Get the raw text without escape processing.
    pub fn raw_text(&self) -> String {
        self.0.to_string()
    }
}

// === Value ===

impl Value {
    /// Get the inner value as an enum.
    pub fn kind(&self) -> ValueKind {
        for child in self.0.children() {
            match child.kind() {
                SyntaxKind::SCALAR => return ValueKind::Scalar(Scalar::cast(child).unwrap()),
                SyntaxKind::OBJECT => return ValueKind::Object(Object::cast(child).unwrap()),
                SyntaxKind::SEQUENCE => return ValueKind::Sequence(Sequence::cast(child).unwrap()),
                SyntaxKind::UNIT => return ValueKind::Unit(Unit::cast(child).unwrap()),
                SyntaxKind::TAG => return ValueKind::Tag(Tag::cast(child).unwrap()),
                SyntaxKind::HEREDOC => return ValueKind::Heredoc(Heredoc::cast(child).unwrap()),
                _ => continue,
            }
        }
        ValueKind::Missing
    }
}

/// The kind of value in an entry.
#[derive(Debug, Clone)]
pub enum ValueKind {
    /// A scalar value.
    Scalar(Scalar),
    /// An object.
    Object(Object),
    /// A sequence.
    Sequence(Sequence),
    /// A unit value.
    Unit(Unit),
    /// A tag.
    Tag(Tag),
    /// A heredoc.
    Heredoc(Heredoc),
    /// Missing value (parse error).
    Missing,
}

// === Object ===

/// The separator mode detected in an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    /// Entries separated by newlines.
    Newline,
    /// Entries separated by commas.
    Comma,
    /// Mixed separators (error).
    Mixed,
}

impl Object {
    /// Iterate over entries in this object.
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.0.children().filter_map(Entry::cast)
    }

    /// Detect the separator mode used in this object.
    pub fn separator(&self) -> Separator {
        let mut has_comma = false;
        let mut has_newline = false;

        for token in self
            .0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
        {
            match token.kind() {
                SyntaxKind::COMMA => has_comma = true,
                SyntaxKind::NEWLINE => has_newline = true,
                _ => {}
            }
        }

        if has_comma && has_newline {
            Separator::Mixed
        } else if has_newline {
            Separator::Newline
        } else {
            // Comma-separated or no separators (single/empty) = inline format
            Separator::Comma
        }
    }

    /// Get an entry by key name.
    pub fn get(&self, key: &str) -> Option<Entry> {
        self.entries()
            .find(|e| e.key_text().as_deref() == Some(key))
    }
}

// === Sequence ===

impl Sequence {
    /// Iterate over elements in this sequence.
    ///
    /// The parser wraps sequence elements in ENTRY/KEY nodes for uniformity.
    /// This method extracts the actual value from each entry.
    pub fn elements(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children().filter_map(|n| {
            if n.kind() == SyntaxKind::ENTRY {
                // Find the KEY child, then get its first value child
                n.children()
                    .find(|c| c.kind() == SyntaxKind::KEY)
                    .and_then(|key| {
                        key.children().find(|c| {
                            matches!(
                                c.kind(),
                                SyntaxKind::SCALAR
                                    | SyntaxKind::OBJECT
                                    | SyntaxKind::SEQUENCE
                                    | SyntaxKind::UNIT
                                    | SyntaxKind::TAG
                                    | SyntaxKind::HEREDOC
                            )
                        })
                    })
            } else {
                // Fallback: direct value children (shouldn't happen with current parser)
                matches!(
                    n.kind(),
                    SyntaxKind::SCALAR
                        | SyntaxKind::OBJECT
                        | SyntaxKind::SEQUENCE
                        | SyntaxKind::UNIT
                        | SyntaxKind::TAG
                        | SyntaxKind::HEREDOC
                )
                .then_some(n)
            }
        })
    }

    /// Iterate over entries in this sequence (ENTRY nodes).
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.0.children().filter_map(Entry::cast)
    }

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.elements().count()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the sequence is multiline (contains newlines or comments).
    pub fn is_multiline(&self) -> bool {
        self.0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .any(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::NEWLINE | SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT
                )
            })
    }
}

// === Scalar ===

/// The kind of scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// Bare (unquoted) scalar.
    Bare,
    /// Quoted string.
    Quoted,
    /// Raw string.
    Raw,
}

impl Scalar {
    /// Get the text content, processing escapes for quoted strings.
    pub fn text_content(&self) -> String {
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
        {
            return match token.kind() {
                SyntaxKind::BARE_SCALAR => token.text().to_string(),
                SyntaxKind::QUOTED_SCALAR => unescape_quoted(token.text()),
                SyntaxKind::RAW_SCALAR => token.text().to_string(),
                _ => continue,
            };
        }
        String::new()
    }

    /// Get the raw text without escape processing.
    pub fn raw_text(&self) -> String {
        self.0.to_string()
    }

    /// Get the kind of scalar.
    pub fn kind(&self) -> ScalarKind {
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
        {
            return match token.kind() {
                SyntaxKind::BARE_SCALAR => ScalarKind::Bare,
                SyntaxKind::QUOTED_SCALAR => ScalarKind::Quoted,
                SyntaxKind::RAW_SCALAR => ScalarKind::Raw,
                _ => continue,
            };
        }
        ScalarKind::Bare
    }
}

// === Tag ===

impl Tag {
    /// Get the tag name (without @).
    pub fn name(&self) -> Option<String> {
        // The tag token is @name, so we strip the @ prefix
        self.0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.kind() == SyntaxKind::TAG_TOKEN)
            .map(|t| t.text()[1..].to_string()) // Skip the '@' prefix
    }

    /// Get the tag payload if present.
    pub fn payload(&self) -> Option<SyntaxNode> {
        self.0
            .children()
            .find(|n| n.kind() == SyntaxKind::TAG_PAYLOAD)
            .and_then(|n| n.children().next())
    }
}

// === Heredoc ===

impl Heredoc {
    /// Get the heredoc content (without delimiters).
    pub fn content(&self) -> String {
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
        {
            if token.kind() == SyntaxKind::HEREDOC_CONTENT {
                return token.text().to_string();
            }
        }
        String::new()
    }

    /// Get the delimiter name.
    pub fn delimiter(&self) -> Option<String> {
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|el| el.into_token())
        {
            if token.kind() == SyntaxKind::HEREDOC_START {
                // Extract delimiter from <<DELIM\n
                let text = token.text();
                if let Some(rest) = text.strip_prefix("<<") {
                    return Some(rest.trim_end().to_string());
                }
            }
        }
        None
    }
}

// === Helpers ===

/// Process escape sequences in a quoted string.
fn unescape_quoted(text: &str) -> String {
    // Remove surrounding quotes
    let inner = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(text);

    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some(c) => {
                    // Unknown escape, keep as-is
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn doc(source: &str) -> Document {
        let p = parse(source);
        assert!(p.is_ok(), "parse errors: {:?}", p.errors());
        Document::cast(p.syntax()).unwrap()
    }

    #[test]
    fn test_document_entries() {
        let d = doc("a 1\nb 2\nc 3");
        let entries: Vec<_> = d.entries().collect();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_entry_key_value() {
        let d = doc("host localhost");
        let entry = d.entries().next().unwrap();

        assert_eq!(entry.key_text(), Some("host".to_string()));

        let value = entry.value().unwrap();
        if let ValueKind::Scalar(s) = value.kind() {
            assert_eq!(s.text_content(), "localhost");
        } else {
            panic!("expected scalar value");
        }
    }

    #[test]
    fn test_object_entries() {
        let d = doc("config { host localhost, port 8080 }");
        let entry = d.entries().next().unwrap();
        let value = entry.value().unwrap();

        if let ValueKind::Object(obj) = value.kind() {
            assert_eq!(obj.separator(), Separator::Comma);

            let entries: Vec<_> = obj.entries().collect();
            assert_eq!(entries.len(), 2);

            assert_eq!(entries[0].key_text(), Some("host".to_string()));
            assert_eq!(entries[1].key_text(), Some("port".to_string()));
        } else {
            panic!("expected object value");
        }
    }

    #[test]
    fn test_object_get() {
        let d = doc("{ name Alice, age 30 }");
        let entry = d.entries().next().unwrap();
        let key = entry.key().unwrap();
        let obj_node = key.syntax().children().next().unwrap();
        let obj = Object::cast(obj_node).unwrap();

        let name_entry = obj.get("name").unwrap();
        let val = name_entry.value().unwrap();
        if let ValueKind::Scalar(s) = val.kind() {
            assert_eq!(s.text_content(), "Alice");
        }
    }

    #[test]
    fn test_sequence() {
        let d = doc("items (a b c)");
        let entry = d.entries().next().unwrap();
        let value = entry.value().unwrap();

        if let ValueKind::Sequence(seq) = value.kind() {
            assert_eq!(seq.len(), 3);
        } else {
            panic!("expected sequence value");
        }
    }

    #[test]
    fn test_quoted_string_escapes() {
        let d = doc(r#"msg "hello\nworld""#);
        let entry = d.entries().next().unwrap();
        let value = entry.value().unwrap();

        if let ValueKind::Scalar(s) = value.kind() {
            assert_eq!(s.text_content(), "hello\nworld");
            assert_eq!(s.kind(), ScalarKind::Quoted);
        } else {
            panic!("expected scalar value");
        }
    }

    #[test]
    fn test_tag() {
        // Tag with attached payload (no space) - payload IS part of tag
        let d = doc("key @Some(value)");
        let entry = d.entries().next().unwrap();
        let value = entry.value().unwrap();
        let tag_node = value.syntax().children().next().unwrap();
        let tag = Tag::cast(tag_node).unwrap();

        assert_eq!(tag.name(), Some("Some".to_string()));
        assert!(tag.payload().is_some(), "attached payload should exist");
    }

    #[test]
    fn test_tag_without_payload() {
        // Tag with space before next value - NO payload (per grammar)
        let d = doc("@Some value");
        let entry = d.entries().next().unwrap();
        let key = entry.key().unwrap();
        let tag_node = key.syntax().children().next().unwrap();
        let tag = Tag::cast(tag_node).unwrap();

        assert_eq!(tag.name(), Some("Some".to_string()));
        assert!(
            tag.payload().is_none(),
            "spaced value should not be payload"
        );

        // The value should be separate
        let value = entry.value().unwrap();
        assert!(matches!(value.kind(), ValueKind::Scalar(_)));
    }

    #[test]
    fn test_unit() {
        let d = doc("empty @");
        let entry = d.entries().next().unwrap();
        let value = entry.value().unwrap();

        assert!(matches!(value.kind(), ValueKind::Unit(_)));
    }

    #[test]
    fn test_unescape_quoted() {
        assert_eq!(unescape_quoted(r#""hello""#), "hello");
        assert_eq!(unescape_quoted(r#""hello\nworld""#), "hello\nworld");
        assert_eq!(unescape_quoted(r#""tab\there""#), "tab\there");
        assert_eq!(unescape_quoted(r#""quote\"here""#), "quote\"here");
        assert_eq!(unescape_quoted(r#""back\\slash""#), "back\\slash");
    }
}
