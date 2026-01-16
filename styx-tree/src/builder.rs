//! Tree builder from parse events.

use std::borrow::Cow;

use styx_parse::{Event, ParseCallback, ParseErrorKind, Separator, Span};

use crate::value::{Entry, Object, Payload, Scalar, Sequence, Tag, Value};

/// Error during tree building.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildError {
    /// Unexpected event during building.
    UnexpectedEvent(String),
    /// Unclosed structure.
    UnclosedStructure,
    /// Empty document.
    EmptyDocument,
    /// Parse error from the lexer/parser.
    Parse(ParseErrorKind, Span),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::UnexpectedEvent(msg) => write!(f, "unexpected event: {}", msg),
            BuildError::UnclosedStructure => write!(f, "unclosed structure"),
            BuildError::EmptyDocument => write!(f, "empty document"),
            BuildError::Parse(kind, span) => {
                write!(f, "parse error at {}-{}: {}", span.start, span.end, kind)
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl BuildError {
    /// If this is a parse error, return it as a `ParseError` for diagnostic rendering.
    pub fn as_parse_error(&self) -> Option<crate::diagnostic::ParseError> {
        match self {
            BuildError::Parse(kind, span) => {
                Some(crate::diagnostic::ParseError::new(kind.clone(), *span))
            }
            _ => None,
        }
    }
}

/// Builder that constructs a tree from parse events.
pub struct TreeBuilder {
    stack: Vec<BuilderFrame>,
    root_entries: Vec<Entry>,
    pending_doc_comment: Option<String>,
    errors: Vec<(ParseErrorKind, Span)>,
}

enum BuilderFrame {
    Object {
        entries: Vec<Entry>,
        separator: Separator,
        span: Span,
        pending_doc_comment: Option<String>,
    },
    Sequence {
        items: Vec<Value>,
        span: Span,
    },
    Tag {
        name: String,
        span: Span,
    },
    Entry {
        key: Option<Value>,
        doc_comment: Option<String>,
    },
}

impl TreeBuilder {
    /// Create a new tree builder.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            root_entries: Vec::new(),
            pending_doc_comment: None,
            errors: Vec::new(),
        }
    }

    /// Finish building and return the root value.
    pub fn finish(self) -> Result<Value, BuildError> {
        // Return the first error if any occurred during parsing
        if let Some((kind, span)) = self.errors.into_iter().next() {
            return Err(BuildError::Parse(kind, span));
        }

        if !self.stack.is_empty() {
            return Err(BuildError::UnclosedStructure);
        }

        // Root is always an implicit object (no tag)
        Ok(Value {
            tag: None,
            payload: Some(Payload::Object(Object {
                entries: self.root_entries,
                separator: Separator::Newline,
                span: None,
            })),
            span: None,
        })
    }

    /// Push a value to the current context.
    fn push_value(&mut self, value: Value) {
        // First, check if we're in a Tag frame - if so, the value becomes the tag's payload
        if let Some(BuilderFrame::Tag { .. }) = self.stack.last() {
            // Pop the tag frame
            if let Some(BuilderFrame::Tag { name, span }) = self.stack.pop() {
                // Create tagged value: the tag wraps the value's payload
                let tagged = Value {
                    tag: Some(Tag {
                        name,
                        span: Some(span),
                    }),
                    payload: value.payload,
                    span: value.span,
                };
                // Recursively push the tagged value
                self.push_value(tagged);
            }
            return;
        }

        // Check if we're in an Entry frame with a key - if so, this value completes the entry
        if let Some(BuilderFrame::Entry { key: Some(_), .. }) = self.stack.last() {
            // Pop the entry frame and add the complete entry to parent
            if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.pop() {
                let key_val = key.unwrap();
                match self.stack.last_mut() {
                    Some(BuilderFrame::Object { entries, .. }) => {
                        entries.push(Entry {
                            key: key_val,
                            value,
                            doc_comment,
                        });
                    }
                    _ => {
                        self.root_entries.push(Entry {
                            key: key_val,
                            value,
                            doc_comment,
                        });
                    }
                }
                // Re-push an empty entry frame for potential continuation
                self.stack.push(BuilderFrame::Entry {
                    key: None,
                    doc_comment: None,
                });
            }
            return;
        }

        match self.stack.last_mut() {
            Some(BuilderFrame::Object {
                entries,
                pending_doc_comment,
                ..
            }) => {
                // Value for an entry without explicit key - use unit key
                entries.push(Entry {
                    key: Value::unit(),
                    value,
                    doc_comment: pending_doc_comment.take(),
                });
            }
            Some(BuilderFrame::Sequence { items, .. }) => {
                items.push(value);
            }
            Some(BuilderFrame::Tag { .. }) => {
                // Already handled above
                unreachable!()
            }
            Some(BuilderFrame::Entry { key, .. }) => {
                if key.is_none() {
                    // This is the key
                    *key = Some(value);
                }
            }
            None => {
                // Root level - treat as entry in implicit object
                self.root_entries.push(Entry {
                    key: Value::unit(),
                    value,
                    doc_comment: self.pending_doc_comment.take(),
                });
            }
        }
    }
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<'src> ParseCallback<'src> for TreeBuilder {
    fn event(&mut self, event: Event<'src>) -> bool {
        match event {
            Event::DocumentStart | Event::DocumentEnd => {
                // No-op for tree building
            }

            Event::ObjectStart { span, separator } => {
                self.stack.push(BuilderFrame::Object {
                    entries: Vec::new(),
                    separator,
                    span,
                    pending_doc_comment: None,
                });
            }

            Event::ObjectEnd { span } => {
                if let Some(BuilderFrame::Object {
                    entries,
                    separator,
                    span: start_span,
                    ..
                }) = self.stack.pop()
                {
                    let obj = Value {
                        tag: None,
                        payload: Some(Payload::Object(Object {
                            entries,
                            separator,
                            span: Some(Span {
                                start: start_span.start,
                                end: span.end,
                            }),
                        })),
                        span: Some(Span {
                            start: start_span.start,
                            end: span.end,
                        }),
                    };
                    self.push_value(obj);
                }
            }

            Event::SequenceStart { span } => {
                self.stack.push(BuilderFrame::Sequence {
                    items: Vec::new(),
                    span,
                });
            }

            Event::SequenceEnd { span } => {
                if let Some(BuilderFrame::Sequence {
                    items,
                    span: start_span,
                }) = self.stack.pop()
                {
                    let seq = Value {
                        tag: None,
                        payload: Some(Payload::Sequence(Sequence {
                            items,
                            span: Some(Span {
                                start: start_span.start,
                                end: span.end,
                            }),
                        })),
                        span: Some(Span {
                            start: start_span.start,
                            end: span.end,
                        }),
                    };
                    self.push_value(seq);
                }
            }

            Event::EntryStart => {
                let doc_comment = match self.stack.last_mut() {
                    Some(BuilderFrame::Object {
                        pending_doc_comment,
                        ..
                    }) => pending_doc_comment.take(),
                    _ => self.pending_doc_comment.take(),
                };
                self.stack.push(BuilderFrame::Entry {
                    key: None,
                    doc_comment,
                });
            }

            Event::EntryEnd => {
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.pop()
                    && let Some(key) = key
                {
                    // We have a key but might not have a value yet
                    match self.stack.last_mut() {
                        Some(BuilderFrame::Object { entries, .. }) => {
                            // Check if last entry needs this key
                            if let Some(last) = entries.last_mut()
                                && last.key.is_unit()
                                && last.doc_comment.is_none()
                            {
                                last.key = key;
                                last.doc_comment = doc_comment;
                                return true;
                            }
                            // Otherwise add as unit-valued entry
                            entries.push(Entry {
                                key,
                                value: Value::unit(),
                                doc_comment,
                            });
                        }
                        _ => {
                            // Root level
                            if let Some(last) = self.root_entries.last_mut()
                                && last.key.is_unit()
                                && last.doc_comment.is_none()
                            {
                                last.key = key;
                                last.doc_comment = doc_comment;
                                return true;
                            }
                            self.root_entries.push(Entry {
                                key,
                                value: Value::unit(),
                                doc_comment,
                            });
                        }
                    }
                }
            }

            Event::Key {
                span,
                tag,
                payload,
                kind,
            } => {
                let key_value = Value {
                    tag: tag.map(|name| Tag {
                        name: name.to_string(),
                        span: Some(span),
                    }),
                    payload: payload.map(|text| {
                        Payload::Scalar(Scalar {
                            text: cow_to_string(text),
                            kind,
                            span: Some(span),
                        })
                    }),
                    span: Some(span),
                };
                if let Some(BuilderFrame::Entry { key, .. }) = self.stack.last_mut() {
                    *key = Some(key_value);
                }
            }

            Event::Scalar { span, value, kind } => {
                let scalar = Value {
                    tag: None,
                    payload: Some(Payload::Scalar(Scalar {
                        text: cow_to_string(value),
                        kind,
                        span: Some(span),
                    })),
                    span: Some(span),
                };

                // Check if we're in an entry context with a key already set
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut()
                    && key.is_some()
                {
                    // We have a key, this is the value
                    let key_val = key.take().unwrap();
                    let doc = doc_comment.take();

                    // Pop the entry frame
                    self.stack.pop();

                    // Add to parent
                    match self.stack.last_mut() {
                        Some(BuilderFrame::Object { entries, .. }) => {
                            entries.push(Entry {
                                key: key_val,
                                value: scalar,
                                doc_comment: doc,
                            });
                        }
                        _ => {
                            self.root_entries.push(Entry {
                                key: key_val,
                                value: scalar,
                                doc_comment: doc,
                            });
                        }
                    }
                    // Re-push entry frame for potential more processing
                    self.stack.push(BuilderFrame::Entry {
                        key: None,
                        doc_comment: None,
                    });
                    return true;
                }

                self.push_value(scalar);
            }

            Event::Unit { span } => {
                let unit = Value {
                    tag: None,
                    payload: None,
                    span: Some(span),
                };

                // Similar logic to Scalar for entry handling
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut()
                    && key.is_some()
                {
                    let key_val = key.take().unwrap();
                    let doc = doc_comment.take();
                    self.stack.pop();

                    match self.stack.last_mut() {
                        Some(BuilderFrame::Object { entries, .. }) => {
                            entries.push(Entry {
                                key: key_val,
                                value: unit,
                                doc_comment: doc,
                            });
                        }
                        _ => {
                            self.root_entries.push(Entry {
                                key: key_val,
                                value: unit,
                                doc_comment: doc,
                            });
                        }
                    }
                    self.stack.push(BuilderFrame::Entry {
                        key: None,
                        doc_comment: None,
                    });
                    return true;
                }

                self.push_value(unit);
            }

            Event::TagStart { span, name } => {
                self.stack.push(BuilderFrame::Tag {
                    name: name.to_string(),
                    span,
                });
            }

            Event::TagEnd => {
                // Only pop if the top frame is a Tag - otherwise the tag was already
                // consumed when its payload was processed
                if !matches!(self.stack.last(), Some(BuilderFrame::Tag { .. })) {
                    return true;
                }
                if let Some(BuilderFrame::Tag { name, span }) = self.stack.pop() {
                    // Tag with no payload - just the tag itself
                    let tagged = Value {
                        tag: Some(Tag {
                            name,
                            span: Some(span),
                        }),
                        payload: None,
                        span: Some(span),
                    };

                    // Similar to scalar handling
                    if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut()
                        && key.is_some()
                    {
                        let key_val = key.take().unwrap();
                        let doc = doc_comment.take();
                        self.stack.pop();

                        match self.stack.last_mut() {
                            Some(BuilderFrame::Object { entries, .. }) => {
                                entries.push(Entry {
                                    key: key_val,
                                    value: tagged,
                                    doc_comment: doc,
                                });
                            }
                            _ => {
                                self.root_entries.push(Entry {
                                    key: key_val,
                                    value: tagged,
                                    doc_comment: doc,
                                });
                            }
                        }
                        self.stack.push(BuilderFrame::Entry {
                            key: None,
                            doc_comment: None,
                        });
                        return true;
                    }

                    self.push_value(tagged);
                }
            }

            Event::DocComment { text, .. } => {
                let comment = extract_doc_comment(text);
                match self.stack.last_mut() {
                    Some(BuilderFrame::Object {
                        pending_doc_comment,
                        ..
                    }) => {
                        *pending_doc_comment = Some(comment);
                    }
                    _ => {
                        self.pending_doc_comment = Some(comment);
                    }
                }
            }

            Event::Comment { .. } => {
                // Ignore regular comments for tree building
            }

            Event::Error { span, kind } => {
                self.errors.push((kind, span));
            }
        }

        true
    }
}

fn cow_to_string(cow: Cow<'_, str>) -> String {
    cow.into_owned()
}

fn extract_doc_comment(text: &str) -> String {
    // Strip leading `///` and optional space
    text.strip_prefix("///")
        .map(|s| s.strip_prefix(' ').unwrap_or(s))
        .unwrap_or(text)
        .to_string()
}

#[cfg(test)]
mod tests {
    use styx_parse::Parser;

    use super::*;

    fn parse(source: &str) -> Value {
        let parser = Parser::new(source);
        let mut builder = TreeBuilder::new();
        parser.parse(&mut builder);
        builder.finish().unwrap()
    }

    #[test]
    fn test_empty_document() {
        let value = parse("");
        assert!(value.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_simple_entry() {
        let value = parse("name Alice");
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
    }

    #[test]
    fn test_multiple_entries() {
        let value = parse("name Alice\nage 30");
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(obj.get("age").and_then(|v| v.as_str()), Some("30"));
    }

    #[test]
    fn test_path_access() {
        let value = parse("name Alice\nage 30");
        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(value.get("age").and_then(|v| v.as_str()), Some("30"));
    }

    #[test]
    fn test_unit_value() {
        let value = parse("enabled @");
        let obj = value.as_object().unwrap();
        assert!(obj.get("enabled").unwrap().is_unit());
    }

    #[test]
    fn test_unit_key() {
        // @ followed by a value should create a unit key
        let value = parse("@ server.schema.styx");
        let obj = value.as_object().unwrap();
        // The unit key entry
        let unit_entry = obj.entries.iter().find(|e| e.key.is_unit());
        assert!(
            unit_entry.is_some(),
            "should have unit key entry, got: {:?}",
            obj.entries
                .iter()
                .map(|e| format!("key={:?}", e.key))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            unit_entry.unwrap().value.as_str(),
            Some("server.schema.styx")
        );
    }

    #[test]
    fn test_tag() {
        let value = parse("type @user");
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.tag_name()), Some("user"));
    }

    #[test]
    fn test_tag_with_object_payload() {
        let value = parse("result @err{message \"failed\"}");
        let obj = value.as_object().unwrap();
        let result = obj.get("result").unwrap();
        assert_eq!(result.tag_name(), Some("err"));
        // Check payload is an object with message field
        let payload_obj = result.as_object().expect("payload should be object");
        assert_eq!(
            payload_obj.get("message").and_then(|v| v.as_str()),
            Some("failed")
        );
    }

    #[test]
    fn test_tag_with_sequence_payload() {
        let value = parse("color @rgb(255 128 0)");
        let obj = value.as_object().unwrap();
        let color = obj.get("color").unwrap();
        assert_eq!(color.tag_name(), Some("rgb"));
        // Check payload is a sequence
        let payload_seq = color.as_sequence().expect("payload should be sequence");
        assert_eq!(payload_seq.len(), 3);
        assert_eq!(payload_seq.get(0).and_then(|v| v.as_str()), Some("255"));
        assert_eq!(payload_seq.get(1).and_then(|v| v.as_str()), Some("128"));
        assert_eq!(payload_seq.get(2).and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_schema_structure_with_space() {
        // @ @object { ... } with space before brace
        let source = r#"schema {
  @ @object {
    name @string
  }
}"#;

        // Debug: print all events
        struct EventPrinter;
        impl<'src> styx_parse::ParseCallback<'src> for EventPrinter {
            fn event(&mut self, event: styx_parse::Event<'src>) -> bool {
                eprintln!("Event: {:?}", event);
                true
            }
        }

        eprintln!("=== Events for with-space version ===");
        let parser = styx_parse::Parser::new(source);
        parser.parse(&mut EventPrinter);

        let value = parse(source);
        let obj = value.as_object().unwrap();
        assert!(obj.get("schema").is_some(), "should have schema entry");
        let schema = obj.get("schema").unwrap();
        assert!(
            schema.as_object().is_some(),
            "schema should be an object, got tag={:?} payload={:?}",
            schema.tag,
            schema.payload.is_some()
        );
    }

    #[test]
    fn test_schema_structure_no_space() {
        // @ @object{ ... } without space before brace
        let source = r#"schema {
  @ @object{
    name @string
  }
}"#;

        // Debug: print all events
        struct EventPrinter;
        impl<'src> styx_parse::ParseCallback<'src> for EventPrinter {
            fn event(&mut self, event: styx_parse::Event<'src>) -> bool {
                eprintln!("Event: {:?}", event);
                true
            }
        }

        eprintln!("=== Events for no-space version ===");
        let parser = styx_parse::Parser::new(source);
        parser.parse(&mut EventPrinter);

        let value = parse(source);
        let obj = value.as_object().unwrap();
        eprintln!(
            "Root entries: {:?}",
            obj.entries
                .iter()
                .map(|e| e.key.as_str())
                .collect::<Vec<_>>()
        );
        assert!(obj.get("schema").is_some(), "should have schema entry");
        let schema = obj.get("schema").unwrap();
        assert!(
            schema.as_object().is_some(),
            "schema should be an object, got tag={:?} payload={:?}",
            schema.tag,
            schema.payload.is_some()
        );
    }
}
