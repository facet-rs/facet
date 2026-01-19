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
    pub use jiff::{civil::Date, civil::Time, Timestamp};
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

// Convenient prelude for generated code
pub mod prelude {
    pub use facet::Facet;
    pub use facet_tokio_postgres::from_row;

    pub use super::types::*;
    pub use super::QueryError;
}
