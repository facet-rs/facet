//! Format `styx_tree::Value` to Styx text.

use styx_tree::{Entry, Object, Payload, Sequence, Value};

use crate::{FormatOptions, StyxWriter, format_source};

/// Format a Value as a Styx document string.
///
/// The value is treated as the root of a document, so if it's an Object,
/// it will be formatted without braces (implicit root object).
///
/// This first serializes the Value to text, then pipes through the CST-based
/// formatter to normalize whitespace and indentation.
pub fn format_value(value: &Value, options: FormatOptions) -> String {
    let mut formatter = ValueFormatter::new(options.clone());
    formatter.format_root(value);
    let raw = formatter.finish();
    // Normalize through CST formatter
    format_source(&raw, options)
}

/// Format a Value as a Styx document string with default options.
pub fn format_value_default(value: &Value) -> String {
    format_value(value, FormatOptions::default())
}

/// Format an Object directly (with braces), not as a root document.
///
/// This is useful for code actions that need to format a single object
/// while respecting its separator style.
pub fn format_object_braced(obj: &Object, options: FormatOptions) -> String {
    let mut formatter = ValueFormatter::new(options.clone());
    formatter.format_object(obj);
    let raw = formatter.finish();
    format_source(&raw, options)
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
            && let Some(Payload::Object(obj)) = &value.payload
        {
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
        let has_tag = value.tag.is_some();

        // Write tag if present
        if let Some(tag) = &value.tag {
            self.writer.write_tag(&tag.name);
        }

        // Write payload if present
        match &value.payload {
            None => {
                // No payload - if no tag either, this is unit (@)
                if !has_tag {
                    self.writer.write_str("@");
                }
                // If there's a tag but no payload, tag was already written
            }
            Some(Payload::Scalar(s)) => {
                // If tagged, wrap scalar in parens: @tag(scalar)
                if has_tag {
                    self.writer.begin_seq_after_tag();
                    self.writer.write_scalar(&s.text);
                    self.writer.end_seq().ok();
                } else {
                    self.writer.write_scalar(&s.text);
                }
            }
            Some(Payload::Sequence(seq)) => {
                // If tagged, sequence attaches directly: @tag(...)
                self.format_sequence_inner(seq, has_tag);
            }
            Some(Payload::Object(obj)) => {
                // If tagged, object attaches directly: @tag{...}
                self.format_object_inner(obj, has_tag);
            }
        }
    }

    fn format_sequence_inner(&mut self, seq: &Sequence, after_tag: bool) {
        if after_tag {
            self.writer.begin_seq_after_tag();
        } else {
            self.writer.begin_seq();
        }
        for item in &seq.items {
            self.format_value(item);
        }
        self.writer.end_seq().ok();
    }

    fn format_object(&mut self, obj: &Object) {
        self.format_object_inner(obj, false);
    }

    fn format_object_inner(&mut self, obj: &Object, after_tag: bool) {
        // Preserve the original separator style - if it was newline-separated, keep it multiline
        let force_multiline = matches!(obj.separator, styx_parse::Separator::Newline);
        if after_tag {
            self.writer.begin_struct_after_tag(force_multiline);
        } else {
            self.writer
                .begin_struct_with_options(false, force_multiline);
        }
        self.format_object_entries(obj);
        self.writer.end_struct().ok();
    }

    fn format_object_entries(&mut self, obj: &Object) {
        let entry_count = obj.entries.len();
        for (i, entry) in obj.entries.iter().enumerate() {
            self.format_entry(entry);

            // Add blank lines for readability at root level
            if self.writer.depth() == 1 && i < entry_count - 1 {
                // Blank line after:
                // - schema declaration (@schema path/to/schema.styx)
                // - entries with doc comments (type definitions)
                let is_schema_decl = i == 0 && entry.key.is_schema_tag();
                if is_schema_decl || entry.doc_comment.is_some() {
                    self.writer.write_str("\n");
                }
            }
        }
    }

    fn format_entry(&mut self, entry: &Entry) {
        // Format the key (which is itself a Value - scalar or unit, optionally tagged)
        let key_str = self.format_key(&entry.key);

        // Write doc comment + key together, or just key
        if let Some(doc) = &entry.doc_comment {
            self.writer.write_doc_comment_and_key_raw(doc, &key_str);
        } else {
            self.writer.field_key_raw(&key_str).ok();
        }

        self.format_value(&entry.value);
    }

    /// Format a key value to string.
    /// Keys are scalars or unit, optionally tagged.
    fn format_key(&self, key: &Value) -> String {
        let mut result = String::new();

        // Tag prefix if present
        if let Some(tag) = &key.tag {
            result.push('@');
            result.push_str(&tag.name);
        }

        // Payload (scalar text or unit)
        match &key.payload {
            None => {
                // Unit - if no tag, write @
                if key.tag.is_none() {
                    result.push('@');
                }
                // If tagged with no payload, tag is already written (e.g., @schema)
            }
            Some(Payload::Scalar(s)) => {
                // Format scalar based on its kind
                use styx_parse::ScalarKind;
                match s.kind {
                    ScalarKind::Bare => result.push_str(&s.text),
                    ScalarKind::Quoted => {
                        result.push('"');
                        result.push_str(&crate::scalar::escape_quoted(&s.text));
                        result.push('"');
                    }
                    ScalarKind::Raw => {
                        // For raw strings, just quote them normally for simplicity
                        result.push('"');
                        result.push_str(&crate::scalar::escape_quoted(&s.text));
                        result.push('"');
                    }
                    ScalarKind::Heredoc => {
                        // Heredocs can't be keys, but format as quoted if somehow here
                        result.push('"');
                        result.push_str(&crate::scalar::escape_quoted(&s.text));
                        result.push('"');
                    }
                }
            }
            Some(Payload::Sequence(_) | Payload::Object(_)) => {
                panic!("object key cannot be a sequence or object: {:?}", key);
            }
        }

        result
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

    // =========================================================================
    // Edge case tests for formatting
    // =========================================================================

    /// Helper to create a newline-separated object value
    fn obj_multiline(entries: Vec<Entry>) -> Value {
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

    /// Helper to create a comma-separated (inline) object value
    fn obj_inline(entries: Vec<Entry>) -> Value {
        Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries,
                separator: Separator::Comma,
                span: None,
            })),
            span: None,
        }
    }

    /// Helper to create a tagged value with object payload
    fn tagged_obj(tag_name: &str, entries: Vec<Entry>, separator: Separator) -> Value {
        Value {
            tag: Some(Tag {
                name: tag_name.to_string(),
                span: None,
            }),
            payload: Some(Payload::Object(Object {
                entries,
                separator,
                span: None,
            })),
            span: None,
        }
    }

    /// Helper to create a tagged value with a single scalar payload
    fn tagged_scalar(tag_name: &str, text: &str) -> Value {
        Value {
            tag: Some(Tag {
                name: tag_name.to_string(),
                span: None,
            }),
            payload: Some(Payload::Scalar(Scalar {
                text: text.to_string(),
                kind: ScalarKind::Bare,
                span: None,
            })),
            span: None,
        }
    }

    /// Helper to create a unit entry (@ key)
    fn unit_entry(value: Value) -> Entry {
        Entry {
            key: Value::unit(),
            value,
            doc_comment: None,
        }
    }

    /// Helper to create a schema declaration entry (@schema key)
    fn schema_entry(value: Value) -> Entry {
        Entry {
            key: Value::tag("schema"),
            value,
            doc_comment: None,
        }
    }

    // --- Edge Case 1: Schema declaration with blank line after ---
    #[test]
    fn test_edge_case_01_schema_declaration_blank_line() {
        // @schema schema.styx followed by other fields should have blank line
        let obj = obj_multiline(vec![
            schema_entry(scalar("schema.styx")),
            entry("name", scalar("test")),
            entry("port", scalar("8080")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 2: Nested multiline objects preserve structure ---
    #[test]
    fn test_edge_case_02_nested_multiline_objects() {
        let inner = obj_multiline(vec![
            entry("host", scalar("localhost")),
            entry("port", scalar("8080")),
        ]);
        let obj = obj_multiline(vec![entry("name", scalar("myapp")), entry("server", inner)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 3: Deeply nested multiline objects (3 levels) ---
    #[test]
    fn test_edge_case_03_deeply_nested_multiline() {
        let level3 = obj_multiline(vec![
            entry("cert", scalar("/path/to/cert")),
            entry("key", scalar("/path/to/key")),
        ]);
        let level2 = obj_multiline(vec![
            entry("host", scalar("localhost")),
            entry("tls", level3),
        ]);
        let obj = obj_multiline(vec![
            entry("name", scalar("myapp")),
            entry("server", level2),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 4: Mixed inline and multiline ---
    #[test]
    fn test_edge_case_04_mixed_inline_multiline() {
        // Outer is multiline, inner is inline
        let inner = obj_inline(vec![entry("x", scalar("1")), entry("y", scalar("2"))]);
        let obj = obj_multiline(vec![entry("name", scalar("point")), entry("coords", inner)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 5: Tagged object with multiline content ---
    #[test]
    fn test_edge_case_05_tagged_multiline_object() {
        let obj = obj_multiline(vec![entry(
            "type",
            tagged_obj(
                "object",
                vec![entry("name", tagged("string")), entry("age", tagged("int"))],
                Separator::Newline,
            ),
        )]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 6: Tagged object with inline content ---
    #[test]
    fn test_edge_case_06_tagged_inline_object() {
        let obj = obj_multiline(vec![entry(
            "point",
            tagged_obj(
                "point",
                vec![entry("x", scalar("1")), entry("y", scalar("2"))],
                Separator::Comma,
            ),
        )]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 7: Schema-like structure with @object tags ---
    #[test]
    fn test_edge_case_07_schema_structure() {
        // meta { ... }
        // schema { @ @object{ ... } }
        let meta = obj_multiline(vec![
            entry("id", scalar("https://example.com/schema")),
            entry("version", scalar("1.0")),
        ]);
        let schema_obj = tagged_obj(
            "object",
            vec![
                entry("name", tagged("string")),
                entry("port", tagged("int")),
            ],
            Separator::Newline,
        );
        let schema = obj_multiline(vec![unit_entry(schema_obj)]);
        let root = obj_multiline(vec![entry("meta", meta), entry("schema", schema)]);

        let result = format_value_default(&root);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 8: Optional wrapped types ---
    #[test]
    fn test_edge_case_08_optional_types() {
        let obj = obj_multiline(vec![
            entry("required", tagged("string")),
            entry("optional", tagged_scalar("optional", "@bool")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 9: Empty object ---
    #[test]
    fn test_edge_case_09_empty_object() {
        let obj = obj_multiline(vec![entry("empty", obj_multiline(vec![]))]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 10: Empty inline object ---
    #[test]
    fn test_edge_case_10_empty_inline_object() {
        let obj = obj_multiline(vec![entry("empty", obj_inline(vec![]))]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 11: Sequence of objects ---
    #[test]
    fn test_edge_case_11_sequence_of_objects() {
        let item1 = obj_inline(vec![entry("name", scalar("Alice"))]);
        let item2 = obj_inline(vec![entry("name", scalar("Bob"))]);
        let seq = Value {
            tag: None,
            payload: Some(Payload::Sequence(Sequence {
                items: vec![item1, item2],
                span: None,
            })),
            span: None,
        };
        let obj = obj_multiline(vec![entry("users", seq)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 12: Quoted strings that need escaping ---
    #[test]
    fn test_edge_case_12_quoted_strings() {
        let obj = obj_multiline(vec![
            entry("message", scalar(r#""Hello, World!""#)),
            entry("path", scalar("/path/with spaces/file.txt")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 13: Keys that need quoting ---
    #[test]
    fn test_edge_case_13_quoted_keys() {
        let obj = obj_multiline(vec![
            entry("normal-key", scalar("value1")),
            entry("key with spaces", scalar("value2")),
            entry("123numeric", scalar("value3")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 14: Schema declaration (now using @schema tag) ---
    #[test]
    fn test_edge_case_14_schema_declaration() {
        let obj = obj_multiline(vec![
            schema_entry(scalar("first.styx")),
            entry("name", scalar("test")),
        ]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 15: Nested sequences ---
    #[test]
    fn test_edge_case_15_nested_sequences() {
        let inner_seq = seq_value(vec![scalar("a"), scalar("b")]);
        let outer_seq = Value {
            tag: None,
            payload: Some(Payload::Sequence(Sequence {
                items: vec![inner_seq, seq_value(vec![scalar("c"), scalar("d")])],
                span: None,
            })),
            span: None,
        };
        let obj = obj_multiline(vec![entry("matrix", outer_seq)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 16: Tagged sequence ---
    #[test]
    fn test_edge_case_16_tagged_sequence() {
        let tagged_seq = Value {
            tag: Some(Tag {
                name: "seq".to_string(),
                span: None,
            }),
            payload: Some(Payload::Sequence(Sequence {
                items: vec![tagged("string")],
                span: None,
            })),
            span: None,
        };
        let obj = obj_multiline(vec![entry("items", tagged_seq)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 17: Doc comments on nested entries ---
    #[test]
    fn test_edge_case_17_nested_doc_comments() {
        let inner = Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: vec![
                    entry_with_doc("host", scalar("localhost"), "The server hostname"),
                    entry_with_doc("port", scalar("8080"), "The server port"),
                ],
                separator: Separator::Newline,
                span: None,
            })),
            span: None,
        };
        let obj = obj_multiline(vec![entry_with_doc(
            "server",
            inner,
            "Server configuration",
        )]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 18: Very long inline object should stay inline if marked ---
    #[test]
    fn test_edge_case_18_long_inline_stays_inline() {
        let inner = obj_inline(vec![
            entry("field1", scalar("value1")),
            entry("field2", scalar("value2")),
            entry("field3", scalar("value3")),
            entry("field4", scalar("value4")),
        ]);
        let obj = obj_multiline(vec![entry("data", inner)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 19: Multiline with single field ---
    #[test]
    fn test_edge_case_19_multiline_single_field() {
        let inner = obj_multiline(vec![entry("only", scalar("one"))]);
        let obj = obj_multiline(vec![entry("wrapper", inner)]);

        let result = format_value_default(&obj);
        insta::assert_snapshot!(result);
    }

    // --- Edge Case 20: Full schema file simulation ---
    #[test]
    fn test_edge_case_20_full_schema_simulation() {
        // Simulates: meta { id ..., version ..., description ... }
        //            schema { @ @object{ name @string, server @object{ host @string, port @int } } }
        let meta = obj_multiline(vec![
            entry("id", scalar("https://example.com/config")),
            entry("version", scalar("2024-01-01")),
            entry("description", scalar("\"A test schema\"")),
        ]);

        let _server_fields = obj_multiline(vec![
            entry("host", tagged("string")),
            entry("port", tagged("int")),
        ]);
        let server_schema = tagged_obj(
            "object",
            vec![
                entry("host", tagged("string")),
                entry("port", tagged("int")),
            ],
            Separator::Newline,
        );

        let root_schema = tagged_obj(
            "object",
            vec![
                entry("name", tagged("string")),
                entry("server", server_schema),
            ],
            Separator::Newline,
        );

        let schema = obj_multiline(vec![unit_entry(root_schema)]);

        let root = obj_multiline(vec![entry("meta", meta), entry("schema", schema)]);

        let result = format_value_default(&root);
        insta::assert_snapshot!(result);
    }
}
