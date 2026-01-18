//! Dibs service implementation.
//!
//! This module provides the server-side implementation of the `DibsService` trait,
//! which handles requests from the `dibs` CLI.
//!
//! # Example
//!
//! In your `my-app-db` crate's `main.rs`:
//!
//! ```ignore
//! #[tokio::main]
//! async fn main() {
//!     dibs::run_service().await;
//! }
//! ```

use crate::{Change, Schema};
use dibs_proto::*;

/// Default implementation of the DibsService trait.
///
/// This struct implements the service by using dibs's Schema::collect()
/// and Schema::from_database() to handle schema and diff requests.
pub struct DibsServiceImpl;

impl DibsServiceImpl {
    /// Create a new service implementation.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DibsServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl DibsService for DibsServiceImpl {
    async fn schema(&self) -> SchemaInfo {
        let schema = Schema::collect();
        schema_to_info(&schema)
    }

    async fn diff(&self, request: DiffRequest) -> Result<DiffResult, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        // Get schemas
        let rust_schema = Schema::collect();
        let db_schema = Schema::from_database(&client)
            .await
            .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        // Compute diff
        let diff = rust_schema.diff(&db_schema);

        Ok(diff_to_result(&diff))
    }

    async fn migration_status(
        &self,
        request: MigrationStatusRequest,
    ) -> Result<Vec<MigrationInfo>, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        // Get migration status
        let runner = crate::MigrationRunner::new(&client);
        let status = runner
            .status()
            .await
            .map_err(|e| DibsError::MigrationFailed(e.to_string()))?;

        Ok(status
            .into_iter()
            .map(|s| MigrationInfo {
                version: s.version.to_string(),
                name: s.name.to_string(),
                applied: s.applied,
                applied_at: None, // TODO: track this
            })
            .collect())
    }

    async fn migrate(
        &self,
        request: MigrateRequest,
        logs: roam::Tx<MigrationLog>,
    ) -> Result<MigrateResult, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let start = std::time::Instant::now();

        // Run migrations
        let runner = crate::MigrationRunner::new(&client);

        // Log start
        let _ = logs
            .send(&MigrationLog {
                level: LogLevel::Info,
                message: "Starting migrations...".to_string(),
                migration: None,
            })
            .await;

        let applied = if let Some(migration) = request.migration {
            // Run specific migration - not yet implemented
            return Err(DibsError::InvalidRequest(format!(
                "Running specific migration '{}' not yet implemented",
                migration
            )));
        } else {
            // Run all pending
            runner
                .migrate()
                .await
                .map_err(|e| DibsError::MigrationFailed(e.to_string()))?
        };

        for version in &applied {
            let _ = logs
                .send(&MigrationLog {
                    level: LogLevel::Info,
                    message: format!("Applied {}", version),
                    migration: Some(version.to_string()),
                })
                .await;
        }

        let total_time_ms = start.elapsed().as_millis() as u64;

        let _ = logs
            .send(&MigrationLog {
                level: LogLevel::Info,
                message: format!(
                    "Done. Applied {} migration(s) in {}ms",
                    applied.len(),
                    total_time_ms
                ),
                migration: None,
            })
            .await;

        Ok(MigrateResult {
            applied: applied.into_iter().map(|s| s.to_string()).collect(),
            total_time_ms,
        })
    }
}

/// Convert a Schema to SchemaInfo for the wire protocol.
fn schema_to_info(schema: &Schema) -> SchemaInfo {
    SchemaInfo {
        tables: schema
            .tables
            .iter()
            .map(|t| TableInfo {
                name: t.name.clone(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| ColumnInfo {
                        name: c.name.clone(),
                        sql_type: c.pg_type.to_string(),
                        nullable: c.nullable,
                        default: c.default.clone(),
                        primary_key: c.primary_key,
                        unique: c.unique,
                    })
                    .collect(),
                foreign_keys: t
                    .foreign_keys
                    .iter()
                    .map(|fk| ForeignKeyInfo {
                        columns: fk.columns.clone(),
                        references_table: fk.references_table.clone(),
                        references_columns: fk.references_columns.clone(),
                    })
                    .collect(),
                indices: t
                    .indices
                    .iter()
                    .map(|idx| IndexInfo {
                        name: idx.name.clone(),
                        columns: idx.columns.clone(),
                        unique: idx.unique,
                    })
                    .collect(),
                source_file: t.source.file.clone(),
                source_line: t.source.line,
                doc: t.doc.clone(),
            })
            .collect(),
    }
}

/// Convert a SchemaDiff to DiffResult for the wire protocol.
fn diff_to_result(diff: &crate::SchemaDiff) -> DiffResult {
    DiffResult {
        table_diffs: diff
            .table_diffs
            .iter()
            .map(|td| TableDiffInfo {
                table: td.table.clone(),
                changes: td
                    .changes
                    .iter()
                    .map(|c| {
                        let kind = match c {
                            Change::AddTable(_)
                            | Change::AddColumn(_)
                            | Change::AddPrimaryKey(_)
                            | Change::AddForeignKey(_)
                            | Change::AddIndex(_)
                            | Change::AddUnique(_) => ChangeKind::Add,
                            Change::DropTable(_)
                            | Change::DropColumn(_)
                            | Change::DropPrimaryKey
                            | Change::DropForeignKey(_)
                            | Change::DropIndex(_)
                            | Change::DropUnique(_) => ChangeKind::Drop,
                            Change::AlterColumnType { .. }
                            | Change::AlterColumnNullable { .. }
                            | Change::AlterColumnDefault { .. } => ChangeKind::Alter,
                        };
                        ChangeInfo {
                            description: format!("{}", c),
                            kind,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}
