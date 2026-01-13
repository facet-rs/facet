//! Styx serialization implementation.

use std::borrow::Cow;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// Options for Styx serialization.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Indentation string (default: "    " - 4 spaces)
    pub indent: &'static str,

    /// Max line width before wrapping (default: 80)
    pub max_width: usize,

    /// Minimum available width to even consider inline (default: 30)
    /// If depth eats into max_width below this, force multi-line
    pub min_inline_width: usize,

    /// Inline objects with ≤ N entries (default: 4)
    pub inline_object_threshold: usize,

    /// Inline sequences with ≤ N items (default: 8)
    pub inline_sequence_threshold: usize,

    /// Use heredocs for strings with > N lines (default: 2)
    pub heredoc_line_threshold: usize,

    /// Force all objects to use newline separators (default: false)
    pub force_multiline: bool,

    /// Force all objects to use comma separators (default: false)
    /// Takes precedence over force_multiline if both set
    pub force_inline: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            indent: "    ",
            max_width: 80,
            min_inline_width: 30,
            inline_object_threshold: 4,
            inline_sequence_threshold: 8,
            heredoc_line_threshold: 2,
            force_multiline: false,
            force_inline: false,
        }
    }
}

impl SerializeOptions {
    /// Create new default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Force all output to be multi-line (newline separators).
    pub fn multiline(mut self) -> Self {
        self.force_multiline = true;
        self.force_inline = false;
        self
    }

    /// Force all output to be inline (comma separators, single line).
    pub fn inline(mut self) -> Self {
        self.force_inline = true;
        self.force_multiline = false;
        self
    }

    /// Set a custom indentation string.
    pub fn indent(mut self, indent: &'static str) -> Self {
        self.indent = indent;
        self
    }

    /// Set max line width.
    pub fn max_width(mut self, width: usize) -> Self {
        self.max_width = width;
        self
    }
}

/// Error type for Styx serialization.
#[derive(Debug)]
pub struct StyxSerializeError {
    msg: Cow<'static, str>,
}

impl StyxSerializeError {
    fn new(msg: impl Into<Cow<'static, str>>) -> Self {
        Self { msg: msg.into() }
    }
}

impl core::fmt::Display for StyxSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for StyxSerializeError {}

/// Context for tracking serialization state.
#[derive(Debug, Clone, Copy)]
enum Ctx {
    /// Inside a struct/object - tracks if we've written any fields
    Struct { first: bool, is_root: bool },
    /// Inside a sequence - tracks if we've written any items
    Seq { first: bool },
}

/// Styx serializer with configurable formatting options.
pub struct StyxSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    options: SerializeOptions,
}

impl StyxSerializer {
    /// Create a new Styx serializer with default options.
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new Styx serializer with the given options.
    pub fn with_options(options: SerializeOptions) -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
            options,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Current nesting depth.
    fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Calculate available width at current depth.
    fn available_width(&self) -> usize {
        let used = self.depth() * self.options.indent.len();
        self.options.max_width.saturating_sub(used)
    }

    /// Check if we should use inline formatting at current depth.
    fn should_inline(&self) -> bool {
        if self.options.force_inline {
            return true;
        }
        if self.options.force_multiline {
            return false;
        }
        // Root level always uses newlines
        if self.depth() == 0 {
            return false;
        }
        // Check if we're inside a root struct
        if let Some(Ctx::Struct { is_root: true, .. }) = self.stack.first()
            && self.depth() == 1
        {
            return false;
        }
        // If available width is too small, force multiline
        self.available_width() >= self.options.min_inline_width
    }

    /// Write indentation for the current depth.
    fn write_indent(&mut self) {
        for _ in 0..self.depth() {
            self.out.extend_from_slice(self.options.indent.as_bytes());
        }
    }

    /// Write a newline and indentation.
    fn write_newline_indent(&mut self) {
        self.out.push(b'\n');
        self.write_indent();
    }

    /// Check if a string can be written as a bare scalar.
    fn can_be_bare(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        // Cannot start with characters that look like other syntax
        if s.starts_with("//") || s.starts_with("r#") || s.starts_with("<<") {
            return false;
        }
        // Cannot contain special characters
        !s.chars().any(|c| {
            matches!(c, '{' | '}' | '(' | ')' | ',' | '"' | '=' | '@') || c.is_whitespace()
        })
    }

    /// Count escape sequences needed for a quoted string.
    fn count_escapes(s: &str) -> usize {
        s.chars()
            .filter(|c| matches!(c, '"' | '\\' | '\n' | '\r' | '\t'))
            .count()
    }

    /// Count newlines in a string.
    fn count_newlines(s: &str) -> usize {
        s.chars().filter(|&c| c == '\n').count()
    }

    /// Write a scalar value with appropriate quoting.
    fn write_scalar_string(&mut self, s: &str) {
        // Rule 1: Prefer bare scalars when valid
        if Self::can_be_bare(s) {
            self.out.extend_from_slice(s.as_bytes());
            return;
        }

        let newline_count = Self::count_newlines(s);
        let escape_count = Self::count_escapes(s);

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
        for c in s.chars() {
            match c {
                '"' => self.out.extend_from_slice(b"\\\""),
                '\\' => self.out.extend_from_slice(b"\\\\"),
                '\n' => self.out.extend_from_slice(b"\\n"),
                '\r' => self.out.extend_from_slice(b"\\r"),
                '\t' => self.out.extend_from_slice(b"\\t"),
                c if c.is_ascii_control() => {
                    // Write as \uXXXX
                    let code = c as u32;
                    let hex = |d: u32| {
                        if d < 10 {
                            b'0' + d as u8
                        } else {
                            b'a' + (d - 10) as u8
                        }
                    };
                    self.out.extend_from_slice(&[
                        b'\\',
                        b'u',
                        hex((code >> 12) & 0xF),
                        hex((code >> 8) & 0xF),
                        hex((code >> 4) & 0xF),
                        hex(code & 0xF),
                    ]);
                }
                c => {
                    let mut buf = [0u8; 4];
                    let len = c.encode_utf8(&mut buf).len();
                    self.out.extend_from_slice(&buf[..len]);
                }
            }
        }
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

    /// Handle separator before a value in a container.
    fn before_value(&mut self) {
        // Extract state first to avoid borrow conflicts
        let (is_seq, is_first) = match self.stack.last() {
            Some(Ctx::Seq { first }) => (true, *first),
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
        if let Some(Ctx::Seq { first }) = self.stack.last_mut() {
            *first = false;
        }
    }
}

impl Default for StyxSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for StyxSerializer {
    type Error = StyxSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.before_value();

        let is_root = self.stack.is_empty();

        if is_root {
            // Root struct: no braces, just fields
            self.stack.push(Ctx::Struct {
                first: true,
                is_root: true,
            });
        } else {
            self.out.push(b'{');
            self.stack.push(Ctx::Struct {
                first: true,
                is_root: false,
            });
            if !self.should_inline() {
                // Will write newline before first field
            }
        }
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        // Extract state first to avoid borrow conflicts
        let (is_struct, is_first, is_root) = match self.stack.last() {
            Some(Ctx::Struct { first, is_root }) => (true, *first, *is_root),
            _ => (false, true, false),
        };

        if !is_struct {
            return Err(StyxSerializeError::new(
                "field_key called outside of struct",
            ));
        }

        let should_inline = self.should_inline();

        if !is_first {
            if should_inline && !is_root {
                self.out.extend_from_slice(b", ");
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
        if let Some(Ctx::Struct { first, .. }) = self.stack.last_mut() {
            *first = false;
        }

        // Write the key - keys are typically bare identifiers
        if Self::can_be_bare(key) {
            self.out.extend_from_slice(key.as_bytes());
        } else {
            self.write_quoted_string(key);
        }
        self.out.push(b' ');
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        // Check should_inline before popping (need stack state)
        let should_inline = self.should_inline();

        match self.stack.pop() {
            Some(Ctx::Struct { first, is_root }) => {
                if is_root {
                    // Root struct: add trailing newline if we wrote anything
                    if !first {
                        self.out.push(b'\n');
                    }
                } else {
                    if !first && !should_inline {
                        // Dedent before closing brace
                        self.write_newline_indent();
                    }
                    self.out.push(b'}');
                }
                Ok(())
            }
            _ => Err(StyxSerializeError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.before_value();
        self.out.push(b'(');
        self.stack.push(Ctx::Seq { first: true });
        if !self.should_inline() {
            // Will write newline before first item
        }
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        // Check should_inline before popping (need stack state)
        let should_inline = self.should_inline();

        match self.stack.pop() {
            Some(Ctx::Seq { first }) => {
                if !first && !should_inline {
                    self.write_newline_indent();
                }
                self.out.push(b')');
                Ok(())
            }
            _ => Err(StyxSerializeError::new(
                "end_seq called without matching begin_seq",
            )),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.before_value();
        match scalar {
            ScalarValue::Null => {
                self.out.push(b'@');
            }
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true");
                } else {
                    self.out.extend_from_slice(b"false");
                }
            }
            ScalarValue::Char(c) => {
                // Single char as string
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                self.write_scalar_string(s);
            }
            ScalarValue::I64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::I128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::F64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::Str(s) | ScalarValue::StringlyTyped(s) => {
                self.write_scalar_string(&s);
            }
            ScalarValue::Bytes(bytes) => {
                // Encode bytes as base64 or hex? For now, use hex
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
        }
        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        // In Styx, None is represented as @ (unit value)
        // Note: The field key has already been emitted by the framework
        self.before_value();
        self.out.push(b'@');
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Serialize a value to a Styx string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_styx::to_string;
///
/// #[derive(Facet)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let config = Config { name: "myapp".into(), port: 8080 };
/// let styx = to_string(&config).unwrap();
/// assert!(styx.contains("name myapp"));
/// assert!(styx.contains("port 8080"));
/// ```
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<StyxSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_string_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to a compact Styx string (single line, comma separators).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_styx::to_string_compact;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let styx = to_string_compact(&point).unwrap();
/// assert_eq!(styx, "{x 10, y 20}");
/// ```
pub fn to_string_compact<'facet, T>(value: &T) -> Result<String, SerializeError<StyxSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    // For compact mode, we don't want the root to be unwrapped
    let options = SerializeOptions::default().inline();
    let mut serializer = CompactStyxSerializer::with_options(options);
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

/// Serialize a value to a Styx string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<StyxSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = StyxSerializer::with_options(options.clone());
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

/// Serialize a `Peek` instance to a Styx string.
pub fn peek_to_string<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<String, SerializeError<StyxSerializeError>> {
    peek_to_string_with_options(peek, &SerializeOptions::default())
}

/// Serialize a `Peek` instance to a Styx string with custom options.
pub fn peek_to_string_with_options<'input, 'facet>(
    peek: Peek<'input, 'facet>,
    options: &SerializeOptions,
) -> Result<String, SerializeError<StyxSerializeError>> {
    let mut serializer = StyxSerializer::with_options(options.clone());
    serialize_root(&mut serializer, peek)?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Compact serializer (always uses braces, never unwraps root)
// ─────────────────────────────────────────────────────────────────────────────

/// A variant of StyxSerializer that always wraps in braces (for compact mode).
struct CompactStyxSerializer {
    inner: StyxSerializer,
}

impl CompactStyxSerializer {
    fn with_options(options: SerializeOptions) -> Self {
        Self {
            inner: StyxSerializer::with_options(options),
        }
    }

    fn finish(self) -> Vec<u8> {
        self.inner.finish()
    }
}

impl FormatSerializer for CompactStyxSerializer {
    type Error = StyxSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.inner.before_value();
        self.inner.out.push(b'{');
        self.inner.stack.push(Ctx::Struct {
            first: true,
            is_root: false, // Never treat as root in compact mode
        });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.inner.field_key(key)
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.inner.stack.pop() {
            Some(Ctx::Struct { .. }) => {
                self.inner.out.push(b'}');
                Ok(())
            }
            _ => Err(StyxSerializeError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.inner.begin_seq()
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.inner.end_seq()
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.inner.scalar(scalar)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.inner.serialize_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet, Debug)]
    struct Simple {
        name: String,
        value: i32,
    }

    #[derive(Facet, Debug)]
    struct Nested {
        inner: Simple,
    }

    #[derive(Facet, Debug)]
    struct WithVec {
        items: Vec<i32>,
    }

    #[derive(Facet, Debug)]
    struct WithOptional {
        required: String,
        optional: Option<i32>,
    }

    #[test]
    fn test_simple_struct() {
        let value = Simple {
            name: "hello".into(),
            value: 42,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("name hello"));
        assert!(result.contains("value 42"));
    }

    #[test]
    fn test_compact_struct() {
        let value = Simple {
            name: "hello".into(),
            value: 42,
        };
        let result = to_string_compact(&value).unwrap();
        assert_eq!(result, "{name hello, value 42}");
    }

    #[test]
    fn test_nested_struct() {
        let value = Nested {
            inner: Simple {
                name: "test".into(),
                value: 123,
            },
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("inner"));
        // Nested struct should be inline by default
        assert!(result.contains("{name test, value 123}"));
    }

    #[test]
    fn test_sequence() {
        let value = WithVec {
            items: vec![1, 2, 3, 4, 5],
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("items (1 2 3 4 5)"));
    }

    #[test]
    fn test_quoted_string() {
        let value = Simple {
            name: "hello world".into(), // Has space, needs quoting
            value: 42,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("name \"hello world\""));
    }

    #[test]
    fn test_special_chars_need_quoting() {
        let value = Simple {
            name: "{braces}".into(),
            value: 42,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("name \"{braces}\""));
    }

    #[test]
    fn test_optional_none() {
        let value = WithOptional {
            required: "hello".into(),
            optional: None,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("required hello"));
        // optional None is serialized as @ (unit value)
        assert!(result.contains("optional @"));
    }

    #[test]
    fn test_optional_some() {
        let value = WithOptional {
            required: "hello".into(),
            optional: Some(42),
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("required hello"));
        assert!(result.contains("optional 42"));
    }

    #[test]
    fn test_bool_values() {
        #[derive(Facet, Debug)]
        struct Flags {
            enabled: bool,
            debug: bool,
        }

        let value = Flags {
            enabled: true,
            debug: false,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("enabled true"));
        assert!(result.contains("debug false"));
    }

    #[test]
    fn test_bare_scalar_rules() {
        // These should be bare
        assert!(StyxSerializer::can_be_bare("localhost"));
        assert!(StyxSerializer::can_be_bare("8080"));
        assert!(StyxSerializer::can_be_bare("hello-world"));
        assert!(StyxSerializer::can_be_bare("https://example.com/path"));

        // These must be quoted
        assert!(!StyxSerializer::can_be_bare("")); // empty
        assert!(!StyxSerializer::can_be_bare("hello world")); // space
        assert!(!StyxSerializer::can_be_bare("{braces}")); // braces
        assert!(!StyxSerializer::can_be_bare("(parens)")); // parens
        assert!(!StyxSerializer::can_be_bare("key=value")); // equals
        assert!(!StyxSerializer::can_be_bare("@tag")); // at sign
        assert!(!StyxSerializer::can_be_bare("//comment")); // looks like comment
        assert!(!StyxSerializer::can_be_bare("r#raw")); // looks like raw string
        assert!(!StyxSerializer::can_be_bare("<<HERE")); // looks like heredoc
    }

    #[test]
    fn test_roundtrip_simple() {
        use crate::from_str;

        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            port: u16,
            debug: bool,
        }

        let original = Config {
            name: "myapp".into(),
            port: 8080,
            debug: true,
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Config = from_str(&serialized).unwrap();

        assert_eq!(original.name, parsed.name);
        assert_eq!(original.port, parsed.port);
        assert_eq!(original.debug, parsed.debug);
    }

    #[test]
    fn test_roundtrip_nested() {
        use crate::from_str;

        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            x: i32,
            y: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            name: String,
            point: Inner,
        }

        let original = Outer {
            name: "origin".into(),
            point: Inner { x: 10, y: 20 },
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Outer = from_str(&serialized).unwrap();

        assert_eq!(original.name, parsed.name);
        assert_eq!(original.point.x, parsed.point.x);
        assert_eq!(original.point.y, parsed.point.y);
    }

    #[test]
    fn test_roundtrip_with_vec() {
        use crate::from_str;

        #[derive(Facet, Debug, PartialEq)]
        struct Data {
            values: Vec<i32>,
        }

        let original = Data {
            values: vec![1, 2, 3, 4, 5],
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Data = from_str(&serialized).unwrap();

        assert_eq!(original.values, parsed.values);
    }

    #[test]
    fn test_roundtrip_quoted_string() {
        use crate::from_str;

        #[derive(Facet, Debug, PartialEq)]
        struct Message {
            text: String,
        }

        let original = Message {
            text: "hello world with spaces".into(),
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Message = from_str(&serialized).unwrap();

        assert_eq!(original.text, parsed.text);
    }
}
