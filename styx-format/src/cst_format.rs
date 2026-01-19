//! CST-based formatter for Styx documents.
//!
//! This formatter works directly with the lossless CST (Concrete Syntax Tree),
//! preserving all comments and producing properly indented output.

use styx_cst::ast::{AstNode, Document, Entry, Object, Separator, Sequence};
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
            // Nodes
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
            SyntaxKind::ATTRIBUTES => self.format_attributes(node),
            SyntaxKind::ATTRIBUTE => self.format_attribute(node),

            // Tokens - should not appear as nodes, but handle gracefully
            SyntaxKind::L_BRACE
            | SyntaxKind::R_BRACE
            | SyntaxKind::L_PAREN
            | SyntaxKind::R_PAREN
            | SyntaxKind::COMMA
            | SyntaxKind::GT
            | SyntaxKind::AT
            | SyntaxKind::BARE_SCALAR
            | SyntaxKind::QUOTED_SCALAR
            | SyntaxKind::RAW_SCALAR
            | SyntaxKind::HEREDOC_START
            | SyntaxKind::HEREDOC_CONTENT
            | SyntaxKind::HEREDOC_END
            | SyntaxKind::LINE_COMMENT
            | SyntaxKind::DOC_COMMENT
            | SyntaxKind::WHITESPACE
            | SyntaxKind::NEWLINE
            | SyntaxKind::EOF
            | SyntaxKind::ERROR
            | SyntaxKind::__LAST_TOKEN => {
                // Tokens shouldn't be passed to format_node (they're not nodes)
                // but if they are, ignore them - they're handled by their parent
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

        // Check if the object contains any comments (even if no entries)
        let has_comments = node.children_with_tokens().any(|el| {
            matches!(
                el.kind(),
                SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT
            )
        });

        // Empty object with no comments
        if entries.is_empty() && !has_comments {
            self.write("}");
            return;
        }

        // Determine if we need multiline format
        let is_multiline = matches!(separator, Separator::Newline | Separator::Mixed)
            || has_comments
            || entries.is_empty(); // Empty with comments needs multiline

        if is_multiline {
            // Multiline format - preserve comments as children of the object
            self.write_newline();
            self.indent_level += 1;

            // Iterate through all children to preserve comments in order
            // Track consecutive newlines to preserve blank lines
            let mut wrote_content = false;
            let mut consecutive_newlines = 0;
            for el in node.children_with_tokens() {
                match el.kind() {
                    SyntaxKind::NEWLINE => {
                        consecutive_newlines += 1;
                    }
                    SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT => {
                        if let Some(token) = el.into_token() {
                            if wrote_content {
                                self.write_newline();
                                // 2+ consecutive newlines means there was a blank line
                                if consecutive_newlines >= 2 {
                                    self.write_newline();
                                }
                            }
                            self.write(token.text());
                            wrote_content = true;
                            consecutive_newlines = 0;
                        }
                    }
                    SyntaxKind::ENTRY => {
                        if let Some(entry_node) = el.into_node() {
                            if wrote_content {
                                self.write_newline();
                                // 2+ consecutive newlines means there was a blank line
                                if consecutive_newlines >= 2 {
                                    self.write_newline();
                                }
                            }
                            self.format_node(&entry_node);
                            wrote_content = true;
                            consecutive_newlines = 0;
                        }
                    }
                    // Skip whitespace, braces - we handle formatting ourselves
                    // Whitespace doesn't reset newline count (it comes between newlines)
                    SyntaxKind::WHITESPACE | SyntaxKind::L_BRACE | SyntaxKind::R_BRACE => {}
                    _ => {
                        consecutive_newlines = 0;
                    }
                }
            }

            self.write_newline();
            self.indent_level -= 1;
            self.write("}");
        } else {
            // Inline format (comma-separated, no comments)
            for (i, entry) in entries.iter().enumerate() {
                self.format_node(entry.syntax());

                if i < entries.len() - 1 {
                    self.write(", ");
                }
            }
            self.write("}");
        }
    }

    fn format_sequence(&mut self, node: &SyntaxNode) {
        let seq = Sequence::cast(node.clone()).unwrap();
        let entries: Vec<_> = seq.entries().collect();

        self.write("(");

        // Check if the sequence contains any comments (even if no entries)
        let has_comments = node.children_with_tokens().any(|el| {
            matches!(
                el.kind(),
                SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT
            )
        });

        // Empty sequence with no comments
        if entries.is_empty() && !has_comments {
            self.write(")");
            return;
        }

        // Determine if we need multiline format
        let is_multiline = seq.is_multiline() || has_comments || entries.is_empty();

        if is_multiline {
            // Multiline format - preserve comments as children of the sequence
            self.write_newline();
            self.indent_level += 1;

            // Iterate through all children to preserve comments in order
            let mut wrote_content = false;
            let mut consecutive_newlines = 0;
            for el in node.children_with_tokens() {
                match el.kind() {
                    SyntaxKind::NEWLINE => {
                        consecutive_newlines += 1;
                    }
                    SyntaxKind::LINE_COMMENT | SyntaxKind::DOC_COMMENT => {
                        if let Some(token) = el.into_token() {
                            if wrote_content {
                                self.write_newline();
                                // 2+ consecutive newlines means there was a blank line
                                if consecutive_newlines >= 2 {
                                    self.write_newline();
                                }
                            }
                            self.write(token.text());
                            wrote_content = true;
                            consecutive_newlines = 0;
                        }
                    }
                    SyntaxKind::ENTRY => {
                        if let Some(entry_node) = el.into_node() {
                            if wrote_content {
                                self.write_newline();
                                // 2+ consecutive newlines means there was a blank line
                                if consecutive_newlines >= 2 {
                                    self.write_newline();
                                }
                            }
                            // Format the entry's value (sequence entries have implicit unit keys)
                            if let Some(key) =
                                entry_node.children().find(|n| n.kind() == SyntaxKind::KEY)
                            {
                                for child in key.children() {
                                    self.format_node(&child);
                                }
                            }
                            wrote_content = true;
                            consecutive_newlines = 0;
                        }
                    }
                    // Skip whitespace, parens - we handle formatting ourselves
                    SyntaxKind::WHITESPACE | SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
                    _ => {
                        consecutive_newlines = 0;
                    }
                }
            }

            self.write_newline();
            self.indent_level -= 1;
            self.write(")");
        } else {
            // Inline format - single line with spaces (no comments possible here)
            for (i, entry) in entries.iter().enumerate() {
                // Get the actual value from the entry's key
                if let Some(key) = entry
                    .syntax()
                    .children()
                    .find(|n| n.kind() == SyntaxKind::KEY)
                {
                    for child in key.children() {
                        self.format_node(&child);
                    }
                }

                if i < entries.len() - 1 {
                    self.write(" ");
                }
            }
            self.write(")");
        }
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

        // Per grammar: Tag ::= '@' TagName TagPayload?
        // TagPayload must be immediately attached (no whitespace allowed)
        // TagPayload ::= Object | Sequence | QuotedScalar | RawScalar | HeredocScalar | '@'
        for el in node.children_with_tokens() {
            if let rowan::NodeOrToken::Node(child) = el {
                match child.kind() {
                    SyntaxKind::TAG_NAME => self.format_tag_name(&child),
                    SyntaxKind::TAG_PAYLOAD => self.format_tag_payload(&child),
                    _ => {}
                }
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

    fn format_attributes(&mut self, node: &SyntaxNode) {
        let attrs: Vec<_> = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::ATTRIBUTE)
            .collect();

        for (i, attr) in attrs.iter().enumerate() {
            self.format_attribute(attr);
            if i < attrs.len() - 1 {
                self.write(" ");
            }
        }
    }

    fn format_attribute(&mut self, node: &SyntaxNode) {
        // Attribute structure: BARE_SCALAR ">" SCALAR
        for el in node.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::BARE_SCALAR => self.write(token.text()),
                    SyntaxKind::GT => self.write(">"),
                    _ => {}
                },
                rowan::NodeOrToken::Node(child) => {
                    self.format_node(&child);
                }
            }
        }
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

/// Check if an entry is a schema declaration (@schema tag as key).
fn is_schema_declaration(entry: &Entry) -> bool {
    if let Some(key) = entry.key() {
        // Check if the key contains a @schema tag
        key.syntax().children().any(|n| {
            if n.kind() == SyntaxKind::TAG {
                // Look for TAG_NAME child with text "schema"
                n.children().any(|child| {
                    child.kind() == SyntaxKind::TAG_NAME && child.to_string() == "schema"
                })
            } else {
                false
            }
        })
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
        let input = "@schema schema.styx\n\nname test";
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

    #[test]
    fn test_path_syntax_in_object() {
        let input = r#"resources {
    limits cpu>500m memory>256Mi
    requests cpu>100m memory>128Mi
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_syntax_error_space_after_gt() {
        // Space after > is a syntax error - should return original
        let input = "limits cpu> 500m";
        let parsed = styx_cst::parse(input);
        assert!(!parsed.is_ok(), "should have parse error");
        let output = format(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_syntax_error_space_before_gt() {
        // Space before > is a syntax error - should return original
        let input = "limits cpu >500m";
        let parsed = styx_cst::parse(input);
        assert!(!parsed.is_ok(), "should have parse error");
        let output = format(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_tag_with_separate_sequence() {
        // @a () has space between tag and sequence - must be preserved
        // (whitespace affects parsing semantics for tag payloads)
        let input = "@a ()";
        let output = format(input);
        assert_eq!(output.trim(), "@a ()");
    }

    #[test]
    fn test_tag_with_attached_sequence() {
        // @a() has no space - compact form must stay compact
        let input = "@a()";
        let output = format(input);
        assert_eq!(output.trim(), "@a()");
    }

    // === Sequence comment tests ===

    #[test]
    fn test_multiline_sequence_preserves_structure() {
        let input = r#"items (
  a
  b
  c
)"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_sequence_with_trailing_comment() {
        let input = r#"extends (
  "@eslint/js:recommended"
  typescript-eslint:strictTypeChecked
  // don't fold
)"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_sequence_with_inline_comments() {
        let input = r#"items (
  // first item
  a
  // second item
  b
)"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_sequence_comment_idempotent() {
        let input = r#"extends (
  "@eslint/js:recommended"
  typescript-eslint:strictTypeChecked
  // don't fold
)"#;
        let once = format(input);
        let twice = format(&once);
        assert_eq!(once, twice, "formatting should be idempotent");
    }

    #[test]
    fn test_inline_sequence_stays_inline() {
        // No newlines or comments = stays inline
        let input = "items (a b c)";
        let output = format(input);
        assert_eq!(output.trim(), "items (a b c)");
    }

    #[test]
    fn test_sequence_with_doc_comment() {
        let input = r#"items (
  /// Documentation for first
  a
  b
)"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_nested_multiline_sequence() {
        let input = r#"outer (
  (a b)
  // between
  (c d)
)"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_sequence_in_object_with_comment() {
        let input = r#"config {
  items (
    a
    // comment
    b
  )
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_object_with_only_comments() {
        // Regression test: objects containing only comments should preserve them
        let input = r#"pre-commit {
    // generate-readmes false
    // rustfmt false
    // cargo-lock false
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_object_comments_with_blank_line() {
        // Regression test: blank lines between comment groups should be preserved
        let input = r#"config {
    // first group
    // still first group

    // second group after blank line
    // still second group
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_object_mixed_entries_and_comments() {
        // Test mixing actual entries with commented-out entries
        let input = r#"settings {
    enabled true
    // disabled-option false
    name "test"
    // another-disabled option
}"#;
        let output = format(input);
        insta::assert_snapshot!(output);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a valid bare scalar (no special chars)
    fn bare_scalar() -> impl Strategy<Value = String> {
        // Start with letter, then alphanumeric + some allowed chars
        prop::string::string_regex("[a-zA-Z][a-zA-Z0-9_-]{0,10}")
            .unwrap()
            .prop_filter("non-empty", |s| !s.is_empty())
    }

    /// Generate a quoted scalar with potential escape sequences
    fn quoted_scalar() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple quoted string
            prop::string::string_regex(r#"[a-zA-Z0-9 _-]{0,20}"#)
                .unwrap()
                .prop_map(|s| format!("\"{}\"", s)),
            // With common escapes
            prop::string::string_regex(r#"[a-zA-Z0-9 ]{0,10}"#)
                .unwrap()
                .prop_map(|s| format!("\"hello\\n{}\\t\"", s)),
        ]
    }

    /// Generate a raw scalar (r"..." or r#"..."#)
    fn raw_scalar() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple raw string
            prop::string::string_regex(r#"[a-zA-Z0-9/_\\.-]{0,15}"#)
                .unwrap()
                .prop_map(|s| format!("r\"{}\"", s)),
            // Raw string with # delimiters (can contain quotes)
            prop::string::string_regex(r#"[a-zA-Z0-9 "/_\\.-]{0,15}"#)
                .unwrap()
                .prop_map(|s| format!("r#\"{}\"#", s)),
        ]
    }

    /// Generate a scalar (bare, quoted, or raw)
    fn scalar() -> impl Strategy<Value = String> {
        prop_oneof![
            4 => bare_scalar(),
            3 => quoted_scalar(),
            1 => raw_scalar(),
        ]
    }

    /// Generate a tag name
    fn tag_name() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z][a-zA-Z0-9_-]{0,8}")
            .unwrap()
            .prop_filter("non-empty", |s| !s.is_empty())
    }

    /// Generate a tag (@name or @name with payload)
    /// Per grammar: TagPayload must be immediately attached (no whitespace)
    /// TagPayload ::= Object | Sequence | QuotedScalar | RawScalar | HeredocScalar | '@'
    /// Note: BareScalar is NOT a valid TagPayload
    fn tag() -> impl Strategy<Value = String> {
        prop_oneof![
            // Unit tag (just @)
            Just("@".to_string()),
            // Simple tag (no payload)
            tag_name().prop_map(|n| format!("@{n}")),
            // Tag with sequence payload (must be attached, no space)
            (tag_name(), flat_sequence()).prop_map(|(n, s)| format!("@{n}{s}")),
            // Tag with inline object payload (must be attached, no space)
            (tag_name(), inline_object()).prop_map(|(n, o)| format!("@{n}{o}")),
            // Tag with quoted scalar payload (must be attached, no space)
            (tag_name(), quoted_scalar()).prop_map(|(n, q)| format!("@{n}{q}")),
            // Tag with unit payload
            (tag_name()).prop_map(|n| format!("@{n} @")),
        ]
    }

    /// Generate an attribute (key>value)
    fn attribute() -> impl Strategy<Value = String> {
        (bare_scalar(), scalar()).prop_map(|(k, v)| format!("{k}>{v}"))
    }

    /// Generate a flat sequence of scalars (no nesting)
    fn flat_sequence() -> impl Strategy<Value = String> {
        prop::collection::vec(scalar(), 0..5).prop_map(|items| {
            if items.is_empty() {
                "()".to_string()
            } else {
                format!("({})", items.join(" "))
            }
        })
    }

    /// Generate a nested sequence like ((a b) (c d))
    fn nested_sequence() -> impl Strategy<Value = String> {
        prop::collection::vec(flat_sequence(), 1..4)
            .prop_map(|seqs| format!("({})", seqs.join(" ")))
    }

    /// Generate a sequence (flat or nested)
    fn sequence() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => flat_sequence(),
            1 => nested_sequence(),
        ]
    }

    /// Generate an inline object {key value, ...}
    fn inline_object() -> impl Strategy<Value = String> {
        prop::collection::vec((bare_scalar(), scalar()), 0..4).prop_map(|entries| {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let inner: Vec<String> = entries
                    .into_iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        })
    }

    /// Generate a multiline object
    fn multiline_object() -> impl Strategy<Value = String> {
        prop::collection::vec((bare_scalar(), scalar()), 1..4).prop_map(|entries| {
            let inner: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("  {k} {v}"))
                .collect();
            format!("{{\n{}\n}}", inner.join("\n"))
        })
    }

    /// Generate a line comment
    fn line_comment() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9 _-]{0,30}")
            .unwrap()
            .prop_map(|s| format!("// {}", s.trim()))
    }

    /// Generate a doc comment
    fn doc_comment() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9 _-]{0,30}")
            .unwrap()
            .prop_map(|s| format!("/// {}", s.trim()))
    }

    /// Generate a heredoc
    fn heredoc() -> impl Strategy<Value = String> {
        let delimiters = prop_oneof![
            Just("EOF".to_string()),
            Just("END".to_string()),
            Just("TEXT".to_string()),
            Just("CODE".to_string()),
        ];
        let content = prop::string::string_regex("[a-zA-Z0-9 \n_.-]{0,50}").unwrap();
        let lang_hint = prop_oneof![
            Just("".to_string()),
            Just(",txt".to_string()),
            Just(",rust".to_string()),
        ];
        (delimiters, content, lang_hint)
            .prop_map(|(delim, content, hint)| format!("<<{delim}{hint}\n{content}\n{delim}"))
    }

    /// Generate a simple value (scalar, sequence, or attributes)
    fn simple_value() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => scalar(),
            2 => sequence(),
            2 => tag(),
            1 => inline_object(),
            1 => multiline_object(),
            1 => heredoc(),
            // Multiple attributes (path syntax)
            1 => prop::collection::vec(attribute(), 1..4).prop_map(|attrs| attrs.join(" ")),
        ]
    }

    /// Generate a simple entry (key value)
    fn entry() -> impl Strategy<Value = String> {
        prop_oneof![
            // Regular entry
            (bare_scalar(), simple_value()).prop_map(|(k, v)| format!("{k} {v}")),
            // Tag as key
            (tag(), simple_value()).prop_map(|(t, v)| format!("{t} {v}")),
        ]
    }

    /// Generate an entry optionally preceded by a comment
    fn commented_entry() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => entry(),
            1 => (doc_comment(), entry()).prop_map(|(c, e)| format!("{c}\n{e}")),
            1 => (line_comment(), entry()).prop_map(|(c, e)| format!("{c}\n{e}")),
        ]
    }

    /// Generate a simple document (multiple entries)
    fn document() -> impl Strategy<Value = String> {
        prop::collection::vec(commented_entry(), 1..5).prop_map(|entries| entries.join("\n"))
    }

    /// Generate a deeply nested object (recursive)
    fn deep_object(depth: usize) -> BoxedStrategy<String> {
        if depth == 0 {
            scalar().boxed()
        } else {
            prop_oneof![
                // Scalar leaf
                2 => scalar(),
                // Nested object
                1 => prop::collection::vec(
                    (bare_scalar(), deep_object(depth - 1)),
                    1..3
                ).prop_map(|entries| {
                    let inner: Vec<String> = entries.into_iter()
                        .map(|(k, v)| format!("  {k} {v}"))
                        .collect();
                    format!("{{\n{}\n}}", inner.join("\n"))
                }),
            ]
            .boxed()
        }
    }

    /// Generate a sequence containing tags
    fn sequence_of_tags() -> impl Strategy<Value = String> {
        prop::collection::vec(tag(), 1..5).prop_map(|tags| format!("({})", tags.join(" ")))
    }

    /// Generate an object with sequence values
    fn object_with_sequences() -> impl Strategy<Value = String> {
        prop::collection::vec((bare_scalar(), flat_sequence()), 1..4).prop_map(|entries| {
            let inner: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("  {k} {v}"))
                .collect();
            format!("{{\n{}\n}}", inner.join("\n"))
        })
    }

    /// Strip spans from a value tree for comparison (spans change after formatting)
    fn strip_spans(value: &mut styx_tree::Value) {
        value.span = None;
        if let Some(ref mut tag) = value.tag {
            tag.span = None;
        }
        if let Some(ref mut payload) = value.payload {
            match payload {
                styx_tree::Payload::Scalar(s) => s.span = None,
                styx_tree::Payload::Sequence(seq) => {
                    seq.span = None;
                    for item in &mut seq.items {
                        strip_spans(item);
                    }
                }
                styx_tree::Payload::Object(obj) => {
                    obj.span = None;
                    for entry in &mut obj.entries {
                        strip_spans(&mut entry.key);
                        strip_spans(&mut entry.value);
                    }
                }
            }
        }
    }

    /// Parse source into a comparable tree (spans stripped)
    fn parse_to_tree(source: &str) -> Option<styx_tree::Value> {
        let mut value = styx_tree::parse(source).ok()?;
        strip_spans(&mut value);
        Some(value)
    }

    proptest! {
        /// Formatting must preserve document semantics
        #[test]
        fn format_preserves_semantics(input in document()) {
            let tree1 = parse_to_tree(&input);

            // Skip if original doesn't parse (shouldn't happen with our generator)
            if tree1.is_none() {
                return Ok(());
            }
            let tree1 = tree1.unwrap();

            let formatted = format_source(&input, FormatOptions::default());
            let tree2 = parse_to_tree(&formatted);

            prop_assert!(
                tree2.is_some(),
                "Formatted output should parse. Input:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
            let tree2 = tree2.unwrap();

            prop_assert_eq!(
                tree1,
                tree2,
                "Formatting changed semantics!\nInput:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
        }

        /// Formatting should be idempotent
        #[test]
        fn format_is_idempotent(input in document()) {
            let once = format_source(&input, FormatOptions::default());
            let twice = format_source(&once, FormatOptions::default());

            prop_assert_eq!(
                &once,
                &twice,
                "Formatting is not idempotent!\nInput:\n{}\nOnce:\n{}\nTwice:\n{}",
                input,
                &once,
                &twice
            );
        }

        /// Deeply nested objects should format correctly
        #[test]
        fn format_deep_objects(key in bare_scalar(), value in deep_object(4)) {
            let input = format!("{key} {value}");
            let tree1 = parse_to_tree(&input);

            if tree1.is_none() {
                return Ok(());
            }
            let tree1 = tree1.unwrap();

            let formatted = format_source(&input, FormatOptions::default());
            let tree2 = parse_to_tree(&formatted);

            prop_assert!(
                tree2.is_some(),
                "Deep object should parse after formatting. Input:\n{}\nFormatted:\n{}",
                input,
                formatted
            );

            prop_assert_eq!(
                tree1,
                tree2.unwrap(),
                "Deep object semantics changed!\nInput:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
        }

        /// Sequences of tags should format correctly
        #[test]
        fn format_sequence_of_tags(key in bare_scalar(), seq in sequence_of_tags()) {
            let input = format!("{key} {seq}");
            let tree1 = parse_to_tree(&input);

            if tree1.is_none() {
                return Ok(());
            }
            let tree1 = tree1.unwrap();

            let formatted = format_source(&input, FormatOptions::default());
            let tree2 = parse_to_tree(&formatted);

            prop_assert!(
                tree2.is_some(),
                "Tag sequence should parse. Input:\n{}\nFormatted:\n{}",
                input,
                formatted
            );

            prop_assert_eq!(
                tree1,
                tree2.unwrap(),
                "Tag sequence semantics changed!\nInput:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
        }

        /// Objects containing sequences should format correctly
        #[test]
        fn format_objects_with_sequences(key in bare_scalar(), obj in object_with_sequences()) {
            let input = format!("{key} {obj}");
            let tree1 = parse_to_tree(&input);

            if tree1.is_none() {
                return Ok(());
            }
            let tree1 = tree1.unwrap();

            let formatted = format_source(&input, FormatOptions::default());
            let tree2 = parse_to_tree(&formatted);

            prop_assert!(
                tree2.is_some(),
                "Object with sequences should parse. Input:\n{}\nFormatted:\n{}",
                input,
                formatted
            );

            prop_assert_eq!(
                tree1,
                tree2.unwrap(),
                "Object with sequences semantics changed!\nInput:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
        }

        /// Formatting must preserve all comments (line and doc comments)
        #[test]
        fn format_preserves_comments(input in document_with_comments()) {
            let original_comments = extract_comments(&input);

            // Skip if no comments (not interesting for this test)
            if original_comments.is_empty() {
                return Ok(());
            }

            let formatted = format_source(&input, FormatOptions::default());
            let formatted_comments = extract_comments(&formatted);

            prop_assert_eq!(
                original_comments.len(),
                formatted_comments.len(),
                "Comment count changed!\nInput ({} comments):\n{}\nFormatted ({} comments):\n{}\nOriginal comments: {:?}\nFormatted comments: {:?}",
                original_comments.len(),
                input,
                formatted_comments.len(),
                formatted,
                original_comments,
                formatted_comments
            );

            // Check that each comment text is preserved (order may change slightly due to formatting)
            for comment in &original_comments {
                prop_assert!(
                    formatted_comments.contains(comment),
                    "Comment lost during formatting!\nMissing: {:?}\nInput:\n{}\nFormatted:\n{}\nOriginal comments: {:?}\nFormatted comments: {:?}",
                    comment,
                    input,
                    formatted,
                    original_comments,
                    formatted_comments
                );
            }
        }

        /// Objects with only comments should preserve them
        #[test]
        fn format_preserves_comments_in_empty_objects(
            key in bare_scalar(),
            comments in prop::collection::vec(line_comment(), 1..5)
        ) {
            let inner = comments.iter()
                .map(|c| format!("    {c}"))
                .collect::<Vec<_>>()
                .join("\n");
            let input = format!("{key} {{\n{inner}\n}}");

            let original_comments = extract_comments(&input);
            let formatted = format_source(&input, FormatOptions::default());
            let formatted_comments = extract_comments(&formatted);

            prop_assert_eq!(
                original_comments.len(),
                formatted_comments.len(),
                "Comments in empty object lost!\nInput:\n{}\nFormatted:\n{}",
                input,
                formatted
            );
        }

        /// Objects with mixed entries and comments should preserve all comments
        #[test]
        fn format_preserves_comments_mixed_with_entries(
            key in bare_scalar(),
            items in prop::collection::vec(
                prop_oneof![
                    // Entry
                    (bare_scalar(), scalar()).prop_map(|(k, v)| format!("{k} {v}")),
                    // Comment
                    line_comment(),
                ],
                2..6
            )
        ) {
            let inner = items.iter()
                .map(|item| format!("    {item}"))
                .collect::<Vec<_>>()
                .join("\n");
            let input = format!("{key} {{\n{inner}\n}}");

            let original_comments = extract_comments(&input);
            let formatted = format_source(&input, FormatOptions::default());
            let formatted_comments = extract_comments(&formatted);

            prop_assert_eq!(
                original_comments.len(),
                formatted_comments.len(),
                "Comments mixed with entries lost!\nInput:\n{}\nFormatted:\n{}\nOriginal: {:?}\nFormatted: {:?}",
                input,
                formatted,
                original_comments,
                formatted_comments
            );
        }

        /// Sequences with comments should preserve them
        #[test]
        fn format_preserves_comments_in_sequences(
            key in bare_scalar(),
            items in prop::collection::vec(
                prop_oneof![
                    // Scalar item
                    2 => scalar(),
                    // Comment
                    1 => line_comment(),
                ],
                2..6
            )
        ) {
            // Only create multiline sequence if we have comments
            let has_comment = items.iter().any(|i| i.starts_with("//"));
            if !has_comment {
                return Ok(());
            }

            let inner = items.iter()
                .map(|item| format!("    {item}"))
                .collect::<Vec<_>>()
                .join("\n");
            let input = format!("{key} (\n{inner}\n)");

            let original_comments = extract_comments(&input);
            let formatted = format_source(&input, FormatOptions::default());
            let formatted_comments = extract_comments(&formatted);

            prop_assert_eq!(
                original_comments.len(),
                formatted_comments.len(),
                "Comments in sequence lost!\nInput:\n{}\nFormatted:\n{}\nOriginal: {:?}\nFormatted: {:?}",
                input,
                formatted,
                original_comments,
                formatted_comments
            );
        }
    }

    /// Generate a document that definitely contains comments in various positions
    fn document_with_comments() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                // Regular entry
                2 => entry(),
                // Entry preceded by comment
                2 => (line_comment(), entry()).prop_map(|(c, e)| format!("{c}\n{e}")),
                // Entry preceded by doc comment
                1 => (doc_comment(), entry()).prop_map(|(c, e)| format!("{c}\n{e}")),
                // Object with comments inside
                1 => object_with_internal_comments(),
            ],
            1..5,
        )
        .prop_map(|entries| entries.join("\n"))
    }

    /// Generate an object that has comments inside it
    fn object_with_internal_comments() -> impl Strategy<Value = String> {
        (
            bare_scalar(),
            prop::collection::vec(
                prop_oneof![
                    // Entry
                    2 => (bare_scalar(), scalar()).prop_map(|(k, v)| format!("{k} {v}")),
                    // Comment
                    1 => line_comment(),
                ],
                1..5,
            ),
        )
            .prop_map(|(key, items)| {
                let inner = items
                    .iter()
                    .map(|item| format!("    {item}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{key} {{\n{inner}\n}}")
            })
    }

    /// Extract all comments from source text (both line and doc comments)
    fn extract_comments(source: &str) -> Vec<String> {
        let mut comments = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("///") || trimmed.starts_with("//") {
                comments.push(trimmed.to_string());
            }
        }
        comments
    }
}
