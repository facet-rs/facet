//! Error types for KDL serialization and deserialization.

use std::{
    error::Error,
    fmt::{self, Debug, Display},
};

use facet_reflect::ReflectError;
use kdl::KdlError as KdlParseError;
use miette::SourceSpan;

use facet_core::Def;

/// Error type for KDL deserialization.
#[derive(Clone)]
pub struct KdlError {
    /// The specific kind of error
    pub(crate) kind: KdlErrorKind,
    /// Source code for diagnostics
    pub(crate) source_code: Option<String>,
    /// Primary span where the error occurred
    pub(crate) span: Option<SourceSpan>,
}

impl KdlError {
    /// Returns a reference to the error kind for detailed error inspection.
    pub fn kind(&self) -> &KdlErrorKind {
        &self.kind
    }

    /// Create a new error with the given kind.
    pub(crate) fn new(kind: impl Into<KdlErrorKind>) -> Self {
        KdlError {
            kind: kind.into(),
            source_code: None,
            span: None,
        }
    }

    /// Attach source code to this error for diagnostics.
    pub(crate) fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source_code = Some(source.into());
        self
    }

    /// Attach a span to this error for diagnostics.
    pub(crate) fn with_span(mut self, span: impl Into<SourceSpan>) -> Self {
        self.span = Some(span.into());
        self
    }
}

impl Display for KdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let kind = &self.kind;
        write!(f, "{kind}")
    }
}

impl Error for KdlError {}

impl Debug for KdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Build a miette::Report and forward to its Debug impl to use the global hook
        write!(f, "{:?}", miette::Report::new_boxed(Box::new(self.clone())))
    }
}

impl<K: Into<KdlErrorKind>> From<K> for KdlError {
    fn from(value: K) -> Self {
        KdlError::new(value)
    }
}

/// Detailed classification of KDL errors.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum KdlErrorKind {
    // Deserialization errors
    /// The document shape is invalid (expected struct with child/children fields).
    InvalidDocumentShape(&'static Def),
    /// Failed to parse the KDL document.
    Parse(KdlParseError),
    /// Error from the reflection system during deserialization.
    Reflect(ReflectError),
    /// Encountered an unsupported shape during deserialization.
    UnsupportedShape(String),
    /// No field matches the given node name.
    NoMatchingField(String),
    /// No property field matches the given property name.
    NoMatchingProperty(String),
    /// Unknown property encountered.
    UnknownProperty {
        /// The unknown property name.
        property: String,
        /// List of expected property names.
        expected: Vec<&'static str>,
    },
    /// No field matches the argument value.
    NoMatchingArgument,
    /// Unexpected argument after arguments list.
    UnexpectedArgument,
    /// Unsupported value definition.
    UnsupportedValueDef(String),
    /// Value doesn't fit the expected shape.
    InvalidValueForShape(String),
    /// Solver error (ambiguous or no matching variant for flattened enum).
    Solver(facet_solver::SolverError),
    /// Schema construction error.
    SchemaError(facet_solver::SchemaError),

    // Serialization errors
    /// IO error during serialization.
    Io(String),
    /// Expected a struct for KDL document serialization.
    SerializeNotStruct,
    /// Expected a list for children/arguments field.
    SerializeNotList,
    /// Unknown node type during serialization.
    SerializeUnknownNodeType,
    /// Unknown value type during serialization.
    SerializeUnknownValueType,
}

impl KdlErrorKind {
    /// Returns an error code for this error kind.
    pub fn code(&self) -> &'static str {
        match self {
            KdlErrorKind::InvalidDocumentShape(_) => "kdl::invalid_document_shape",
            KdlErrorKind::Parse(_) => "kdl::parse",
            KdlErrorKind::Reflect(_) => "kdl::reflect",
            KdlErrorKind::UnsupportedShape(_) => "kdl::unsupported_shape",
            KdlErrorKind::NoMatchingField(_) => "kdl::no_matching_field",
            KdlErrorKind::NoMatchingProperty(_) => "kdl::no_matching_property",
            KdlErrorKind::UnknownProperty { .. } => "kdl::unknown_property",
            KdlErrorKind::NoMatchingArgument => "kdl::no_matching_argument",
            KdlErrorKind::UnexpectedArgument => "kdl::unexpected_argument",
            KdlErrorKind::UnsupportedValueDef(_) => "kdl::unsupported_value_def",
            KdlErrorKind::InvalidValueForShape(_) => "kdl::invalid_value",
            KdlErrorKind::Solver(_) => "kdl::solver",
            KdlErrorKind::SchemaError(_) => "kdl::schema",
            KdlErrorKind::Io(_) => "kdl::io",
            KdlErrorKind::SerializeNotStruct => "kdl::serialize_not_struct",
            KdlErrorKind::SerializeNotList => "kdl::serialize_not_list",
            KdlErrorKind::SerializeUnknownNodeType => "kdl::serialize_unknown_node_type",
            KdlErrorKind::SerializeUnknownValueType => "kdl::serialize_unknown_value_type",
        }
    }
}

impl Display for KdlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KdlErrorKind::InvalidDocumentShape(def) => {
                write!(
                    f,
                    "invalid shape {def:#?} — needed struct with child/children fields"
                )
            }
            KdlErrorKind::Parse(kdl_error) => write!(f, "{kdl_error}"),
            KdlErrorKind::Reflect(reflect_error) => write!(f, "{reflect_error}"),
            KdlErrorKind::UnsupportedShape(msg) => write!(f, "unsupported shape: {msg}"),
            KdlErrorKind::NoMatchingField(node_name) => {
                write!(f, "no matching field for node '{node_name}'")
            }
            KdlErrorKind::NoMatchingProperty(prop_name) => {
                write!(f, "no matching property field for '{prop_name}'")
            }
            KdlErrorKind::UnknownProperty { property, expected } => {
                write!(
                    f,
                    "unknown property '{}', expected one of: {}",
                    property,
                    expected.join(", ")
                )
            }
            KdlErrorKind::NoMatchingArgument => {
                write!(f, "no matching argument field for value")
            }
            KdlErrorKind::UnexpectedArgument => {
                write!(f, "unexpected argument after arguments list")
            }
            KdlErrorKind::UnsupportedValueDef(msg) => {
                write!(f, "unsupported value definition: {msg}")
            }
            KdlErrorKind::InvalidValueForShape(msg) => {
                write!(f, "invalid value for shape: {msg}")
            }
            KdlErrorKind::Solver(e) => write!(f, "{e}"),
            KdlErrorKind::SchemaError(e) => write!(f, "schema error: {e}"),
            KdlErrorKind::Io(msg) => write!(f, "IO error: {msg}"),
            KdlErrorKind::SerializeNotStruct => {
                write!(f, "expected struct for KDL document serialization")
            }
            KdlErrorKind::SerializeNotList => {
                write!(f, "expected list for children/arguments field")
            }
            KdlErrorKind::SerializeUnknownNodeType => {
                write!(
                    f,
                    "cannot determine node name for value (expected enum or struct with node_name)"
                )
            }
            KdlErrorKind::SerializeUnknownValueType => {
                write!(f, "cannot serialize value: unknown type")
            }
        }
    }
}

impl From<KdlParseError> for KdlErrorKind {
    fn from(value: KdlParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<ReflectError> for KdlErrorKind {
    fn from(value: ReflectError) -> Self {
        Self::Reflect(value)
    }
}

impl From<facet_solver::SchemaError> for KdlErrorKind {
    fn from(value: facet_solver::SchemaError) -> Self {
        Self::SchemaError(value)
    }
}

// ============================================================================
// Diagnostic Implementation
// ============================================================================

impl miette::Diagnostic for KdlError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(self.kind.code()))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        // For parse errors, delegate to the inner kdl::KdlError which has the source
        if let KdlErrorKind::Parse(kdl_err) = &self.kind {
            return kdl_err.source_code();
        }
        self.source_code
            .as_ref()
            .map(|s| s as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // If we have a span, create a label for it
        if let Some(span) = self.span {
            let label = match &self.kind {
                KdlErrorKind::Solver(solver_err) => {
                    // For solver errors, try to get suggestion labels
                    if let Some(labels) =
                        build_solver_labels(self.source_code.as_deref(), solver_err)
                    {
                        return Some(Box::new(labels.into_iter()));
                    }
                    "error occurred here".to_string()
                }
                KdlErrorKind::UnknownProperty { property, .. } => {
                    format!("unknown property `{property}`")
                }
                KdlErrorKind::NoMatchingField(name) => {
                    format!("no field matches `{name}`")
                }
                _ => "error occurred here".to_string(),
            };
            Some(Box::new(std::iter::once(miette::LabeledSpan::at(
                span, label,
            ))))
        } else if let KdlErrorKind::Solver(solver_err) = &self.kind {
            // Even without a primary span, we might have suggestion labels
            if let Some(labels) = build_solver_labels(self.source_code.as_deref(), solver_err) {
                return Some(Box::new(labels.into_iter()));
            }
            None
        } else {
            None
        }
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        // For parse errors, delegate to the inner kdl::KdlError which has sub-diagnostics
        if let KdlErrorKind::Parse(kdl_err) = &self.kind {
            return kdl_err.related();
        }
        None
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        match &self.kind {
            KdlErrorKind::Solver(solver_err) => Some(Box::new(format_solver_help(solver_err))),
            KdlErrorKind::UnknownProperty { expected, .. } => Some(Box::new(format!(
                "expected one of: {}",
                expected.join(", ")
            ))),
            _ => None,
        }
    }
}

/// Find the byte offset of a property name in KDL source.
/// Returns (start, length) for use as a span.
fn find_property_span(source: &str, property_name: &str) -> Option<(usize, usize)> {
    // Look for "property_name=" pattern
    let pattern = format!("{property_name}=");
    if let Some(start) = source.find(&pattern) {
        return Some((start, property_name.len()));
    }
    // Also try without = (in case of different syntax)
    if let Some(start) = source.find(property_name) {
        return Some((start, property_name.len()));
    }
    None
}

/// Build labels for solver error suggestions pointing to exact locations in source.
fn build_solver_labels(
    source: Option<&str>,
    err: &facet_solver::SolverError,
) -> Option<Vec<miette::LabeledSpan>> {
    let source = source?;

    if let facet_solver::SolverError::NoMatch { suggestions, .. } = err {
        if suggestions.is_empty() {
            return None;
        }

        let mut labels = Vec::new();
        for suggestion in suggestions {
            if let Some((start, len)) = find_property_span(source, &suggestion.unknown) {
                let label = format!("did you mean `{}`?", suggestion.suggestion);
                labels.push(miette::LabeledSpan::at(start..start + len, label));
            }
        }

        if labels.is_empty() {
            return None;
        }
        return Some(labels);
    }

    None
}

/// Format help text from a SolverError.
fn format_solver_help(err: &facet_solver::SolverError) -> String {
    match err {
        facet_solver::SolverError::Ambiguous {
            candidates,
            disambiguating_fields,
        } => {
            let mut help = format!("multiple variants match: {}\n", candidates.join(", "));
            if !disambiguating_fields.is_empty() {
                help.push_str(&format!(
                    "add one of these fields to disambiguate: {}",
                    disambiguating_fields.join(", ")
                ));
            } else {
                help.push_str("use a KDL type annotation to specify the variant, e.g.: (VariantName)node-name ...");
            }
            help
        }
        facet_solver::SolverError::NoMatch {
            candidate_failures,
            suggestions,
            ..
        } => {
            let mut help = String::new();

            // Check if there's a clear "best" candidate
            let best_candidate = candidate_failures.first();
            let second_best = candidate_failures.get(1);

            let has_clear_winner = match (best_candidate, second_best) {
                (Some(best), Some(second)) => best.suggestion_matches > second.suggestion_matches,
                (Some(best), None) => best.suggestion_matches > 0,
                _ => false,
            };

            if has_clear_winner {
                let best = best_candidate.unwrap();
                help.push_str(&format!("did you mean {}?\n\n", best.variant_name));
            }

            // Show why each candidate failed
            if !candidate_failures.is_empty() {
                if has_clear_winner {
                    help.push_str("all variants checked:\n");
                } else {
                    help.push_str("no variant matched:\n");
                }
                for failure in candidate_failures {
                    help.push_str(&format!("  - {}", failure.variant_name));

                    if !failure.missing_fields.is_empty() {
                        let missing: Vec<_> =
                            failure.missing_fields.iter().map(|m| m.name).collect();
                        help.push_str(&format!(": missing {}", missing.join(", ")));
                    }
                    if !failure.unknown_fields.is_empty() {
                        if failure.missing_fields.is_empty() {
                            help.push(':');
                        } else {
                            help.push(',');
                        }
                        help.push_str(&format!(
                            " unexpected {}",
                            failure.unknown_fields.join(", ")
                        ));
                    }
                    help.push('\n');
                }
            }

            // Show "did you mean?" suggestions
            if !suggestions.is_empty() {
                help.push('\n');
                for suggestion in suggestions {
                    help.push_str(&format!(
                        "  {} -> {} (did you mean {}?)\n",
                        suggestion.unknown, suggestion.suggestion, suggestion.suggestion,
                    ));
                }
            }

            help
        }
    }
}
