//! Styx serialization implementation.

use std::borrow::Cow;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;
use styx_format::{FormatOptions, StyxWriter};

// Re-export FormatOptions as SerializeOptions for backwards compatibility
pub use styx_format::FormatOptions as SerializeOptions;

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

/// Styx serializer with configurable formatting options.
pub struct StyxSerializer {
    writer: StyxWriter,
    /// Track if we're at root level (for struct unwrapping)
    at_root: bool,
    /// Track if we just wrote a variant tag (to skip None payload)
    just_wrote_tag: bool,
}

impl StyxSerializer {
    /// Create a new Styx serializer with default options.
    pub fn new() -> Self {
        Self::with_options(FormatOptions::default())
    }

    /// Create a new Styx serializer with the given options.
    pub fn with_options(options: FormatOptions) -> Self {
        Self {
            writer: StyxWriter::with_options(options),
            at_root: true,
            just_wrote_tag: false,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.writer.finish()
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
        let is_root = self.at_root;
        self.at_root = false;
        self.writer.begin_struct(is_root);
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.writer.field_key(key).map_err(StyxSerializeError::new)
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        self.writer.end_struct().map_err(StyxSerializeError::new)
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.at_root = false;
        self.writer.begin_seq();
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.writer.end_seq().map_err(StyxSerializeError::new)
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.at_root = false;
        self.just_wrote_tag = false;
        match scalar {
            ScalarValue::Unit | ScalarValue::Null => self.writer.write_null(),
            ScalarValue::Bool(v) => self.writer.write_bool(v),
            ScalarValue::Char(c) => self.writer.write_char(c),
            ScalarValue::I64(v) => self.writer.write_i64(v),
            ScalarValue::U64(v) => self.writer.write_u64(v),
            ScalarValue::I128(v) => self.writer.write_i128(v),
            ScalarValue::U128(v) => self.writer.write_u128(v),
            ScalarValue::F64(v) => self.writer.write_f64(v),
            ScalarValue::Str(s) => self.writer.write_string(&s),
            ScalarValue::Bytes(bytes) => self.writer.write_bytes(&bytes),
        }
        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        // If we just wrote a tag, skip the None payload (e.g., @string instead of @string@)
        if self.just_wrote_tag {
            self.just_wrote_tag = false;
            return Ok(());
        }
        self.at_root = false;
        self.writer.write_null();
        Ok(())
    }

    fn write_variant_tag(&mut self, variant_name: &str) -> Result<bool, Self::Error> {
        self.at_root = false;
        self.just_wrote_tag = true;
        self.writer.write_tag(variant_name);
        Ok(true)
    }

    fn begin_struct_after_tag(&mut self) -> Result<(), Self::Error> {
        self.just_wrote_tag = false;
        self.writer.begin_struct_after_tag(false);
        Ok(())
    }

    fn begin_seq_after_tag(&mut self) -> Result<(), Self::Error> {
        self.just_wrote_tag = false;
        self.writer.begin_seq_after_tag();
        Ok(())
    }

    fn raw_serialize_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawStyx::SHAPE)
    }

    fn raw_scalar(&mut self, content: &str) -> Result<(), Self::Error> {
        // For RawStyx, output the content directly without quoting
        self.at_root = false;
        self.just_wrote_tag = false;
        self.writer.before_value();
        self.writer.write_str(content);
        Ok(())
    }

    fn serialize_map_key(&mut self, key: Peek<'_, '_>) -> Result<bool, Self::Error> {
        // Handle Option<String> keys specially: None becomes @ (unit)
        if let Ok(opt) = key.into_option() {
            match opt.value() {
                Some(inner) => {
                    // Some(string) - use the string as the key
                    if let Some(s) = inner.as_str() {
                        self.writer.field_key(s).map_err(StyxSerializeError::new)?;
                        return Ok(true);
                    }
                }
                None => {
                    // None - write @ as a raw key
                    self.writer
                        .field_key_raw("@")
                        .map_err(StyxSerializeError::new)?;
                    return Ok(true);
                }
            }
        }
        // Fall back to default behavior for other key types
        Ok(false)
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
    to_string_with_options(value, &FormatOptions::default())
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
    let options = FormatOptions::default().inline();
    let mut serializer = CompactStyxSerializer::with_options(options);
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

/// Serialize a value to a Styx string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &T,
    options: &FormatOptions,
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
    peek_to_string_with_options(peek, &FormatOptions::default())
}

/// Serialize a `Peek` instance to a Styx string with custom options.
pub fn peek_to_string_with_options<'input, 'facet>(
    peek: Peek<'input, 'facet>,
    options: &FormatOptions,
) -> Result<String, SerializeError<StyxSerializeError>> {
    let mut serializer = StyxSerializer::with_options(options.clone());
    serialize_root(&mut serializer, peek)?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

/// Serialize a `Peek` instance to a Styx expression string.
///
/// Unlike `peek_to_string`, this always wraps objects in braces `{}`,
/// making it suitable for embedding as a value within a larger document.
pub fn peek_to_string_expr<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<String, SerializeError<StyxSerializeError>> {
    let options = FormatOptions::default().inline();
    let mut serializer = CompactStyxSerializer::with_options(options);
    serialize_root(&mut serializer, peek)?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Styx output should always be valid UTF-8"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Compact serializer (always uses braces, never unwraps root)
// ─────────────────────────────────────────────────────────────────────────────

/// A variant of StyxSerializer that always wraps in braces (for compact mode).
struct CompactStyxSerializer {
    writer: StyxWriter,
}

impl CompactStyxSerializer {
    fn with_options(options: FormatOptions) -> Self {
        Self {
            writer: StyxWriter::with_options(options),
        }
    }

    fn finish(self) -> Vec<u8> {
        self.writer.finish()
    }
}

impl FormatSerializer for CompactStyxSerializer {
    type Error = StyxSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // Never treat as root in compact mode
        self.writer.begin_struct(false);
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.writer.field_key(key).map_err(StyxSerializeError::new)
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        self.writer.end_struct().map_err(StyxSerializeError::new)
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.writer.begin_seq();
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.writer.end_seq().map_err(StyxSerializeError::new)
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        match scalar {
            ScalarValue::Unit | ScalarValue::Null => self.writer.write_null(),
            ScalarValue::Bool(v) => self.writer.write_bool(v),
            ScalarValue::Char(c) => self.writer.write_char(c),
            ScalarValue::I64(v) => self.writer.write_i64(v),
            ScalarValue::U64(v) => self.writer.write_u64(v),
            ScalarValue::I128(v) => self.writer.write_i128(v),
            ScalarValue::U128(v) => self.writer.write_u128(v),
            ScalarValue::F64(v) => self.writer.write_f64(v),
            ScalarValue::Str(s) => self.writer.write_string(&s),
            ScalarValue::Bytes(bytes) => self.writer.write_bytes(&bytes),
        }
        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.writer.write_null();
        Ok(())
    }

    fn write_variant_tag(&mut self, variant_name: &str) -> Result<bool, Self::Error> {
        self.writer.write_tag(variant_name);
        Ok(true)
    }

    fn begin_struct_after_tag(&mut self) -> Result<(), Self::Error> {
        self.writer.begin_struct_after_tag(false);
        Ok(())
    }

    fn begin_seq_after_tag(&mut self) -> Result<(), Self::Error> {
        self.writer.begin_seq_after_tag();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_testhelpers::test;

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
        use styx_format::can_be_bare;

        // These should be bare
        assert!(can_be_bare("localhost"));
        assert!(can_be_bare("8080"));
        assert!(can_be_bare("hello-world"));
        assert!(can_be_bare("https://example.com/path"));

        // These must be quoted
        assert!(!can_be_bare("")); // empty
        assert!(!can_be_bare("hello world")); // space
        assert!(!can_be_bare("{braces}")); // braces
        assert!(!can_be_bare("(parens)")); // parens
        assert!(!can_be_bare("key=value")); // equals
        assert!(!can_be_bare("@tag")); // at sign
        assert!(!can_be_bare("//comment")); // looks like comment
        assert!(!can_be_bare("r#raw")); // looks like raw string
        assert!(!can_be_bare("<<HERE")); // looks like heredoc
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

    #[test]
    fn test_peek_to_string_expr_wraps_objects() {
        // Expression mode should always wrap objects in braces
        let value = Simple {
            name: "test".into(),
            value: 42,
        };
        let peek = Peek::new(&value);
        let result = peek_to_string_expr(peek).unwrap();

        // Should have braces (unlike document mode which omits them for root)
        assert!(
            result.starts_with('{'),
            "expression should start with brace: {}",
            result
        );
        assert!(
            result.ends_with('}'),
            "expression should end with brace: {}",
            result
        );
        assert!(result.contains("name test"));
        assert!(result.contains("value 42"));
    }

    #[test]
    fn test_peek_to_string_expr_nested() {
        // Nested objects should also have braces
        let value = Nested {
            inner: Simple {
                name: "nested".into(),
                value: 123,
            },
        };
        let peek = Peek::new(&value);
        let result = peek_to_string_expr(peek).unwrap();

        assert!(result.starts_with('{'));
        assert!(result.contains("inner {"));
    }

    #[test]
    fn test_peek_to_string_expr_scalar() {
        // Scalars should just be the value
        let value: i32 = 42;
        let peek = Peek::new(&value);
        let result = peek_to_string_expr(peek).unwrap();
        assert_eq!(result, "42");
    }
}
