//! Errors from parsing TOML documents.

use core::ops::Range;

use alloc::{
    format,
    string::{String, ToString},
};
#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Label, Report, ReportKind, Source};
use facet_core::Shape;
use facet_reflect::ReflectError;
use owo_colors::OwoColorize;

/// Any error from deserializing TOML.
pub struct TomlDeError<'input, 'shape> {
    /// Type of error.
    pub kind: TomlDeErrorKind<'shape>,
    /// Reference to the TOML source.
    #[cfg_attr(not(feature = "rich-diagnostics"), allow(dead_code))]
    toml: &'input str,
    /// Which part of the TOML this error applies to.
    #[cfg_attr(not(feature = "rich-diagnostics"), allow(dead_code))]
    span: Option<Range<usize>>,
    /// Full Wip path.
    path: String,
}

impl<'input, 'shape> TomlDeError<'input, 'shape> {
    /// Create a new error.
    pub fn new(
        toml: &'input str,
        kind: TomlDeErrorKind<'shape>,
        span: Option<Range<usize>>,
        path: String,
    ) -> Self {
        Self {
            kind,
            toml,
            span,
            path,
        }
    }

    /// Message for this specific error.
    pub fn message(&self) -> String {
        match &self.kind {
            TomlDeErrorKind::GenericReflect(reflect_error) => {
                format!("Error while reflecting type: {reflect_error}")
            }
            TomlDeErrorKind::GenericTomlError(message) => format!("TOML error: {message}"),
            TomlDeErrorKind::FailedTypeConversion {
                toml_type_name,
                rust_type,
                reason,
            } => {
                if let Some(reason) = reason {
                    format!("Can't parse type '{rust_type}' from '{toml_type_name}': {reason}")
                } else {
                    format!("Can't parse type '{rust_type}' from '{toml_type_name}'")
                }
            }
            TomlDeErrorKind::ExpectedType { expected, got } => {
                format!("Expected type '{expected}', got type '{got}'")
            }
            TomlDeErrorKind::UnrecognizedType(r#type) => format!("Unrecognized type '{type}'"),
            TomlDeErrorKind::UnrecognizedScalar(scalar_type) => {
                format!("Unrecognized Rust scalar type '{scalar_type}'",)
            }
            TomlDeErrorKind::InvalidKey(field) => {
                format!("Invalid Rust key '{field}'")
            }
            TomlDeErrorKind::ExpectedFieldWithName(name) => {
                format!("Expected field with name '{name}'")
            }
            TomlDeErrorKind::ExpectedAtLeastOneField => {
                "Expected at least one field, got zero".to_string()
            }
            TomlDeErrorKind::ExpectedExactlyOneField => {
                "Expected exactly one field, got multiple".to_string()
            }
            TomlDeErrorKind::ParseSingleValueAsMultipleFieldStruct => {
                "Can't parse a single value as a struct with multiple fields".to_string()
            }
        }
    }
}

#[cfg(not(feature = "rich-diagnostics"))]
impl<'shape> core::fmt::Display for TomlDeError<'_, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} in path {}", self.message(), self.path)
    }
}

#[cfg(feature = "rich-diagnostics")]
impl<'shape> core::fmt::Display for TomlDeError<'_, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Don't print the TOML source if no span is set
        let Some(span) = &self.span else {
            return writeln!(f, "{} in path {}", self.message(), self.path);
        };

        let source_id = "toml";

        // Create a nicely formatted report
        let mut report = Report::build(ReportKind::Error, (source_id, span.clone()))
            .with_message(format!("Error at {}", self.path.yellow()));

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

impl<'shape> core::error::Error for TomlDeError<'_, 'shape> {}

impl<'shape> core::fmt::Debug for TomlDeError<'_, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

/// Type of error.
#[derive(Debug, PartialEq)]
pub enum TomlDeErrorKind<'shape> {
    /// Any error from facet.
    GenericReflect(ReflectError<'shape>),
    /// Parsing TOML document error.
    GenericTomlError(String),
    /// Parsing a TOML type as a Rust type failed.
    FailedTypeConversion {
        /// TOML type that failed to convert.
        toml_type_name: &'static str,
        /// Rust that type didn't match the TOML type.
        rust_type: &'shape Shape<'shape>,
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
    UnrecognizedScalar(&'shape Shape<'shape>),
    /// Rust value is not a valid key.
    InvalidKey(&'shape Shape<'shape>),
    /// Expected a TOML field with the specified name, but couldn't find it.
    ExpectedFieldWithName(&'shape str),
    /// Expected at least one field, got zero.
    ExpectedAtLeastOneField,
    /// Expected a single value, got multiple field.
    ExpectedExactlyOneField,
    /// Tried parsing a single value as a struct with multiple fields.
    ParseSingleValueAsMultipleFieldStruct,
}
