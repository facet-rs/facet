//! Format `styx_tree::Value` to Styx text.

use styx_tree::{Entry, Object, Sequence, Tagged, Value};

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
        match value {
            Value::Object(obj) => {
                // Root object - no braces
                self.writer.begin_struct(true);
                self.format_object_entries(obj);
                self.writer.end_struct().ok();
            }
            _ => {
                // Non-object root - just format the value
                self.format_value(value);
            }
        }
    }

    fn format_value(&mut self, value: &Value) {
        match value {
            Value::Scalar(s) => {
                self.writer.write_scalar(&s.text);
            }
            Value::Unit => {
                self.writer.write_str("@");
            }
            Value::Tagged(tagged) => {
                self.format_tagged(tagged);
            }
            Value::Sequence(seq) => {
                self.format_sequence(seq);
            }
            Value::Object(obj) => {
                self.format_object(obj);
            }
        }
    }

    fn format_tagged(&mut self, tagged: &Tagged) {
        self.writer.write_tag(&tagged.tag);
        if let Some(payload) = &tagged.payload {
            // If payload is a tagged value (or scalar), wrap in parens for clarity
            // e.g., @optional(@string) not @optional@string
            match payload.as_ref() {
                Value::Tagged(_) | Value::Scalar(_) | Value::Unit => {
                    // Use begin_seq_after_tag to avoid spurious space
                    self.writer.begin_seq_after_tag();
                    self.format_value(payload);
                    self.writer.end_seq().ok();
                }
                _ => {
                    // Sequences and objects are already delimited
                    self.format_value(payload);
                }
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
        // Get key as string
        let key = match &entry.key {
            Value::Scalar(s) => s.text.as_str(),
            Value::Unit => "@",
            _ => "?", // shouldn't happen for well-formed objects
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
    use styx_tree::{Object, Scalar, Sequence, Tagged};

    fn scalar(text: &str) -> Value {
        Value::Scalar(Scalar {
            text: text.to_string(),
            kind: ScalarKind::Bare,
            span: None,
        })
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

    #[test]
    fn test_format_simple_object() {
        let obj = Value::Object(Object {
            entries: vec![entry("name", scalar("Alice")), entry("age", scalar("30"))],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_nested_object() {
        let obj = Value::Object(Object {
            entries: vec![entry(
                "user",
                Value::Object(Object {
                    entries: vec![entry("name", scalar("Alice")), entry("age", scalar("30"))],
                    separator: Separator::Comma,
                    span: None,
                }),
            )],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_tagged() {
        let tagged = Value::Tagged(Tagged {
            tag: "string".to_string(),
            payload: None,
            span: None,
        });

        let obj = Value::Object(Object {
            entries: vec![entry("type", tagged)],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_sequence() {
        let seq = Value::Sequence(Sequence {
            items: vec![scalar("a"), scalar("b"), scalar("c")],
            span: None,
        });

        let obj = Value::Object(Object {
            entries: vec![entry("items", seq)],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_with_doc_comments() {
        let obj = Value::Object(Object {
            entries: vec![
                entry_with_doc("name", scalar("Alice"), "The user's name"),
                entry_with_doc("age", scalar("30"), "Age in years"),
            ],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_format_unit() {
        let obj = Value::Object(Object {
            entries: vec![entry("flag", Value::Unit)],
            separator: Separator::Newline,
            span: None,
        });

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }
}
