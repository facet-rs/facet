//! Roam service API for dibs.
//!
//! This module defines the RPC interface between the `dibs` CLI and
//! the user's db crate (e.g., `my-app-db`).
//!
//! The db crate runs as a short-lived roam service, responding to
//! schema and migration queries from the CLI.

use facet::Facet;
use roam::service;

/// Schema information for a table.
#[derive(Debug, Clone, Facet)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Column definitions
    pub columns: Vec<ColumnInfo>,
    /// Foreign key constraints
    pub foreign_keys: Vec<ForeignKeyInfo>,
    /// Indices
    pub indices: Vec<IndexInfo>,
    /// Source file (if known)
    pub source_file: Option<String>,
    /// Source line (if known)
    pub source_line: Option<u32>,
    /// Doc comment (if any)
    pub doc: Option<String>,
}

/// Column information.
#[derive(Debug, Clone, Facet)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// SQL type (e.g., "BIGINT", "TEXT")
    pub sql_type: String,
    /// Whether the column is nullable
    pub nullable: bool,
    /// Default value expression (if any)
    pub default: Option<String>,
    /// Whether this is a primary key
    pub primary_key: bool,
    /// Whether this has a unique constraint
    pub unique: bool,
}

/// Foreign key information.
#[derive(Debug, Clone, Facet)]
pub struct ForeignKeyInfo {
    /// Columns in this table
    pub columns: Vec<String>,
    /// Referenced table
    pub references_table: String,
    /// Referenced columns
    pub references_columns: Vec<String>,
}

/// Index information.
#[derive(Debug, Clone, Facet)]
pub struct IndexInfo {
    /// Index name
    pub name: String,
    /// Columns in the index
    pub columns: Vec<String>,
    /// Whether this is a unique index
    pub unique: bool,
}

/// The full schema (list of tables).
#[derive(Debug, Clone, Facet)]
pub struct SchemaInfo {
    /// All tables in the schema
    pub tables: Vec<TableInfo>,
}

/// A single schema change.
#[derive(Debug, Clone, Facet)]
pub struct ChangeInfo {
    /// Human-readable description of the change
    pub description: String,
    /// Change kind (for coloring/icons)
    pub kind: ChangeKind,
}

/// Kind of schema change.
#[derive(Debug, Clone, Copy, Facet)]
#[repr(u8)]
pub enum ChangeKind {
    /// Something is being added
    Add = 0,
    /// Something is being removed
    Drop = 1,
    /// Something is being modified
    Alter = 2,
}

/// Diff result for a single table.
#[derive(Debug, Clone, Facet)]
pub struct TableDiffInfo {
    /// Table name
    pub table: String,
    /// Changes for this table
    pub changes: Vec<ChangeInfo>,
}

/// Full diff result.
#[derive(Debug, Clone, Facet)]
pub struct DiffResult {
    /// Diffs organized by table
    pub table_diffs: Vec<TableDiffInfo>,
}

/// Migration status.
#[derive(Debug, Clone, Facet)]
pub struct MigrationInfo {
    /// Migration version/name
    pub version: String,
    /// Human-readable name
    pub name: String,
    /// Whether this migration has been applied
    pub applied: bool,
    /// When it was applied (if applied)
    pub applied_at: Option<String>,
}

/// Request to diff schema against a database.
#[derive(Debug, Clone, Facet)]
pub struct DiffRequest {
    /// Database connection URL
    pub database_url: String,
}

/// Request to get migration status.
#[derive(Debug, Clone, Facet)]
pub struct MigrationStatusRequest {
    /// Database connection URL
    pub database_url: String,
}

/// Request to run migrations.
#[derive(Debug, Clone, Facet)]
pub struct MigrateRequest {
    /// Database connection URL
    pub database_url: String,
    /// Specific migration to run (if None, run all pending)
    pub migration: Option<String>,
}

/// Result of running migrations.
#[derive(Debug, Clone, Facet)]
pub struct MigrateResult {
    /// Migrations that were applied
    pub applied: Vec<String>,
    /// Total execution time in milliseconds
    pub total_time_ms: u64,
}

/// Log message streamed during migration.
#[derive(Debug, Clone, Facet)]
pub struct MigrationLog {
    /// Log level
    pub level: LogLevel,
    /// Message
    pub message: String,
    /// Migration this log is from (if applicable)
    pub migration: Option<String>,
}

/// Log level.
#[derive(Debug, Clone, Copy, Facet)]
#[repr(u8)]
pub enum LogLevel {
    /// Debug information
    Debug = 0,
    /// Informational message
    Info = 1,
    /// Warning
    Warn = 2,
    /// Error
    Error = 3,
}

/// Error from the dibs service.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum DibsError {
    /// Database connection failed
    ConnectionFailed(String) = 0,
    /// Migration failed
    MigrationFailed(String) = 1,
    /// Invalid request
    InvalidRequest(String) = 2,
}

/// The dibs service trait.
///
/// Implemented by the user's db crate, called by the dibs CLI.
#[service]
pub trait DibsService {
    /// Get the schema defined in Rust code.
    async fn schema(&self) -> SchemaInfo;

    /// Diff the Rust schema against a live database.
    async fn diff(&self, request: DiffRequest) -> Result<DiffResult, DibsError>;

    /// Get migration status (applied vs pending).
    async fn migration_status(
        &self,
        request: MigrationStatusRequest,
    ) -> Result<Vec<MigrationInfo>, DibsError>;

    /// Run migrations, streaming logs back.
    async fn migrate(
        &self,
        request: MigrateRequest,
        logs: roam::Tx<MigrationLog>,
    ) -> Result<MigrateResult, DibsError>;
}
