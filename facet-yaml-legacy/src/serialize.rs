//! YAML serialization using facet-reflect's Peek API.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::io::Write;

use facet_core::{Facet, Field};
use facet_reflect::{HasFields, Peek};

use crate::error::{YamlError, YamlErrorKind};

/// Get the serialized name of a field (respecting rename attributes).
fn get_serialized_field_name(field: &Field) -> &'static str {
    // Look for rename attribute using extension syntax: #[facet(serde::rename = "value")]
    if let Some(ext) = field.get_attr(Some("serde"), "rename")
        && let Some(Some(name)) = ext.get_as::<Option<&'static str>>()
    {
        return name;
    }
    // Default to the field name
    field.name
}

type Result<T> = core::result::Result<T, YamlError>;

/// Serialize a value to a YAML string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml_legacy::to_string;
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
pub fn to_string<T: Facet<'static>>(value: &T) -> Result<String> {
    let mut output = Vec::new();
    to_writer(&mut output, value)?;
    let mut s = String::from_utf8(output).expect("YAML output should be valid UTF-8");
    // Remove trailing newline for consistency
    if s.ends_with('\n') {
        s.pop();
    }
    Ok(s)
}

/// Serialize a value to a writer as YAML.
///
/// This is the streaming version of [`to_string`].
pub fn to_writer<W: Write, T: Facet<'static>>(mut writer: W, value: &T) -> Result<()> {
    // Write document start marker
    writeln!(writer, "---")
        .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
    let peek = Peek::new(value);
    let mut serializer = YamlSerializer::new(writer);
    serializer.serialize_value(peek, 0, true)
}

struct YamlSerializer<W> {
    writer: W,
}

impl<W: Write> YamlSerializer<W> {
    fn new(writer: W) -> Self {
        Self { writer }
    }

    fn write_indent(&mut self, level: usize) -> Result<()> {
        for _ in 0..level {
            write!(self.writer, "  ")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }
        Ok(())
    }

    fn write_key(&mut self, key: &str) -> Result<()> {
        // Check if key needs quoting (contains special characters or is empty)
        let needs_quotes = key.is_empty()
            || key.contains(':')
            || key.contains('#')
            || key.contains('\n')
            || key.contains('\r')
            || key.contains('"')
            || key.contains('\'')
            || key.starts_with(' ')
            || key.ends_with(' ')
            || key.starts_with('-')
            || key.starts_with('?')
            || key.starts_with('*')
            || key.starts_with('&')
            || key.starts_with('!')
            || key.starts_with('|')
            || key.starts_with('>')
            || key.starts_with('%')
            || key.starts_with('@')
            || key.starts_with('`')
            || key.starts_with('[')
            || key.starts_with('{')
            || looks_like_bool(key)
            || looks_like_null(key)
            || looks_like_number(key);

        if needs_quotes {
            write!(self.writer, "\"{}\"", escape_string(key))
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        } else {
            write!(self.writer, "{key}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }
        Ok(())
    }

    fn serialize_value<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        indent: usize,
        is_root: bool,
    ) -> Result<()> {
        // Handle Option first
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                write!(self.writer, "null")
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
                if is_root {
                    writeln!(self.writer)
                        .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
                }
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_value(inner, indent, is_root);
            }
            return Ok(());
        }

        // Unwrap transparent wrappers
        let peek = peek.innermost_peek();

        // Try struct
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct(struct_peek, indent, is_root);
        }

        // Try enum
        if let Ok(enum_peek) = peek.into_enum() {
            return self.serialize_enum(enum_peek, indent, is_root);
        }

        // Try list
        if let Ok(list_peek) = peek.into_list() {
            return self.serialize_list(list_peek, indent, is_root);
        }

        // Try map
        if let Ok(map_peek) = peek.into_map() {
            return self.serialize_map(map_peek, indent, is_root);
        }

        // Scalar value
        self.serialize_scalar(peek, indent)?;
        if is_root {
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }
        Ok(())
    }

    fn serialize_scalar<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        indent: usize,
    ) -> Result<()> {
        // Try string first
        if let Some(s) = peek.as_str() {
            self.write_string(s, indent)?;
            return Ok(());
        }

        // Try various numeric and boolean types
        if let Ok(v) = peek.get::<bool>() {
            write!(self.writer, "{}", if *v { "true" } else { "false" })
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }

        // Signed integers
        if let Ok(v) = peek.get::<i8>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i16>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i32>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i64>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i128>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<isize>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }

        // Unsigned integers
        if let Ok(v) = peek.get::<u8>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u16>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u32>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u64>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u128>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<usize>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }

        // Floats
        if let Ok(v) = peek.get::<f32>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<f64>() {
            write!(self.writer, "{v}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            return Ok(());
        }

        // Char - always use inline format (single chars don't need block scalars)
        if let Ok(v) = peek.get::<char>() {
            self.write_string_inline(&v.to_string())?;
            return Ok(());
        }

        // Fallback: try to get string representation
        write!(self.writer, "null")
            .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        Ok(())
    }

    /// Write a string value, potentially using block scalar syntax for multi-line strings.
    fn write_string(&mut self, s: &str, indent: usize) -> Result<()> {
        // Use block scalar syntax for multi-line strings when appropriate
        if should_use_block_scalar(s) {
            return self.write_block_scalar(s, indent);
        }

        self.write_string_inline(s)
    }

    /// Write a string in inline (quoted or plain) format, never using block scalars.
    /// Used for map keys and situations where block scalars aren't appropriate.
    fn write_string_inline(&mut self, s: &str) -> Result<()> {
        // Check if we need to quote the string
        let needs_quotes = s.is_empty()
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
            || looks_like_number(s);

        if needs_quotes {
            write!(self.writer, "\"{}\"", escape_string(s))
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        } else {
            write!(self.writer, "{s}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }
        Ok(())
    }

    /// Write a string using YAML literal block scalar syntax (|).
    ///
    /// Block scalars preserve newlines exactly as written, making multi-line
    /// strings much more readable than escaped inline strings.
    fn write_block_scalar(&mut self, s: &str, indent: usize) -> Result<()> {
        // Determine the chomping indicator:
        // - `-` (strip): remove all trailing newlines
        // - `` (clip, default): keep single trailing newline
        // - `+` (keep): keep all trailing newlines
        let chomping = if s.ends_with('\n') {
            if s.ends_with("\n\n") {
                "+" // Keep all trailing newlines
            } else {
                "" // Clip: single trailing newline (default)
            }
        } else {
            "-" // Strip: no trailing newline
        };

        // Write the block scalar header
        write!(self.writer, "|{chomping}")
            .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;

        // Write each line with proper indentation
        let content = if chomping == "-" {
            s
        } else {
            s.trim_end_matches('\n')
        };

        for line in content.split('\n') {
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            self.write_indent(indent)?;
            write!(self.writer, "{line}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        // For "keep" chomping (+), we need to preserve trailing newlines
        if chomping == "+" {
            let trailing_newlines = s.len() - s.trim_end_matches('\n').len();
            // We already wrote one newline per line including the last one in content
            // We need to write (trailing_newlines - 1) more newlines
            for _ in 1..trailing_newlines {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
        }

        Ok(())
    }

    fn serialize_struct<'mem, 'facet>(
        &mut self,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
        indent: usize,
        is_root: bool,
    ) -> Result<()> {
        let mut first = true;

        // Use fields_for_serialize() which properly handles:
        // - skip_serializing attribute
        // - skip_serializing_if predicate
        // - flatten attribute
        for (field_item, field_peek) in struct_peek.fields_for_serialize() {
            if first && !is_root {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }

            if !first {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }

            self.write_indent(indent)?;
            // Use serialized name (respecting rename attribute)
            // Skip flattened map entries (field is None)
            let Some(field) = field_item.field else {
                continue;
            };
            let serialized_name = get_serialized_field_name(&field);
            self.write_key(serialized_name)?;
            write!(self.writer, ": ")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;

            self.serialize_value(field_peek, indent + 1, false)?;

            first = false;
        }

        if is_root && !first {
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        Ok(())
    }

    fn serialize_enum<'mem, 'facet>(
        &mut self,
        enum_peek: facet_reflect::PeekEnum<'mem, 'facet>,
        indent: usize,
        is_root: bool,
    ) -> Result<()> {
        let variant_name = enum_peek.variant_name_active().map_err(|e| {
            YamlError::without_span(YamlErrorKind::InvalidValue {
                message: format!("failed to get variant name: {e}"),
            })
        })?;

        // Check if it's a unit variant
        let fields: Vec<_> = enum_peek.fields().collect();
        if fields.is_empty() {
            // Unit variant: just the name (always inline - variant names don't have newlines)
            self.write_string_inline(variant_name)?;
            if is_root {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
            return Ok(());
        }

        // Check variant kind
        let is_newtype = fields.len() == 1 && fields[0].0.name == "0";
        let is_tuple = fields.iter().all(|(f, _)| f.name.parse::<usize>().is_ok());

        // Externally tagged format
        write!(self.writer, "{variant_name}:")
            .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;

        if is_newtype {
            // Newtype variant: VariantName: value
            write!(self.writer, " ")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            self.serialize_value(fields[0].1, indent + 1, false)?;
        } else if is_tuple {
            // Tuple variant: VariantName: [items]
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            for (_, field_peek) in fields {
                self.write_indent(indent + 1)?;
                write!(self.writer, "- ")
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
                self.serialize_value(field_peek, indent + 2, false)?;
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
        } else {
            // Struct variant: VariantName: {fields}
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            for (field, field_peek) in fields {
                self.write_indent(indent + 1)?;
                write!(self.writer, "{}: ", &field.name)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
                self.serialize_value(field_peek, indent + 2, false)?;
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
        }

        if is_root && (is_newtype || !is_tuple) {
            // Already handled newline
        }

        Ok(())
    }

    fn serialize_list<'mem, 'facet>(
        &mut self,
        list_peek: facet_reflect::PeekList<'mem, 'facet>,
        indent: usize,
        is_root: bool,
    ) -> Result<()> {
        let items: Vec<_> = list_peek.iter().collect();

        if items.is_empty() {
            write!(self.writer, "[]")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            if is_root {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
            return Ok(());
        }

        // Block style list
        if !is_root {
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        for item in items {
            self.write_indent(indent)?;
            write!(self.writer, "- ")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            self.serialize_value(item, indent + 1, false)?;
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        Ok(())
    }

    fn serialize_map<'mem, 'facet>(
        &mut self,
        map_peek: facet_reflect::PeekMap<'mem, 'facet>,
        indent: usize,
        is_root: bool,
    ) -> Result<()> {
        let entries: Vec<_> = map_peek.iter().collect();

        if entries.is_empty() {
            write!(self.writer, "{{}}")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            if is_root {
                writeln!(self.writer)
                    .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
            }
            return Ok(());
        }

        if !is_root {
            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        for (key_peek, value_peek) in entries {
            self.write_indent(indent)?;

            // Serialize key (must be scalar-ish)
            // Keys should never use block scalars, so we use inline style (indent 0 signals inline)
            if let Some(s) = key_peek.as_str() {
                self.write_string_inline(s)?;
            } else {
                self.serialize_scalar(key_peek, 0)?;
            }

            write!(self.writer, ": ")
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;

            self.serialize_value(value_peek, indent + 1, false)?;

            writeln!(self.writer)
                .map_err(|e| YamlError::without_span(YamlErrorKind::Io(e.to_string())))?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn is_complex_value<'mem, 'facet>(&self, peek: &Peek<'mem, 'facet>) -> bool {
        let peek = peek.innermost_peek();

        // Check if it's a struct, list, or map
        if peek.into_struct().is_ok() {
            return true;
        }
        if peek.into_list().is_ok()
            && let Ok(list) = peek.into_list()
        {
            // Only complex if non-empty
            return list.iter().next().is_some();
        }
        if peek.into_map().is_ok()
            && let Ok(map) = peek.into_map()
        {
            return map.iter().next().is_some();
        }
        false
    }
}

/// Determine if a string should use block scalar syntax.
///
/// Block scalars are preferred for multi-line strings as they're much more
/// readable. However, we avoid them in certain edge cases.
fn should_use_block_scalar(s: &str) -> bool {
    // Must contain at least one newline to benefit from block scalar
    if !s.contains('\n') {
        return false;
    }

    // Avoid block scalar for strings with only whitespace (including newlines)
    // as these are better represented with quoted strings
    if s.trim().is_empty() {
        return false;
    }

    // Avoid block scalar if any line has trailing whitespace that would be lost
    // (though YAML 1.2 preserves trailing whitespace, some parsers don't handle it well)
    // We'll keep trailing whitespace for now as it's valid YAML

    // Avoid block scalar for strings with carriage returns (Windows line endings)
    // as block scalars don't handle \r well
    if s.contains('\r') {
        return false;
    }

    true
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
    // Check for integer or float
    s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok()
}

/// Escape special characters in a YAML string
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}
