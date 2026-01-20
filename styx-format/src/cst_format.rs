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

        // Track consecutive newlines to preserve blank lines from input
        let mut consecutive_newlines = 0;
        let mut entry_index = 0;
        let mut wrote_content = false;
        // Track if we just wrote a doc comment (entry should follow without blank line)
        let mut just_wrote_doc_comment = false;

        for el in node.children_with_tokens() {
            match el.kind() {
                SyntaxKind::NEWLINE => {
                    consecutive_newlines += 1;
                }
                SyntaxKind::WHITESPACE => {
                    // Ignore whitespace
                }
                SyntaxKind::LINE_COMMENT => {
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
                        just_wrote_doc_comment = false;
                    }
                }
                SyntaxKind::DOC_COMMENT => {
                    if let Some(token) = el.into_token() {
                        if wrote_content {
                            self.write_newline();

                            // Add extra blank line before doc comment:
                            // - if source had 2+ consecutive newlines (preserve existing)
                            // - if previous entry was schema declaration
                            // - if previous entry had doc comments
                            // - if previous entry is a block (issue #28)
                            let had_blank_line = consecutive_newlines >= 2;
                            let prev_was_schema =
                                entry_index == 1 && is_schema_declaration(&entries[0]);
                            let prev_had_doc = entry_index > 0
                                && entries[entry_index - 1].doc_comments().next().is_some();
                            let prev_is_block =
                                entry_index > 0 && is_block_entry(&entries[entry_index - 1]);

                            if had_blank_line || prev_was_schema || prev_had_doc || prev_is_block {
                                self.write_newline();
                            }
                        }
                        self.write(token.text());
                        wrote_content = true;
                        consecutive_newlines = 0;
                        just_wrote_doc_comment = true;
                    }
                }
                SyntaxKind::ENTRY => {
                    if let Some(entry_node) = el.into_node() {
                        let entry = &entries[entry_index];

                        if wrote_content {
                            self.write_newline();

                            // Add extra blank line before entry (only if not preceded by doc comment):
                            // - if source had 2+ consecutive newlines (preserve existing blank lines)
                            // - if previous entry was schema declaration (@ entry at root)
                            // - if previous entry had doc comments (and this entry has none)
                            // - if previous or current entry is a block (issue #28)
                            if !just_wrote_doc_comment {
                                let had_blank_line = consecutive_newlines >= 2;
                                let prev_was_schema =
                                    entry_index == 1 && is_schema_declaration(&entries[0]);
                                let prev_had_doc = entry_index > 0
                                    && entries[entry_index - 1].doc_comments().next().is_some();
                                let prev_is_block =
                                    entry_index > 0 && is_block_entry(&entries[entry_index - 1]);
                                let current_is_block = is_block_entry(entry);

                                if had_blank_line
                                    || prev_was_schema
                                    || prev_had_doc
                                    || prev_is_block
                                    || current_is_block
                                {
                                    self.write_newline();
                                }
                            }
                        }

                        self.format_node(&entry_node);
                        wrote_content = true;
                        consecutive_newlines = 0;
                        entry_index += 1;
                        just_wrote_doc_comment = false;
                    }
                }
                _ => {
                    // Skip other tokens
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

        // Check if any entry contains a block object - if so, parent should expand too
        let has_block_child = entries.iter().any(|e| contains_block_object(e.syntax()));

        // Determine if we need multiline format
        let is_multiline = matches!(separator, Separator::Newline | Separator::Mixed)
            || has_comments
            || has_block_child
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
        // But collapse trivial sequences: single simple element should be inline
        let should_collapse = !has_comments
            && entries.len() == 1
            && !contains_block_object(entries[0].syntax());

        // Special case: single entry that is a tag with block payload - format inline with paren
        // e.g., @optional(@object{...}) should format as (@object{\n...\n}) not (\n@object{...}\n)
        let single_tag_with_block = !has_comments
            && entries.len() == 1
            && is_tag_with_block_payload(entries[0].syntax());

        let is_multiline = !should_collapse && !single_tag_with_block && (seq.is_multiline() || has_comments || entries.is_empty());

        if single_tag_with_block {
            // Format the single entry inline with the paren - no newline after (
            if let Some(key) = entries[0]
                .syntax()
                .children()
                .find(|n| n.kind() == SyntaxKind::KEY)
            {
                for child in key.children() {
                    self.format_node(&child);
                }
            }
            self.write(")");
        } else if is_multiline {
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
}

/// Check if an entry is a "block" entry (contains a multiline object at top level).
/// Block entries need blank lines around them per issue #28.
fn is_block_entry(entry: &Entry) -> bool {
    if let Some(value) = entry.value() {
        // Check if the value directly contains a block-style object
        contains_block_object(value.syntax())
    } else {
        false
    }
}

/// Check if a sequence entry is a tag with a block object payload.
/// Used to format `(@object{...})` as `(@object{\n...\n})` not `(\n@object{...}\n)`.
fn is_tag_with_block_payload(entry_node: &SyntaxNode) -> bool {
    // Find the KEY child of the entry
    let key = match entry_node.children().find(|n| n.kind() == SyntaxKind::KEY) {
        Some(k) => k,
        None => return false,
    };

    // Look for a TAG child in the key
    for child in key.children() {
        if child.kind() == SyntaxKind::TAG {
            // Check if this tag has a TAG_PAYLOAD with a block object
            for tag_child in child.children() {
                if tag_child.kind() == SyntaxKind::TAG_PAYLOAD {
                    // Check if the payload contains a block object
                    return contains_block_object(&tag_child);
                }
            }
        }
    }

    false
}

/// Recursively check if a node contains a block-style object or doc comments.
/// Objects with doc comments also need to be block-formatted.
fn contains_block_object(node: &SyntaxNode) -> bool {
    // Check this node if it's an object
    if node.kind() == SyntaxKind::OBJECT {
        if let Some(obj) = Object::cast(node.clone()) {
            let sep = obj.separator();
            if matches!(sep, Separator::Newline | Separator::Mixed) {
                return true;
            }
            // Also check if the object contains doc comments
            if node
                .children_with_tokens()
                .any(|el| el.kind() == SyntaxKind::DOC_COMMENT)
            {
                return true;
            }
        }
    }

    // Recursively check all descendants
    for child in node.children() {
        if contains_block_object(&child) {
            return true;
        }
    }

    false
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
    fn test_parse_errors_detected() {
        // This input has a parse error - space-separated entries in inline object
        let input = "config {a 1 b 2}";
        let parsed = styx_cst::parse(input);
        assert!(
            !parsed.is_ok(),
            "Expected parse errors for '{}', but got none. Errors: {:?}",
            input,
            parsed.errors()
        );
        // Formatter should return original source for documents with errors
        let output = format(input);
        assert_eq!(output, input, "Formatter should return original source for documents with parse errors");
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

    #[test]
    fn test_schema_with_doc_comments_in_inline_object() {
        // Regression test: doc comments inside an inline object must be preserved
        // and the object must be expanded to multiline format
        let input = include_str!("fixtures/before-format.styx");
        let output = format(input);

        // The doc comments must be preserved
        assert!(
            output.contains("/// Features to use for clippy"),
            "Doc comment for clippy-features was lost!\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("/// Features to use for docs"),
            "Doc comment for docs-features was lost!\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("/// Features to use for doc tests"),
            "Doc comment for doc-test-features was lost!\nOutput:\n{}",
            output
        );

        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_dibs_extracted_schema() {
        // Complex schema extracted from dibs binary - tests deeply nested structures
        let input = include_str!("fixtures/dibs-extracted.styx");
        let output = format(input);
        insta::assert_snapshot!(output);
    }

    // ============================================================
    // SYSTEMATIC FORMATTER TESTS - 100 cases of increasing complexity
    // ============================================================

    // --- 1-10: Basic scalars and simple entries ---

    #[test]
    fn fmt_001_bare_scalar() {
        insta::assert_snapshot!(format("foo bar"));
    }

    #[test]
    fn fmt_002_quoted_scalar() {
        insta::assert_snapshot!(format(r#"foo "hello world""#));
    }

    #[test]
    fn fmt_003_raw_scalar() {
        insta::assert_snapshot!(format(r#"path r"/usr/bin""#));
    }

    #[test]
    fn fmt_004_multiple_entries() {
        insta::assert_snapshot!(format("foo bar\nbaz qux"));
    }

    #[test]
    fn fmt_005_unit_tag() {
        insta::assert_snapshot!(format("empty @"));
    }

    #[test]
    fn fmt_006_simple_tag() {
        insta::assert_snapshot!(format("type @string"));
    }

    #[test]
    fn fmt_007_tag_with_scalar_payload() {
        insta::assert_snapshot!(format(r#"default @default("hello")"#));
    }

    #[test]
    fn fmt_008_nested_tags() {
        insta::assert_snapshot!(format("type @optional(@string)"));
    }

    #[test]
    fn fmt_009_deeply_nested_tags() {
        insta::assert_snapshot!(format("type @seq(@optional(@string))"));
    }

    #[test]
    fn fmt_010_path_syntax() {
        insta::assert_snapshot!(format("limits cpu>500m memory>256Mi"));
    }

    // --- 11-20: Inline objects ---

    #[test]
    fn fmt_011_empty_inline_object() {
        insta::assert_snapshot!(format("config {}"));
    }

    #[test]
    fn fmt_012_single_entry_inline_object() {
        insta::assert_snapshot!(format("config {name foo}"));
    }

    #[test]
    fn fmt_013_multi_entry_inline_object() {
        insta::assert_snapshot!(format("point {x 1, y 2, z 3}"));
    }

    #[test]
    fn fmt_014_nested_inline_objects() {
        insta::assert_snapshot!(format("outer {inner {value 42}}"));
    }

    #[test]
    fn fmt_015_inline_object_with_tags() {
        insta::assert_snapshot!(format("schema {name @string, age @int}"));
    }

    #[test]
    fn fmt_016_tag_with_inline_object_payload() {
        insta::assert_snapshot!(format("type @object{name @string}"));
    }

    #[test]
    fn fmt_017_inline_object_no_commas() {
        // Parser might accept this - test what formatter does
        insta::assert_snapshot!(format("config {a 1 b 2}"));
    }

    #[test]
    fn fmt_018_inline_object_mixed_separators() {
        insta::assert_snapshot!(format("config {a 1, b 2 c 3}"));
    }

    #[test]
    fn fmt_019_deeply_nested_inline() {
        insta::assert_snapshot!(format("a {b {c {d {e 1}}}}"));
    }

    #[test]
    fn fmt_020_inline_with_unit_values() {
        insta::assert_snapshot!(format("flags {debug @, verbose @}"));
    }

    // --- 21-30: Block objects ---

    #[test]
    fn fmt_021_simple_block_object() {
        insta::assert_snapshot!(format("config {\n  name foo\n  value bar\n}"));
    }

    #[test]
    fn fmt_022_block_object_irregular_indent() {
        insta::assert_snapshot!(format("config {\n    name foo\n  value bar\n}"));
    }

    #[test]
    fn fmt_023_nested_block_objects() {
        insta::assert_snapshot!(format("outer {\n  inner {\n    value 42\n  }\n}"));
    }

    #[test]
    fn fmt_024_block_with_inline_child() {
        insta::assert_snapshot!(format("config {\n  point {x 1, y 2}\n  name foo\n}"));
    }

    #[test]
    fn fmt_025_inline_with_block_child() {
        // Inline object containing a block - should this expand?
        insta::assert_snapshot!(format("config {nested {\n  a 1\n}}"));
    }

    #[test]
    fn fmt_026_block_object_blank_lines() {
        insta::assert_snapshot!(format("config {\n  a 1\n\n  b 2\n}"));
    }

    #[test]
    fn fmt_027_block_object_multiple_blank_lines() {
        insta::assert_snapshot!(format("config {\n  a 1\n\n\n\n  b 2\n}"));
    }

    #[test]
    fn fmt_028_empty_block_object() {
        insta::assert_snapshot!(format("config {\n}"));
    }

    #[test]
    fn fmt_029_block_single_entry() {
        insta::assert_snapshot!(format("config {\n  only_one value\n}"));
    }

    #[test]
    fn fmt_030_mixed_block_inline_siblings() {
        insta::assert_snapshot!(format("a {x 1}\nb {\n  y 2\n}"));
    }

    // --- 31-40: Sequences ---

    #[test]
    fn fmt_031_empty_sequence() {
        insta::assert_snapshot!(format("items ()"));
    }

    #[test]
    fn fmt_032_single_item_sequence() {
        insta::assert_snapshot!(format("items (one)"));
    }

    #[test]
    fn fmt_033_multi_item_sequence() {
        insta::assert_snapshot!(format("items (a b c d e)"));
    }

    #[test]
    fn fmt_034_nested_sequences() {
        insta::assert_snapshot!(format("matrix ((1 2) (3 4))"));
    }

    #[test]
    fn fmt_035_sequence_of_objects() {
        insta::assert_snapshot!(format("points ({x 1} {x 2})"));
    }

    #[test]
    fn fmt_036_block_sequence() {
        insta::assert_snapshot!(format("items (\n  a\n  b\n  c\n)"));
    }

    #[test]
    fn fmt_037_sequence_with_trailing_newline() {
        insta::assert_snapshot!(format("items (a b c\n)"));
    }

    #[test]
    fn fmt_038_tag_with_sequence_payload() {
        insta::assert_snapshot!(format("type @seq(a b c)"));
    }

    #[test]
    fn fmt_039_tag_sequence_attached() {
        insta::assert_snapshot!(format("type @seq()"));
    }

    #[test]
    fn fmt_040_tag_sequence_detached() {
        insta::assert_snapshot!(format("type @seq ()"));
    }

    // --- 41-50: Comments ---

    #[test]
    fn fmt_041_line_comment_before_entry() {
        insta::assert_snapshot!(format("// comment\nfoo bar"));
    }

    #[test]
    fn fmt_042_doc_comment_before_entry() {
        insta::assert_snapshot!(format("/// doc comment\nfoo bar"));
    }

    #[test]
    fn fmt_043_comment_inside_block_object() {
        insta::assert_snapshot!(format("config {\n  // comment\n  foo bar\n}"));
    }

    #[test]
    fn fmt_044_doc_comment_inside_block_object() {
        insta::assert_snapshot!(format("config {\n  /// doc\n  foo bar\n}"));
    }

    #[test]
    fn fmt_045_comment_between_entries() {
        insta::assert_snapshot!(format("config {\n  a 1\n  // middle\n  b 2\n}"));
    }

    #[test]
    fn fmt_046_comment_at_end_of_object() {
        insta::assert_snapshot!(format("config {\n  a 1\n  // trailing\n}"));
    }

    #[test]
    fn fmt_047_inline_object_with_doc_comment() {
        // Doc comment forces expansion
        insta::assert_snapshot!(format("config {/// doc\na 1, b 2}"));
    }

    #[test]
    fn fmt_048_comment_in_sequence() {
        insta::assert_snapshot!(format("items (\n  // comment\n  a\n  b\n)"));
    }

    #[test]
    fn fmt_049_multiple_comments_grouped() {
        insta::assert_snapshot!(format("config {\n  // first\n  // second\n  a 1\n}"));
    }

    #[test]
    fn fmt_050_comments_with_blank_line_between() {
        insta::assert_snapshot!(format("config {\n  // group 1\n\n  // group 2\n  a 1\n}"));
    }

    // --- 51-60: The problematic cases from styx extract ---

    #[test]
    fn fmt_051_optional_with_newline_before_close() {
        // This is the minimal repro of the dibs issue
        insta::assert_snapshot!(format("foo @optional(@string\n)"));
    }

    #[test]
    fn fmt_052_seq_with_newline_before_close() {
        insta::assert_snapshot!(format("foo @seq(@string\n)"));
    }

    #[test]
    fn fmt_053_object_with_newline_before_close() {
        insta::assert_snapshot!(format("foo @object{a @string\n}"));
    }

    #[test]
    fn fmt_054_deeply_nested_with_weird_breaks() {
        insta::assert_snapshot!(format("foo @optional(@object{a @seq(@string\n)\n})"));
    }

    #[test]
    fn fmt_055_closing_delimiters_on_own_lines() {
        insta::assert_snapshot!(format("foo @a(@b{x 1\n}\n)"));
    }

    #[test]
    fn fmt_056_inline_entries_one_has_doc_comment() {
        // If ANY entry has doc comment, whole object should be block
        insta::assert_snapshot!(format("config {a @unit, /// doc\nb @unit, c @unit}"));
    }

    #[test]
    fn fmt_057_mixed_inline_block_with_doc() {
        insta::assert_snapshot!(format("schema {@ @object{a @unit, /// doc\nb @string}}"));
    }

    #[test]
    fn fmt_058_tag_map_with_doc_comments() {
        insta::assert_snapshot!(format("fields @map(@string@enum{/// variant a\na @unit, /// variant b\nb @unit})"));
    }

    #[test]
    fn fmt_059_nested_enums_with_docs() {
        insta::assert_snapshot!(format("type @enum{/// first\na @object{/// inner\nx @int}, b @unit}"));
    }

    #[test]
    fn fmt_060_the_dibs_pattern() {
        // Simplified version of dibs schema structure
        insta::assert_snapshot!(format(r#"schema {@ @object{decls @map(@string@enum{
    /// A query
    query @object{
        params @optional(@object{params @map(@string@enum{uuid @unit, /// doc
            optional @seq(@type{name T})
        })})
    }
})}}"#));
    }

    // --- 61-70: Top-level spacing (issue #28) ---

    #[test]
    fn fmt_061_two_inline_entries() {
        insta::assert_snapshot!(format("a 1\nb 2"));
    }

    #[test]
    fn fmt_062_two_block_entries() {
        insta::assert_snapshot!(format("a {\n  x 1\n}\nb {\n  y 2\n}"));
    }

    #[test]
    fn fmt_063_inline_then_block() {
        insta::assert_snapshot!(format("a 1\nb {\n  y 2\n}"));
    }

    #[test]
    fn fmt_064_block_then_inline() {
        insta::assert_snapshot!(format("a {\n  x 1\n}\nb 2"));
    }

    #[test]
    fn fmt_065_inline_inline_with_existing_blank() {
        insta::assert_snapshot!(format("a 1\n\nb 2"));
    }

    #[test]
    fn fmt_066_three_entries_mixed() {
        insta::assert_snapshot!(format("a 1\nb {\n  x 1\n}\nc 3"));
    }

    #[test]
    fn fmt_067_meta_then_schema_blocks() {
        insta::assert_snapshot!(format("meta {\n  id test\n}\nschema {\n  @ @string\n}"));
    }

    #[test]
    fn fmt_068_doc_comment_entry_spacing() {
        insta::assert_snapshot!(format("/// doc for a\na 1\n/// doc for b\nb 2"));
    }

    #[test]
    fn fmt_069_multiple_blocks_no_blanks() {
        insta::assert_snapshot!(format("a {\nx 1\n}\nb {\ny 2\n}\nc {\nz 3\n}"));
    }

    #[test]
    fn fmt_070_schema_declaration_spacing() {
        insta::assert_snapshot!(format("@schema foo.styx\nname test"));
    }

    // --- 71-80: Edge cases with tags ---

    #[test]
    fn fmt_071_tag_chain() {
        insta::assert_snapshot!(format("type @optional @string"));
    }

    #[test]
    fn fmt_072_tag_with_object_then_scalar() {
        insta::assert_snapshot!(format("type @default({x 1} @object{x @int})"));
    }

    #[test]
    fn fmt_073_multiple_tags_same_entry() {
        insta::assert_snapshot!(format("field @deprecated @optional(@string)"));
    }

    #[test]
    fn fmt_074_tag_payload_is_unit() {
        insta::assert_snapshot!(format("empty @some(@)"));
    }

    #[test]
    fn fmt_075_tag_with_heredoc() {
        insta::assert_snapshot!(format("sql @raw(<<EOF\nSELECT *\nEOF)"));
    }

    #[test]
    fn fmt_076_tag_payload_sequence_of_tags() {
        insta::assert_snapshot!(format("types @union(@string @int @bool)"));
    }

    #[test]
    fn fmt_077_tag_map_compact() {
        insta::assert_snapshot!(format("fields @map(@string@int)"));
    }

    #[test]
    fn fmt_078_tag_map_with_complex_value() {
        insta::assert_snapshot!(format("fields @map(@string@object{x @int, y @int})"));
    }

    #[test]
    fn fmt_079_tag_type_reference() {
        insta::assert_snapshot!(format("field @type{name MyType}"));
    }

    #[test]
    fn fmt_080_tag_default_with_at() {
        insta::assert_snapshot!(format("opt @default(@ @optional(@string))"));
    }

    // --- 81-90: Heredocs ---

    #[test]
    fn fmt_081_simple_heredoc() {
        insta::assert_snapshot!(format("text <<EOF\nhello\nworld\nEOF"));
    }

    #[test]
    fn fmt_082_heredoc_in_object() {
        insta::assert_snapshot!(format("config {\n  sql <<SQL\nSELECT *\nSQL\n}"));
    }

    #[test]
    fn fmt_083_heredoc_indented_content() {
        insta::assert_snapshot!(format("code <<END\n  indented\n    more\nEND"));
    }

    #[test]
    fn fmt_084_multiple_heredocs() {
        insta::assert_snapshot!(format("a <<A\nfirst\nA\nb <<B\nsecond\nB"));
    }

    #[test]
    fn fmt_085_heredoc_empty() {
        insta::assert_snapshot!(format("empty <<EOF\nEOF"));
    }

    // --- 86-90: Quoted strings edge cases ---

    #[test]
    fn fmt_086_quoted_with_escapes() {
        insta::assert_snapshot!(format(r#"msg "hello\nworld\ttab""#));
    }

    #[test]
    fn fmt_087_quoted_with_quotes() {
        insta::assert_snapshot!(format(r#"msg "say \"hello\"""#));
    }

    #[test]
    fn fmt_088_raw_string_with_hashes() {
        insta::assert_snapshot!(format(r##"pattern r#"foo"bar"#"##));
    }

    #[test]
    fn fmt_089_quoted_empty() {
        insta::assert_snapshot!(format(r#"empty """#));
    }

    #[test]
    fn fmt_090_mixed_scalar_types() {
        insta::assert_snapshot!(format(r#"config {bare word, quoted "str", raw r"path"}"#));
    }

    // --- 91-100: Complex real-world-like structures ---

    #[test]
    fn fmt_091_schema_with_meta() {
        insta::assert_snapshot!(format(r#"meta {id "app:config@1", cli myapp}
schema {@ @object{
    name @string
    port @default(8080 @int)
}}"#));
    }

    #[test]
    fn fmt_092_enum_with_object_variants() {
        insta::assert_snapshot!(format(r#"type @enum{
    /// A simple variant
    simple @unit
    /// Complex variant
    complex @object{x @int, y @int}
}"#));
    }

    #[test]
    fn fmt_093_nested_optionals() {
        insta::assert_snapshot!(format("type @optional(@optional(@optional(@string)))"));
    }

    #[test]
    fn fmt_094_map_of_maps() {
        insta::assert_snapshot!(format("data @map(@string@map(@string@int))"));
    }

    #[test]
    fn fmt_095_sequence_of_enums() {
        insta::assert_snapshot!(format("items @seq(@enum{a @unit, b @unit, c @unit})"));
    }

    #[test]
    fn fmt_096_all_builtin_types() {
        insta::assert_snapshot!(format("types {s @string, i @int, b @bool, f @float, u @unit}"));
    }

    #[test]
    fn fmt_097_deep_nesting_mixed() {
        insta::assert_snapshot!(format("a @object{b @seq(@enum{c @object{d @optional(@map(@string@int))}})}"));
    }

    #[test]
    fn fmt_098_realistic_config_schema() {
        insta::assert_snapshot!(format(r#"meta {id "crate:myapp@1", cli myapp, description "My application config"}
schema {@ @object{
    /// Server configuration
    server @object{
        /// Hostname to bind
        host @default("localhost" @string)
        /// Port number
        port @default(8080 @int)
    }
    /// Database settings
    database @optional(@object{
        url @string
        pool_size @default(10 @int)
    })
}}"#));
    }

    #[test]
    fn fmt_099_attributes_syntax() {
        insta::assert_snapshot!(format("resource limits>cpu>500m limits>memory>256Mi"));
    }

    #[test]
    fn fmt_100_everything_combined() {
        insta::assert_snapshot!(format(r#"// Top level comment
meta {id "test@1"}

/// Schema documentation
schema {@ @object{
    /// A string field
    name @string

    /// An enum with variants
    kind @enum{
        /// Simple kind
        simple @unit
        /// Complex kind
        complex @object{
            /// Nested value
            value @optional(@int)
        }
    }

    /// A sequence
    items @seq(@string)

    /// A map
    data @map(@string@object{x @int, y @int})
}}"#));
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
