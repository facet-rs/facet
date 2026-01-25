extern crate alloc;

use alloc::{string::String, vec::Vec};

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// Options for JSON serialization.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false)
    pub pretty: bool,

    /// Indentation string for pretty-printing (default: "  ")
    pub indent: &'static str,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: "  ",
        }
    }
}

impl SerializeOptions {
    /// Create new default options (compact output).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable pretty-printing with default indentation.
    pub const fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set a custom indentation string (implies pretty-printing).
    pub const fn indent(mut self, indent: &'static str) -> Self {
        self.indent = indent;
        self.pretty = true;
        self
    }
}

#[derive(Debug)]
pub struct JsonSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for JsonSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for JsonSerializeError {}

#[derive(Debug, Clone, Copy)]
enum Ctx {
    Struct { first: bool },
    Seq { first: bool },
}

/// JSON serializer with configurable formatting options.
pub struct JsonSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    options: SerializeOptions,
}

impl JsonSerializer {
    /// Create a new JSON serializer with default (compact) options.
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new JSON serializer with the given options.
    pub const fn with_options(options: SerializeOptions) -> Self {
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

    /// Current nesting depth (for indentation).
    const fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Write a newline and indentation if in pretty mode.
    fn write_indent(&mut self) {
        if self.options.pretty {
            self.out.push(b'\n');
            for _ in 0..self.depth() {
                self.out.extend_from_slice(self.options.indent.as_bytes());
            }
        }
    }

    fn before_value(&mut self) -> Result<(), JsonSerializeError> {
        match self.stack.last_mut() {
            Some(Ctx::Seq { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
                self.write_indent();
            }
            Some(Ctx::Struct { .. }) => {
                // struct values are separated by `field_key`
            }
            None => {}
        }
        Ok(())
    }

    /// Optimized JSON string writing with SIMD-like 16-byte fast path.
    ///
    /// For ASCII strings without special characters, processes 16 bytes at a time.
    /// Falls back to character-by-character escaping when needed.
    fn write_json_string(&mut self, s: &str) {
        const STEP_SIZE: usize = 16; // u128 = 16 bytes
        type Chunk = [u8; STEP_SIZE];

        self.out.push(b'"');

        let mut s = s;
        while let Some(Ok(chunk)) = s.as_bytes().get(..STEP_SIZE).map(Chunk::try_from) {
            let window = u128::from_ne_bytes(chunk);
            // Check all 16 bytes at once:
            // 1. All ASCII (high bit clear): window & 0x80...80 == 0
            // 2. No quotes (0x22): !contains_byte(window, 0x22)
            // 3. No backslashes (0x5c): !contains_byte(window, 0x5c)
            // 4. No control chars (< 0x20): top 3 bits set for all bytes
            let completely_ascii = window & 0x80808080808080808080808080808080 == 0;
            let quote_free = !contains_byte(window, 0x22);
            let backslash_free = !contains_byte(window, 0x5c);
            let control_char_free = no_control_chars(window);

            if completely_ascii && quote_free && backslash_free && control_char_free {
                // Fast path: copy 16 bytes directly
                self.out.extend_from_slice(&chunk);
                s = &s[STEP_SIZE..];
            } else {
                // Slow path: escape character by character for this chunk
                let mut chars = s.chars();
                let mut count = STEP_SIZE;
                for c in &mut chars {
                    self.write_json_escaped_char(c);
                    count = count.saturating_sub(c.len_utf8());
                    if count == 0 {
                        break;
                    }
                }
                s = chars.as_str();
            }
        }

        // Handle remaining bytes (< 16)
        for c in s.chars() {
            self.write_json_escaped_char(c);
        }

        self.out.push(b'"');
    }

    #[inline]
    fn write_json_escaped_char(&mut self, c: char) {
        match c {
            '"' => self.out.extend_from_slice(b"\\\""),
            '\\' => self.out.extend_from_slice(b"\\\\"),
            '\n' => self.out.extend_from_slice(b"\\n"),
            '\r' => self.out.extend_from_slice(b"\\r"),
            '\t' => self.out.extend_from_slice(b"\\t"),
            '\u{08}' => self.out.extend_from_slice(b"\\b"),
            '\u{0C}' => self.out.extend_from_slice(b"\\f"),
            c if c.is_ascii_control() => {
                let code_point = c as u32;
                let to_hex = |d: u32| {
                    if d < 10 {
                        b'0' + d as u8
                    } else {
                        b'a' + (d - 10) as u8
                    }
                };
                let buf = [
                    b'\\',
                    b'u',
                    to_hex((code_point >> 12) & 0xF),
                    to_hex((code_point >> 8) & 0xF),
                    to_hex((code_point >> 4) & 0xF),
                    to_hex(code_point & 0xF),
                ];
                self.out.extend_from_slice(&buf);
            }
            c if c.is_ascii() => {
                self.out.push(c as u8);
            }
            c => {
                let mut buf = [0u8; 4];
                let len = c.encode_utf8(&mut buf).len();
                self.out.extend_from_slice(&buf[..len]);
            }
        }
    }
}

/// Check if any byte in the u128 equals the target byte.
/// Uses the SWAR (SIMD Within A Register) technique.
#[inline]
const fn contains_byte(val: u128, byte: u8) -> bool {
    let mask = 0x01010101010101010101010101010101u128 * (byte as u128);
    let xor_result = val ^ mask;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

/// Check that all bytes have at least one of the top 3 bits set (i.e., >= 0x20).
/// This means no control characters (0x00-0x1F).
#[inline]
const fn no_control_chars(value: u128) -> bool {
    let masked = value & 0xe0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0;
    let has_zero = (masked.wrapping_sub(0x01010101010101010101010101010101))
        & !masked
        & 0x80808080808080808080808080808080;
    has_zero == 0
}

impl Default for JsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for JsonSerializer {
    type Error = JsonSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.before_value()?;
        self.out.push(b'{');
        self.stack.push(Ctx::Struct { first: true });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        match self.stack.last_mut() {
            Some(Ctx::Struct { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
                self.write_indent();
                self.write_json_string(key);
                self.out.push(b':');
                if self.options.pretty {
                    self.out.push(b' ');
                }
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "field_key called outside of a struct",
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct { first }) => {
                // Only add newline/indent before closing brace if struct was non-empty
                if !first {
                    self.write_indent();
                }
                self.out.push(b'}');
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "end_struct called without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.before_value()?;
        self.out.push(b'[');
        self.stack.push(Ctx::Seq { first: true });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { first }) => {
                // Only add newline/indent before closing bracket if seq was non-empty
                if !first {
                    self.write_indent();
                }
                self.out.push(b']');
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "end_seq called without matching begin_seq",
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.before_value()?;
        match scalar {
            ScalarValue::Null | ScalarValue::Unit => self.out.extend_from_slice(b"null"),
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true")
                } else {
                    self.out.extend_from_slice(b"false")
                }
            }
            ScalarValue::Char(c) => {
                self.out.push(b'"');
                self.write_json_escaped_char(c);
                self.out.push(b'"');
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
                if v.is_nan() || v.is_infinite() {
                    self.out.extend_from_slice(b"null");
                } else {
                    #[cfg(feature = "fast")]
                    self.out
                        .extend_from_slice(zmij::Buffer::new().format(v).as_bytes());
                    #[cfg(not(feature = "fast"))]
                    self.out.extend_from_slice(v.to_string().as_bytes());
                }
            }
            ScalarValue::Str(s) => self.write_json_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(JsonSerializeError {
                    msg: "bytes serialization unsupported for json",
                });
            }
        }
        Ok(())
    }

    fn raw_serialize_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawJson::SHAPE)
    }

    fn raw_scalar(&mut self, content: &str) -> Result<(), Self::Error> {
        // For RawJson, output the content directly without escaping
        self.before_value()?;
        self.out.extend_from_slice(content.as_bytes());
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("json")
    }
}

/// Serialize a value to JSON bytes.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_vec;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec(&point).unwrap();
/// assert_eq!(bytes, br#"{"x":10,"y":20}"#);
/// ```
pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to pretty-printed JSON bytes.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_vec_pretty;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec_pretty(&point).unwrap();
/// assert!(bytes.contains(&b'\n'));
/// ```
pub fn to_vec_pretty<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to JSON bytes with custom options.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::{to_vec_with_options, SerializeOptions};
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
///
/// // Compact output
/// let bytes = to_vec_with_options(&point, &SerializeOptions::default()).unwrap();
/// assert_eq!(bytes, br#"{"x":10,"y":20}"#);
///
/// // Pretty output with tabs
/// let bytes = to_vec_with_options(&point, &SerializeOptions::default().indent("\t")).unwrap();
/// assert!(bytes.contains(&b'\n'));
/// ```
pub fn to_vec_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = JsonSerializer::with_options(options.clone());
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to a JSON string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_string;
///
/// #[derive(Facet)]
/// struct Person { name: String, age: u32 }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let json = to_string(&person).unwrap();
/// assert_eq!(json, r#"{"name":"Alice","age":30}"#);
/// ```
pub fn to_string<'facet, T>(value: &'_ T) -> Result<String, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    // JSON output is always valid UTF-8, so this unwrap is safe
    Ok(String::from_utf8(bytes).expect("JSON output should always be valid UTF-8"))
}

/// Serialize a value to a pretty-printed JSON string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_string_pretty;
///
/// #[derive(Facet)]
/// struct Person { name: String, age: u32 }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let json = to_string_pretty(&person).unwrap();
/// assert!(json.contains('\n'));
/// ```
pub fn to_string_pretty<'facet, T>(
    value: &'_ T,
) -> Result<String, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec_pretty(value)?;
    Ok(String::from_utf8(bytes).expect("JSON output should always be valid UTF-8"))
}

/// Serialize a value to a JSON string with custom options.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::{to_string_with_options, SerializeOptions};
///
/// #[derive(Facet)]
/// struct Person { name: String, age: u32 }
///
/// let person = Person { name: "Alice".into(), age: 30 };
///
/// // Compact output
/// let json = to_string_with_options(&person, &SerializeOptions::default()).unwrap();
/// assert_eq!(json, r#"{"name":"Alice","age":30}"#);
///
/// // Pretty output with tabs
/// let json = to_string_with_options(&person, &SerializeOptions::default().indent("\t")).unwrap();
/// assert!(json.contains('\n'));
/// ```
pub fn to_string_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec_with_options(value, options)?;
    Ok(String::from_utf8(bytes).expect("JSON output should always be valid UTF-8"))
}

// ── Peek-based serialization ──

/// Serialize a `Peek` instance to a JSON string.
///
/// This allows serializing values without requiring ownership, useful when
/// you already have a `Peek` from reflection operations.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_reflect::Peek;
/// use facet_json::peek_to_string;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let json = peek_to_string(Peek::new(&point)).unwrap();
/// assert_eq!(json, r#"{"x":10,"y":20}"#);
/// ```
pub fn peek_to_string<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<String, SerializeError<JsonSerializeError>> {
    peek_to_string_with_options(peek, &SerializeOptions::default())
}

/// Serialize a `Peek` instance to a pretty-printed JSON string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_reflect::Peek;
/// use facet_json::peek_to_string_pretty;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let json = peek_to_string_pretty(Peek::new(&point)).unwrap();
/// assert!(json.contains('\n'));
/// ```
pub fn peek_to_string_pretty<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<String, SerializeError<JsonSerializeError>> {
    peek_to_string_with_options(peek, &SerializeOptions::default().pretty())
}

/// Serialize a `Peek` instance to a JSON string with custom options.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_reflect::Peek;
/// use facet_json::{peek_to_string_with_options, SerializeOptions};
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let json = peek_to_string_with_options(
///     Peek::new(&point),
///     &SerializeOptions::default().indent("\t"),
/// ).unwrap();
/// assert!(json.contains('\n'));
/// ```
pub fn peek_to_string_with_options<'input, 'facet>(
    peek: Peek<'input, 'facet>,
    options: &SerializeOptions,
) -> Result<String, SerializeError<JsonSerializeError>> {
    let mut serializer = JsonSerializer::with_options(options.clone());
    serialize_root(&mut serializer, peek)?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("JSON output should always be valid UTF-8"))
}

// ── Writer-based serialization (std::io::Write) ──

/// Serialize a value to JSON and write it to a `std::io::Write` writer.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_writer_std;
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let mut buffer = Vec::new();
/// to_writer_std(&mut buffer, &person).unwrap();
/// assert_eq!(buffer, br#"{"name":"Alice","age":30}"#);
/// ```
pub fn to_writer_std<'facet, W, T>(writer: W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    peek_to_writer_std(writer, Peek::new(value))
}

/// Serialize a value to pretty-printed JSON and write it to a `std::io::Write` writer.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::to_writer_std_pretty;
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let mut buffer = Vec::new();
/// to_writer_std_pretty(&mut buffer, &person).unwrap();
/// assert!(String::from_utf8_lossy(&buffer).contains('\n'));
/// ```
pub fn to_writer_std_pretty<'facet, W, T>(writer: W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    peek_to_writer_std_pretty(writer, Peek::new(value))
}

/// Serialize a value to JSON with custom options and write it to a `std::io::Write` writer.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::{to_writer_std_with_options, SerializeOptions};
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person = Person { name: "Alice".into(), age: 30 };
///
/// // Compact output
/// let mut buffer = Vec::new();
/// to_writer_std_with_options(&mut buffer, &person, &SerializeOptions::default()).unwrap();
/// assert_eq!(buffer, br#"{"name":"Alice","age":30}"#);
///
/// // Pretty output with tabs
/// let mut buffer = Vec::new();
/// to_writer_std_with_options(&mut buffer, &person, &SerializeOptions::default().indent("\t")).unwrap();
/// assert!(String::from_utf8_lossy(&buffer).contains('\n'));
/// ```
pub fn to_writer_std_with_options<'facet, W, T>(
    writer: W,
    value: &T,
    options: &SerializeOptions,
) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    peek_to_writer_std_with_options(writer, Peek::new(value), options)
}

/// Serialize a `Peek` instance to JSON and write it to a `std::io::Write` writer.
pub fn peek_to_writer_std<'input, 'facet, W>(
    writer: W,
    peek: Peek<'input, 'facet>,
) -> std::io::Result<()>
where
    W: std::io::Write,
{
    peek_to_writer_std_with_options(writer, peek, &SerializeOptions::default())
}

/// Serialize a `Peek` instance to pretty-printed JSON and write it to a `std::io::Write` writer.
pub fn peek_to_writer_std_pretty<'input, 'facet, W>(
    writer: W,
    peek: Peek<'input, 'facet>,
) -> std::io::Result<()>
where
    W: std::io::Write,
{
    peek_to_writer_std_with_options(writer, peek, &SerializeOptions::default().pretty())
}

/// Serialize a `Peek` instance to JSON with custom options and write it to a `std::io::Write` writer.
pub fn peek_to_writer_std_with_options<'input, 'facet, W>(
    mut writer: W,
    peek: Peek<'input, 'facet>,
    options: &SerializeOptions,
) -> std::io::Result<()>
where
    W: std::io::Write,
{
    // Serialize to bytes first, then write
    // This is simpler and avoids the complexity of incremental writes
    let mut serializer = JsonSerializer::with_options(options.clone());
    serialize_root(&mut serializer, peek)
        .map_err(|e| std::io::Error::other(alloc::format!("{:?}", e)))?;
    writer.write_all(&serializer.finish())
}
