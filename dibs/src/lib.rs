//! Postgres toolkit for Rust, powered by facet reflection.
//!
//! This crate provides:
//! - Database migrations as Rust functions
//! - Schema introspection via facet reflection
//! - Query building (planned)
//!
//! # Migrations
//!
//! Migrations are registered using the `#[dibs::migration]` attribute:
//!
//! ```ignore
//! #[dibs::migration("2026-01-17-create-users")]
//! async fn create_users(ctx: &mut MigrationContext) -> Result<()> {
//!     ctx.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT NOT NULL)").await?;
//!     Ok(())
//! }
//! ```
//!
//! Run migrations with `MigrationRunner`:
//!
//! ```ignore
//! let runner = MigrationRunner::new(&client);
//! runner.migrate().await?;
//! ```

use std::future::Future;
use std::pin::Pin;

mod diff;
mod error;
mod introspect;
pub mod meta;
mod migrate;
mod plugin;
pub mod schema;
pub mod service;

pub use diff::{Change, SchemaDiff, TableDiff};
pub use error::Error;
pub use meta::{create_meta_tables_sql, record_migration_sql, sync_tables_sql};
pub use migrate::{Migration, MigrationContext, MigrationRunner, MigrationStatus};
pub use service::{DibsServiceImpl, run_service};

// Re-export proto types for convenience
pub use dibs_proto::*;
pub use schema::{
    Attr, Column, CompositeIndex, ForeignKey, Index, PgType, Schema, SourceLocation, Table,
    TableDef,
};

// Re-export inventory for the proc macro
pub use inventory;

// Re-export the proc macro
pub use dibs_macros::migration;

/// Result type for dibs operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Type alias for migration functions.
///
/// Migration functions are async functions that take a mutable reference to a
/// `MigrationContext` and return a `Result<()>`.
pub type MigrationFn = for<'a> fn(
    &'a mut MigrationContext<'a>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

// Register Migration with inventory
inventory::collect!(Migration);
