//! Lints for dibs query files.
//!
//! Each lint is in its own module for maintainability. Lints are applied
//! during diagnostics collection.

mod empty_select;
mod mutation_without_where;
mod pagination;
mod redundant_param;
mod relation;
mod soft_delete;
mod type_mismatch;
mod unknown_column;
mod unknown_table;
mod unused_param;

use dibs_proto::SchemaInfo;
use dibs_query_schema::*;
use styx_lsp_ext::{Diagnostic, DiagnosticSeverity};

pub use empty_select::*;
pub use mutation_without_where::*;
pub use pagination::*;
pub use redundant_param::*;
pub use relation::*;
pub use soft_delete::*;
pub use type_mismatch::*;
pub use unknown_column::*;
pub use unknown_table::*;
pub use unused_param::*;

/// Convert dibs_query_schema::Span to styx_tree::Span.
pub fn to_styx_span(span: Span) -> styx_tree::Span {
    styx_tree::Span {
        start: span.offset,
        end: (span.offset + span.len),
    }
}

/// Builder for creating diagnostics with less boilerplate.
pub struct DiagnosticBuilder {
    span: Option<styx_tree::Span>,
    severity: DiagnosticSeverity,
    message: String,
    code: &'static str,
}

impl DiagnosticBuilder {
    pub fn error(code: &'static str) -> Self {
        Self {
            span: None,
            severity: DiagnosticSeverity::Error,
            message: String::new(),
            code,
        }
    }

    pub fn warning(code: &'static str) -> Self {
        Self {
            span: None,
            severity: DiagnosticSeverity::Warning,
            message: String::new(),
            code,
        }
    }

    #[allow(dead_code)]
    pub fn hint(code: &'static str) -> Self {
        Self {
            span: None,
            severity: DiagnosticSeverity::Hint,
            message: String::new(),
            code,
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(to_styx_span(span));
        self
    }

    pub fn msg(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Emit to a diagnostics vec if span is present.
    pub fn emit(self, diagnostics: &mut Vec<Diagnostic>) {
        if let Some(span) = self.span {
            diagnostics.push(Diagnostic {
                span,
                severity: self.severity,
                message: self.message,
                source: Some(SOURCE.to_string()),
                code: Some(self.code.to_string()),
                data: None,
            });
        }
    }

    /// Emit with custom data if span is present.
    pub fn emit_with_data(self, diagnostics: &mut Vec<Diagnostic>, data: styx_tree::Value) {
        if let Some(span) = self.span {
            diagnostics.push(Diagnostic {
                span,
                severity: self.severity,
                message: self.message,
                source: Some(SOURCE.to_string()),
                code: Some(self.code.to_string()),
                data: Some(data),
            });
        }
    }
}

const SOURCE: &str = "dibs";

/// Context passed to lints.
pub struct LintContext<'a> {
    pub schema: &'a SchemaInfo,
    pub diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> LintContext<'a> {
    pub fn new(schema: &'a SchemaInfo, diagnostics: &'a mut Vec<Diagnostic>) -> Self {
        Self {
            schema,
            diagnostics,
        }
    }

    /// Find a table by name in the schema.
    pub fn find_table(&self, name: &str) -> Option<&'a dibs_proto::TableInfo> {
        self.schema.tables.iter().find(|t| t.name == name)
    }
}
