//! SQL AST and rendering.
//!
//! Build SQL as a typed AST, then render to a string with automatic
//! parameter numbering and formatting.

use strid::*;

mod expr;
pub use expr::*;

mod render;
pub use render::*;

mod stmt;
pub use stmt::*;

/// Result of rendering SQL.
#[derive(Debug, Clone)]
pub struct RenderedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,
}

/// The name of a table or table alias.
///
/// Used in FROM, JOIN, INSERT INTO, UPDATE, and DELETE clauses.
#[braid]
pub struct TableName;

/// The name of a column or column alias.
///
/// Used in SELECT, INSERT, UPDATE SET, and RETURNING clauses.
#[braid]
pub struct ColumnName;

/// The name of a query parameter.
///
/// Parameters are named (e.g., `handle`) and automatically assigned
/// positional placeholders (`$1`, `$2`, etc.) during rendering.
#[braid]
pub struct ParamName;

/// A PostgreSQL type name for casts.
///
/// Used in type cast expressions like `$1::text[]` or `value::integer`.
#[braid]
pub struct PgType;

/// A PostgreSQL string literal wrapper.
///
/// Display writes the value escaped and quoted with single quotes.
///
/// # Example
/// ```
/// use dibs_sql::Lit;
/// assert_eq!(format!("{}", Lit("foo")), "'foo'");
/// assert_eq!(format!("{}", Lit("it's")), "'it''s'");
/// ```
pub struct Lit<T: AsRef<str>>(pub T);

impl<T: AsRef<str>> std::fmt::Display for Lit<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'")?;
        for c in self.0.as_ref().chars() {
            if c == '\'' {
                write!(f, "''")?;
            } else {
                write!(f, "{}", c)?;
            }
        }
        write!(f, "'")
    }
}

/// A PostgreSQL identifier wrapper.
///
/// Display writes the value escaped and quoted with double quotes.
///
/// # Example
/// ```
/// use dibs_sql::Ident;
/// assert_eq!(format!("{}", Ident("user")), "\"user\"");
/// assert_eq!(format!("{}", Ident("bla\"h")), "\"bla\"\"h\"");
/// ```
pub struct Ident<T: AsRef<str>>(pub T);

impl<T: AsRef<str>> std::fmt::Display for Ident<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"")?;
        for c in self.0.as_ref().chars() {
            if c == '"' {
                write!(f, "\"\"")?;
            } else {
                write!(f, "{}", c)?;
            }
        }
        write!(f, "\"")
    }
}

/// Escape a string literal for SQL.
pub fn escape_string(s: &str) -> String {
    format!("{}", Lit(s))
}

/// Quote a PostgreSQL identifier.
///
/// Always quotes identifiers to avoid issues with reserved keywords like
/// `user`, `order`, `table`, `group`, etc. Doubles any embedded quotes.
pub fn quote_ident(name: &str) -> String {
    format!("{}", Ident(name))
}

/// Generate a standard index name for a table and columns.
///
/// Uses the convention `idx_{table}_{columns}` where columns are joined by underscore.
pub fn index_name(table: &str, columns: &[impl AsRef<str>]) -> String {
    let cols: Vec<&str> = columns.iter().map(|c| c.as_ref()).collect();
    format!("idx_{}_{}", table, cols.join("_"))
}

/// Generate a standard unique index name for a table and columns.
///
/// Uses the convention `uq_{table}_{columns}` where columns are joined by underscore.
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
