use crate::span::Span;
use core::fmt;
use facet_core::{Field, Shape};
use facet_reflect::ReflectError;
use miette::{Diagnostic, LabeledSpan};

/// An args parsing error, with input info, so that it can be formatted nicely
#[derive(Debug)]
pub struct ArgsErrorWithInput {
    /// The inner error
    pub(crate) inner: ArgsError,

    /// All CLI arguments joined by a space
    pub(crate) flattened_args: String,
}

impl core::fmt::Display for ArgsErrorWithInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Could not parse CLI arguments")
    }
}

impl core::error::Error for ArgsErrorWithInput {}

impl Diagnostic for ArgsErrorWithInput {
    fn code<'a>(&'a self) -> Option<Box<dyn core::fmt::Display + 'a>> {
        None
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(miette::Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn core::fmt::Display + 'a>> {
        None
    }

    fn url<'a>(&'a self) -> Option<Box<dyn core::fmt::Display + 'a>> {
        None
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.flattened_args)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        Some(Box::new(core::iter::once(LabeledSpan::new(
            Some(self.inner.kind.to_string()),
            self.inner.span.start,
            self.inner.span.len(),
        ))))
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        None
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        None
    }
}

/// An args parsing error (without input info)
#[derive(Debug)]
pub struct ArgsError {
    /// Where the error occured
    pub span: Span,

    /// The specific error that occurred while parsing arguments JSON.
    pub kind: ArgsErrorKind,
}

/// An error kind for JSON parsing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ArgsErrorKind {
    /// Did not expect a positional argument at this position
    UnexpectedPositionalArgument,

    /// Wanted to look up a fiedl, for example `--something` in a struct,
    /// but the current shape was not a struct.
    NoFields { shape: &'static Shape },

    /// Passed `--something` (see span), no such long flag
    UnknownLongFlag,

    /// Passed `-j` (see span), no such short flag
    UnknownShortFlag,

    /// Struct/type expected a certain argument to be passed and it wasn't
    MissingArgument { field: &'static Field },

    /// Expected a value of type shape, got EOF
    ExpectedValueGotEof { shape: &'static Shape },

    /// Generic reflection error: something went wrong
    ReflectError(ReflectError),
}

impl core::fmt::Display for ArgsErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArgsErrorKind::UnexpectedPositionalArgument => {
                write!(f, "unexpected positional argument")
            }
            ArgsErrorKind::NoFields { shape } => {
                write!(f, "no fields available for shape: {shape}")
            }
            ArgsErrorKind::UnknownLongFlag => {
                write!(f, "unknown long flag")
            }
            ArgsErrorKind::UnknownShortFlag => {
                write!(f, "unknown short flag")
            }
            ArgsErrorKind::ExpectedValueGotEof { shape } => {
                write!(f, "expected value of type '{shape}', got end of input")
            }
            ArgsErrorKind::ReflectError(err) => {
                write!(f, "reflection error: {err}")
            }
            ArgsErrorKind::MissingArgument { field } => {
                write!(f, "missing argument: {}", field.name)
            }
        }
    }
}

impl From<ReflectError> for ArgsErrorKind {
    fn from(error: ReflectError) -> Self {
        ArgsErrorKind::ReflectError(error)
    }
}

impl ArgsError {
    /// Creates a new args error
    pub fn new(kind: ArgsErrorKind, span: Span) -> Self {
        Self { span, kind }
    }
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
