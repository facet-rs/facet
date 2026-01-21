//! SQL AST and rendering.
//!
//! Build SQL as a typed AST, then render to a string with automatic
//! parameter numbering and formatting.

mod expr;
mod render;
mod stmt;

pub use expr::*;
pub use render::*;
pub use stmt::*;

/// Result of rendering SQL.
#[derive(Debug, Clone)]
pub struct RenderedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<String>,
}

/// Quote a SQL identifier (table or column name).
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Escape a string literal for SQL.
pub fn escape_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
