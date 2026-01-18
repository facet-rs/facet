//! Postgres toolkit for Rust, powered by facet reflection.
//!
//! This crate provides:
//! - Database migrations as Rust functions
//! - Schema introspection via facet reflection
//! - Query building (planned)
//!
//! # Migrations
//!
//! Migrations are registered using the `#[dibs::migration]` attribute.
//! The version is automatically derived from the filename:
//!
//! ```ignore
//! // In file: src/migrations/m_2026_01_17_120000_create_users.rs
//! #[dibs::migration]
//! async fn migrate(ctx: &mut MigrationContext) -> Result<()> {
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

mod backoffice;
mod diff;
mod error;
mod introspect;
pub mod meta;
mod migrate;
mod plugin;
pub mod query;
pub mod schema;
pub mod service;

pub use backoffice::SquelServiceImpl;
pub use diff::{Change, SchemaDiff, TableDiff};
pub use error::{Error, SqlErrorContext};
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

/// Derive migration version from filename.
///
/// This is used internally by the `#[dibs::migration]` macro to derive the
/// version from the filename when no explicit version is provided.
///
/// Converts `m_2026_01_18_173711_create_users.rs` to `2026_01_18_173711-create_users`.
#[doc(hidden)]
pub const fn __derive_migration_version(filename: &str) -> &str {
    // Strip .rs extension
    let bytes = filename.as_bytes();
    let len = bytes.len();

    // Find where .rs starts (should be at len - 3)
    let without_ext_len = if len > 3 && bytes[len - 3] == b'.' && bytes[len - 2] == b'r' && bytes[len - 1] == b's' {
        len - 3
    } else {
        len
    };

    // Strip leading "m_" if present
    let (start, version_len) = if without_ext_len > 2 && bytes[0] == b'm' && bytes[1] == b'_' {
        (2, without_ext_len - 2)
    } else {
        (0, without_ext_len)
    };

    // SAFETY: we're slicing at valid UTF-8 boundaries (ASCII characters)
    unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes.as_ptr().add(start), version_len))
    }
}

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
