//! Format `styx_tree::Value` to Styx text.

use styx_tree::{Entry, Object, Payload, Sequence, Value};

use crate::{FormatOptions, StyxWriter};

/// Format a Value as a Styx document string.
///
/// The value is treated as the root of a document, so if it's an Object,
/// it will be formatted without braces (implicit root object).
pub fn format_value(value: &Value, options: FormatOptions) -> String {
    let mut formatter = ValueFormatter::new(options);
    formatter.format_root(value);
    formatter.finish()
}

/// Format a Value as a Styx document string with default options.
pub fn format_value_default(value: &Value) -> String {
    format_value(value, FormatOptions::default())
}

struct ValueFormatter {
    writer: StyxWriter,
}

impl ValueFormatter {
    fn new(options: FormatOptions) -> Self {
        Self {
            writer: StyxWriter::with_options(options),
        }
    }

    fn finish(self) -> String {
        self.writer.finish_string()
    }

    fn format_root(&mut self, value: &Value) {
        // Root is typically an untagged object
        if value.tag.is_none()
            && let Some(Payload::Object(obj)) = &value.payload {
                // Root object - no braces
                self.writer.begin_struct(true);
                self.format_object_entries(obj);
                self.writer.end_struct().ok();
                return;
            }
        // Non-object root or tagged root - just format the value
        self.format_value(value);
    }

    fn format_value(&mut self, value: &Value) {
        // Write tag if present
        if let Some(tag) = &value.tag {
            self.writer.write_tag(&tag.name);
        }

        // Write payload if present
        match &value.payload {
            None => {
                // No payload - if no tag either, this is unit (@)
                if value.tag.is_none() {
                    self.writer.write_str("@");
                }
                // If there's a tag but no payload, tag was already written
            }
            Some(Payload::Scalar(s)) => {
                // If tagged, wrap scalar in parens: @tag(scalar)
                if value.tag.is_some() {
                    self.writer.begin_seq_after_tag();
                    self.writer.write_scalar(&s.text);
                    self.writer.end_seq().ok();
                } else {
                    self.writer.write_scalar(&s.text);
                }
            }
            Some(Payload::Sequence(seq)) => {
                self.format_sequence(seq);
            }
            Some(Payload::Object(obj)) => {
                self.format_object(obj);
            }
        }
    }

    fn format_sequence(&mut self, seq: &Sequence) {
        self.writer.begin_seq();
        for item in &seq.items {
            self.format_value(item);
        }
        self.writer.end_seq().ok();
    }

    fn format_object(&mut self, obj: &Object) {
        self.writer.begin_struct(false);
        self.format_object_entries(obj);
        self.writer.end_struct().ok();
    }

    fn format_object_entries(&mut self, obj: &Object) {
        for entry in &obj.entries {
            self.format_entry(entry);
        }
    }

    fn format_entry(&mut self, entry: &Entry) {
        // Handle unit key specially - write @ directly (no quoting)
        if entry.key.is_unit() {
            if let Some(doc) = &entry.doc_comment {
                self.writer.write_doc_comment_and_key_raw(doc, "@");
            } else {
                self.writer.field_key_raw("@").ok();
            }
            self.format_value(&entry.value);
            return;
        }

        // Get key as string - must be an untagged scalar
        let Some(key) = entry.key.as_str() else {
            panic!(
                "object key must be untagged Scalar or Unit, got {:?}",
                entry.key
            );
        };

        // Write doc comment + key together, or just key
        if let Some(doc) = &entry.doc_comment {
            self.writer.write_doc_comment_and_key(doc, key);
        } else {
            self.writer.field_key(key).ok();
        }

        self.format_value(&entry.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use styx_parse::{ScalarKind, Separator};
    use styx_tree::{Object, Payload, Scalar, Sequence, Tag};

    fn scalar(text: &str) -> Value {
        Value {
            tag: None,
            payload: Some(Payload::Scalar(Scalar {
                text: text.to_string(),
                kind: ScalarKind::Bare,
                span: None,
            })),
            span: None,
        }
    }

    fn tagged(name: &str) -> Value {
        Value {
            tag: Some(Tag {
                name: name.to_string(),
                span: None,
            }),
            payload: None,
            span: None,
        }
    }

    fn entry(key: &str, value: Value) -> Entry {
        Entry {
            key: scalar(key),
            value,
            doc_comment: None,
        }
    }

    fn entry_with_doc(key: &str, value: Value, doc: &str) -> Entry {
        Entry {
            key: scalar(key),
            value,
            doc_comment: Some(doc.to_string()),
        }
    }

    fn obj_value(entries: Vec<Entry>) -> Value {
        Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries,
                separator: Separator::Newline,
                span: None,
            })),
            span: None,
        }
    }

    fn seq_value(items: Vec<Value>) -> Value {
        Value {
            tag: None,
            payload: Some(Payload::Sequence(Sequence { items, span: None })),
            span: None,
        }
    }

    #[test]
    fn test_format_simple_object() {
        let obj = obj_value(vec![
            entry("name", scalar("Alice")),
            entry("age", scalar("30")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_nested_object() {
        let inner = Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: vec![entry("name", scalar("Alice")), entry("age", scalar("30"))],
                separator: Separator::Comma,
                span: None,
            })),
            span: None,
        };

        let obj = obj_value(vec![entry("user", inner)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_tagged() {
        let obj = obj_value(vec![entry("type", tagged("string"))]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_sequence() {
        let seq = seq_value(vec![scalar("a"), scalar("b"), scalar("c")]);

        let obj = obj_value(vec![entry("items", seq)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_with_doc_comments() {
        let obj = obj_value(vec![
            entry_with_doc("name", scalar("Alice"), "The user's name"),
            entry_with_doc("age", scalar("30"), "Age in years"),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_unit() {
        let obj = obj_value(vec![entry("flag", Value::unit())]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }
}
