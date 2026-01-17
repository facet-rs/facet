//! WebAssembly bindings for the Styx parser.
//!
//! This crate provides JavaScript-callable functions for parsing Styx documents,
//! converting to JSON, and getting diagnostics.

use serde::Serialize;
use serde_json::json;
use styx_parse::{ScalarKind, Separator};
use styx_tree::{Entry, Object, Payload, Scalar, Sequence, Tag, Value};
use wasm_bindgen::prelude::*;

/// Serialize a value to JsValue using plain objects (not Maps).
fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value.serialize(&serializer)
}

/// A diagnostic message from the parser.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// The kind of error.
    pub message: String,
    /// Start offset in the source.
    pub start: u32,
    /// End offset in the source.
    pub end: u32,
    /// Severity: "error" or "warning".
    pub severity: String,
}

/// Result of parsing a Styx document.
#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    /// Whether parsing succeeded (no errors).
    pub success: bool,
    /// List of diagnostics (errors and warnings).
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse a Styx document and return diagnostics.
///
/// Returns a JSON object with `success` boolean and `diagnostics` array.
#[wasm_bindgen]
pub fn parse(source: &str) -> JsValue {
    let parser = styx_parse::Parser::new(source);
    let mut events = Vec::new();
    parser.parse(&mut events);

    let mut diagnostics = Vec::new();
    for event in events {
        if let styx_parse::Event::Error { span, kind } = event {
            diagnostics.push(Diagnostic {
                message: format_error(&kind),
                start: span.start,
                end: span.end,
                severity: "error".to_string(),
            });
        }
    }

    let result = ParseResult {
        success: diagnostics.is_empty(),
        diagnostics,
    };

    to_js_value(&result).unwrap_or(JsValue::NULL)
}

/// Convert a Styx document to JSON.
///
/// Returns a JSON string representation of the Styx document.
/// Tags are represented as `{"$tag": "tagname", "$value": ...}`.
/// Returns an error object if parsing fails.
#[wasm_bindgen]
pub fn to_json(source: &str) -> JsValue {
    match styx_tree::parse(source) {
        Ok(value) => {
            let json_value = value_to_json(&value);
            let json_string =
                serde_json::to_string_pretty(&json_value).unwrap_or_else(|e| e.to_string());

            to_js_value(&json!({
                "success": true,
                "json": json_value,
                "jsonString": json_string
            }))
            .unwrap_or(JsValue::NULL)
        }
        Err(e) => to_js_value(&json!({
            "success": false,
            "error": e.to_string()
        }))
        .unwrap_or(JsValue::NULL),
    }
}

/// Convert a Styx Value to a JSON value.
fn value_to_json(value: &Value) -> serde_json::Value {
    let tag = value.tag.as_ref().map(|t| t.name.as_str());
    let payload = value.payload.as_ref().map(payload_to_json);

    match (tag, payload) {
        // Unit with no tag: null
        (None, None) => json!(null),
        // Scalar/sequence/object with no tag: just the payload
        (None, Some(p)) => p,
        // Tag with no payload: {"$tag": "name"}
        (Some(t), None) => json!({"$tag": t}),
        // Tagged value: {"$tag": "name", "$value": payload}
        (Some(t), Some(p)) => json!({"$tag": t, "$value": p}),
    }
}

/// Convert a Styx Payload to a JSON value.
fn payload_to_json(payload: &Payload) -> serde_json::Value {
    match payload {
        Payload::Scalar(s) => {
            // Try to parse as number or boolean
            if let Ok(n) = s.text.parse::<i64>() {
                json!(n)
            } else if let Ok(n) = s.text.parse::<f64>() {
                json!(n)
            } else if s.text == "true" {
                json!(true)
            } else if s.text == "false" {
                json!(false)
            } else if s.text == "null" {
                json!(null)
            } else {
                json!(s.text)
            }
        }
        Payload::Sequence(seq) => sequence_to_json(seq),
        Payload::Object(obj) => object_to_json(obj),
    }
}

/// Convert a Styx Sequence to a JSON array.
fn sequence_to_json(seq: &Sequence) -> serde_json::Value {
    let items: Vec<serde_json::Value> = seq.items.iter().map(value_to_json).collect();
    json!(items)
}

/// Convert a Styx Object to a JSON object.
fn object_to_json(obj: &Object) -> serde_json::Value {
    let mut map = serde_json::Map::new();

    for entry in &obj.entries {
        // Get key as string
        let key = if entry.key.is_unit() {
            "@".to_string()
        } else if let Some(s) = entry.key.as_str() {
            s.to_string()
        } else if let Some(tag) = entry.key.tag_name() {
            format!("@{}", tag)
        } else {
            // Complex key - serialize it
            format!("{:?}", entry.key)
        };

        map.insert(key, value_to_json(&entry.value));
    }

    serde_json::Value::Object(map)
}

/// Format a parse error kind into a human-readable message.
fn format_error(kind: &styx_parse::ParseErrorKind) -> String {
    use styx_parse::ParseErrorKind::*;
    match kind {
        DuplicateKey { .. } => "Duplicate key in object".to_string(),
        MixedSeparators => "Mixed separators: use either commas or newlines, not both".to_string(),
        UnclosedObject => "Unclosed object: missing '}'".to_string(),
        UnclosedSequence => "Unclosed sequence: missing ')'".to_string(),
        InvalidEscape(seq) => format!("Invalid escape sequence: '{}'", seq),
        UnexpectedToken => "Unexpected token".to_string(),
        ExpectedKey => "Expected a key".to_string(),
        ExpectedValue => "Expected a value".to_string(),
        UnexpectedEof => "Unexpected end of input".to_string(),
        InvalidTagName => "Invalid tag name: must match @[A-Za-z_][A-Za-z0-9_.-]*".to_string(),
        InvalidKey => "Invalid key: cannot use objects, sequences, or heredocs as keys".to_string(),
        DanglingDocComment => "Doc comment (///) must be followed by an entry".to_string(),
        TooManyAtoms => {
            "Too many atoms: did you mean @tag{}? No whitespace between tag and payload".to_string()
        }
    }
}

/// Validate a Styx document and return whether it's valid.
#[wasm_bindgen]
pub fn validate(source: &str) -> bool {
    let parser = styx_parse::Parser::new(source);
    let mut events = Vec::new();
    parser.parse(&mut events);
    !events
        .iter()
        .any(|e| matches!(e, styx_parse::Event::Error { .. }))
}

/// Convert a JSON string to Styx format.
///
/// Returns a Styx document string representation of the JSON.
/// Tagged values ({"$tag": "name", "$value": ...}) are converted back to tags.
#[wasm_bindgen]
pub fn from_json(json_source: &str) -> JsValue {
    match serde_json::from_str::<serde_json::Value>(json_source) {
        Ok(json_value) => {
            let styx_value = json_to_value(&json_value);
            let styx_string =
                styx_format::format_value(&styx_value, styx_format::FormatOptions::default());

            to_js_value(&json!({
                "success": true,
                "styxString": styx_string
            }))
            .unwrap_or(JsValue::NULL)
        }
        Err(e) => to_js_value(&json!({
            "success": false,
            "error": e.to_string()
        }))
        .unwrap_or(JsValue::NULL),
    }
}

/// Convert a JSON value to a Styx Value.
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::unit(),

        serde_json::Value::Bool(b) => Value {
            tag: None,
            payload: Some(Payload::Scalar(Scalar {
                text: b.to_string(),
                kind: ScalarKind::Bare,
                span: None,
            })),
            span: None,
        },

        serde_json::Value::Number(n) => Value {
            tag: None,
            payload: Some(Payload::Scalar(Scalar {
                text: n.to_string(),
                kind: ScalarKind::Bare,
                span: None,
            })),
            span: None,
        },

        serde_json::Value::String(s) => {
            // Check if it needs quoting
            let kind = if needs_quoting(s) {
                ScalarKind::Quoted
            } else {
                ScalarKind::Bare
            };
            Value {
                tag: None,
                payload: Some(Payload::Scalar(Scalar {
                    text: s.clone(),
                    kind,
                    span: None,
                })),
                span: None,
            }
        }

        serde_json::Value::Array(arr) => {
            let items = arr.iter().map(json_to_value).collect();
            Value {
                tag: None,
                payload: Some(Payload::Sequence(Sequence { items, span: None })),
                span: None,
            }
        }

        serde_json::Value::Object(obj) => {
            // Check for tagged value: {"$tag": "name", "$value": ...}
            if let Some(serde_json::Value::String(tag_name)) = obj.get("$tag") {
                let payload = obj.get("$value").and_then(|v| json_to_value(v).payload);
                return Value {
                    tag: Some(Tag {
                        name: tag_name.clone(),
                        span: None,
                    }),
                    payload,
                    span: None,
                };
            }

            // Regular object
            let entries = obj
                .iter()
                .map(|(k, v)| Entry {
                    key: Value {
                        tag: None,
                        payload: Some(Payload::Scalar(Scalar {
                            text: k.clone(),
                            kind: if needs_quoting(k) {
                                ScalarKind::Quoted
                            } else {
                                ScalarKind::Bare
                            },
                            span: None,
                        })),
                        span: None,
                    },
                    value: json_to_value(v),
                    doc_comment: None,
                })
                .collect();

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
    }
}

/// Check if a string needs quoting in Styx.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }

    // Check for characters that require quoting
    s.chars().any(|c| {
        matches!(
            c,
            ' ' | '\t' | '\n' | '\r' | '"' | '{' | '}' | '(' | ')' | ',' | '@' | '>' | '/'
        )
    }) || s.starts_with("//")
}

/// Get the version of the Styx WASM library.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
