use std::panic::Location;
use thiserror::Error;

/// Rich SQL error with context for display.
#[derive(Debug, Clone)]
pub struct SqlErrorContext {
    /// The error message
    pub message: String,
    /// The SQL that caused the error
    pub sql: String,
    /// Position in the SQL where the error occurred (1-indexed byte offset)
    pub position: Option<usize>,
    /// Hint from postgres (if any)
    pub hint: Option<String>,
    /// Detail from postgres (if any)
    pub detail: Option<String>,
    /// Source location where the error occurred (file:line:col)
    pub caller: Option<String>,
}

/// Error type for migrations that captures caller location via `#[track_caller]`.
///
/// When you use `?` on a Result in a migration function, the `From` impl captures
/// the exact source location where the error occurred.
#[derive(Debug)]
pub struct MigrationError {
    /// The underlying error
    pub inner: Error,
    /// Source location where the error was converted (via `?`)
    pub caller: &'static Location<'static>,
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.inner, self.caller)
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

impl From<Error> for MigrationError {
    #[track_caller]
    fn from(e: Error) -> Self {
        MigrationError {
            inner: e,
            caller: Location::caller(),
        }
    }
}

impl From<tokio_postgres::Error> for MigrationError {
    #[track_caller]
    fn from(e: tokio_postgres::Error) -> Self {
        MigrationError {
            inner: Error::Postgres(e),
            caller: Location::caller(),
        }
    }
}

impl std::fmt::Display for SqlErrorContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(detail) = &self.detail {
            write!(f, "\nDetail: {}", detail)?;
        }
        if let Some(hint) = &self.hint {
            write!(f, "\nHint: {}", hint)?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{}", format_postgres_error(.0))]
    Postgres(#[from] tokio_postgres::Error),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("{0}")]
    SqlWithContext(SqlErrorContext),

    #[error("migration {version} has already been applied")]
    AlreadyApplied { version: String },

    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),

    #[error("unsupported type: {0}")]
    UnsupportedType(String),

    #[error("unknown table: {0}")]
    UnknownTable(String),

    #[error("unknown column: {table}.{column}")]
    UnknownColumn { table: String, column: String },

    #[error(
        "type mismatch reading {table}.{column}: database column is '{actual}', but dibs schema expects '{expected}'\nhint: you may need to run migrations to update the database schema"
    )]
    TypeMismatch {
        table: String,
        column: String,
        expected: String,
        actual: String,
    },

    #[error("failed to read {table}.{column} (expected {expected}): {message}")]
    ColumnReadError {
        table: String,
        column: String,
        expected: String,
        message: String,
    },

    #[error("connection pool error: {0}")]
    Pool(String),
}

impl Error {
    /// Create an error from a postgres error with SQL context.
    pub fn from_postgres_with_sql(err: tokio_postgres::Error, sql: &str) -> Self {
        if let Some(db_err) = err.as_db_error() {
            let position = match db_err.position() {
                Some(tokio_postgres::error::ErrorPosition::Original(pos)) => Some(*pos as usize),
                Some(tokio_postgres::error::ErrorPosition::Internal { position, .. }) => {
                    Some(*position as usize)
                }
                None => None,
            };

            Error::SqlWithContext(SqlErrorContext {
                message: format!("{}: {}", db_err.severity(), db_err.message()),
                sql: sql.to_string(),
                position,
                hint: db_err.hint().map(|s| s.to_string()),
                detail: db_err.detail().map(|s| s.to_string()),
                caller: None, // Would need macro-based approach for async fn caller tracking
            })
        } else {
            // Fall back to simple error
            Error::Migration(err.to_string())
        }
    }

    /// Get SQL context if this is a SqlWithContext error.
    pub fn sql_context(&self) -> Option<&SqlErrorContext> {
        match self {
            Error::SqlWithContext(ctx) => Some(ctx),
            _ => None,
        }
    }
}

/// Format a postgres error with full details from DbError if available.
fn format_postgres_error(err: &tokio_postgres::Error) -> String {
    // Try to get the underlying DbError which has the actual details
    if let Some(db_err) = err.as_db_error() {
        let mut msg = format!("{}: {}", db_err.severity(), db_err.message());

        if let Some(detail) = db_err.detail() {
            msg.push_str(&format!("\nDetail: {}", detail));
        }
        if let Some(hint) = db_err.hint() {
            msg.push_str(&format!("\nHint: {}", hint));
        }
        if let Some(where_) = db_err.where_() {
            msg.push_str(&format!("\nWhere: {}", where_));
        }
        if let Some(schema) = db_err.schema() {
            msg.push_str(&format!("\nSchema: {}", schema));
        }
        if let Some(table) = db_err.table() {
            msg.push_str(&format!("\nTable: {}", table));
        }
        if let Some(column) = db_err.column() {
            msg.push_str(&format!("\nColumn: {}", column));
        }
        if let Some(constraint) = db_err.constraint() {
            msg.push_str(&format!("\nConstraint: {}", constraint));
        }
        if let Some(position) = db_err.position() {
            msg.push_str(&format!("\nPosition: {:?}", position));
        }

        msg
    } else {
        // Fall back to the standard error message
        err.to_string()
    }
}
