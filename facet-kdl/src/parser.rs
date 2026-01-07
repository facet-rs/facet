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

use facet_core::Shape;
use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use miette::{LabeledSpan, NamedSource};

/// KDL parser that converts KDL documents to FormatParser events.
pub struct KdlParser<'de> {
    events: Vec<ParseEvent<'de>>,
    /// Source spans for each event (parallel to events vec).
    spans: Vec<facet_reflect::Span>,
    idx: usize,
    pending_error: Option<KdlError>,
}

impl<'de> KdlParser<'de> {
    /// Create a new KDL parser from input string.
    pub fn new(input: &'de str) -> Self {
        match build_events(input) {
            Ok((events, spans)) => Self {
                events,
                spans,
                idx: 0,
                pending_error: None,
            },
            Err(err) => Self {
                events: Vec::new(),
                spans: Vec::new(),
                idx: 0,
                pending_error: Some(err),
            },
        }
    }
}

/// Error type for KDL parsing.
///
/// This error type preserves the original `kdl::KdlError` for parse errors,
/// allowing full miette diagnostic information to be displayed including
/// source spans, labels, and help text.
#[derive(Debug, Clone)]
pub enum KdlError {
    /// Parse error from the kdl crate (preserved for full diagnostics).
    ParseError(kdl::KdlError),
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
            KdlError::ParseError(e) => write!(f, "{}", e),
            KdlError::UnexpectedEof => write!(f, "Unexpected end of KDL"),
            KdlError::InvalidStructure(msg) => write!(f, "Invalid KDL structure: {}", msg),
            KdlError::InvalidUtf8(e) => write!(f, "Invalid UTF-8: {}", e),
        }
    }
}

impl std::error::Error for KdlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KdlError::ParseError(e) => Some(e),
            KdlError::InvalidUtf8(e) => Some(e),
            _ => None,
        }
    }
}

impl miette::Diagnostic for KdlError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        match self {
            KdlError::ParseError(e) => e.code(),
            _ => None,
        }
    }

    fn severity(&self) -> Option<miette::Severity> {
        match self {
            KdlError::ParseError(e) => e.severity(),
            _ => Some(miette::Severity::Error),
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        match self {
            KdlError::ParseError(e) => e.help(),
            _ => None,
        }
    }

    fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        match self {
            KdlError::ParseError(e) => e.url(),
            _ => None,
        }
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        match self {
            KdlError::ParseError(e) => e.source_code(),
            _ => None,
        }
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        match self {
            KdlError::ParseError(e) => e.labels(),
            _ => None,
        }
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        match self {
            KdlError::ParseError(e) => e.related(),
            _ => None,
        }
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        match self {
            KdlError::ParseError(e) => e.diagnostic_source(),
            _ => None,
        }
    }
}

// Type context diagnostic is provided by facet_path::PathDiagnostic

/// A KDL deserialization error with source code context for rich diagnostics.
///
/// This wrapper type carries the original input alongside the error, enabling
/// miette to display the source with highlighted error locations. It also
/// optionally carries the target Rust type to show what structure was expected.
#[derive(Debug)]
pub struct KdlDeserializeError {
    /// The underlying deserialization error.
    pub inner: facet_format::DeserializeError<KdlError>,
    /// The original KDL source input (named for syntax highlighting).
    pub source_input: NamedSource<String>,
    /// The target type we were deserializing into (for help messages).
    target_shape: Option<&'static Shape>,
    /// Type context diagnostic showing the target Rust type (computed once at construction).
    type_context: Option<Box<facet_path::pretty::PathDiagnostic>>,
}

impl KdlDeserializeError {
    /// Create a new KdlDeserializeError with the target shape for better diagnostics.
    pub fn new(
        inner: facet_format::DeserializeError<KdlError>,
        source_input: String,
        target_shape: Option<&'static Shape>,
    ) -> Self {
        // Compute type context upfront (only for non-parse errors)
        let type_context = Self::compute_type_context(&inner, target_shape);

        Self {
            inner,
            // Name with .kdl extension so miette-arborium can syntax highlight
            source_input: NamedSource::new("input.kdl", source_input),
            target_shape,
            type_context,
        }
    }

    /// Compute the type context diagnostic if applicable.
    fn compute_type_context(
        inner: &facet_format::DeserializeError<KdlError>,
        target_shape: Option<&'static Shape>,
    ) -> Option<Box<facet_path::pretty::PathDiagnostic>> {
        // Don't show type context for parse errors - syntax errors aren't about types
        if matches!(
            inner,
            facet_format::DeserializeError::Parser(KdlError::ParseError(_))
        ) {
            return None;
        }

        let shape = target_shape?;

        // Get the path from the inner error (if available)
        let path = inner.path().cloned().unwrap_or_else(facet_path::Path::new);

        // For MissingField errors, extract the field name so we can highlight
        // the specific missing field in the type definition
        let leaf_field = match inner {
            facet_format::DeserializeError::MissingField { field, .. } => Some(*field),
            _ => None,
        };

        // Use facet-path's PathDiagnostic to show the type with the error location highlighted
        Some(Box::new(path.to_diagnostic(
            shape,
            alloc::format!("expected type `{}`", shape.type_identifier),
            None,
            leaf_field,
        )))
    }

    /// Get the inner kdl::KdlError if this is a parse error.
    fn get_kdl_parse_error(&self) -> Option<&kdl::KdlError> {
        match &self.inner {
            facet_format::DeserializeError::Parser(KdlError::ParseError(e)) => Some(e),
            _ => None,
        }
    }

    /// Get the type context diagnostic showing the target Rust type.
    fn get_type_context(&self) -> Option<&facet_path::pretty::PathDiagnostic> {
        self.type_context.as_deref()
    }
}

impl fmt::Display for KdlDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for KdlDeserializeError {
    // Don't return inner as source - we're a wrapper providing source code context,
    // not a cause chain. Returning inner here causes duplicate error messages since
    // our Display just delegates to inner.
}

impl miette::Diagnostic for KdlDeserializeError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.inner.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.inner.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        // Check for "expected scalar, got struct" which suggests property vs child mismatch
        if let facet_format::DeserializeError::ExpectedScalarGotStruct {
            path: Some(path), ..
        } = &self.inner
            && let Some(target_shape) = self.target_shape
            && let Some(field) = path.resolve_leaf_field(target_shape)
            && field.get_attr(Some("kdl"), "property").is_some()
        {
            return Some(Box::new(alloc::format!(
                "field `{}` is marked with `#[facet(kdl::property)]`, so use `{}=\"value\"` syntax instead of `{} \"value\"`",
                field.name,
                field.name,
                field.name
            )));
        }
        self.inner.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.inner.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        // Always use our named source (with .kdl extension for syntax highlighting)
        Some(&self.source_input)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        // For parse errors, extract labels from the kdl diagnostics
        // (kdl::KdlError stores its diagnostics in related(), not labels())
        if let Some(kdl_error) = self.get_kdl_parse_error() {
            // Collect all labels from all diagnostics
            let labels: Vec<LabeledSpan> = kdl_error
                .diagnostics
                .iter()
                .filter_map(|diag| diag.labels())
                .flatten()
                .collect();
            if !labels.is_empty() {
                return Some(Box::new(labels.into_iter()));
            }
        }
        // For other errors, forward to inner
        self.inner.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        // Show the target Rust type as a related diagnostic (when available)
        // Don't forward to inner.related() - we want all rendering to use our NamedSource
        self.get_type_context().map(|type_ctx| {
            Box::new(core::iter::once(type_ctx as &dyn miette::Diagnostic))
                as Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>
        })
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        // Don't forward to inner - we want all rendering to use our NamedSource
        // for consistent file names and syntax highlighting
        None
    }
}

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

    fn current_span(&self) -> Option<facet_reflect::Span> {
        // Return the span of the most recently consumed event (idx was incremented after consuming)
        if self.idx > 0 && self.idx <= self.spans.len() {
            Some(self.spans[self.idx - 1])
        } else {
            None
        }
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

/// A buffer for events and their corresponding source spans.
struct EventBuffer<'de> {
    events: Vec<ParseEvent<'de>>,
    spans: Vec<facet_reflect::Span>,
}

impl<'de> EventBuffer<'de> {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Push an event with its source span.
    fn push(&mut self, event: ParseEvent<'de>, span: miette::SourceSpan) {
        self.events.push(event);
        self.spans.push(span.into());
    }

    fn into_parts(self) -> (Vec<ParseEvent<'de>>, Vec<facet_reflect::Span>) {
        (self.events, self.spans)
    }
}

/// Build ParseEvents from KDL input, along with source spans for each event.
fn build_events<'de>(
    input: &str,
) -> Result<(Vec<ParseEvent<'de>>, Vec<facet_reflect::Span>), KdlError> {
    let doc: kdl::KdlDocument = input.parse().map_err(KdlError::ParseError)?;

    let mut buf = EventBuffer::new();

    // A KDL document is a sequence of nodes at the root level.
    // We always wrap root nodes in a document struct so that the schema (Rust types
    // with kdl::* attributes) determines how the document is interpreted, not the
    // document structure itself.
    //
    // This means:
    // - `kdl::children` fields will receive root nodes that match via singularization
    // - `kdl::child` fields will receive a specific named root node
    // - For a struct that IS the root node, use a wrapper: `config { ... }`
    let nodes = doc.nodes();

    if nodes.is_empty() {
        // Empty document - emit empty struct with document span
        buf.push(ParseEvent::StructStart(ContainerKind::Element), doc.span());
        buf.push(ParseEvent::StructEnd, doc.span());
    } else {
        // Wrap all root nodes in a document struct.
        // Each node becomes a child field, allowing schemas like:
        //   struct AppConfig {
        //       #[facet(kdl::child)] server: Server,
        //       #[facet(kdl::child)] database: Database,
        //   }
        //
        // For roundtripping single-node documents (e.g., `config host=...` -> Config),
        // the deserializer handles auto-drilling into a matching root node.
        buf.push(ParseEvent::StructStart(ContainerKind::Element), doc.span());
        for node in nodes {
            let key = FieldKey::new(
                Cow::Owned(node.name().value().to_string()),
                FieldLocationHint::Child,
            );
            buf.push(ParseEvent::FieldKey(key), node.name().span());
            emit_node_events(node, &mut buf);
        }
        buf.push(ParseEvent::StructEnd, doc.span());
    }

    Ok(buf.into_parts())
}

/// Emit ParseEvents for a single KDL node.
///
/// Every KDL node is emitted as a struct. The node name is emitted as `_node_name`,
/// arguments become `_arg` (single) or `_arguments` (sequence for multiple),
/// properties become named fields with `FieldLocationHint::Property`, and children
/// become fields with `FieldLocationHint::Child`.
fn emit_node_events<'de>(node: &kdl::KdlNode, buf: &mut EventBuffer<'de>) {
    let entries = node.entries();
    let children = node.children();
    let node_span = node.span();

    let args: Vec<_> = entries.iter().filter(|e| e.name().is_none()).collect();
    let props: Vec<_> = entries.iter().filter(|e| e.name().is_some()).collect();
    let has_children = children.is_some_and(|c| !c.nodes().is_empty());

    // Case 1: Node with no entries and no children → emit struct with just node name
    // Still emit _node_name for kdl::node_name support
    if args.is_empty() && props.is_empty() && !has_children {
        buf.push(ParseEvent::StructStart(ContainerKind::Element), node_span);
        let node_name_key = FieldKey::new(Cow::Borrowed("_node_name"), FieldLocationHint::Argument);
        buf.push(ParseEvent::FieldKey(node_name_key), node.name().span());
        buf.push(
            ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                node.name().value().to_string(),
            ))),
            node.name().span(),
        );
        buf.push(ParseEvent::StructEnd, node_span);
        return;
    }

    // Case 2: Complex node → emit as struct with fields
    buf.push(ParseEvent::StructStart(ContainerKind::Element), node_span);

    // Emit node name first for kdl::node_name fields
    let node_name_key = FieldKey::new(Cow::Borrowed("_node_name"), FieldLocationHint::Argument);
    buf.push(ParseEvent::FieldKey(node_name_key), node.name().span());
    buf.push(
        ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            node.name().value().to_string(),
        ))),
        node.name().span(),
    );

    // Emit positional arguments
    // - Single argument: emit as `_arg` scalar
    // - Multiple arguments: emit as `_arguments` sequence AND as individual `_arg` scalars
    //   (the sequence is for kdl::arguments, individual is for backwards compat)
    if !args.is_empty() {
        // Always emit _arguments as a sequence for kdl::arguments (plural) support
        let args_key = FieldKey::new(Cow::Borrowed("_arguments"), FieldLocationHint::Argument);
        // Use span of first argument for the sequence key
        buf.push(ParseEvent::FieldKey(args_key), args[0].span());
        buf.push(
            ParseEvent::SequenceStart(ContainerKind::Element),
            args[0].span(),
        );
        for entry in &args {
            emit_kdl_value(entry, buf);
        }
        // Use span of last argument for sequence end
        buf.push(
            ParseEvent::SequenceEnd,
            args.last().map(|e| e.span()).unwrap_or(node_span),
        );

        // Also emit individual arguments for kdl::argument (singular) support
        if args.len() == 1 {
            let key = FieldKey::new(Cow::Borrowed("_arg"), FieldLocationHint::Argument);
            buf.push(ParseEvent::FieldKey(key), args[0].span());
            emit_kdl_value(args[0], buf);
        } else {
            for (idx, entry) in args.iter().enumerate() {
                let key = FieldKey::new(Cow::Owned(idx.to_string()), FieldLocationHint::Argument);
                buf.push(ParseEvent::FieldKey(key), entry.span());
                emit_kdl_value(entry, buf);
            }
        }
    }

    // Emit properties
    for entry in &props {
        let name = entry.name().unwrap();
        let key = FieldKey::new(
            Cow::Owned(name.value().to_string()),
            FieldLocationHint::Property,
        );
        buf.push(ParseEvent::FieldKey(key), name.span());
        emit_kdl_value(entry, buf);
    }

    // Emit children - mark them as child nodes
    if let Some(children_doc) = children {
        for child in children_doc.nodes() {
            let key = FieldKey::new(
                Cow::Owned(child.name().value().to_string()),
                FieldLocationHint::Child,
            );
            buf.push(ParseEvent::FieldKey(key), child.name().span());
            emit_node_events(child, buf);
        }
    }

    buf.push(ParseEvent::StructEnd, node_span);
}

/// Emit a KDL entry's value as a ParseEvent scalar, with source span.
fn emit_kdl_value<'de>(entry: &kdl::KdlEntry, buf: &mut EventBuffer<'de>) {
    let value = entry.value();
    let span = entry.span();
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
    buf.push(ParseEvent::Scalar(scalar), span);
}
