use crate::support::Span;

/// Stable diagnostic identity. Rendered prose is deliberately not the API.
///
/// r[impl lang.diagnostics.typed]
/// r[impl machine.ir.inspectable]
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DiagnosticCode {
    ParseRejected,
    DuplicateDefinition,
    InvalidTestSignature,
    UnsupportedExpression,
    TypeMismatch,
    UnknownName,
    InvalidArity,
    LoweringUnsupported,
    RuntimeInvariant,
    DuplicateBinding,
    UnknownField,
    MissingField,
    DuplicateField,
    UnknownVariant,
    DuplicateVariant,
    VariantPayloadMismatch,
    NonExhaustiveMatch,
    ExpressionStatement,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub text: String,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticPayload {
    Parse {
        detail: String,
    },
    Name {
        name: String,
    },
    Type {
        expected: String,
        found: String,
    },
    Arity {
        expected: u32,
        found: u32,
    },
    Unsupported {
        construct: String,
    },
    Invariant {
        detail: String,
    },
    Field {
        record: String,
        field: String,
    },
    Variant {
        enumeration: String,
        variant: String,
    },
    Match {
        missing: Vec<String>,
    },
    ExpressionStatement,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub primary: Span,
    pub labels: Vec<Label>,
    pub payload: DiagnosticPayload,
}

impl Diagnostic {
    #[must_use]
    pub fn unsupported(span: Span, construct: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode::UnsupportedExpression,
            primary: span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Unsupported {
                construct: construct.into(),
            },
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match &self.payload {
            DiagnosticPayload::Parse { detail } | DiagnosticPayload::Invariant { detail } => {
                detail.clone()
            }
            DiagnosticPayload::Name { name } => name.clone(),
            DiagnosticPayload::Type { expected, found } => {
                format!("expected {expected}, found {found}")
            }
            DiagnosticPayload::Arity { expected, found } => {
                format!("expected {expected} arguments, found {found}")
            }
            DiagnosticPayload::Unsupported { construct } => construct.clone(),
            DiagnosticPayload::Field { field, .. } => match self.code {
                DiagnosticCode::UnknownField => format!("unknown field {field}"),
                DiagnosticCode::MissingField => format!("missing field {field}"),
                DiagnosticCode::DuplicateField => format!("duplicate field {field}"),
                _ => field.clone(),
            },
            DiagnosticPayload::Variant { variant, .. } => match self.code {
                DiagnosticCode::UnknownVariant => format!("unknown variant {variant}"),
                DiagnosticCode::DuplicateVariant => format!("duplicate variant {variant}"),
                _ => variant.clone(),
            },
            DiagnosticPayload::Match { .. } => "non-exhaustive match".to_owned(),
            DiagnosticPayload::ExpressionStatement => "expression statement".to_owned(),
        }
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    pub entries: Vec<Diagnostic>,
}

impl Diagnostics {
    #[must_use]
    pub fn one(diagnostic: Diagnostic) -> Self {
        Self {
            entries: vec![diagnostic],
        }
    }
}

impl core::fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} Vix diagnostic(s)", self.entries.len())
    }
}

impl std::error::Error for Diagnostics {}
