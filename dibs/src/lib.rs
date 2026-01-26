#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::should_implement_trait)]

//! Postgres toolkit for Rust, powered by facet reflection.
//!
//! This crate provides:
//! - Database migrations as Rust functions
//! - Schema introspection via facet reflection
//! - Query building (planned)
//!
//! # Naming Convention
//!
//! **Table names use singular form** (e.g., `user`, `post`, `comment`).
//!
//! This convention treats each table as a definition of what a single record
//! represents, rather than a container of multiple records. It reads more
//! naturally in code: `User::find(id)` returns "a user", and foreign keys
//! like `author_id` reference "the user table".
//!
//! Junction tables for many-to-many relationships use singular forms joined
//! by underscore: `post_tag`, `post_like`, `user_follow`.
//!
//! # Migrations
//!
//! Migrations are registered using the `#[dibs::migration]` attribute.
//! The version is automatically derived from the filename:
//!
//! ```ignore
//! // In file: src/migrations/m_2026_01_17_120000_create_user.rs
//! #[dibs::migration]
//! async fn migrate(ctx: &mut MigrationContext) -> MigrationResult<()> {
//!     ctx.execute("CREATE TABLE user (id SERIAL PRIMARY KEY, name TEXT NOT NULL)").await?;
//!     Ok(())
//! }
//! ```
//!
//! Use `MigrationResult` instead of `Result` to enable `#[track_caller]` - when an
//! error occurs, the exact source location (file:line:column) is captured.
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
mod jsonb;
pub mod meta;
mod migrate;
mod plugin;
pub mod query;
pub mod schema;
pub mod service;
pub mod solver;

pub use backoffice::SquelServiceImpl;
pub use diff::{Change, SchemaDiff, TableDiff};
pub use error::{Error, MigrationError, SqlErrorContext};
pub use jsonb::Jsonb;
pub use meta::{create_meta_tables_sql, record_migration_sql, sync_tables_sql};
pub use migrate::{Migration, MigrationContext, MigrationRunner, MigrationStatus};
pub use service::{DibsServiceImpl, run_service};

// Re-export proto types for convenience
pub use dibs_proto::*;
pub use schema::{
    Attr, Check, CheckConstraint, Column, CompositeIndex, CompositeUnique, ForeignKey, Index,
    IndexColumn, NullsOrder, PgType, Schema, SortOrder, SourceLocation, Table, TableDef,
    TriggerCheck, TriggerCheckConstraint,
};

// Re-export inventory for the proc macro
pub use inventory;

// Re-export the proc macro
pub use dibs_macros::migration;

// Re-export query DSL codegen types
pub use dibs_query_gen::{
    ColumnInfo, GeneratedCode, PlannerForeignKey, PlannerSchema, PlannerTable, QueryFile,
    SchemaInfo, TableInfo, generate_rust_code, generate_rust_code_with_planner,
    generate_rust_code_with_schema, parse_query_file,
};

/// Quote a PostgreSQL identifier.
///
/// Always quotes identifiers to avoid issues with reserved keywords like
/// `user`, `order`, `table`, `group`, etc. Doubles any embedded quotes.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Generate a standard index name for a table and columns.
///
/// Uses the convention `idx_{table}_{columns}` where columns are joined by underscore.
///
/// # Examples
///
/// ```
/// assert_eq!(dibs::index_name("user", &["email"]), "idx_user_email");
/// assert_eq!(dibs::index_name("post", &["author_id", "created_at"]), "idx_post_author_id_created_at");
/// ```
pub fn index_name(table: &str, columns: &[impl AsRef<str>]) -> String {
    let cols: Vec<&str> = columns.iter().map(|c| c.as_ref()).collect();
    format!("idx_{}_{}", table, cols.join("_"))
}

/// Generate a standard unique index name for a table and columns.
///
/// Uses the convention `uq_{table}_{columns}` where columns are joined by underscore.
///
/// # Examples
///
/// ```
/// assert_eq!(dibs::unique_index_name("user", &["email"]), "uq_user_email");
/// assert_eq!(dibs::unique_index_name("category", &["shop_id", "handle"]), "uq_category_shop_id_handle");
/// ```
pub fn unique_index_name(table: &str, columns: &[impl AsRef<str>]) -> String {
    let cols: Vec<&str> = columns.iter().map(|c| c.as_ref()).collect();
    format!("uq_{}_{}", table, cols.join("_"))
}

/// Generate a deterministic CHECK constraint name for a table and expression.
///
/// Constraint names must be unique within a schema, so we include the table name
/// and a stable hash of the expression (after whitespace normalization).
pub fn check_constraint_name(table: &str, expr: &str) -> String {
    let normalized = normalize_sql_expr_for_hash(expr);
    let hex = blake3::hash(normalized.as_bytes()).to_hex().to_string();
    let suffix = &hex[..16];

    const PG_IDENT_MAX: usize = 63;
    let prefix_overhead = "ck__".len(); // "ck_" + "_" between table and suffix
    let suffix_len = suffix.len();
    let max_table_len = PG_IDENT_MAX.saturating_sub(prefix_overhead + suffix_len);

    let table_part = if table.len() <= max_table_len {
        table
    } else {
        // Table names are expected to be ASCII snake_case; still, avoid splitting UTF-8.
        let mut len = max_table_len.min(table.len());
        while len > 0 && !table.is_char_boundary(len) {
            len -= 1;
        }
        &table[..len]
    };

    format!("ck_{}_{}", table_part, suffix)
}

/// Generate a deterministic trigger name for a trigger-enforced check.
///
/// Trigger names are scoped to a table in Postgres, but we still include the table name
/// and a stable hash of the expression for readability and determinism.
pub fn trigger_check_name(table: &str, expr: &str) -> String {
    let normalized = normalize_sql_expr_for_hash(expr);
    let hex = blake3::hash(normalized.as_bytes()).to_hex().to_string();
    let suffix = &hex[..16];

    const PG_IDENT_MAX: usize = 63;
    let prefix_overhead = "trgck__".len(); // "trgck_" + "_" between table and suffix
    let suffix_len = suffix.len();
    let max_table_len = PG_IDENT_MAX.saturating_sub(prefix_overhead + suffix_len);

    let table_part = if table.len() <= max_table_len {
        table
    } else {
        let mut len = max_table_len.min(table.len());
        while len > 0 && !table.is_char_boundary(len) {
            len -= 1;
        }
        &table[..len]
    };

    format!("trgck_{}_{}", table_part, suffix)
}

/// Derive the trigger function name for a trigger-enforced check.
///
/// The function name is derived from the trigger name (hashed) so we don't
/// accidentally exceed Postgres' identifier length limit.
pub fn trigger_check_function_name(trigger_name: &str) -> String {
    let hex = blake3::hash(trigger_name.as_bytes()).to_hex().to_string();
    format!("trgfn_{}", &hex[..20])
}

fn normalize_sql_expr_for_hash(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut pending_space = false;

    let mut in_single_quote = false;
    let mut in_double_quote = false;

    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_single_quote {
            out.push(ch);
            if ch == '\'' {
                // SQL escapes single quotes by doubling them: ''
                if matches!(chars.peek(), Some('\'')) {
                    out.push(chars.next().expect("peeked"));
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }

        if in_double_quote {
            out.push(ch);
            if ch == '"' {
                // SQL escapes double quotes in identifiers by doubling them: ""
                if matches!(chars.peek(), Some('"')) {
                    out.push(chars.next().expect("peeked"));
                } else {
                    in_double_quote = false;
                }
            }
            continue;
        }

        match ch {
            '\'' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push('\'');
                in_single_quote = true;
            }
            '"' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push('"');
                in_double_quote = true;
            }
            c if c.is_whitespace() => {
                pending_space = true;
            }
            c => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(c);
            }
        }
    }

    out.trim().to_string()
}

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
    let without_ext_len =
        if len > 3 && bytes[len - 3] == b'.' && bytes[len - 2] == b'r' && bytes[len - 1] == b's' {
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
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            bytes.as_ptr().add(start),
            version_len,
        ))
    }
}

/// Result type for dibs operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Result type for migration functions, captures caller location on error.
pub type MigrationResult<T> = std::result::Result<T, MigrationError>;

/// Type alias for migration functions.
///
/// Migration functions are async functions that take a mutable reference to a
/// `MigrationContext` and return a `MigrationResult<()>`. Using `MigrationResult`
/// instead of `Result` enables `#[track_caller]` to capture the exact source
/// location where an error occurs (via the `?` operator).
pub type MigrationFn = for<'a> fn(
    &'a mut MigrationContext<'a>,
)
    -> Pin<Box<dyn Future<Output = MigrationResult<()>> + Send + 'a>>;

// Register Migration with inventory
inventory::collect!(Migration);

/// Generate query code from a `.styx` file.
///
/// This is the main entry point for build scripts that generate query code.
/// It collects the schema from inventory, parses the query file, generates
/// Rust code, and writes it to `OUT_DIR`.
///
/// # Example
///
/// ```ignore
/// // build.rs
/// fn main() {
///     // Force the linker to include the db crate's inventory submissions
///     my_db::ensure_linked();
///
///     dibs::build_queries(".dibs-queries/queries.styx");
/// }
/// ```
///
/// # Panics
///
/// Panics if the query file cannot be read or parsed, or if the output cannot be written.
pub fn build_queries(queries_path: impl AsRef<std::path::Path>) {
    let queries_path = queries_path.as_ref();

    println!("cargo::rerun-if-changed={}", queries_path.display());

    // Collect schema from registered tables via inventory
    let dibs_schema = Schema::collect();

    eprintln!(
        "cargo::warning=dibs: found {} tables in schema",
        dibs_schema.tables.len()
    );

    for table in &dibs_schema.tables {
        eprintln!(
            "cargo::warning=dibs: table '{}' with {} columns, {} FKs",
            table.name,
            table.columns.len(),
            table.foreign_keys.len()
        );
    }

    let (schema, planner_schema) = dibs_schema.to_query_schema();

    let source = std::fs::read_to_string(queries_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", queries_path.display(), e));

    let file = parse_query_file(&source)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", queries_path.display(), e));

    let generated = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = std::path::Path::new(&out_dir).join("queries.rs");

    std::fs::write(&dest_path, &generated.code)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", dest_path.display(), e));

    println!("cargo::rustc-env=QUERIES_PATH={}", dest_path.display());
}
