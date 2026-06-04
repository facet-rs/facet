//! Query generation errors.

use camino::Utf8PathBuf;
use dibs_query_schema::Span;
use std::fmt;
use std::sync::Arc;

// ============================================================================
// Error Handling Types
// ============================================================================

#[derive(Clone)]
pub struct QSource {
    /// The original source code (for rendering diagnostics)
    pub source: String,

    /// Path to the source file
    pub source_path: Utf8PathBuf,
}

/// Error during code generation.
/// Carries span information for proper error reporting.
#[derive(Clone)]
pub struct QError {
    /// Source
    pub source: Arc<QSource>,

    /// Location in the source .styx file
    pub span: Span,

    /// Error classification and details
    pub kind: QErrorKind,
}

/// Error classification for query generation.
#[derive(Debug, Clone)]
pub enum QErrorKind {
    /// A column referenced in the query does not exist in the table.
    ColumnNotFound {
        /// The table that was searched.
        table: String,

        /// The column that was not found.
        column: String,

        /// The columns that the table *does* have (for "did you mean" hints).
        available: Vec<String>,
    },

    /// A table referenced in the query does not exist in the schema.
    TableNotFound {
        /// The table that was not found.
        table: String,

        /// The tables that the schema *does* have. Empty means the schema
        /// itself is empty — almost always a missing `ensure_linked()` call.
        available: Vec<String>,
    },

    /// The query references a column that exists but has incompatible properties.
    SchemaMismatch {
        /// The table containing the column.
        table: String,

        /// The column with the mismatch.
        column: String,

        /// Description of the mismatch.
        reason: String,
    },

    /// The query planner failed to produce a plan.
    PlanMissing {
        /// Why the plan could not be generated.
        reason: String,
    },

    /// Failed to parse the styx source file.
    Parse {
        /// The parse error message.
        message: String,
    },

    /// Invalid filter arguments (wrong count)
    InvalidFilterArgCount {
        /// Name of the filter
        filter: String,

        /// Expected number of arguments
        expected: usize,

        /// Actual number of arguments
        actual: usize,
    },

    /// Invalid filter argument type
    InvalidFilterArgType {
        /// Name of the filter
        filter: String,

        /// Description of the type mismatch
        reason: String,
    },
}

impl fmt::Display for QErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QErrorKind::ColumnNotFound {
                table,
                column,
                available,
            } => {
                write!(f, "column '{}' not found in table '{}'", column, table)?;
                if available.is_empty() {
                    write!(f, " (table has no columns)")
                } else {
                    write!(f, " (table has: {})", available.join(", "))
                }
            }
            QErrorKind::TableNotFound { table, available } => {
                if available.is_empty() {
                    write!(
                        f,
                        "table '{table}' not found: the schema is empty (0 tables). \
                         The crate defining your #[facet(dibs::table)] types was probably \
                         not linked, so dibs collected zero tables — call its \
                         `ensure_linked()` in build.rs before generating queries (a \
                         `TypeId::of`/`type_name` reference does NOT force linkage)."
                    )
                } else {
                    write!(
                        f,
                        "table '{table}' not found: the schema has {} table(s): {}",
                        available.len(),
                        available.join(", ")
                    )
                }
            }
            QErrorKind::SchemaMismatch {
                table,
                column,
                reason,
            } => {
                write!(f, "schema mismatch for '{}.{}': {}", table, column, reason)
            }
            QErrorKind::PlanMissing { reason } => {
                write!(f, "query plan missing: {}", reason)
            }
            QErrorKind::Parse { message } => {
                write!(f, "{}", message)
            }
            QErrorKind::InvalidFilterArgCount {
                filter,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "invalid arguments for filter '{}': expected {} arguments, got {}",
                    filter, expected, actual
                )
            }
            QErrorKind::InvalidFilterArgType { filter, reason } => {
                write!(f, "invalid argument for filter '{}': {}", filter, reason)
            }
        }
    }
}

impl fmt::Debug for QError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for QError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ariadne::{Config, Label, Report, ReportKind, Source};

        let message = self.kind.to_string();
        let start = self.span.offset as usize;
        let end = start + self.span.len as usize;

        let mut output = Vec::new();

        let report = Report::build(ReportKind::Error, (&self.source.source_path, start..end))
            .with_message(&message)
            .with_config(Config::default().with_color(false))
            .with_label(Label::new((&self.source.source_path, start..end)).with_message(&message))
            .finish();

        report
            .write(
                (&self.source.source_path, Source::from(&self.source.source)),
                &mut output,
            )
            .ok();

        write!(f, "{}", String::from_utf8_lossy(&output))
    }
}

impl std::error::Error for QError {}
