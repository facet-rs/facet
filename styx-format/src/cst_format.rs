//! CST-based formatter for Styx documents.
//!
//! This formatter works directly with the lossless CST (Concrete Syntax Tree),
//! preserving all comments and producing properly indented output.

use styx_cst::ast::{AstNode, Document, Entry, Object, Separator};
use styx_cst::{SyntaxKind, SyntaxNode};

use crate::FormatOptions;

/// Format a Styx document from its CST.
///
/// This preserves all comments and produces properly indented output.
pub fn format_cst(node: &SyntaxNode, options: FormatOptions) -> String {
    let mut formatter = CstFormatter::new(options);
    formatter.format_node(node);
    formatter.finish()
}

/// Format a Styx document from source text.
///
/// Parses the source, formats the CST, and returns the formatted output.
/// Returns the original source if parsing fails.
pub fn format_source(source: &str, options: FormatOptions) -> String {
    let parsed = styx_cst::parse(source);
    if !parsed.is_ok() {
        // Don't format documents with parse errors
        return source.to_string();
    }
    format_cst(&parsed.syntax(), options)
}

struct CstFormatter {
    out: String,
    options: FormatOptions,
    indent_level: usize,
    /// Track if we're at the start of a line (for indentation)
    at_line_start: bool,
    /// Track if we just wrote a newline
    after_newline: bool,
}

impl CstFormatter {
    fn new(options: FormatOptions) -> Self {
        Self {
            out: String::new(),
            options,
            indent_level: 0,
            at_line_start: true,
            after_newline: false,
        }
    }

    fn finish(mut self) -> String {
        // Ensure trailing newline
        if !self.out.ends_with('\n') && !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out
    }

    fn write_indent(&mut self) {
        if self.at_line_start && self.indent_level > 0 {
            for _ in 0..self.indent_level {
                self.out.push_str(self.options.indent);
            }
        }
        self.at_line_start = false;
    }

    fn write(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.write_indent();
        self.out.push_str(s);
        self.after_newline = false;
    }

    fn write_newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
        self.after_newline = true;
    }

    fn format_node(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::DOCUMENT => self.format_document(node),
            SyntaxKind::ENTRY => self.format_entry(node),
            SyntaxKind::OBJECT => self.format_object(node),
            SyntaxKind::SEQUENCE => self.format_sequence(node),
            SyntaxKind::KEY => self.format_key(node),
            SyntaxKind::VALUE => self.format_value(node),
            SyntaxKind::SCALAR => self.format_scalar(node),
            SyntaxKind::TAG => self.format_tag(node),
            SyntaxKind::TAG_NAME => self.format_tag_name(node),
            SyntaxKind::TAG_PAYLOAD => self.format_tag_payload(node),
            SyntaxKind::UNIT => self.write("@"),
            SyntaxKind::HEREDOC => self.format_heredoc(node),
            _ => {
                // For unknown nodes, format children
                for child in node.children() {
                    self.format_node(&child);
                }
            }
        }
    }

    fn format_document(&mut self, node: &SyntaxNode) {
        let doc = Document::cast(node.clone()).unwrap();
        let entries: Vec<_> = doc.entries().collect();

        for (i, entry) in entries.iter().enumerate() {
            // Write preceding comments
            self.write_preceding_comments(entry.syntax());

            self.format_node(entry.syntax());

            // Add newline after each entry (except possibly the last)
            if i < entries.len() - 1 {
                self.write_newline();

                // Add extra blank line after:
                // - schema declaration (@ entry at root)
                // - entries with doc comments
                let is_schema_decl = i == 0 && is_schema_declaration(entry);
                if is_schema_decl || entry.doc_comments().next().is_some() {
                    self.write_newline();
                }
            }
        }
    }

    fn format_entry(&mut self, node: &SyntaxNode) {
        let entry = Entry::cast(node.clone()).unwrap();

        if let Some(key) = entry.key() {
            self.format_node(key.syntax());
        }

        // Space between key and value
        if entry.value().is_some() {
            self.write(" ");
        }

        if let Some(value) = entry.value() {
            self.format_node(value.syntax());
        }
    }

    fn format_object(&mut self, node: &SyntaxNode) {
        let obj = Object::cast(node.clone()).unwrap();
        let entries: Vec<_> = obj.entries().collect();
        let separator = obj.separator();

        self.write("{");

        if entries.is_empty() {
            self.write("}");
            return;
        }

        match separator {
            Separator::Newline | Separator::Mixed => {
                // Multiline format
                self.write_newline();
                self.indent_level += 1;

                for (i, entry) in entries.iter().enumerate() {
                    self.write_preceding_comments(entry.syntax());
                    self.format_node(entry.syntax());

                    if i < entries.len() - 1 {
                        self.write_newline();
                    }
                }

                self.write_newline();
                self.indent_level -= 1;
                self.write("}");
            }
            Separator::Comma => {
                // Inline format
                for (i, entry) in entries.iter().enumerate() {
                    self.format_node(entry.syntax());

                    if i < entries.len() - 1 {
                        self.write(", ");
                    }
                }
                self.write("}");
            }
        }
    }

    fn format_sequence(&mut self, node: &SyntaxNode) {
        self.write("(");

        // Get all elements (which are wrapped in ENTRY/KEY nodes)
        let elements: Vec<_> = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::ENTRY)
            .collect();

        for (i, entry) in elements.iter().enumerate() {
            // Get the actual value from the entry's key
            if let Some(key) = entry.children().find(|n| n.kind() == SyntaxKind::KEY) {
                for child in key.children() {
                    self.format_node(&child);
                }
            }

            if i < elements.len() - 1 {
                self.write(" ");
            }
        }

        self.write(")");
    }

    fn format_key(&mut self, node: &SyntaxNode) {
        // Format the key content (scalar, tag, unit, etc.)
        for child in node.children() {
            self.format_node(&child);
        }

        // Also check for direct tokens (like BARE_SCALAR in simple keys)
        for token in node.children_with_tokens().filter_map(|el| el.into_token()) {
            match token.kind() {
                SyntaxKind::BARE_SCALAR | SyntaxKind::QUOTED_SCALAR | SyntaxKind::RAW_SCALAR => {
                    self.write(token.text());
                }
                _ => {}
            }
        }
    }

    fn format_value(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            self.format_node(&child);
        }
    }

    fn format_scalar(&mut self, node: &SyntaxNode) {
        // Get the scalar token and write it as-is
        for token in node.children_with_tokens().filter_map(|el| el.into_token()) {
            match token.kind() {
                SyntaxKind::BARE_SCALAR | SyntaxKind::QUOTED_SCALAR | SyntaxKind::RAW_SCALAR => {
                    self.write(token.text());
                }
                _ => {}
            }
        }
    }

    fn format_tag(&mut self, node: &SyntaxNode) {
        self.write("@");

        for child in node.children() {
            match child.kind() {
                SyntaxKind::TAG_NAME => self.format_tag_name(&child),
                SyntaxKind::TAG_PAYLOAD => {
                    // Check if payload starts with sequence/object (no space needed)
                    // or something else (space needed, e.g., @tag value, @outer @inner)
                    let first_child = child.children().next();
                    let needs_space = first_child
                        .as_ref()
                        .map(|c| !matches!(c.kind(), SyntaxKind::SEQUENCE | SyntaxKind::OBJECT))
                        .unwrap_or(false);
                    if needs_space {
                        self.write(" ");
                    }
                    self.format_tag_payload(&child);
                }
                _ => {}
            }
        }
    }

    fn format_tag_name(&mut self, node: &SyntaxNode) {
        for token in node.children_with_tokens().filter_map(|el| el.into_token()) {
            if token.kind() == SyntaxKind::BARE_SCALAR {
                self.write(token.text());
            }
        }
    }

    fn format_tag_payload(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::SEQUENCE => {
                    // Sequence payload: @tag(...)
                    self.format_sequence(&child);
                }
                SyntaxKind::OBJECT => {
                    // Object payload: @tag{...}
                    self.format_object(&child);
                }
                _ => self.format_node(&child),
            }
        }
    }

    fn format_heredoc(&mut self, node: &SyntaxNode) {
        // Heredocs are preserved as-is
        self.write(&node.to_string());
    }

    /// Write any doc comments that precede this node.
    fn write_preceding_comments(&mut self, node: &SyntaxNode) {
        let comments: Vec<_> = node
            .siblings_with_tokens(rowan::Direction::Prev)
            .skip(1) // Skip self
            .take_while(|el| {
                matches!(
                    el.kind(),
                    SyntaxKind::WHITESPACE
                        | SyntaxKind::NEWLINE
                        | SyntaxKind::DOC_COMMENT
                        | SyntaxKind::LINE_COMMENT
                )
            })
            .filter_map(|el| el.into_token())
            .filter(|t| t.kind() == SyntaxKind::DOC_COMMENT || t.kind() == SyntaxKind::LINE_COMMENT)
            .collect();

        // Comments are collected in reverse order, so reverse them
        for comment in comments.into_iter().rev() {
            self.write(comment.text());
            self.write_newline();
        }
    }
}

/// Check if an entry is a schema declaration (unit key with a value).
fn is_schema_declaration(entry: &Entry) -> bool {
    if let Some(key) = entry.key() {
        // Check if the key contains a unit (@)
        key.syntax()
            .children()
            .any(|n| n.kind() == SyntaxKind::UNIT)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(source: &str) -> String {
        format_source(source, FormatOptions::default())
    }

    #[test]
    fn test_simple_document() {
        let input = "name Alice\nage 30";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_preserves_comments() {
        let input = r#"// This is a comment
name Alice
/// Doc comment
age 30"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_inline_object() {
        let input = "point {x 1, y 2}";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_multiline_object() {
        let input = "server {\n  host localhost\n  port 8080\n}";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_nested_objects() {
        let input = "config {\n  server {\n    host localhost\n  }\n}";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_sequence() {
        let input = "items (a b c)";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_tagged_value() {
        let input = "type @string";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_schema_declaration() {
        let input = "@ schema.styx\n\nname test";
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_tag_with_nested_tag_payload() {
        // Note: `@string @Schema` parses as @string with @Schema as its payload
        // This is intentional grammar behavior - tags consume the next value as payload
        let input = "@seq(@string @Schema)";
        let output = format(input);
        // The formatter must preserve the space before the nested payload
        assert_eq!(output.trim(), "@seq(@string @Schema)");
    }

    #[test]
    fn test_sequence_with_multiple_scalars() {
        let input = "(a b c)";
        let output = format(input);
        assert_eq!(output.trim(), "(a b c)");
    }

    #[test]
    fn test_complex_schema() {
        let input = r#"meta {
  id https://example.com/schema
  version 1.0
}
schema {
  @ @object{
    name @string
    port @int
  }
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }
}
