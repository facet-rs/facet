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
    Struct { first: bool, indent: usize },
    /// In a sequence/list
    Seq { first: bool, indent: usize },
}

/// YAML serializer with streaming output.
pub struct YamlSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    /// Whether we've written the document start marker
    doc_started: bool,
    /// Whether the next value should be inline (after a key)
    inline_next: bool,
}

impl YamlSerializer {
    /// Create a new YAML serializer.
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
            doc_started: false,
            inline_next: false,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Current nesting depth (for indentation).
    fn depth(&self) -> usize {
        self.stack
            .last()
            .map(|ctx| match ctx {
                Ctx::Struct { indent, .. } => *indent,
                Ctx::Seq { indent, .. } => *indent,
            })
            .unwrap_or(0)
    }

    /// Check if a string needs quoting.
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

    /// Write a YAML string, quoting if necessary.
    fn write_string(&mut self, s: &str) {
        if Self::needs_quotes(s) {
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
        } else {
            self.out.extend_from_slice(s.as_bytes());
        }
    }

    /// Write indentation for a given depth.
    fn write_indent_for(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.extend_from_slice(b"  ");
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
        // Write document start marker on first content
        if !self.doc_started {
            self.out.extend_from_slice(b"---\n");
            self.doc_started = true;
        }

        let new_indent = self.depth();

        // If we're inline (after a key:), we need a newline before struct content
        if self.inline_next {
            self.out.push(b'\n');
            self.inline_next = false;
        }

        self.stack.push(Ctx::Struct {
            first: true,
            indent: new_indent,
        });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        // Get current state
        let (first, indent) = match self.stack.last() {
            Some(Ctx::Struct { first, indent }) => (*first, *indent),
            _ => {
                return Err(YamlSerializeError::new(
                    "field_key called outside of a struct",
                ));
            }
        };

        if !first {
            self.out.push(b'\n');
        }

        // Write indentation
        self.write_indent_for(indent);

        self.write_string(key);
        self.out.extend_from_slice(b": ");
        self.inline_next = true;

        // Update state
        if let Some(Ctx::Struct {
            first: f,
            indent: i,
        }) = self.stack.last_mut()
        {
            *f = false;
            *i = indent + 1;
        }

        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct { first, .. }) => {
                // Empty struct - write {}
                if first {
                    if self.inline_next {
                        self.inline_next = false;
                    }
                    self.out.extend_from_slice(b"{}");
                }

                // Restore parent indent
                if let Some(Ctx::Struct { indent, .. }) = self.stack.last_mut() {
                    *indent = indent.saturating_sub(1);
                }

                Ok(())
            }
            _ => Err(YamlSerializeError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        // Write document start marker on first content
        if !self.doc_started {
            self.out.extend_from_slice(b"---\n");
            self.doc_started = true;
        }

        let new_indent = self.depth();

        // If we're inline (after a key:), we need a newline before sequence content
        if self.inline_next {
            self.out.push(b'\n');
            self.inline_next = false;
        }

        self.stack.push(Ctx::Seq {
            first: true,
            indent: new_indent,
        });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { first, .. }) => {
                // Empty sequence - write []
                if first {
                    if self.inline_next {
                        self.inline_next = false;
                    }
                    self.out.extend_from_slice(b"[]");
                }

                // Restore parent indent
                if let Some(Ctx::Struct { indent, .. }) = self.stack.last_mut() {
                    *indent = indent.saturating_sub(1);
                }

                Ok(())
            }
            _ => Err(YamlSerializeError::new(
                "end_seq called without matching begin_seq",
            )),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        // Write document start marker on first content
        if !self.doc_started {
            self.out.extend_from_slice(b"---\n");
            self.doc_started = true;
        }

        // Handle sequence item prefix
        if let Some(Ctx::Seq { first, indent }) = self.stack.last_mut() {
            if !*first {
                self.out.push(b'\n');
            }
            *first = false;

            // Write indentation
            let indent_val = *indent;
            self.write_indent_for(indent_val);
            self.out.extend_from_slice(b"- ");
        }

        self.inline_next = false;

        match scalar {
            ScalarValue::Null => self.out.extend_from_slice(b"null"),
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true")
                } else {
                    self.out.extend_from_slice(b"false")
                }
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
            ScalarValue::Str(s) => self.write_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(YamlSerializeError::new(
                    "bytes serialization not supported for YAML",
                ));
            }
        }

        // Restore parent indent after scalar in struct
        if let Some(Ctx::Struct { indent, .. }) = self.stack.last_mut() {
            *indent = indent.saturating_sub(1);
        }

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
/// use facet_format_yaml::to_string;
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
/// use facet_format_yaml::to_vec;
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
/// use facet_format_yaml::to_writer;
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
