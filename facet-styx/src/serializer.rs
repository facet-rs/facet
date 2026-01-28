//! Styx serialization implementation.

use std::borrow::Cow;

use crate::trace;
use facet_core::Facet;
use facet_format::{
    FieldKey, FieldLocationHint, FormatSerializer, ScalarValue, SerializeError, serialize_root,
};
use facet_reflect::{HasFields, Peek};
use styx_format::{FormatOptions, StyxWriter};

// Re-export FormatOptions as SerializeOptions for backwards compatibility
pub use styx_format::FormatOptions as SerializeOptions;

/// Extract a FieldKey from a Peek value (typically a map key).
///
/// Handles metadata containers like `Documented<ObjectKey>` by extracting
/// doc comments, tag, and the actual key name.
fn extract_field_key<'mem, 'facet>(key: Peek<'mem, 'facet>) -> Option<FieldKey<'mem>> {
    // Try to extract from metadata container
    if key.shape().is_metadata_container()
        && let Ok(container) = key.into_struct()
    {
        let mut doc_lines: Vec<Cow<'mem, str>> = Vec::new();
        let mut tag_value: Option<Cow<'mem, str>> = None;
        let mut name_value: Option<Cow<'mem, str>> = None;

        for (f, field_value) in container.fields() {
            if f.metadata_kind() == Some("doc") {
                // Extract doc lines
                if let Ok(opt) = field_value.into_option()
                    && let Some(inner) = opt.value()
                    && let Ok(list) = inner.into_list_like()
                {
                    for item in list.iter() {
                        if let Some(line) = item.as_str() {
                            doc_lines.push(Cow::Borrowed(line));
                        }
                    }
                }
            } else if f.metadata_kind() == Some("tag") {
                // Extract tag
                if let Ok(opt) = field_value.into_option()
                    && let Some(inner) = opt.value()
                    && let Some(s) = inner.as_str()
                {
                    tag_value = Some(Cow::Borrowed(s));
                }
            } else if f.metadata_kind().is_none() {
                // This is the value field - might be another metadata container
                let (inner_name, inner_tag) = extract_name_and_tag(field_value);
                if inner_name.is_some() {
                    name_value = inner_name;
                }
                if inner_tag.is_some() {
                    tag_value = inner_tag;
                }
            }
        }

        // Construct FieldKey using available constructors
        // Priority: if we have a name, use it; otherwise use tag
        return Some(match (name_value, tag_value) {
            (Some(name), _) => {
                // Name takes priority - use with_doc if we have doc lines
                FieldKey::with_doc(name, FieldLocationHint::KeyValue, doc_lines)
            }
            (None, Some(tag)) => {
                // Tag only - use tagged_with_doc
                FieldKey::tagged_with_doc(tag, FieldLocationHint::KeyValue, doc_lines)
            }
            (None, None) => {
                // Unit key with optional doc
                FieldKey::unit_with_doc(FieldLocationHint::KeyValue, doc_lines)
            }
        });
    }

    // Try Option<String> - None becomes unit key (@)
    if let Ok(opt) = key.into_option() {
        return match opt.value() {
            Some(inner) => inner
                .as_str()
                .map(|s| FieldKey::new(s, FieldLocationHint::KeyValue)),
            None => {
                // None -> unit key (@)
                Some(FieldKey::unit(FieldLocationHint::KeyValue))
            }
        };
    }

    // Try direct string
    if let Some(s) = key.as_str() {
        return Some(FieldKey::new(s, FieldLocationHint::KeyValue));
    }

    None
}

/// Extract name and tag from a value (possibly a nested metadata container).
fn extract_name_and_tag<'mem, 'facet>(
    value: Peek<'mem, 'facet>,
) -> (Option<Cow<'mem, str>>, Option<Cow<'mem, str>>) {
    // Direct string
    if let Some(s) = value.as_str() {
        return (Some(Cow::Borrowed(s)), None);
    }

    // Option<String>
    if let Ok(opt) = value.into_option() {
        return match opt.value() {
            Some(inner) => {
                if let Some(s) = inner.as_str() {
                    (Some(Cow::Borrowed(s)), None)
                } else {
                    (None, None)
                }
            }
            None => (None, None),
        };
    }

    // Nested metadata container (like ObjectKey)
    if value.shape().is_metadata_container()
        && let Ok(container) = value.into_struct()
    {
        let mut name: Option<Cow<'mem, str>> = None;
        let mut tag: Option<Cow<'mem, str>> = None;

        for (f, field_value) in container.fields() {
            if f.metadata_kind() == Some("tag") {
                if let Ok(opt) = field_value.into_option()
                    && let Some(inner) = opt.value()
                    && let Some(s) = inner.as_str()
                {
                    tag = Some(Cow::Borrowed(s));
                }
            } else if f.metadata_kind().is_none() {
                // Value field
                if let Some(s) = field_value.as_str() {
                    name = Some(Cow::Borrowed(s));
                } else if let Ok(opt) = field_value.into_option()
                    && let Some(inner) = opt.value()
                    && let Some(s) = inner.as_str()
                {
                    name = Some(Cow::Borrowed(s));
                }
            }
        }

        return (name, tag);
    }

    (None, None)
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

    /// Consume the serializer and return the output bytes, ensuring trailing newline.
    pub fn finish(self) -> Vec<u8> {
        self.writer.finish_document()
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
        trace!(is_root, "begin_struct");
        self.at_root = false;
        self.writer.begin_struct(is_root);
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        trace!(key, "field_key");
        self.writer.field_key(key).map_err(StyxSerializeError::new)
    }

    fn emit_field_key(&mut self, key: &facet_format::FieldKey<'_>) -> Result<(), Self::Error> {
        trace!(?key, "emit_field_key");

        let doc_lines: Vec<&str> = key
            .doc()
            .map(|d| d.iter().map(|s| s.as_ref()).collect())
            .unwrap_or_default();

        // Build the key based on tag and name:
        // - `@` → tag=Some(""), name=None
        // - `@tag` → tag=Some("tag"), name=None
        // - `name` → tag=None, name=Some("name")
        // - `@tag"name"` → tag=Some("tag"), name=Some("name")
        match (
            key.tag().map(|c| c.as_ref()),
            key.name().map(|c| c.as_ref()),
        ) {
            (Some(tag), Some(name)) => {
                // @tag"name" - tagged with value
                let key_str = if tag.is_empty() {
                    format!("@\"{}\"", name)
                } else {
                    format!("@{}\"{}\"", tag, name)
                };
                if !doc_lines.is_empty() {
                    self.writer
                        .write_doc_comment_and_key_raw(&doc_lines.join("\n"), &key_str);
                } else {
                    self.writer
                        .field_key_raw(&key_str)
                        .map_err(StyxSerializeError::new)?;
                }
            }
            (Some(tag), None) => {
                // @tag or @ - typed catch-all or unit
                let key_str = if tag.is_empty() {
                    "@".to_string()
                } else {
                    format!("@{}", tag)
                };
                if !doc_lines.is_empty() {
                    self.writer
                        .write_doc_comment_and_key_raw(&doc_lines.join("\n"), &key_str);
                } else {
                    self.writer
                        .field_key_raw(&key_str)
                        .map_err(StyxSerializeError::new)?;
                }
            }
            (None, Some(name)) => {
                // name - regular named field
                if !doc_lines.is_empty() {
                    self.writer
                        .write_doc_comment_and_key(&doc_lines.join("\n"), name);
                } else {
                    self.writer
                        .field_key(name)
                        .map_err(StyxSerializeError::new)?;
                }
            }
            (None, None) => {
                // Shouldn't happen, but fall back to @
                if !doc_lines.is_empty() {
                    self.writer
                        .write_doc_comment_and_key_raw(&doc_lines.join("\n"), "@");
                } else {
                    self.writer
                        .field_key_raw("@")
                        .map_err(StyxSerializeError::new)?;
                }
            }
        }
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        trace!("end_struct");
        self.writer.end_struct().map_err(StyxSerializeError::new)
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        trace!("begin_seq");
        self.at_root = false;
        self.writer.begin_seq();
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        trace!("end_seq");
        self.writer.end_seq().map_err(StyxSerializeError::new)
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        trace!(?scalar, "scalar");
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
        trace!(just_wrote_tag = self.just_wrote_tag, "serialize_none");
        // If we just wrote a tag, skip the None payload (e.g., @string instead of @string@)
        if self.just_wrote_tag {
            self.just_wrote_tag = false;
            // Clear the skip flag so the next element gets proper spacing
            self.writer.clear_skip_before_value();
            return Ok(());
        }
        self.at_root = false;
        self.writer.write_null();
        Ok(())
    }

    fn write_variant_tag(&mut self, variant_name: &str) -> Result<bool, Self::Error> {
        trace!(variant_name, "write_variant_tag");
        self.at_root = false;
        self.just_wrote_tag = true;
        self.writer.write_tag(variant_name);
        Ok(true)
    }

    fn begin_struct_after_tag(&mut self) -> Result<(), Self::Error> {
        trace!("begin_struct_after_tag");
        self.just_wrote_tag = false;
        self.writer.begin_struct_after_tag(false);
        Ok(())
    }

    fn begin_seq_after_tag(&mut self) -> Result<(), Self::Error> {
        trace!("begin_seq_after_tag");
        self.just_wrote_tag = false;
        self.writer.begin_seq_after_tag();
        Ok(())
    }

    fn raw_serialize_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawStyx::SHAPE)
    }

    fn raw_scalar(&mut self, content: &str) -> Result<(), Self::Error> {
        trace!(content, "raw_scalar");
        // For RawStyx, output the content directly without quoting
        self.at_root = false;
        self.just_wrote_tag = false;
        self.writer.before_value();
        self.writer.write_str(content);
        Ok(())
    }

    fn serialize_map_key(&mut self, key: Peek<'_, '_>) -> Result<bool, Self::Error> {
        trace!(shape = key.shape().type_identifier, "serialize_map_key");

        // Try to extract a FieldKey from the map key
        if let Some(field_key) = extract_field_key(key) {
            trace!(?field_key, "serialize_map_key: extracted FieldKey");
            self.emit_field_key(&field_key)?;
            return Ok(true);
        }

        // Fall back to default behavior for other key types
        trace!("serialize_map_key: falling back to default");
        Ok(false)
    }

    fn field_metadata_with_value(
        &mut self,
        field_item: &facet_reflect::FieldItem,
        value: Peek<'_, '_>,
    ) -> Result<bool, Self::Error> {
        let is_metadata_container = value.shape().is_metadata_container();
        trace!(
            field_name = field_item.effective_name(),
            is_metadata_container,
            value_shape = value.shape().type_identifier,
            "field_metadata_with_value"
        );

        // First, check if the field value is a metadata container (like Documented<T>)
        // This takes precedence over Field::doc since it's runtime data
        if is_metadata_container && let Ok(container) = value.into_struct() {
            // Collect doc lines from the metadata container
            let mut doc_lines: Vec<&str> = Vec::new();
            for (f, field_value) in container.fields() {
                trace!(
                    metadata_kind = ?f.metadata_kind(),
                    field = f.effective_name(),
                    "field_metadata_with_value: inspecting container field"
                );
                if f.metadata_kind() == Some("doc")
                    && let Ok(opt) = field_value.into_option()
                    && let Some(inner) = opt.value()
                    && let Ok(list) = inner.into_list_like()
                {
                    for item in list.iter() {
                        if let Some(line) = item.as_str() {
                            doc_lines.push(line);
                        }
                    }
                }
            }

            // If we have doc lines from the container, use them
            if !doc_lines.is_empty() {
                trace!(doc_lines = ?doc_lines, "field_metadata_with_value: emitting doc comment");
                let doc = doc_lines.join("\n");
                self.writer
                    .write_doc_comment_and_key(&doc, field_item.effective_name());
                return Ok(true);
            }
        }

        // Note: We intentionally do NOT emit Field::doc (Rust doc comments) when serializing
        // regular values. Doc comments should only be emitted when:
        // 1. The field value is a metadata container (like Documented<T>) - checked above
        // 2. When serializing schemas (where we use Documented<Schema> to carry the docs)

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
        // Compact mode is for inline embedding - no trailing newline
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

    #[test]
    fn test_doc_metadata_field() {
        use crate::schema_types::Documented;

        // A struct with documented fields
        #[derive(Facet, Debug)]
        struct Config {
            name: Documented<String>,
            port: Documented<u16>,
        }

        let config = Config {
            name: Documented::with_doc_line("myapp".into(), "The application name"),
            port: Documented::with_doc_line(8080, "Port to listen on"),
        };

        let serialized = to_string(&config).unwrap();

        // Doc comments should appear before the field key
        assert!(serialized.contains("/// The application name\nname myapp"));
        assert!(serialized.contains("/// Port to listen on\nport 8080"));
    }

    #[test]
    fn test_field_doc_comments_not_emitted_for_regular_values() {
        // Doc comments on Rust fields should NOT be emitted when serializing regular values.
        // Only Documented<T> (metadata containers) should emit doc comments.
        #[derive(Facet, Debug)]
        struct Server {
            /// The hostname to bind to
            host: String,
            /// The port number (1-65535)
            port: u16,
        }

        let server = Server {
            host: "localhost".into(),
            port: 8080,
        };

        let serialized = to_string(&server).unwrap();

        // Doc comments should NOT appear - we're serializing a value, not a schema
        assert!(!serialized.contains("///"));
        assert!(serialized.contains("host localhost"));
        assert!(serialized.contains("port 8080"));
    }

    #[test]
    fn test_hashmap_with_documented_keys_serialize() {
        use crate::schema_types::Documented;
        use std::collections::HashMap;

        // A HashMap with Documented keys - doc comments are attached to keys, not values
        let mut map: HashMap<Documented<String>, i32> = HashMap::new();
        map.insert(
            Documented::with_doc_line("port".to_string(), "The port to listen on"),
            8080,
        );
        map.insert(
            Documented::with_doc_line("timeout".to_string(), "Timeout in seconds"),
            30,
        );

        let serialized = to_string(&map).unwrap();
        tracing::debug!("Serialized HashMap:\n{}", serialized);

        // Doc comments should appear before each key
        assert!(serialized.contains("/// The port to listen on\nport 8080"));
        assert!(serialized.contains("/// Timeout in seconds\ntimeout 30"));
    }

    #[test]
    fn test_hashmap_with_documented_keys_roundtrip() {
        use crate::schema_types::Documented;
        use std::collections::HashMap;

        // Parse a styx document with doc comments into HashMap<Documented<String>, i32>
        let input = r#"
/// The port to listen on
port 8080
/// Timeout in seconds
timeout 30
"#;

        let parsed: HashMap<Documented<String>, i32> =
            crate::from_str(input).expect("should parse");

        tracing::debug!("Parsed HashMap: {:?}", parsed);

        // Check we got the right values
        assert_eq!(
            parsed.get(&Documented::new("port".to_string())),
            Some(&8080)
        );
        assert_eq!(
            parsed.get(&Documented::new("timeout".to_string())),
            Some(&30)
        );

        // Check we got the doc comments
        let port_key = parsed
            .keys()
            .find(|k| k.value == "port")
            .expect("should have port key");
        assert_eq!(
            port_key.doc(),
            Some(&["The port to listen on".to_string()][..])
        );

        let timeout_key = parsed
            .keys()
            .find(|k| k.value == "timeout")
            .expect("should have timeout key");
        assert_eq!(
            timeout_key.doc(),
            Some(&["Timeout in seconds".to_string()][..])
        );
    }
}
