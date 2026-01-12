//! Tree builder from parse events.

use std::borrow::Cow;

use styx_parse::{Event, ParseCallback, Separator, Span};

use crate::value::{Entry, Object, Scalar, Sequence, Tagged, Value};

/// Error during tree building.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildError {
    /// Unexpected event during building.
    UnexpectedEvent(String),
    /// Unclosed structure.
    UnclosedStructure,
    /// Empty document.
    EmptyDocument,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::UnexpectedEvent(msg) => write!(f, "unexpected event: {}", msg),
            BuildError::UnclosedStructure => write!(f, "unclosed structure"),
            BuildError::EmptyDocument => write!(f, "empty document"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Builder that constructs a tree from parse events.
pub struct TreeBuilder {
    stack: Vec<BuilderFrame>,
    root_entries: Vec<Entry>,
    pending_doc_comment: Option<String>,
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
        }
    }

    /// Finish building and return the root value.
    pub fn finish(self) -> Result<Value, BuildError> {
        if !self.stack.is_empty() {
            return Err(BuildError::UnclosedStructure);
        }

        // If we have root entries, wrap them in an implicit object
        if self.root_entries.is_empty() {
            // Empty document - return empty object
            Ok(Value::Object(Object {
                entries: Vec::new(),
                separator: Separator::Newline,
                span: None,
            }))
        } else {
            Ok(Value::Object(Object {
                entries: self.root_entries,
                separator: Separator::Newline,
                span: None,
            }))
        }
    }

    /// Push a value to the current context.
    fn push_value(&mut self, value: Value) {
        match self.stack.last_mut() {
            Some(BuilderFrame::Object {
                entries,
                pending_doc_comment,
                ..
            }) => {
                // Value for an entry - but we need a key first
                // This shouldn't happen in normal flow; entries handle this
                entries.push(Entry {
                    key: Value::Unit,
                    value,
                    doc_comment: pending_doc_comment.take(),
                });
            }
            Some(BuilderFrame::Sequence { items, .. }) => {
                items.push(value);
            }
            Some(BuilderFrame::Tag { .. }) => {
                // Tag payload - will be handled when tag ends
            }
            Some(BuilderFrame::Entry { key, .. }) => {
                if key.is_none() {
                    // This is the key
                    *key = Some(value);
                }
                // If key is already set, this is the value - handled elsewhere
            }
            None => {
                // Root level - treat as entry in implicit object
                self.root_entries.push(Entry {
                    key: Value::Unit,
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
                    let obj = Value::Object(Object {
                        entries,
                        separator,
                        span: Some(Span {
                            start: start_span.start,
                            end: span.end,
                        }),
                    });
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
                    let seq = Value::Sequence(Sequence {
                        items,
                        span: Some(Span {
                            start: start_span.start,
                            end: span.end,
                        }),
                    });
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
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.pop() {
                    if let Some(key) = key {
                        // We have a key but might not have a value yet
                        // The value should have been pushed to parent already
                        // Just add the entry to parent
                        match self.stack.last_mut() {
                            Some(BuilderFrame::Object { entries, .. }) => {
                                // Check if last entry needs this key
                                if let Some(last) = entries.last_mut() {
                                    if matches!(last.key, Value::Unit) && last.doc_comment.is_none()
                                    {
                                        last.key = key;
                                        last.doc_comment = doc_comment;
                                        return true;
                                    }
                                }
                                // Otherwise add as unit-valued entry
                                entries.push(Entry {
                                    key,
                                    value: Value::Unit,
                                    doc_comment,
                                });
                            }
                            _ => {
                                // Root level
                                if let Some(last) = self.root_entries.last_mut() {
                                    if matches!(last.key, Value::Unit) && last.doc_comment.is_none()
                                    {
                                        last.key = key;
                                        last.doc_comment = doc_comment;
                                        return true;
                                    }
                                }
                                self.root_entries.push(Entry {
                                    key,
                                    value: Value::Unit,
                                    doc_comment,
                                });
                            }
                        }
                    }
                }
            }

            Event::Key { span, value, kind } => {
                let scalar = Value::Scalar(Scalar {
                    text: cow_to_string(value),
                    kind,
                    span: Some(span),
                });
                if let Some(BuilderFrame::Entry { key, .. }) = self.stack.last_mut() {
                    *key = Some(scalar);
                }
            }

            Event::Scalar { span, value, kind } => {
                let scalar = Value::Scalar(Scalar {
                    text: cow_to_string(value),
                    kind,
                    span: Some(span),
                });

                // Check if we're in an entry context
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut() {
                    if key.is_some() {
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
                }

                self.push_value(scalar);
            }

            Event::Unit { span } => {
                let unit = Value::Unit;

                // Similar logic to Scalar for entry handling
                if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut() {
                    if key.is_some() {
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
                }

                let _ = span; // suppress unused warning
                self.push_value(unit);
            }

            Event::TagStart { span, name } => {
                self.stack.push(BuilderFrame::Tag {
                    name: name.to_string(),
                    span,
                });
            }

            Event::TagEnd => {
                if let Some(BuilderFrame::Tag { name, span }) = self.stack.pop() {
                    let tagged = Value::Tagged(Tagged {
                        tag: name,
                        payload: None, // TODO: handle payloads
                        span: Some(span),
                    });

                    // Similar to scalar handling
                    if let Some(BuilderFrame::Entry { key, doc_comment }) = self.stack.last_mut() {
                        if key.is_some() {
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

            Event::Error { .. } => {
                // TODO: handle errors
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
    fn test_tag() {
        let value = parse("type @user");
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.tag()), Some("user"));
    }
}
