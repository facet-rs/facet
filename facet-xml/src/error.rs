//! Error types for XML serialization and deserialization.

use std::{
    error::Error,
    fmt::{self, Display},
};

use facet_core::Def;
use facet_reflect::ReflectError;
use miette::SourceSpan;

/// Error type for XML deserialization.
#[derive(Debug)]
pub struct XmlError {
    /// The specific kind of error
    pub(crate) kind: XmlErrorKind,
    /// Source code for diagnostics
    pub(crate) source_code: Option<String>,
    /// Primary span where the error occurred
    pub(crate) span: Option<SourceSpan>,
}

impl XmlError {
    /// Returns a reference to the error kind for detailed error inspection.
    pub fn kind(&self) -> &XmlErrorKind {
        &self.kind
    }

    /// Create a new error with the given kind.
    pub(crate) fn new(kind: impl Into<XmlErrorKind>) -> Self {
        XmlError {
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

impl Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let kind = &self.kind;
        write!(f, "{kind}")
    }
}

impl Error for XmlError {}

impl<K: Into<XmlErrorKind>> From<K> for XmlError {
    fn from(value: K) -> Self {
        XmlError::new(value)
    }
}

/// Public phase indicator for [`XmlErrorKind::MissingXmlAnnotations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingAnnotationPhase {
    /// Error triggered while serializing.
    Serialize,
    /// Error triggered while deserializing.
    Deserialize,
}

/// Detailed classification of XML errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum XmlErrorKind {
    // Deserialization errors
    /// The document shape is invalid (expected struct with element fields).
    InvalidDocumentShape(&'static Def),
    /// Failed to parse the XML document.
    Parse(String),
    /// Error from the reflection system during deserialization.
    Reflect(ReflectError),
    /// Encountered an unsupported shape during deserialization.
    UnsupportedShape(String),
    /// No field matches the given element name.
    NoMatchingElement(String),
    /// No field matches the given attribute name.
    NoMatchingAttribute(String),
    /// Unknown attribute encountered.
    UnknownAttribute {
        /// The unknown attribute name.
        attribute: String,
        /// List of expected attribute names.
        expected: Vec<&'static str>,
    },
    /// No text field found for text content.
    NoTextField,
    /// Unexpected text content.
    UnexpectedText,
    /// Unsupported value definition.
    UnsupportedValueDef(String),
    /// Value doesn't fit the expected shape.
    InvalidValueForShape(String),
    /// Solver error (ambiguous or no matching variant for flattened enum).
    Solver(facet_solver::SolverError),
    /// Schema construction error.
    SchemaError(facet_solver::SchemaError),
    /// Unexpected end of input.
    UnexpectedEof,
    /// Unexpected XML event.
    UnexpectedEvent(String),
    /// Missing required element.
    MissingElement(String),
    /// Missing required attribute.
    MissingAttribute(String),
    /// Invalid attribute value.
    InvalidAttributeValue {
        /// The attribute name.
        name: String,
        /// The invalid value.
        value: String,
        /// The expected type.
        expected_type: String,
    },

    /// Unknown field encountered when deny_unknown_fields is set.
    UnknownField {
        /// The unknown field name.
        field: String,
        /// List of expected field names.
        expected: Vec<&'static str>,
    },
    /// Invalid UTF-8 in input.
    InvalidUtf8(String),
    /// Base64 decoding error.
    Base64Decode(String),

    // Serialization errors
    /// IO error during serialization.
    Io(String),
    /// Expected a struct for XML document serialization.
    SerializeNotStruct,
    /// Expected a list for elements field.
    SerializeNotList,
    /// Unknown element type during serialization.
    SerializeUnknownElementType,
    /// Unknown value type during serialization.
    SerializeUnknownValueType,
    /// Struct fields lack XML annotations, so they cannot be mapped automatically.
    MissingXmlAnnotations {
        /// Fully-qualified type name of the struct.
        type_name: &'static str,
        /// Whether the failure happened while serializing or deserializing.
        phase: MissingAnnotationPhase,
        /// Offending fields paired with their Rust type identifiers.
        fields: Vec<(&'static str, &'static str)>,
    },
}

impl XmlErrorKind {
    /// Returns an error code for this error kind.
    pub fn code(&self) -> &'static str {
        match self {
            XmlErrorKind::InvalidDocumentShape(_) => "xml::invalid_document_shape",
            XmlErrorKind::Parse(_) => "xml::parse",
            XmlErrorKind::Reflect(_) => "xml::reflect",
            XmlErrorKind::UnsupportedShape(_) => "xml::unsupported_shape",
            XmlErrorKind::NoMatchingElement(_) => "xml::no_matching_element",
            XmlErrorKind::NoMatchingAttribute(_) => "xml::no_matching_attribute",
            XmlErrorKind::UnknownAttribute { .. } => "xml::unknown_attribute",
            XmlErrorKind::NoTextField => "xml::no_text_field",
            XmlErrorKind::UnexpectedText => "xml::unexpected_text",
            XmlErrorKind::UnsupportedValueDef(_) => "xml::unsupported_value_def",
            XmlErrorKind::InvalidValueForShape(_) => "xml::invalid_value",
            XmlErrorKind::Solver(_) => "xml::solver",
            XmlErrorKind::SchemaError(_) => "xml::schema",
            XmlErrorKind::UnexpectedEof => "xml::unexpected_eof",
            XmlErrorKind::UnexpectedEvent(_) => "xml::unexpected_event",
            XmlErrorKind::MissingElement(_) => "xml::missing_element",
            XmlErrorKind::MissingAttribute(_) => "xml::missing_attribute",
            XmlErrorKind::InvalidAttributeValue { .. } => "xml::invalid_attribute_value",
            XmlErrorKind::UnknownField { .. } => "xml::unknown_field",
            XmlErrorKind::InvalidUtf8(_) => "xml::invalid_utf8",
            XmlErrorKind::Base64Decode(_) => "xml::base64_decode",
            XmlErrorKind::Io(_) => "xml::io",
            XmlErrorKind::SerializeNotStruct => "xml::serialize_not_struct",
            XmlErrorKind::SerializeNotList => "xml::serialize_not_list",
            XmlErrorKind::SerializeUnknownElementType => "xml::serialize_unknown_element_type",
            XmlErrorKind::SerializeUnknownValueType => "xml::serialize_unknown_value_type",
            XmlErrorKind::MissingXmlAnnotations { .. } => "xml::missing_xml_annotations",
        }
    }
}

impl Display for XmlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlErrorKind::InvalidDocumentShape(def) => {
                write!(
                    f,
                    "invalid shape {def:#?} — expected struct with element/attribute fields"
                )
            }
            XmlErrorKind::Parse(msg) => write!(f, "XML parse error: {msg}"),
            XmlErrorKind::Reflect(reflect_error) => write!(f, "{reflect_error}"),
            XmlErrorKind::UnsupportedShape(msg) => write!(f, "unsupported shape: {msg}"),
            XmlErrorKind::NoMatchingElement(element_name) => {
                write!(f, "no matching field for element '{element_name}'")
            }
            XmlErrorKind::NoMatchingAttribute(attr_name) => {
                write!(f, "no matching field for attribute '{attr_name}'")
            }
            XmlErrorKind::UnknownAttribute {
                attribute,
                expected,
            } => {
                write!(
                    f,
                    "unknown attribute '{}', expected one of: {}",
                    attribute,
                    expected.join(", ")
                )
            }
            XmlErrorKind::NoTextField => {
                write!(f, "no field marked with xml::text to receive text content")
            }
            XmlErrorKind::UnexpectedText => {
                write!(f, "unexpected text content")
            }
            XmlErrorKind::UnsupportedValueDef(msg) => {
                write!(f, "unsupported value definition: {msg}")
            }
            XmlErrorKind::InvalidValueForShape(msg) => {
                write!(f, "invalid value for shape: {msg}")
            }
            XmlErrorKind::Solver(e) => write!(f, "{e}"),
            XmlErrorKind::SchemaError(e) => write!(f, "schema error: {e}"),
            XmlErrorKind::UnexpectedEof => write!(f, "unexpected end of XML input"),
            XmlErrorKind::UnexpectedEvent(msg) => write!(f, "unexpected XML event: {msg}"),
            XmlErrorKind::MissingElement(name) => write!(f, "missing required element '{name}'"),
            XmlErrorKind::MissingAttribute(name) => {
                write!(f, "missing required attribute '{name}'")
            }
            XmlErrorKind::InvalidAttributeValue {
                name,
                value,
                expected_type,
            } => {
                write!(
                    f,
                    "invalid value '{value}' for attribute '{name}', expected {expected_type}"
                )
            }
            XmlErrorKind::UnknownField { field, expected } => {
                write!(
                    f,
                    "unknown field '{}', expected one of: {}",
                    field,
                    expected.join(", ")
                )
            }
            XmlErrorKind::InvalidUtf8(msg) => write!(f, "invalid UTF-8: {msg}"),
            XmlErrorKind::Base64Decode(msg) => write!(f, "base64 decode error: {msg}"),
            XmlErrorKind::Io(msg) => write!(f, "IO error: {msg}"),
            XmlErrorKind::SerializeNotStruct => {
                write!(f, "expected struct for XML document serialization")
            }
            XmlErrorKind::SerializeNotList => {
                write!(f, "expected list for elements field")
            }
            XmlErrorKind::SerializeUnknownElementType => {
                write!(
                    f,
                    "cannot determine element name for value (expected enum or struct with element_name)"
                )
            }
            XmlErrorKind::SerializeUnknownValueType => {
                write!(f, "cannot serialize value: unknown type")
            }
            XmlErrorKind::MissingXmlAnnotations {
                type_name,
                phase,
                fields,
            } => {
                let verb = match phase {
                    MissingAnnotationPhase::Serialize => "serialize",
                    MissingAnnotationPhase::Deserialize => "deserialize",
                };

                write!(
                    f,
                    "{type_name} cannot {verb} because these fields lack XML annotations: "
                )?;
                for (idx, (field, ty)) in fields.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}: {ty}")?;
                }
                write!(
                    f,
                    ". Each field must opt into XML via one of:\n\
                     • #[facet(xml::attribute)]  → <{type_name} field=\"…\" /> (attributes)\n\
                     • #[facet(xml::element)]    → <{type_name}><field>…</field></{type_name}> (single child)\n\
                     • #[facet(xml::elements)]   → <{type_name}><field>…</field>…</{type_name}> (lists of children)\n\
                     • #[facet(xml::text)]       → <{type_name}>…</{type_name}> (text content)\n\
                     • #[facet(xml::element_name)] to capture the element/tag name itself.\n\
                     `#[facet(child)]` is accepted as shorthand for xml::element. \
                     Use #[facet(flatten)] or #[facet(skip*)] if the field should be omitted."
                )
            }
        }
    }
}

impl From<ReflectError> for XmlErrorKind {
    fn from(value: ReflectError) -> Self {
        Self::Reflect(value)
    }
}

impl From<facet_solver::SchemaError> for XmlErrorKind {
    fn from(value: facet_solver::SchemaError) -> Self {
        Self::SchemaError(value)
    }
}

// ============================================================================
// Diagnostic Implementation
// ============================================================================

impl miette::Diagnostic for XmlError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(self.kind.code()))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source_code
            .as_ref()
            .map(|s| s as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        if let Some(span) = self.span {
            let label = match &self.kind {
                XmlErrorKind::UnknownAttribute { attribute, .. } => {
                    format!("unknown attribute `{attribute}`")
                }
                XmlErrorKind::NoMatchingElement(name) => {
                    format!("no field matches `{name}`")
                }
                XmlErrorKind::NoMatchingAttribute(name) => {
                    format!("no field matches attribute `{name}`")
                }
                XmlErrorKind::MissingElement(name) => {
                    format!("missing element `{name}`")
                }
                XmlErrorKind::MissingAttribute(name) => {
                    format!("missing attribute `{name}`")
                }
                XmlErrorKind::UnknownField { field, .. } => {
                    format!("unknown field `{field}`")
                }
                _ => "error occurred here".to_string(),
            };
            Some(Box::new(std::iter::once(miette::LabeledSpan::at(
                span, label,
            ))))
        } else {
            None
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        match &self.kind {
            XmlErrorKind::UnknownAttribute { expected, .. } => Some(Box::new(format!(
                "expected one of: {}",
                expected.join(", ")
            ))),
            XmlErrorKind::NoTextField => Some(Box::new(
                "add #[facet(xml::text)] to a String field to capture text content",
            )),
            XmlErrorKind::UnknownField { expected, .. } => Some(Box::new(format!(
                "expected one of: {}",
                expected.join(", ")
            ))),
            _ => None,
        }
    }
}
