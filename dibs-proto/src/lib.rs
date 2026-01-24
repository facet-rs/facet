//! Protocol definitions for dibs CLI-to-service communication.
//!
//! This crate defines the roam service interface between the `dibs` CLI
//! and the user's db crate (e.g., `my-app-db`).
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
    /// Lucide icon name for display in admin UI
    pub icon: Option<String>,
}

/// Column information.
#[derive(Debug, Clone, Facet)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// SQL type (e.g., "BIGINT", "TEXT")
    pub sql_type: String,
    /// Rust type name (e.g., "i64", "String", "jiff::Timestamp")
    pub rust_type: Option<String>,
    /// Whether the column is nullable
    pub nullable: bool,
    /// Default value expression (if any)
    pub default: Option<String>,
    /// Whether this is a primary key
    pub primary_key: bool,
    /// Whether this has a unique constraint
    pub unique: bool,
    /// Whether this column is auto-generated (serial, uuid default, etc.)
    pub auto_generated: bool,
    /// Whether this is a long text field (use textarea instead of input)
    pub long: bool,
    /// Whether this column should be used as the display label for the row
    pub label: bool,
    /// Enum variants (if this is an enum type)
    pub enum_variants: Vec<String>,
    /// Doc comment (if any)
    pub doc: Option<String>,
    /// Language/format for code editor (e.g., "markdown", "json")
    pub lang: Option<String>,
    /// Lucide icon name for display in admin UI
    pub icon: Option<String>,
    /// Semantic subtype of the column (e.g., "email", "url", "password")
    pub subtype: Option<String>,
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
    /// Optional WHERE clause for partial indexes
    pub where_clause: Option<String>,
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
    /// Source file path (if known)
    pub source_file: Option<String>,
    /// Source code (if available)
    pub source: Option<String>,
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

/// SQL error with context for rich error display.
#[derive(Debug, Clone, Facet)]
pub struct SqlError {
    /// The error message
    pub message: String,
    /// The SQL that caused the error (if available)
    pub sql: Option<String>,
    /// Position in the SQL where the error occurred (1-indexed byte offset)
    pub position: Option<u32>,
    /// Hint from postgres (if any)
    pub hint: Option<String>,
    /// Detail from postgres (if any)
    pub detail: Option<String>,
    /// Source location where the error occurred (file:line:col)
    pub caller: Option<String>,
}

/// Error from the dibs service.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum DibsError {
    /// Database connection failed
    ConnectionFailed(String) = 0,
    /// Migration failed with SQL context
    MigrationFailed(SqlError) = 1,
    /// Invalid request
    InvalidRequest(String) = 2,
    /// Unknown table
    UnknownTable(String) = 3,
    /// Unknown column
    UnknownColumn(String) = 4,
    /// Query error
    QueryError(String) = 5,
}

// =============================================================================
// Backoffice types
// =============================================================================

/// A runtime value for backoffice queries.
///
/// Mirrors the internal dibs::query::Value type for wire transmission.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum Value {
    /// NULL
    Null = 0,
    /// Boolean
    Bool(bool) = 1,
    /// 16-bit integer
    I16(i16) = 2,
    /// 32-bit integer
    I32(i32) = 3,
    /// 64-bit integer
    I64(i64) = 4,
    /// 32-bit float
    F32(f32) = 5,
    /// 64-bit float
    F64(f64) = 6,
    /// String
    String(String) = 7,
    /// Binary data
    Bytes(Vec<u8>) = 8,
}

/// A row of data as field name → value pairs.
#[derive(Debug, Clone, Facet)]
pub struct Row {
    /// Fields in the row
    pub fields: Vec<RowField>,
}

/// A single field in a row.
#[derive(Debug, Clone, Facet)]
pub struct RowField {
    /// Field name
    pub name: String,
    /// Field value
    pub value: Value,
}

/// Filter operator for backoffice queries.
#[derive(Debug, Clone, Copy, Facet)]
#[repr(u8)]
pub enum FilterOp {
    /// Equal (=)
    Eq,
    /// Not equal (!=)
    Ne,
    /// Less than (<)
    Lt,
    /// Less than or equal (<=)
    Lte,
    /// Greater than (>)
    Gt,
    /// Greater than or equal (>=)
    Gte,
    /// LIKE pattern match
    Like,
    /// Case-insensitive LIKE
    ILike,
    /// IS NULL
    IsNull,
    /// IS NOT NULL
    IsNotNull,
    /// IN (value1, value2, ...) - uses `values` field instead of `value`
    In,
    /// JSONB get object operator (->)
    JsonGet,
    /// JSONB get text operator (->>)
    JsonGetText,
    /// Contains operator (@>)
    Contains,
    /// Key exists operator (?)
    KeyExists,
}

/// A single filter condition.
#[derive(Debug, Clone, Facet)]
pub struct Filter {
    /// Column name
    pub field: String,
    /// Operator
    pub op: FilterOp,
    /// Value to compare (ignored for IsNull/IsNotNull/In)
    pub value: Value,
    /// Values for IN operator
    pub values: Vec<Value>,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, Facet)]
#[repr(u8)]
pub enum SortDir {
    /// Ascending
    Asc = 0,
    /// Descending
    Desc = 1,
}

/// A sort clause.
#[derive(Debug, Clone, Facet)]
pub struct Sort {
    /// Column name
    pub field: String,
    /// Direction
    pub dir: SortDir,
}

/// Request to list rows from a table.
#[derive(Debug, Clone, Facet)]
pub struct ListRequest {
    /// Database connection URL
    pub database_url: String,
    /// Table name
    pub table: String,
    /// Filter conditions (ANDed together)
    pub filters: Vec<Filter>,
    /// Sort order
    pub sort: Vec<Sort>,
    /// Maximum rows to return
    pub limit: Option<u32>,
    /// Offset for pagination
    pub offset: Option<u32>,
    /// Columns to select (empty = all)
    pub select: Vec<String>,
}

/// Response from listing rows.
#[derive(Debug, Clone, Facet)]
pub struct ListResponse {
    /// The rows
    pub rows: Vec<Row>,
    /// Total count (if requested)
    pub total: Option<u64>,
}

/// Request to get a single row by primary key.
#[derive(Debug, Clone, Facet)]
pub struct GetRequest {
    /// Database connection URL
    pub database_url: String,
    /// Table name
    pub table: String,
    /// Primary key value
    pub pk: Value,
}

/// Request to create a new row.
#[derive(Debug, Clone, Facet)]
pub struct CreateRequest {
    /// Database connection URL
    pub database_url: String,
    /// Table name
    pub table: String,
    /// Row data
    pub data: Row,
}

/// Request to update a row.
#[derive(Debug, Clone, Facet)]
pub struct UpdateRequest {
    /// Database connection URL
    pub database_url: String,
    /// Table name
    pub table: String,
    /// Primary key value
    pub pk: Value,
    /// Fields to update
    pub data: Row,
}

/// Request to delete a row.
#[derive(Debug, Clone, Facet)]
pub struct DeleteRequest {
    /// Database connection URL
    pub database_url: String,
    /// Table name
    pub table: String,
    /// Primary key value
    pub pk: Value,
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

    /// Generate migration SQL from a diff against the database.
    async fn generate_migration_sql(&self, request: DiffRequest) -> Result<String, DibsError>;

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

/// The Squel service trait - the data plane.
///
/// Provides generic CRUD operations for any registered table.
/// Used by admin UIs that dynamically discover and interact with the schema.
///
/// Named "Squel" as a cute play on SQL.
#[service]
pub trait SquelService {
    /// Get the schema for all registered tables.
    async fn schema(&self) -> SchemaInfo;

    /// List rows from a table with filtering, sorting, and pagination.
    async fn list(&self, request: ListRequest) -> Result<ListResponse, DibsError>;

    /// Get a single row by primary key.
    async fn get(&self, request: GetRequest) -> Result<Option<Row>, DibsError>;

    /// Create a new row.
    async fn create(&self, request: CreateRequest) -> Result<Row, DibsError>;

    /// Update an existing row.
    async fn update(&self, request: UpdateRequest) -> Result<Row, DibsError>;

    /// Delete a row.
    async fn delete(&self, request: DeleteRequest) -> Result<u64, DibsError>;
}
