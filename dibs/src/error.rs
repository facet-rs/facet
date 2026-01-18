use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{}", format_postgres_error(.0))]
    Postgres(#[from] tokio_postgres::Error),

    #[error("migration failed: {0}")]
    Migration(String),

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
