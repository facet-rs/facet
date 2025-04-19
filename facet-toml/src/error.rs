//! Errors from parsing TOML documents.

use std::ops::Range;

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
use facet_reflect::ReflectError;
use toml_edit::TomlError as LibTomlError;

/// Any error from deserializing TOML.
#[derive(Debug)]
pub struct TomlError<'input> {
    /// Type of error.
    pub kind: TomlErrorKind,
    /// Reference to the TOML source.
    input: &'input str,
}

impl<'input> TomlError<'input> {
    /// Create a new error.
    pub fn new(toml: &'input str, kind: TomlErrorKind, span: Option<Range<usize>>) -> Self {
        let input = if let Some(span) = span { todo!() } else { toml };

        Self { kind, input }
    }

    /// Message for this specific error.
    pub fn message(&self) -> String {
        match &self.kind {
            _ => todo!(),
        }
    }
}

impl<'input> core::fmt::Display for TomlError<'input> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl<'input> std::error::Error for TomlError<'input> {}

/// Type of error.
#[derive(Debug)]
pub enum TomlErrorKind {
    /// Any error from facet.
    GenericReflect(ReflectError),
    /// Parsing TOML document error.
    GenericTomlError(LibTomlError),
    /// Parsing a TOML type as a Rust type failed.
    TypeConversion {
        toml_type: &'static str,
        rust_type: &'static str,
        reason: Option<String>,
    },
    /// Expected a certain TOML type, but got something else.
    ExpectedType {
        expected: &'static str,
        got: &'static str,
    },
    /// Tried parsing a single value as a struct with multiple fields.
    ParseSingleValueAsMultipleFieldStruct,
}
