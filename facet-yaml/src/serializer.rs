//! YAML serializer implementing the FormatSerializer trait.

extern crate alloc;

#[cfg_attr(feature = "fast", allow(unused_imports))]
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::{self, Debug};

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// Error type for YAML serialization.
#[derive(Debug)]
pub struct YamlSerializeError {
    msg: String,
}

impl fmt::Display for YamlSerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for YamlSerializeError {}

impl YamlSerializeError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

/// Context for tracking where we are in the output structure.
#[derive(Debug, Clone, Copy)]
enum Ctx {
    /// In a struct/mapping
    Struct { indent: usize, has_fields: bool },
    /// In a sequence/list
    Seq { indent: usize, has_items: bool },
}

/// Where we are on the current line
#[derive(Debug, Clone, Copy, PartialEq)]
enum LinePos {
    /// At the start of a new line (or document start)
    Start,
    /// Inline after "- " (first field of seq-item struct can go here)
    AfterSeqMarker,
    /// Inline somewhere else (after key:, after scalar, etc.)
    Inline,
}

/// YAML serializer with streaming output.
pub struct YamlSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    /// Whether we've written the document start marker
    doc_started: bool,
    /// Current position on the line
    line_pos: LinePos,
}

impl YamlSerializer {
    /// Create a new YAML serializer.
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
            doc_started: false,
            line_pos: LinePos::Start,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Ensure document has started
    fn ensure_doc_started(&mut self) {
        if !self.doc_started {
            self.out.extend_from_slice(b"---\n");
            self.doc_started = true;
            self.line_pos = LinePos::Start;
        }
    }

    /// Write indentation for a given depth.
    fn write_indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.extend_from_slice(b"  ");
        }
    }

    /// Start a new line if we're not already at line start
    fn newline(&mut self) {
        if self.line_pos != LinePos::Start {
            self.out.push(b'\n');
            self.line_pos = LinePos::Start;
        }
    }

    /// Prepare to write a sequence item.
    /// After this, we're positioned right after "- ".
    fn write_seq_item_prefix(&mut self, seq_indent: usize) {
        self.newline();
        self.write_indent(seq_indent);
        self.out.extend_from_slice(b"- ");
        self.line_pos = LinePos::AfterSeqMarker;
    }

    /// Prepare to write a struct field.
    /// Handles newline and indentation.
    fn write_field_prefix(&mut self, indent: usize) {
        self.newline();
        self.write_indent(indent);
        self.line_pos = LinePos::Inline;
    }

    /// Get the current indentation level based on context stack.
    fn current_indent(&self) -> usize {
        match self.stack.last() {
            Some(Ctx::Struct { indent, .. }) => *indent,
            Some(Ctx::Seq { indent, .. }) => *indent,
            None => 0,
        }
    }

    /// Check if a string should use block scalar syntax.
    /// Returns true for multiline strings that are suitable for literal block style.
    fn should_use_block_scalar(s: &str) -> bool {
        // Must contain at least one newline to benefit from block scalar
        if !s.contains('\n') {
            return false;
        }

        // Don't use block scalar for empty or whitespace-only strings
        if s.trim().is_empty() {
            return false;
        }

        // Avoid carriage returns - they complicate block scalar handling
        if s.contains('\r') {
            return false;
        }

        true
    }

    /// Write a string using block scalar (literal) syntax.
    /// Uses `|` for strings with trailing newline, `|-` for strings without.
    fn write_block_scalar(&mut self, s: &str, indent: usize) {
        // Determine chomping indicator:
        // - `|-` (strip): no trailing newline in output
        // - `|` (clip): single trailing newline
        // - `|+` (keep): preserve all trailing newlines
        let chomping = if s.ends_with('\n') {
            if s.ends_with("\n\n") {
                "+" // keep multiple trailing newlines
            } else {
                "" // clip: single trailing newline (default)
            }
        } else {
            "-" // strip: no trailing newline
        };

        self.out.push(b'|');
        self.out.extend_from_slice(chomping.as_bytes());

        // Write each line with proper indentation
        // For |-/|, trim trailing newlines; for |+, preserve them
        let content = if chomping == "+" {
            s.trim_end_matches('\n')
        } else if chomping == "-" {
            s
        } else {
            s.trim_end_matches('\n')
        };

        for line in content.split('\n') {
            self.out.push(b'\n');
            self.write_indent(indent + 1);
            self.out.extend_from_slice(line.as_bytes());
        }

        // For |+, add the trailing newlines
        if chomping == "+" {
            let trailing_count = s.len() - s.trim_end_matches('\n').len();
            for _ in 1..trailing_count {
                self.out.push(b'\n');
            }
        }

        self.line_pos = LinePos::Inline;
    }

    /// Check if a string needs quoting (for inline/single-line strings).
    fn needs_quotes(s: &str) -> bool {
        s.is_empty()
            || s.contains(':')
            || s.contains('#')
            || s.contains('\n')
            || s.contains('\r')
            || s.contains('"')
            || s.contains('\'')
            || s.starts_with(' ')
            || s.ends_with(' ')
            || s.starts_with('-')
            || s.starts_with('?')
            || s.starts_with('*')
            || s.starts_with('&')
            || s.starts_with('!')
            || s.starts_with('|')
            || s.starts_with('>')
            || s.starts_with('%')
            || s.starts_with('@')
            || s.starts_with('`')
            || s.starts_with('[')
            || s.starts_with('{')
            || looks_like_bool(s)
            || looks_like_null(s)
            || looks_like_number(s)
    }

    /// Write a YAML string, using block scalar for multiline or quoting if necessary.
    fn write_string(&mut self, s: &str) {
        if Self::should_use_block_scalar(s) {
            let indent = self.current_indent();
            self.write_block_scalar(s, indent);
        } else if Self::needs_quotes(s) {
            self.out.push(b'"');
            for c in s.chars() {
                match c {
                    '"' => self.out.extend_from_slice(b"\\\""),
                    '\\' => self.out.extend_from_slice(b"\\\\"),
                    '\n' => self.out.extend_from_slice(b"\\n"),
                    '\r' => self.out.extend_from_slice(b"\\r"),
                    '\t' => self.out.extend_from_slice(b"\\t"),
                    c if c.is_control() => {
                        self.out
                            .extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes());
                    }
                    c => {
                        let mut buf = [0u8; 4];
                        self.out
                            .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
            self.out.push(b'"');
            self.line_pos = LinePos::Inline;
        } else {
            self.out.extend_from_slice(s.as_bytes());
            self.line_pos = LinePos::Inline;
        }
    }
}

impl Default for YamlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for YamlSerializer {
    type Error = YamlSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.ensure_doc_started();

        // Check if we're inside a sequence - if so, this struct is a seq item
        let (struct_indent, seq_indent_for_prefix) = match self.stack.last() {
            Some(Ctx::Seq { indent, .. }) => {
                // Struct fields will be at seq_indent + 1 to align after "- "
                (*indent + 1, Some(*indent))
            }
            Some(Ctx::Struct { indent, .. }) => {
                // Nested struct after a key - indent at parent level + 1
                (*indent + 1, None)
            }
            None => {
                // Top-level struct
                (0, None)
            }
        };

        // If this is a sequence item, write the "- " prefix and mark parent seq
        if let Some(seq_indent) = seq_indent_for_prefix {
            self.write_seq_item_prefix(seq_indent);
            // Mark parent seq as having items
            if let Some(Ctx::Seq { has_items, .. }) = self.stack.last_mut() {
                *has_items = true;
            }
        }

        // has_fields starts as false - we haven't written any fields yet
        // The first field will detect via line_pos that we're inline after "- "
        self.stack.push(Ctx::Struct {
            indent: struct_indent,
            has_fields: false,
        });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        let (indent, has_fields) = match self.stack.last() {
            Some(Ctx::Struct { indent, has_fields }) => (*indent, *has_fields),
            _ => {
                return Err(YamlSerializeError::new(
                    "field_key called outside of a struct",
                ));
            }
        };

        // For the first field of a seq item struct, we're right after "- "
        // Otherwise, we need newline + indent
        if !has_fields && self.line_pos == LinePos::AfterSeqMarker {
            // First field of seq-item struct: already have "- " on this line
            // Don't write newline, just the key
        } else {
            // Normal case: newline + indent
            self.write_field_prefix(indent);
        }

        self.write_string(key);
        self.out.extend_from_slice(b": ");
        self.line_pos = LinePos::Inline;

        // Mark that we've written a field
        if let Some(Ctx::Struct { has_fields, .. }) = self.stack.last_mut() {
            *has_fields = true;
        }

        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct { has_fields, .. }) => {
                // Empty struct - write {}
                if !has_fields {
                    self.out.extend_from_slice(b"{}");
                    self.line_pos = LinePos::Inline;
                }
                Ok(())
            }
            _ => Err(YamlSerializeError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.ensure_doc_started();

        // Check if we're inside a parent sequence
        let (new_seq_indent, parent_seq_indent) = match self.stack.last() {
            Some(Ctx::Seq { indent, .. }) => {
                // Nested seq items will be at indent + 1
                (*indent + 1, Some(*indent))
            }
            Some(Ctx::Struct { indent, .. }) => {
                // Seq after a key like "tags: " - items will be indented at struct indent + 1
                (*indent + 1, None)
            }
            None => {
                // Top-level sequence
                (0, None)
            }
        };

        // If nested inside another sequence, write the "-" prefix
        if let Some(parent_indent) = parent_seq_indent {
            self.newline();
            self.write_indent(parent_indent);
            self.out.extend_from_slice(b"-");
            self.line_pos = LinePos::Inline;
            // Mark parent seq as having items
            if let Some(Ctx::Seq { has_items, .. }) = self.stack.last_mut() {
                *has_items = true;
            }
        }

        self.stack.push(Ctx::Seq {
            indent: new_seq_indent,
            has_items: false,
        });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { has_items, .. }) => {
                // Empty sequence - write []
                if !has_items {
                    self.out.extend_from_slice(b"[]");
                    self.line_pos = LinePos::Inline;
                }
                Ok(())
            }
            _ => Err(YamlSerializeError::new(
                "end_seq called without matching begin_seq",
            )),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.ensure_doc_started();

        // If we're in a sequence, write the item prefix
        let seq_indent = match self.stack.last() {
            Some(Ctx::Seq { indent, .. }) => Some(*indent),
            _ => None,
        };
        if let Some(indent) = seq_indent {
            self.write_seq_item_prefix(indent);
            // Mark seq as having items
            if let Some(Ctx::Seq { has_items, .. }) = self.stack.last_mut() {
                *has_items = true;
            }
        }

        match scalar {
            ScalarValue::Null => self.out.extend_from_slice(b"null"),
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true")
                } else {
                    self.out.extend_from_slice(b"false")
                }
            }
            ScalarValue::Char(c) => {
                let mut buf = [0u8; 4];
                self.write_string(c.encode_utf8(&mut buf));
            }
            ScalarValue::I64(v) => {
                #[cfg(feature = "fast")]
                self.out
                    .extend_from_slice(itoa::Buffer::new().format(v).as_bytes());
                #[cfg(not(feature = "fast"))]
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U64(v) => {
                #[cfg(feature = "fast")]
                self.out
                    .extend_from_slice(itoa::Buffer::new().format(v).as_bytes());
                #[cfg(not(feature = "fast"))]
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::I128(v) => {
                #[cfg(feature = "fast")]
                self.out
                    .extend_from_slice(itoa::Buffer::new().format(v).as_bytes());
                #[cfg(not(feature = "fast"))]
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U128(v) => {
                #[cfg(feature = "fast")]
                self.out
                    .extend_from_slice(itoa::Buffer::new().format(v).as_bytes());
                #[cfg(not(feature = "fast"))]
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::F64(v) => {
                #[cfg(feature = "fast")]
                self.out
                    .extend_from_slice(zmij::Buffer::new().format(v).as_bytes());
                #[cfg(not(feature = "fast"))]
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::Str(s) | ScalarValue::StringlyTyped(s) => self.write_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(YamlSerializeError::new(
                    "bytes serialization not supported for YAML",
                ));
            }
        }

        self.line_pos = LinePos::Inline;
        Ok(())
    }
}

/// Check if string looks like a boolean
fn looks_like_bool(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n"
    )
}

/// Check if string looks like null
fn looks_like_null(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "null" | "~" | "nil" | "none")
}

/// Check if string looks like a number
fn looks_like_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let s = s.trim();
    s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok()
}

// ============================================================================
// Public API
// ============================================================================

/// Serialize a value to a YAML string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml::to_string;
///
/// #[derive(Facet)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let config = Config {
///     name: "myapp".to_string(),
///     port: 8080,
/// };
///
/// let yaml = to_string(&config).unwrap();
/// assert!(yaml.contains("name: myapp"));
/// assert!(yaml.contains("port: 8080"));
/// ```
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<YamlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    Ok(String::from_utf8(bytes).expect("YAML output should always be valid UTF-8"))
}

/// Serialize a value to YAML bytes.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml::to_vec;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec(&point).unwrap();
/// assert!(!bytes.is_empty());
/// ```
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<YamlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = YamlSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    let mut output = serializer.finish();
    // Ensure trailing newline
    if !output.ends_with(b"\n") {
        output.push(b'\n');
    }
    Ok(output)
}

/// Serialize a `Peek` instance to a YAML string.
///
/// This allows serializing values without requiring ownership, useful when
/// you already have a `Peek` from reflection operations.
pub fn peek_to_string<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<String, SerializeError<YamlSerializeError>> {
    let mut serializer = YamlSerializer::new();
    serialize_root(&mut serializer, peek)?;
    let mut output = serializer.finish();
    if !output.ends_with(b"\n") {
        output.push(b'\n');
    }
    Ok(String::from_utf8(output).expect("YAML output should always be valid UTF-8"))
}

/// Serialize a value to YAML and write it to a `std::io::Write` writer.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml::to_writer;
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let mut buffer = Vec::new();
/// to_writer(&mut buffer, &person).unwrap();
/// assert!(!buffer.is_empty());
/// ```
pub fn to_writer<'facet, W, T>(writer: W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    peek_to_writer(writer, Peek::new(value))
}

/// Serialize a `Peek` instance to YAML and write it to a `std::io::Write` writer.
pub fn peek_to_writer<'input, 'facet, W>(
    mut writer: W,
    peek: Peek<'input, 'facet>,
) -> std::io::Result<()>
where
    W: std::io::Write,
{
    let mut serializer = YamlSerializer::new();
    serialize_root(&mut serializer, peek).map_err(|e| std::io::Error::other(format!("{:?}", e)))?;
    let mut output = serializer.finish();
    if !output.ends_with(b"\n") {
        output.push(b'\n');
    }
    writer.write_all(&output)
}
