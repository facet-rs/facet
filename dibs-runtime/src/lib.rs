//! Runtime types for dibs-generated query code.
//!
//! This crate re-exports all types that generated query code needs,
//! so query crates only need to depend on `dibs-runtime`.

// Re-export tokio-postgres for query execution
pub use tokio_postgres;

// Re-export facet for deriving
pub use facet;

// Re-export facet-tokio-postgres for row deserialization
pub use facet_tokio_postgres;

// Re-export common types used in generated structs
pub mod types {
    pub use dibs_jsonb::Jsonb;
    pub use facet_value;
    pub use jiff::{Timestamp, civil::Date, civil::Time};
    pub use rust_decimal::Decimal;
    pub use uuid::Uuid;
}

/// Error type for generated query functions.
#[derive(Debug)]
pub enum QueryError {
    /// Database query execution failed.
    Database(tokio_postgres::Error),
    /// Row deserialization failed.
    Deserialize(facet_tokio_postgres::Error),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::Database(e) => write!(f, "database error: {}", e),
            QueryError::Deserialize(e) => write!(f, "deserialization error: {:?}", e),
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            QueryError::Database(e) => Some(e),
            QueryError::Deserialize(_) => None,
        }
    }
}

impl From<tokio_postgres::Error> for QueryError {
    fn from(e: tokio_postgres::Error) -> Self {
        QueryError::Database(e)
    }
}

impl From<facet_tokio_postgres::Error> for QueryError {
    fn from(e: facet_tokio_postgres::Error) -> Self {
        QueryError::Deserialize(e)
    }
}

/// Extension trait that emits a structured `tracing::error!` event when
/// a query returns `Err`, then yields the `Result` unchanged.
///
/// Generated query functions wrap their entire body in `.trace_err(…)`
/// so the postgres-side detail (SQLSTATE, severity, table, column,
/// constraint, hint, detail, …) always reaches `tracing` — even when
/// the caller silently drops the `QueryError`. With JSON-formatted
/// tracing those fields become Loki labels; with text formatters they
/// land in the structured fields beside the message.
pub trait TraceErr: Sized {
    /// `query` is the styx query name (e.g. `"insert_webhook_event"`)
    /// so a log search can pivot on the originating query.
    fn trace_err(self, query: &'static str) -> Self;
}

impl<T> TraceErr for Result<T, QueryError> {
    fn trace_err(self, query: &'static str) -> Self {
        if let Err(e) = &self {
            log_query_error(query, e);
        }
        self
    }
}

fn log_query_error(query: &'static str, err: &QueryError) {
    match err {
        QueryError::Database(e) => {
            if let Some(db) = e.as_db_error() {
                tracing::error!(
                    query,
                    kind = "database",
                    sqlstate = db.code().code(),
                    severity = db.severity(),
                    db_message = db.message(),
                    detail = db.detail(),
                    hint = db.hint(),
                    schema = db.schema(),
                    table = db.table(),
                    column = db.column(),
                    constraint = db.constraint(),
                    routine = db.routine(),
                    "dibs query failed",
                );
            } else {
                tracing::error!(
                    query,
                    kind = "database",
                    sqlstate = e.code().map(|c| c.code()),
                    error = %e,
                    "dibs query failed (transport / non-db error)",
                );
            }
        }
        QueryError::Deserialize(e) => {
            tracing::error!(
                query,
                kind = "deserialize",
                error = ?e,
                "dibs row deserialization failed",
            );
        }
    }
}

// Convenient prelude for generated code
pub mod prelude {
    pub use facet::Facet;
    pub use facet_tokio_postgres::from_row;

    pub use super::QueryError;
    pub use super::TraceErr;
    pub use super::types::*;
}
