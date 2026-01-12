//! Document tree representation for Styx configuration files.
//!
//! This crate provides a high-level API for working with Styx documents,
//! including parsing, accessing values by path, and serialization.

mod builder;
mod value;

pub use builder::{BuildError, TreeBuilder};
pub use styx_parse::{ScalarKind, Separator, Span};
pub use value::{Entry, Object, Scalar, Sequence, Tagged, Value};

/// Parse a Styx document into a tree.
pub fn parse(source: &str) -> Result<Value, BuildError> {
    let parser = styx_parse::Parser::new(source);
    let mut builder = TreeBuilder::new();
    parser.parse(&mut builder);
    builder.finish()
}

/// A Styx document (root is always an implicit object).
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The root object.
    pub root: Object,
    /// Leading doc comments (before first entry).
    pub leading_comments: Vec<String>,
}

impl Document {
    /// Parse a Styx document.
    pub fn parse(source: &str) -> Result<Self, BuildError> {
        let value = parse(source)?;
        match value {
            Value::Object(root) => Ok(Document {
                root,
                leading_comments: Vec::new(),
            }),
            _ => Err(BuildError::UnexpectedEvent(
                "expected object at root".to_string(),
            )),
        }
    }

    /// Get a value by path.
    pub fn get(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return None;
        }

        let (segment, rest) = split_path(path);
        let value = self.root.get(segment)?;
        if rest.is_empty() {
            Some(value)
        } else {
            value.get(rest)
        }
    }
}

fn split_path(path: &str) -> (&str, &str) {
    if path.starts_with('[') {
        if let Some(end) = path.find(']') {
            let segment = &path[..=end];
            let rest = &path[end + 1..];
            let rest = rest.strip_prefix('.').unwrap_or(rest);
            return (segment, rest);
        }
    }

    let dot_pos = path.find('.');
    let bracket_pos = path.find('[');

    match (dot_pos, bracket_pos) {
        (Some(d), Some(b)) if b < d => (&path[..b], &path[b..]),
        (Some(d), _) => (&path[..d], &path[d + 1..]),
        (None, Some(b)) => (&path[..b], &path[b..]),
        (None, None) => (path, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let doc = Document::parse("name Alice\nage 30").unwrap();
        assert_eq!(doc.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(doc.get("age").and_then(|v| v.as_str()), Some("30"));
    }

    #[test]
    fn test_parse_empty() {
        let doc = Document::parse("").unwrap();
        assert!(doc.root.is_empty());
    }

    #[test]
    fn test_convenience_parse() {
        let value = parse("greeting hello").unwrap();
        assert_eq!(
            value.get("greeting").and_then(|v| v.as_str()),
            Some("hello")
        );
    }
}
