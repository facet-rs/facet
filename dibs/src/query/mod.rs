//! Query builder for dibs.
//!
//! Provides both type-safe and dynamic query APIs that compile to parameterized SQL.
//!
//! # Example
//!
//! ```ignore
//! use dibs::query::{Db, Expr, SortDir};
//!
//! let db = Db::new(&client);
//!
//! // SELECT
//! let rows = db.select("users")?
//!     .filter(Expr::eq("status", "active"))
//!     .order_by("created_at", SortDir::Desc)
//!     .limit(10)
//!     .all()
//!     .await?;
//!
//! // INSERT
//! let row = db.insert("users")?
//!     .values([("name", "Alice"), ("email", "alice@example.com")])
//!     .returning()
//!     .await?;
//!
//! // UPDATE
//! let affected = db.update("users")?
//!     .set([("status", "inactive")])
//!     .filter(Expr::eq("id", 42i64))
//!     .execute()
//!     .await?;
//!
//! // DELETE
//! let affected = db.delete("users")?
//!     .filter(Expr::eq("id", 42i64))
//!     .execute()
//!     .await?;
//! ```

mod ast;
mod build;
mod exec;
mod expr;
mod row;
mod value;

pub use ast::*;
pub use build::BuiltQuery;
pub use exec::{Db, DeleteBuilder, InsertBuilder, SelectBuilder, UpdateBuilder};
pub use expr::*;
pub use row::{Row, SqlParam, pg_row_to_row};
pub use value::*;
