//! Errors from parsing TOML documents.

use core::ops::Range;

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Label, Report, ReportKind, Source};
use facet_core::{Shape, TypeNameOpts};
use facet_reflect::ReflectError;

/// Any error from deserializing TOML.
pub struct TomlError<'input> {
    /// Type of error.
    pub kind: TomlErrorKind,
    /// Reference to the TOML source.
    toml: &'input str,
    /// Which part of the TOML this error applies to.
    span: Option<Range<usize>>,
}

impl<'input> TomlError<'input> {
    /// Create a new error.
    pub fn new(toml: &'input str, kind: TomlErrorKind, span: Option<Range<usize>>) -> Self {
        Self { kind, toml, span }
    }

    /// Message for this specific error.
    pub fn message(&self) -> String {
        match &self.kind {
            TomlErrorKind::GenericReflect(reflect_error) => {
                format!("Error while reflecting type: {reflect_error}")
            }
            TomlErrorKind::GenericTomlError(message) => format!("TOML error: {message}"),
            TomlErrorKind::FailedTypeConversion {
                toml_type_name,
                rust_type,
                reason,
            } => {
                let rust_type_name = TypeNameWriter(rust_type);

                if let Some(reason) = reason {
                    format!("Can't parse type '{rust_type_name}' from '{toml_type_name}': {reason}")
                } else {
                    format!("Can't parse type '{rust_type_name}' from '{toml_type_name}'")
                }
            }
            TomlErrorKind::ExpectedType { expected, got } => {
                format!("Expected type '{expected}', got type '{got}'")
            }
            TomlErrorKind::UnrecognizedType(r#type) => format!("Unrecognized type '{type}'"),
            TomlErrorKind::UnrecognizedScalar(scalar_type) => {
                format!(
                    "Unrecognized Rust scalar type '{}'",
                    TypeNameWriter(scalar_type)
                )
            }
            TomlErrorKind::InvalidKey(field) => {
                format!("Invalid Rust key '{}'", TypeNameWriter(field))
            }
            TomlErrorKind::ExpectedFieldWithName(name) => {
                format!("Expected field with name '{name}'")
            }
            TomlErrorKind::ExpectedAtLeastOneField => {
                "Expected at least one field, got zero".to_string()
            }
            TomlErrorKind::ExpectedExactlyOneField => {
                "Expected exactly one field, got multiple".to_string()
            }
            TomlErrorKind::ParseSingleValueAsMultipleFieldStruct => {
                "Can't parse a single value as a struct with multiple fields".to_string()
            }
        }
    }
}

#[cfg(not(feature = "rich-diagnostics"))]
impl core::fmt::Display for TomlError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message())
    }
}

#[cfg(feature = "rich-diagnostics")]
impl core::fmt::Display for TomlError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Don't print the TOML source if no span is set
        let Some(span) = &self.span else {
            return writeln!(f, "{}", self.message());
        };

        let source_id = "toml";

        // Create a nicely formatted report
        let mut report = Report::build(ReportKind::Error, (source_id, span.clone()))
            .with_message("Parsing TOML");

        // The inline error message in the TOML document
        let label = Label::new((source_id, span.clone()))
            .with_message(self.message())
            .with_color(Color::Red);

        report = report.with_label(label);

        // Define the TOML source code
        let source = Source::from(self.toml);

        // Write to string
        let mut writer = Vec::new();
        if let Err(e) = report.finish().write((source_id, &source), &mut writer) {
            return write!(f, "Error formatting with ariadne: {e}");
        }

        if let Ok(output) = String::from_utf8(writer) {
            write!(f, "{}", output)
        } else {
            write!(f, "Error converting ariadne output to string")
        }
    }
}

#[cfg(feature = "rich-diagnostics")]
impl core::error::Error for TomlError<'_> {}

impl core::fmt::Debug for TomlError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

/// Type of error.
#[derive(Debug, PartialEq)]
pub enum TomlErrorKind {
    /// Any error from facet.
    GenericReflect(ReflectError),
    /// Parsing TOML document error.
    GenericTomlError(String),
    /// Parsing a TOML type as a Rust type failed.
    FailedTypeConversion {
        /// TOML type that failed to convert.
        toml_type_name: &'static str,
        /// Rust that type didn't match the TOML type.
        rust_type: &'static Shape,
        /// Explanation why it failed.
        reason: Option<String>,
    },
    /// Expected a certain TOML type, but got something else.
    ExpectedType {
        /// TOML type that was expected.
        expected: &'static str,
        /// TOML type that we got.
        got: &'static str,
    },
    /// Found a TOML type that we don't know how to handle.
    UnrecognizedType(&'static str),
    /// Found a Rust scalar type that we don't know how to handle.
    UnrecognizedScalar(&'static Shape),
    /// Rust value is not a valid key.
    InvalidKey(&'static Shape),
    /// Expected a TOML field with the specified name, but couldn't find it.
    ExpectedFieldWithName(&'static str),
    /// Expected at least one field, got zero.
    ExpectedAtLeastOneField,
    /// Expected a single value, got multiple field.
    ExpectedExactlyOneField,
    /// Tried parsing a single value as a struct with multiple fields.
    ParseSingleValueAsMultipleFieldStruct,
}

/// Wrap the `Shape` in a type so we can write it's type name directly with display.
struct TypeNameWriter(&'static Shape);

impl core::fmt::Display for TypeNameWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.write_type_name(f, TypeNameOpts::default())
    }
}
