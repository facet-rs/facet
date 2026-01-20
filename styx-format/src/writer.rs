//! Low-level Styx output writer.
//!
//! Provides a structured way to build Styx output with proper formatting,
//! independent of any serialization framework.

use crate::options::{ForceStyle, FormatOptions};
use crate::scalar::{can_be_bare, count_escapes, count_newlines, escape_quoted};

/// Context for tracking serialization state.
#[derive(Debug, Clone)]
pub enum Context {
    /// Inside a struct/object - tracks if we've written any fields
    Struct {
        first: bool,
        is_root: bool,
        force_multiline: bool,
        /// True if this struct started on the same line as its key (inline start)
        inline_start: bool,
        /// Positions of comma separators written in this struct (for fixing mixed separators)
        comma_positions: Vec<usize>,
    },
    /// Inside a sequence - tracks if we've written any items
    Seq {
        first: bool,
        /// True if this sequence started on the same line as its key (inline start)
        inline_start: bool,
    },
}

/// Low-level Styx output writer.
///
/// This writer handles the formatting logic for Styx output, including:
/// - Indentation and newlines
/// - Scalar quoting decisions
/// - Inline vs multi-line formatting
pub struct StyxWriter {
    out: Vec<u8>,
    stack: Vec<Context>,
    options: FormatOptions,
    /// If true, skip the next before_value() call (used after writing a tag)
    skip_next_before_value: bool,
}

impl StyxWriter {
    /// Create a new writer with default options.
    pub fn new() -> Self {
        Self::with_options(FormatOptions::default())
    }

    /// Create a new writer with the given options.
    pub fn with_options(options: FormatOptions) -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
            skip_next_before_value: false,
            options,
        }
    }

    /// Consume the writer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Consume the writer and return the output as a String.
    ///
    /// # Panics
    /// Panics if the output is not valid UTF-8 (should never happen with Styx).
    pub fn finish_string(self) -> String {
        String::from_utf8(self.out).expect("Styx output should always be valid UTF-8")
    }

    /// Current nesting depth.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Effective indentation depth (accounts for root struct and inline containers).
    fn indent_depth(&self) -> usize {
        let mut depth = 0;
        for ctx in &self.stack {
            match ctx {
                Context::Struct { is_root: true, .. } => {
                    // Root struct doesn't add indentation
                }
                Context::Struct {
                    inline_start: true,
                    force_multiline: false,
                    ..
                } => {
                    // Inline struct that stays inline doesn't add indentation
                }
                Context::Seq {
                    inline_start: true, ..
                } => {
                    // Inline-started sequence doesn't add indentation
                }
                _ => {
                    depth += 1;
                }
            }
        }
        depth
    }

    /// Calculate available width at current depth.
    pub fn available_width(&self) -> usize {
        let used = self.depth() * self.options.indent.len();
        self.options.max_width.saturating_sub(used)
    }

    /// Check if we should use inline formatting at current depth.
    pub fn should_inline(&self) -> bool {
        if self.options.force_style == ForceStyle::Inline {
            return true;
        } else if self.options.force_style == ForceStyle::Multiline {
            return false;
        }
        // Root level always uses newlines
        if self.depth() == 0 {
            return false;
        }
        // Check if we're inside a root struct
        if let Some(Context::Struct { is_root: true, .. }) = self.stack.first()
            && self.depth() == 1
        {
            return false;
        }
        // Check if current struct is forced multiline
        if let Some(Context::Struct {
            force_multiline: true,
            ..
        }) = self.stack.last()
        {
            return false;
        }
        // If available width is too small, force multiline
        self.available_width() >= self.options.min_inline_width
    }

    /// Write indentation for the current depth.
    pub fn write_indent(&mut self) {
        for _ in 0..self.indent_depth() {
            self.out.extend_from_slice(self.options.indent.as_bytes());
        }
    }

    /// Write a newline and indentation.
    pub fn write_newline_indent(&mut self) {
        self.out.push(b'\n');
        self.write_indent();
    }

    /// Write raw bytes to the output.
    pub fn write_raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// Write a raw string to the output.
    pub fn write_str(&mut self, s: &str) {
        self.out.extend_from_slice(s.as_bytes());
    }

    /// Write a single byte.
    pub fn write_byte(&mut self, b: u8) {
        self.out.push(b);
    }

    /// Begin a struct/object.
    ///
    /// If `is_root` is true, no braces are written (implicit root object).
    pub fn begin_struct(&mut self, is_root: bool) {
        self.begin_struct_with_options(is_root, false);
    }

    /// Begin a struct/object with explicit multiline control.
    ///
    /// If `is_root` is true, no braces are written (implicit root object).
    /// If `force_multiline` is true, the struct will never be inlined.
    pub fn begin_struct_with_options(&mut self, is_root: bool, force_multiline: bool) {
        self.before_value();

        // A struct starts inline if it's appearing as a value on the same line as its key
        // (i.e., not the root and the opening brace is on the same line)
        let inline_start = !is_root;

        if is_root {
            self.stack.push(Context::Struct {
                first: true,
                is_root: true,
                force_multiline,
                inline_start: false,
                comma_positions: Vec::new(),
            });
        } else {
            self.out.push(b'{');
            self.stack.push(Context::Struct {
                first: true,
                is_root: false,
                force_multiline,
                inline_start,
                comma_positions: Vec::new(),
            });
        }
    }

    /// Begin a struct directly after a tag (no space before the brace).
    pub fn begin_struct_after_tag(&mut self, force_multiline: bool) {
        // Don't call before_value() - we want no space after the tag
        self.out.push(b'{');
        self.stack.push(Context::Struct {
            first: true,
            is_root: false,
            force_multiline,
            inline_start: true,
            comma_positions: Vec::new(),
        });
    }

    /// Write a field key without quoting (raw).
    ///
    /// Use this for keys that should be written exactly as-is, like `@` for unit keys.
    pub fn field_key_raw(&mut self, key: &str) -> Result<(), &'static str> {
        // Extract state first to avoid borrow conflicts
        let (is_struct, is_first, is_root) = match self.stack.last() {
            Some(Context::Struct { first, is_root, .. }) => (true, *first, *is_root),
            _ => (false, true, false),
        };

        if !is_struct {
            return Err("field_key_raw called outside of struct");
        }

        let should_inline = self.should_inline();

        if !is_first {
            if should_inline && !is_root {
                // Record comma position for potential later fixing
                let comma_pos = self.out.len();
                self.out.extend_from_slice(b", ");
                if let Some(Context::Struct { comma_positions, .. }) = self.stack.last_mut() {
                    comma_positions.push(comma_pos);
                }
            } else {
                self.write_newline_indent();
            }
        } else {
            // First field
            if !is_root && !should_inline {
                self.write_newline_indent();
            }
        }

        // Update the first flag
        if let Some(Context::Struct { first, .. }) = self.stack.last_mut() {
            *first = false;
        }

        // Write key as-is (no quoting)
        self.out.extend_from_slice(key.as_bytes());
        self.out.push(b' ');
        Ok(())
    }

    /// Write a field key.
    ///
    /// Returns an error message if called outside of a struct context.
    pub fn field_key(&mut self, key: &str) -> Result<(), &'static str> {
        // Extract state first to avoid borrow conflicts
        let (is_struct, is_first, is_root) = match self.stack.last() {
            Some(Context::Struct { first, is_root, .. }) => (true, *first, *is_root),
            _ => (false, true, false),
        };

        if !is_struct {
            return Err("field_key called outside of struct");
        }

        let should_inline = self.should_inline();

        if !is_first {
            if should_inline && !is_root {
                // Record comma position for potential later fixing
                let comma_pos = self.out.len();
                self.out.extend_from_slice(b", ");
                if let Some(Context::Struct { comma_positions, .. }) = self.stack.last_mut() {
                    comma_positions.push(comma_pos);
                }
            } else {
                self.write_newline_indent();
            }
        } else {
            // First field
            if !is_root && !should_inline {
                self.write_newline_indent();
            }
        }

        // Update the first flag
        if let Some(Context::Struct { first, .. }) = self.stack.last_mut() {
            *first = false;
        }

        // Write the key - keys are typically bare identifiers
        if can_be_bare(key) {
            self.out.extend_from_slice(key.as_bytes());
        } else {
            self.write_quoted_string(key);
        }
        self.out.push(b' ');
        Ok(())
    }

    /// End a struct/object.
    ///
    /// Returns an error message if called without a matching begin_struct.
    pub fn end_struct(&mut self) -> Result<(), &'static str> {
        // Check should_inline before popping (need stack state)
        let should_inline = self.should_inline();

        match self.stack.pop() {
            Some(Context::Struct { first, is_root, .. }) => {
                if is_root {
                    // Root struct: add trailing newline if we wrote anything
                    if !first {
                        self.out.push(b'\n');
                    }
                } else {
                    if !first && !should_inline {
                        // Newline before closing brace
                        self.out.push(b'\n');
                        // Indent at the PARENT level (we already popped)
                        self.write_indent();
                    }
                    self.out.push(b'}');
                }
                Ok(())
            }
            _ => Err("end_struct called without matching begin_struct"),
        }
    }

    /// Begin a sequence.
    pub fn begin_seq(&mut self) {
        self.before_value();
        self.out.push(b'(');
        // Sequences always start inline (on the same line as their key)
        self.stack.push(Context::Seq {
            first: true,
            inline_start: true,
        });
    }

    /// End a sequence.
    ///
    /// Returns an error message if called without a matching begin_seq.
    pub fn end_seq(&mut self) -> Result<(), &'static str> {
        // Check should_inline before popping (need stack state)
        let should_inline = self.should_inline();

        match self.stack.pop() {
            Some(Context::Seq { first, .. }) => {
                if !first && !should_inline {
                    self.write_newline_indent();
                }
                self.out.push(b')');
                Ok(())
            }
            _ => Err("end_seq called without matching begin_seq"),
        }
    }

    /// Write a null/unit value.
    pub fn write_null(&mut self) {
        self.before_value();
        self.out.push(b'@');
    }

    /// Write a boolean value.
    pub fn write_bool(&mut self, v: bool) {
        self.before_value();
        if v {
            self.out.extend_from_slice(b"true");
        } else {
            self.out.extend_from_slice(b"false");
        }
    }

    /// Write an i64 value.
    pub fn write_i64(&mut self, v: i64) {
        self.before_value();
        self.out.extend_from_slice(v.to_string().as_bytes());
    }

    /// Write a u64 value.
    pub fn write_u64(&mut self, v: u64) {
        self.before_value();
        self.out.extend_from_slice(v.to_string().as_bytes());
    }

    /// Write an i128 value.
    pub fn write_i128(&mut self, v: i128) {
        self.before_value();
        self.out.extend_from_slice(v.to_string().as_bytes());
    }

    /// Write a u128 value.
    pub fn write_u128(&mut self, v: u128) {
        self.before_value();
        self.out.extend_from_slice(v.to_string().as_bytes());
    }

    /// Write an f64 value.
    pub fn write_f64(&mut self, v: f64) {
        self.before_value();
        self.out.extend_from_slice(v.to_string().as_bytes());
    }

    /// Write a string value with appropriate quoting.
    pub fn write_string(&mut self, s: &str) {
        self.before_value();
        self.write_scalar_string(s);
    }

    /// Write a char value.
    pub fn write_char(&mut self, c: char) {
        self.before_value();
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.write_scalar_string(s);
    }

    /// Write bytes as hex-encoded string.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.before_value();
        self.out.push(b'"');
        for byte in bytes.iter() {
            let hex = |d: u8| {
                if d < 10 { b'0' + d } else { b'a' + (d - 10) }
            };
            self.out.push(hex(byte >> 4));
            self.out.push(hex(byte & 0xf));
        }
        self.out.push(b'"');
    }

    /// Write a variant tag (e.g., `@some`).
    pub fn write_variant_tag(&mut self, name: &str) {
        self.before_value();
        self.out.push(b'@');
        self.out.extend_from_slice(name.as_bytes());
        // The payload should follow without spacing
        self.skip_next_before_value = true;
    }

    /// Write a scalar value with appropriate quoting.
    /// Alias for write_string, for when you have a pre-existing scalar.
    pub fn write_scalar(&mut self, s: &str) {
        self.write_string(s);
    }

    /// Write a tag (e.g., `@string`). Same as write_variant_tag.
    pub fn write_tag(&mut self, name: &str) {
        self.write_variant_tag(name);
    }

    /// Clear the skip_next_before_value flag.
    /// Call this when a tag's payload is skipped (e.g., None for a unit variant).
    pub fn clear_skip_before_value(&mut self) {
        self.skip_next_before_value = false;
    }

    /// Begin a sequence directly after a tag (no space before the paren).
    pub fn begin_seq_after_tag(&mut self) {
        self.out.push(b'(');
        // Sequences after tags always start inline
        self.stack.push(Context::Seq {
            first: true,
            inline_start: true,
        });
    }

    /// Write a doc comment followed by a field key.
    /// Multiple lines are supported (each line gets `/// ` prefix).
    pub fn write_doc_comment_and_key(&mut self, doc: &str, key: &str) {
        // Check if first field and root
        let (is_first, is_root) = match self.stack.last() {
            Some(Context::Struct { first, is_root, .. }) => (*first, *is_root),
            _ => (true, false),
        };

        // Mark that we're no longer on first field, and force multiline since we have doc comments
        if let Some(Context::Struct {
            first,
            force_multiline,
            ..
        }) = self.stack.last_mut()
        {
            *first = false;
            *force_multiline = true;
        }

        // Fix any commas we wrote before this doc comment (they need to become newlines)
        self.fix_comma_separators();

        // For non-first fields, or non-root structs, add newline before doc
        let need_leading_newline = !is_first || !is_root;

        for (i, line) in doc.lines().enumerate() {
            if i > 0 || need_leading_newline {
                self.write_newline_indent();
            }
            self.out.extend_from_slice(b"/// ");
            self.out.extend_from_slice(line.as_bytes());
        }

        // Newline before the key (but no indent for root first field)
        if is_first && is_root {
            self.out.push(b'\n');
        } else {
            self.write_newline_indent();
        }

        // Write the key
        if can_be_bare(key) {
            self.out.extend_from_slice(key.as_bytes());
        } else {
            self.write_quoted_string(key);
        }
        self.out.push(b' ');
    }

    /// Write a doc comment followed by a raw field key (no quoting).
    /// Use this for keys like `@` that should be written literally.
    pub fn write_doc_comment_and_key_raw(&mut self, doc: &str, key: &str) {
        // Check if first field and root
        let (is_first, is_root) = match self.stack.last() {
            Some(Context::Struct { first, is_root, .. }) => (*first, *is_root),
            _ => (true, false),
        };

        // Mark that we're no longer on first field, and force multiline since we have doc comments
        if let Some(Context::Struct {
            first,
            force_multiline,
            ..
        }) = self.stack.last_mut()
        {
            *first = false;
            *force_multiline = true;
        }

        // Fix any commas we wrote before this doc comment (they need to become newlines)
        self.fix_comma_separators();

        // For non-first fields, or non-root structs, add newline before doc
        let need_leading_newline = !is_first || !is_root;

        for (i, line) in doc.lines().enumerate() {
            if i > 0 || need_leading_newline {
                self.write_newline_indent();
            }
            self.out.extend_from_slice(b"/// ");
            self.out.extend_from_slice(line.as_bytes());
        }

        // Newline before the key (but no indent for root first field)
        if is_first && is_root {
            self.out.push(b'\n');
        } else {
            self.write_newline_indent();
        }

        // Write the key as-is (no quoting)
        self.out.extend_from_slice(key.as_bytes());
        self.out.push(b' ');
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Fix any comma separators in the current struct by replacing them with newlines.
    /// Call this when switching from inline to multiline formatting mid-struct.
    fn fix_comma_separators(&mut self) {
        // Extract comma positions from current struct context
        let comma_positions = match self.stack.last_mut() {
            Some(Context::Struct { comma_positions, .. }) => std::mem::take(comma_positions),
            _ => return,
        };

        if comma_positions.is_empty() {
            return;
        }

        // Calculate the indentation string for this struct
        let indent = self.options.indent.repeat(self.indent_depth());
        let newline_indent = format!("\n{}", indent);

        // Process comma positions from end to start (so earlier positions stay valid)
        for &comma_pos in comma_positions.iter().rev() {
            // Replace ", " (2 bytes) with "\n" + indent
            // First, verify this position has ", " (sanity check)
            if comma_pos + 2 <= self.out.len()
                && self.out[comma_pos] == b','
                && self.out[comma_pos + 1] == b' '
            {
                // Remove ", " and insert newline+indent
                self.out.drain(comma_pos..comma_pos + 2);
                let bytes = newline_indent.as_bytes();
                for (i, &b) in bytes.iter().enumerate() {
                    self.out.insert(comma_pos + i, b);
                }
            }
        }
    }

    /// Handle separator before a value in a container.
    pub fn before_value(&mut self) {
        // If we just wrote a tag, skip spacing for the payload
        if self.skip_next_before_value {
            self.skip_next_before_value = false;
            // Still need to update the first flag
            if let Some(Context::Seq { first, .. }) = self.stack.last_mut() {
                *first = false;
            }
            return;
        }

        // Extract state first to avoid borrow conflicts
        let (is_seq, is_first) = match self.stack.last() {
            Some(Context::Seq { first, .. }) => (true, *first),
            _ => (false, true),
        };

        if is_seq && !is_first {
            if self.should_inline() {
                self.out.push(b' ');
            } else {
                self.write_newline_indent();
            }
        }

        // Update the first flag
        if let Some(Context::Seq { first, .. }) = self.stack.last_mut() {
            *first = false;
        }
    }

    /// Write a scalar value with appropriate quoting.
    fn write_scalar_string(&mut self, s: &str) {
        // Rule 1: Prefer bare scalars when valid
        if can_be_bare(s) {
            self.out.extend_from_slice(s.as_bytes());
            return;
        }

        let newline_count = count_newlines(s);
        let escape_count = count_escapes(s);

        // Rule 3: Use heredocs for multi-line text
        if newline_count >= self.options.heredoc_line_threshold {
            self.write_heredoc(s);
            return;
        }

        // Rule 2: Use raw strings for complex escaping (> 3 escapes)
        if escape_count > 3 && !s.contains("\"#") {
            self.write_raw_string(s);
            return;
        }

        // Default: quoted string
        self.write_quoted_string(s);
    }

    /// Write a quoted string with proper escaping.
    fn write_quoted_string(&mut self, s: &str) {
        self.out.push(b'"');
        let escaped = escape_quoted(s);
        self.out.extend_from_slice(escaped.as_bytes());
        self.out.push(b'"');
    }

    /// Write a raw string (r#"..."#).
    fn write_raw_string(&mut self, s: &str) {
        // Find the minimum number of # needed
        let mut hashes = 0;
        let mut check = String::from("\"");
        while s.contains(&check) {
            hashes += 1;
            check = format!("\"{}#", "#".repeat(hashes - 1));
        }

        self.out.push(b'r');
        for _ in 0..hashes {
            self.out.push(b'#');
        }
        self.out.push(b'"');
        self.out.extend_from_slice(s.as_bytes());
        self.out.push(b'"');
        for _ in 0..hashes {
            self.out.push(b'#');
        }
    }

    /// Write a heredoc string.
    fn write_heredoc(&mut self, s: &str) {
        // Find a delimiter that doesn't appear in the string
        let delimiters = ["TEXT", "END", "HEREDOC", "DOC", "STR", "CONTENT"];
        let delimiter = delimiters
            .iter()
            .find(|d| !s.contains(*d))
            .unwrap_or(&"TEXT");

        self.out.extend_from_slice(b"<<");
        self.out.extend_from_slice(delimiter.as_bytes());
        self.out.push(b'\n');
        self.out.extend_from_slice(s.as_bytes());
        if !s.ends_with('\n') {
            self.out.push(b'\n');
        }
        self.out.extend_from_slice(delimiter.as_bytes());
    }
}

impl Default for StyxWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_struct() {
        let mut w = StyxWriter::new();
        w.begin_struct(true);
        w.field_key("name").unwrap();
        w.write_string("hello");
        w.field_key("value").unwrap();
        w.write_i64(42);
        w.end_struct().unwrap();

        let result = w.finish_string();
        assert!(result.contains("name hello"));
        assert!(result.contains("value 42"));
    }

    #[test]
    fn test_nested_inline() {
        let mut w = StyxWriter::with_options(FormatOptions::default());
        w.begin_struct(true);
        w.field_key("point").unwrap();
        w.begin_struct(false);
        w.field_key("x").unwrap();
        w.write_i64(10);
        w.field_key("y").unwrap();
        w.write_i64(20);
        w.end_struct().unwrap();
        w.end_struct().unwrap();

        let result = w.finish_string();
        // Nested struct should be inline
        assert!(result.contains("{x 10, y 20}"));
    }

    #[test]
    fn test_sequence() {
        let mut w = StyxWriter::new();
        w.begin_struct(true);
        w.field_key("items").unwrap();
        w.begin_seq();
        w.write_i64(1);
        w.write_i64(2);
        w.write_i64(3);
        w.end_seq().unwrap();
        w.end_struct().unwrap();

        let result = w.finish_string();
        assert!(result.contains("items (1 2 3)"));
    }

    #[test]
    fn test_quoted_string() {
        let mut w = StyxWriter::new();
        w.begin_struct(true);
        w.field_key("message").unwrap();
        w.write_string("hello world");
        w.end_struct().unwrap();

        let result = w.finish_string();
        assert!(result.contains("message \"hello world\""));
    }

    #[test]
    fn test_force_inline() {
        let mut w = StyxWriter::with_options(FormatOptions::default().inline());
        w.begin_struct(false);
        w.field_key("a").unwrap();
        w.write_i64(1);
        w.field_key("b").unwrap();
        w.write_i64(2);
        w.end_struct().unwrap();

        let result = w.finish_string();
        assert_eq!(result, "{a 1, b 2}");
    }

    #[test]
    fn test_doc_comment_fixes_commas() {
        // When a doc comment is added mid-struct, any previously written
        // commas should be replaced with newlines to avoid mixed separators.
        let mut w = StyxWriter::with_options(FormatOptions::default().inline());
        w.begin_struct(false);
        w.field_key("a").unwrap();
        w.write_i64(1);
        w.field_key("b").unwrap();
        w.write_i64(2);
        // This doc comment should trigger comma -> newline conversion
        w.write_doc_comment_and_key("A documented field", "c");
        w.write_i64(3);
        w.end_struct().unwrap();

        let result = w.finish_string();
        // Should NOT contain ", " since commas were replaced with newlines
        assert!(
            !result.contains(", "),
            "Result should not contain commas after doc comment: {}",
            result
        );
        // Should contain newline-separated entries
        assert!(result.contains("a 1\n"), "Expected newline after a: {}", result);
    }
}
