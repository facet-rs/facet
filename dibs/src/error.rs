use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("postgres error: {0}")]
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
