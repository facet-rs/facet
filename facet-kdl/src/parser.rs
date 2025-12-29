//! KDL parser implementation using FormatParser trait.
//!
//! KDL documents consist of nodes, where each node has:
//! - A name (identifier)
//! - Positional arguments (values after the name)
//! - Properties (key=value pairs)
//! - Child nodes (inside braces)
//!
//! This maps to the FormatParser model as:
//! - Node → StructStart(Element) ... StructEnd
//! - Arguments → FieldKey(Argument) + Scalar
//! - Properties → FieldKey(Property) + Scalar
//! - Children → FieldKey(Child) + nested node events

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};

/// KDL parser that converts KDL documents to FormatParser events.
pub struct KdlParser<'de> {
    events: Vec<ParseEvent<'de>>,
    idx: usize,
    pending_error: Option<KdlError>,
}

impl<'de> KdlParser<'de> {
    /// Create a new KDL parser from input string.
    pub fn new(input: &'de str) -> Self {
        match build_events(input) {
            Ok(events) => Self {
                events,
                idx: 0,
                pending_error: None,
            },
            Err(err) => Self {
                events: Vec::new(),
                idx: 0,
                pending_error: Some(err),
            },
        }
    }
}

/// Error type for KDL parsing.
#[derive(Debug, Clone)]
pub enum KdlError {
    /// Parse error from the kdl crate.
    ParseError(String),
    /// Unexpected end of input.
    UnexpectedEof,
    /// Invalid KDL structure.
    InvalidStructure(String),
    /// Invalid UTF-8 in input.
    InvalidUtf8(core::str::Utf8Error),
}

impl fmt::Display for KdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KdlError::ParseError(msg) => write!(f, "KDL parse error: {}", msg),
            KdlError::UnexpectedEof => write!(f, "Unexpected end of KDL"),
            KdlError::InvalidStructure(msg) => write!(f, "Invalid KDL structure: {}", msg),
            KdlError::InvalidUtf8(e) => write!(f, "Invalid UTF-8: {}", e),
        }
    }
}

impl std::error::Error for KdlError {}

impl<'de> FormatParser<'de> for KdlParser<'de> {
    type Error = KdlError;
    type Probe<'a>
        = KdlProbe<'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        if self.idx >= self.events.len() {
            return Ok(None);
        }
        let event = self.events[self.idx].clone();
        self.idx += 1;
        Ok(Some(event))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        Ok(self.events.get(self.idx).cloned())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        let mut depth = 0usize;
        let mut pending_field_value = false;

        loop {
            let event = self.next_event()?.ok_or(KdlError::UnexpectedEof)?;
            match event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    pending_field_value = false;
                    depth += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        break;
                    } else {
                        depth -= 1;
                        if depth == 0 && !pending_field_value {
                            break;
                        }
                    }
                }
                ParseEvent::Scalar(_) | ParseEvent::VariantTag(_) => {
                    if depth == 0 && !pending_field_value {
                        break;
                    }
                    pending_field_value = false;
                }
                ParseEvent::FieldKey(_) | ParseEvent::OrderedField => {
                    pending_field_value = true;
                }
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        let evidence = self.build_probe();
        Ok(KdlProbe { evidence, idx: 0 })
    }
}

impl<'de> KdlParser<'de> {
    /// Build field evidence by looking ahead at remaining events.
    fn build_probe(&self) -> Vec<FieldEvidence<'de>> {
        let mut evidence = Vec::new();

        if self.idx >= self.events.len() {
            return evidence;
        }

        // Check if we're about to read a struct
        if !matches!(
            self.events.get(self.idx),
            Some(ParseEvent::StructStart(ContainerKind::Element))
        ) {
            return evidence;
        }

        // Scan the struct's fields
        let mut i = self.idx + 1;
        let mut depth = 0usize;

        while i < self.events.len() {
            match &self.events[i] {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    depth += 1;
                    i += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    i += 1;
                }
                ParseEvent::FieldKey(key) if depth == 0 => {
                    // Top-level field - check if next is scalar
                    let scalar_value = if let Some(ParseEvent::Scalar(sv)) = self.events.get(i + 1)
                    {
                        Some(sv.clone())
                    } else {
                        None
                    };

                    if let Some(sv) = scalar_value {
                        evidence.push(FieldEvidence::with_scalar_value(
                            key.name.clone(),
                            key.location,
                            None,
                            sv,
                            key.namespace.clone(),
                        ));
                    } else {
                        evidence.push(FieldEvidence::new(
                            key.name.clone(),
                            key.location,
                            None,
                            key.namespace.clone(),
                        ));
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        evidence
    }
}

/// Probe stream for KDL parser.
pub struct KdlProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for KdlProbe<'de> {
    type Error = KdlError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        if self.idx >= self.evidence.len() {
            Ok(None)
        } else {
            let ev = self.evidence[self.idx].clone();
            self.idx += 1;
            Ok(Some(ev))
        }
    }
}

/// Build ParseEvents from KDL input.
fn build_events<'de>(input: &str) -> Result<Vec<ParseEvent<'de>>, KdlError> {
    let doc: kdl::KdlDocument = input
        .parse()
        .map_err(|e: kdl::KdlError| KdlError::ParseError(e.to_string()))?;

    let mut events = Vec::new();

    // A KDL document is a sequence of nodes at the root level.
    // If there's exactly one node, emit it directly (common case for config files).
    // If there are multiple nodes, emit them as a document with children.
    let nodes = doc.nodes();

    if nodes.is_empty() {
        // Empty document - emit empty struct
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        events.push(ParseEvent::StructEnd);
    } else if nodes.len() == 1 {
        // Single root node - emit it directly (not a child, so is_child = false)
        emit_node_events(&nodes[0], &mut events, false);
    } else {
        // Multiple root nodes - wrap in a document struct
        // Each node becomes a child field
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        for node in nodes {
            let key = FieldKey::new(
                Cow::Owned(node.name().value().to_string()),
                FieldLocationHint::Child,
            );
            events.push(ParseEvent::FieldKey(key));
            // These are top-level children, but they're inside the document wrapper
            emit_node_events(node, &mut events, true);
        }
        events.push(ParseEvent::StructEnd);
    }

    Ok(events)
}

/// Emit ParseEvents for a single KDL node.
///
/// Every KDL node is emitted as a struct. Arguments become `_arg` or indexed fields,
/// properties become named fields with `FieldLocationHint::Property`, and children
/// become fields with `FieldLocationHint::Child`.
///
/// The `_is_child` parameter is reserved for future use (e.g., format-specific optimizations)
/// but currently all nodes are treated uniformly as structs.
fn emit_node_events<'de>(node: &kdl::KdlNode, events: &mut Vec<ParseEvent<'de>>, _is_child: bool) {
    let entries = node.entries();
    let children = node.children();

    let args: Vec<_> = entries.iter().filter(|e| e.name().is_none()).collect();
    let props: Vec<_> = entries.iter().filter(|e| e.name().is_some()).collect();
    let has_children = children.is_some_and(|c| !c.nodes().is_empty());

    // Case 1: Node with no entries and no children → emit empty struct
    if args.is_empty() && props.is_empty() && !has_children {
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        events.push(ParseEvent::StructEnd);
        return;
    }

    // Case 3: Complex node → emit as struct with fields
    events.push(ParseEvent::StructStart(ContainerKind::Element));

    // Emit positional arguments first
    // For multiple arguments, use indexed names: "0", "1", "2", ...
    // For a single argument field, use "_arg" as a conventional name
    if args.len() == 1 {
        let key = FieldKey::new(Cow::Borrowed("_arg"), FieldLocationHint::Argument);
        events.push(ParseEvent::FieldKey(key));
        emit_kdl_value(args[0].value(), events);
    } else {
        for (idx, entry) in args.iter().enumerate() {
            let key = FieldKey::new(Cow::Owned(idx.to_string()), FieldLocationHint::Argument);
            events.push(ParseEvent::FieldKey(key));
            emit_kdl_value(entry.value(), events);
        }
    }

    // Emit properties
    for entry in &props {
        let name = entry.name().unwrap();
        let key = FieldKey::new(
            Cow::Owned(name.value().to_string()),
            FieldLocationHint::Property,
        );
        events.push(ParseEvent::FieldKey(key));
        emit_kdl_value(entry.value(), events);
    }

    // Emit children - mark them as child nodes
    if let Some(children_doc) = children {
        for child in children_doc.nodes() {
            let key = FieldKey::new(
                Cow::Owned(child.name().value().to_string()),
                FieldLocationHint::Child,
            );
            events.push(ParseEvent::FieldKey(key));
            emit_node_events(child, events, true);
        }
    }

    events.push(ParseEvent::StructEnd);
}

/// Convert a KDL value to a ParseEvent scalar.
fn emit_kdl_value<'de>(value: &kdl::KdlValue, events: &mut Vec<ParseEvent<'de>>) {
    let scalar = match value {
        kdl::KdlValue::Null => ScalarValue::Null,
        kdl::KdlValue::Bool(b) => ScalarValue::Bool(*b),
        kdl::KdlValue::Integer(n) => {
            // KdlValue::Integer contains an i128 directly
            let n: i128 = *n;
            if let Ok(i) = i64::try_from(n) {
                if i >= 0 {
                    ScalarValue::U64(i as u64)
                } else {
                    ScalarValue::I64(i)
                }
            } else if let Ok(u) = u64::try_from(n) {
                ScalarValue::U64(u)
            } else {
                ScalarValue::I128(n)
            }
        }
        kdl::KdlValue::Float(f) => ScalarValue::F64(*f),
        kdl::KdlValue::String(s) => ScalarValue::Str(Cow::Owned(s.clone())),
    };
    events.push(ParseEvent::Scalar(scalar));
}
