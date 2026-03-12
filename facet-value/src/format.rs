//! Pretty formatting for Values with span tracking.
//!
//! This module provides functionality to format a `Value` as JSON-like text,
//! tracking byte spans for each path through the value for use in diagnostics.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::{Value, ValueType};

/// A segment in a path through a Value
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// A key in an object
    Key(String),
    /// An index in an array
    Index(usize),
}

/// A path to a location within a Value
pub type Path = Vec<PathSegment>;

/// A byte span in formatted output (start, end)
pub type Span = (usize, usize);

/// Result of formatting a value with span tracking
#[derive(Debug)]
pub struct FormattedValue {
    /// The formatted text (plain text)
    pub text: String,
    /// Map from paths to their byte spans in `text`
    pub spans: BTreeMap<Path, Span>,
}

/// Format a Value as JSON-like text with span tracking
pub fn format_value_with_spans(value: &Value) -> FormattedValue {
    let mut ctx = FormatContext::new();
    format_value_into(&mut ctx, value, &[]);
    FormattedValue {
        text: ctx.output,
        spans: ctx.spans,
    }
}

/// Format a Value as JSON-like text (no span tracking, just plain output)
pub fn format_value(value: &Value) -> String {
    let mut ctx = FormatContext::new();
    format_value_into(&mut ctx, value, &[]);
    ctx.output
}

struct FormatContext {
    output: String,
    spans: BTreeMap<Path, Span>,
    indent: usize,
}

impl FormatContext {
    const fn new() -> Self {
        Self {
            output: String::new(),
            spans: BTreeMap::new(),
            indent: 0,
        }
    }

    const fn len(&self) -> usize {
        self.output.len()
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    fn record_span(&mut self, path: &[PathSegment], start: usize, end: usize) {
        self.spans.insert(path.to_vec(), (start, end));
    }
}

fn format_value_into(ctx: &mut FormatContext, value: &Value, current_path: &[PathSegment]) {
    let start = ctx.len();

    match value.value_type() {
        ValueType::Null => {
            ctx.output.push_str("null");
        }
        ValueType::Bool => {
            if value.is_true() {
                ctx.output.push_str("true");
            } else {
                ctx.output.push_str("false");
            }
        }
        ValueType::Number => {
            let num = value.as_number().unwrap();
            // Use the numeric representation directly
            if let Some(i) = num.to_i64() {
                let _ = write!(ctx.output, "{i}");
            } else if let Some(u) = num.to_u64() {
                let _ = write!(ctx.output, "{u}");
            } else if let Some(f) = num.to_f64() {
                let _ = write!(ctx.output, "{f}");
            }
        }
        ValueType::String => {
            let s = value.as_string().unwrap();
            // Write as JSON string with escaping
            ctx.output.push('"');
            for c in s.as_str().chars() {
                match c {
                    '"' => ctx.output.push_str("\\\""),
                    '\\' => ctx.output.push_str("\\\\"),
                    '\n' => ctx.output.push_str("\\n"),
                    '\r' => ctx.output.push_str("\\r"),
                    '\t' => ctx.output.push_str("\\t"),
                    c if c.is_control() => {
                        let _ = write!(ctx.output, "\\u{:04x}", c as u32);
                    }
                    c => ctx.output.push(c),
                }
            }
            ctx.output.push('"');
        }
        ValueType::Bytes => {
            let bytes = value.as_bytes().unwrap();
            ctx.output.push_str("<bytes:");
            let _ = write!(ctx.output, "{}", bytes.len());
            ctx.output.push('>');
        }
        ValueType::Array => {
            let arr = value.as_array().unwrap();
            if arr.is_empty() {
                ctx.output.push_str("[]");
            } else {
                ctx.output.push_str("[\n");
                ctx.indent += 1;
                for (i, item) in arr.iter().enumerate() {
                    ctx.write_indent();
                    let mut item_path = current_path.to_vec();
                    item_path.push(PathSegment::Index(i));
                    format_value_into(ctx, item, &item_path);
                    if i < arr.len() - 1 {
                        ctx.output.push(',');
                    }
                    ctx.output.push('\n');
                }
                ctx.indent -= 1;
                ctx.write_indent();
                ctx.output.push(']');
            }
        }
        ValueType::Object => {
            let obj = value.as_object().unwrap();
            if obj.is_empty() {
                ctx.output.push_str("{}");
            } else {
                ctx.output.push_str("{\n");
                ctx.indent += 1;
                let entries: Vec<_> = obj.iter().collect();
                for (i, (key, val)) in entries.iter().enumerate() {
                    ctx.write_indent();
                    // Write key
                    ctx.output.push('"');
                    ctx.output.push_str(key.as_str());
                    ctx.output.push_str("\": ");
                    // Format value with path
                    let mut item_path = current_path.to_vec();
                    item_path.push(PathSegment::Key(key.as_str().into()));
                    format_value_into(ctx, val, &item_path);
                    if i < entries.len() - 1 {
                        ctx.output.push(',');
                    }
                    ctx.output.push('\n');
                }
                ctx.indent -= 1;
                ctx.write_indent();
                ctx.output.push('}');
            }
        }
        ValueType::DateTime => {
            let dt = value.as_datetime().unwrap();
            // Format using Debug which produces ISO 8601 format
            let _ = write!(ctx.output, "{dt:?}");
        }
        ValueType::QName => {
            let qname = value.as_qname().unwrap();
            // Format using Debug which produces {namespace}local_name format
            let _ = write!(ctx.output, "{qname:?}");
        }
        ValueType::Uuid => {
            let uuid = value.as_uuid().unwrap();
            // Format using Debug which produces standard UUID format
            let _ = write!(ctx.output, "{uuid:?}");
        }
    }

    let end = ctx.len();
    ctx.record_span(current_path, start, end);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VArray, VObject, VString};

    #[test]
    fn test_format_primitives() {
        assert_eq!(format_value(&Value::NULL), "null");
        assert_eq!(format_value(&Value::TRUE), "true");
        assert_eq!(format_value(&Value::FALSE), "false");
        assert_eq!(format_value(&Value::from(42i64)), "42");
        assert_eq!(
            format_value(&Value::from(VString::new("hello"))),
            "\"hello\""
        );
    }

    #[test]
    fn test_format_array() {
        let mut arr = VArray::new();
        arr.push(Value::from(1i64));
        arr.push(Value::from(2i64));
        arr.push(Value::from(3i64));
        let value: Value = arr.into();

        let result = format_value_with_spans(&value);
        assert!(result.text.contains("1"));
        assert!(result.text.contains("2"));
        assert!(result.text.contains("3"));

        // Check that array elements have spans
        let path_0 = vec![PathSegment::Index(0)];
        assert!(result.spans.contains_key(&path_0));
    }

    #[test]
    fn test_format_object() {
        let mut obj = VObject::new();
        obj.insert("name", Value::from(VString::new("Alice")));
        obj.insert("age", Value::from(30i64));
        let value: Value = obj.into();

        let result = format_value_with_spans(&value);
        assert!(result.text.contains("\"name\""));
        assert!(result.text.contains("\"Alice\""));
        assert!(result.text.contains("\"age\""));
        assert!(result.text.contains("30"));

        // Check that object fields have spans
        let name_path = vec![PathSegment::Key("name".into())];
        let age_path = vec![PathSegment::Key("age".into())];
        assert!(
            result.spans.contains_key(&name_path),
            "Missing span for 'name'"
        );
        assert!(
            result.spans.contains_key(&age_path),
            "Missing span for 'age'"
        );

        // Verify the span content
        let age_span = result.spans.get(&age_path).unwrap();
        let age_text = &result.text[age_span.0..age_span.1];
        assert_eq!(age_text, "30");
    }

    #[test]
    fn test_format_nested() {
        let mut inner = VObject::new();
        inner.insert("x", Value::from(10i64));

        let mut outer = VObject::new();
        outer.insert("point", Value::from(inner));
        let value: Value = outer.into();

        let result = format_value_with_spans(&value);

        // Check nested path
        let nested_path = vec![
            PathSegment::Key("point".into()),
            PathSegment::Key("x".into()),
        ];
        assert!(
            result.spans.contains_key(&nested_path),
            "Missing span for nested path. Spans: {:?}",
            result.spans
        );

        let span = result.spans.get(&nested_path).unwrap();
        let text = &result.text[span.0..span.1];
        assert_eq!(text, "10");
    }
}
