//! SQL generation from query schema types.

use crate::QSource;
use dibs_db_schema::Schema;
use std::sync::Arc;

mod common;
mod delete;
mod insert;
mod insert_many;
mod select;
mod update;
mod upsert;
mod upsert_many;

// Common helpers are used by submodules via `super::common::`
pub use delete::{GeneratedDelete, generate_delete_sql};
pub use insert::{GeneratedInsert, generate_insert_sql};
pub use insert_many::{GeneratedInsertMany, generate_insert_many_sql};
pub use select::{GeneratedSelect, generate_select_sql};
pub use update::{GeneratedUpdate, generate_update_sql};
pub use upsert::{GeneratedUpsert, generate_upsert_sql};
pub use upsert_many::{GeneratedUpsertMany, generate_upsert_many_sql};

/// Context for SQL generation.
///
/// Carries schema and source information for validation and rich error messages.
pub struct SqlGenContext<'a> {
    /// The database schema for type lookups and validation.
    pub schema: &'a Schema,

    /// The source file for error reporting with proper spans.
    pub source: Arc<QSource>,
}

impl<'a> SqlGenContext<'a> {
    pub fn new(schema: &'a Schema, source: Arc<QSource>) -> Self {
        Self { schema, source }
    }
}

#[cfg(test)]
mod tests;
