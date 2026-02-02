//! SQL expressions.

use crate::{ColumnName, ParamName, PgType, TableName};

/// A SQL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A parameter placeholder (e.g., $handle -> $1)
    Param(ParamName),
    /// A column reference
    Column(ColumnRef),
    /// A string literal
    String(String),
    /// An integer literal
    Int(i64),
    /// A boolean literal
    Bool(bool),
    /// NULL
    Null,
    /// NOW() function
    Now,
    /// DEFAULT keyword
    Default,
    /// Binary operation (e.g., a = b, a AND b)
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// IS NULL / IS NOT NULL
    IsNull { expr: Box<Expr>, negated: bool },
    /// LIKE pattern match (case-sensitive)
    Like { expr: Box<Expr>, pattern: Box<Expr> },
    /// ILIKE pattern match (case-insensitive)
    ILike { expr: Box<Expr>, pattern: Box<Expr> },
    /// = ANY(array) for IN checks with array parameter
    Any { expr: Box<Expr>, array: Box<Expr> },
    /// JSONB -> operator (get object field, returns JSONB)
    JsonGet { expr: Box<Expr>, key: Box<Expr> },
    /// JSONB ->> operator (get object field as text)
    JsonGetText { expr: Box<Expr>, key: Box<Expr> },
    /// @> operator (contains, typically for JSONB)
    Contains { expr: Box<Expr>, value: Box<Expr> },
    /// ? operator (key exists, typically for JSONB)
    KeyExists { expr: Box<Expr>, key: Box<Expr> },
    /// Type cast (e.g., $1::text[], value::integer)
    Cast { expr: Box<Expr>, pg_type: PgType },
    /// EXCLUDED.column reference for ON CONFLICT DO UPDATE
    Excluded(ColumnName),
    /// Function call
    FnCall { name: String, args: Vec<Expr> },
    /// COUNT(table.*) for counting related rows
    Count { table: TableName },
    /// Raw SQL (escape hatch)
    Raw(String),
}

/// A column reference, optionally qualified with table/alias.
///
/// Examples:
/// - `"id"` (unqualified)
/// - `"users"."id"` (qualified with table name)
/// - `"t0"."id"` (qualified with table alias)
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRef {
    /// Table name or alias qualifier. Renders as `"table".` prefix.
    ///
    /// Example: `"users"` in `"users"."id"`, or `"t0"` in `"t0"."id"`
    pub table: Option<TableName>,

    /// The column name. Renders as `"column"`.
    ///
    /// Example: `"id"` in `"users"."id"`
    pub column: ColumnName,
}

impl ColumnRef {
    pub fn new(column: ColumnName) -> Self {
        Self {
            table: None,
            column,
        }
    }

    pub fn qualified(table: TableName, column: ColumnName) -> Self {
        Self {
            table: Some(table),
            column,
        }
    }
}

/// Binary operators for SQL expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// Equality: `=`
    Eq,
    /// Inequality: `<>`
    Ne,
    /// Less than: `<`
    Lt,
    /// Less than or equal: `<=`
    Le,
    /// Greater than: `>`
    Gt,
    /// Greater than or equal: `>=`
    Ge,
    /// Logical AND
    And,
    /// Logical OR
    Or,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "AND",
            BinOp::Or => "OR",
        }
    }
}

// Convenience constructors
impl Expr {
    pub fn param(name: ParamName) -> Self {
        Expr::Param(name)
    }

    pub fn column(name: ColumnName) -> Self {
        Expr::Column(ColumnRef::new(name))
    }

    pub fn qualified_column(table: TableName, column: ColumnName) -> Self {
        Expr::Column(ColumnRef::qualified(table, column))
    }

    pub fn string(s: impl Into<String>) -> Self {
        Expr::String(s.into())
    }

    pub fn int(n: i64) -> Self {
        Expr::Int(n)
    }

    pub fn bool(b: bool) -> Self {
        Expr::Bool(b)
    }

    /// Create an equality expression: self = other
    pub fn eq(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::Eq,
            right: Box::new(other),
        }
    }

    /// Create an AND expression: self AND other
    pub fn and(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::And,
            right: Box::new(other),
        }
    }

    /// Create an OR expression: self OR other
    pub fn or(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::Or,
            right: Box::new(other),
        }
    }

    /// Create IS NULL expression
    pub fn is_null(self) -> Self {
        Expr::IsNull {
            expr: Box::new(self),
            negated: false,
        }
    }

    /// Create IS NOT NULL expression
    pub fn is_not_null(self) -> Self {
        Expr::IsNull {
            expr: Box::new(self),
            negated: true,
        }
    }

    /// Create LIKE expression (case-sensitive pattern match)
    pub fn like(self, pattern: Expr) -> Self {
        Expr::Like {
            expr: Box::new(self),
            pattern: Box::new(pattern),
        }
    }

    /// Create ILIKE expression (case-insensitive pattern match)
    pub fn ilike(self, pattern: Expr) -> Self {
        Expr::ILike {
            expr: Box::new(self),
            pattern: Box::new(pattern),
        }
    }

    /// Create = ANY(array) expression for IN checks
    pub fn any(self, array: Expr) -> Self {
        Expr::Any {
            expr: Box::new(self),
            array: Box::new(array),
        }
    }

    /// Create JSONB -> expression (get object field, returns JSONB)
    pub fn json_get(self, key: Expr) -> Self {
        Expr::JsonGet {
            expr: Box::new(self),
            key: Box::new(key),
        }
    }

    /// Create JSONB ->> expression (get object field as text)
    pub fn json_get_text(self, key: Expr) -> Self {
        Expr::JsonGetText {
            expr: Box::new(self),
            key: Box::new(key),
        }
    }

    /// Create @> expression (contains, typically for JSONB)
    pub fn contains(self, value: Expr) -> Self {
        Expr::Contains {
            expr: Box::new(self),
            value: Box::new(value),
        }
    }

    /// Create ? expression (key exists, typically for JSONB)
    pub fn key_exists(self, key: Expr) -> Self {
        Expr::KeyExists {
            expr: Box::new(self),
            key: Box::new(key),
        }
    }

    /// Create a type cast expression (e.g., `$1::text[]`)
    pub fn cast(self, pg_type: PgType) -> Self {
        Expr::Cast {
            expr: Box::new(self),
            pg_type,
        }
    }

    /// Create an EXCLUDED.column reference for ON CONFLICT DO UPDATE
    pub fn excluded(column: ColumnName) -> Self {
        Expr::Excluded(column)
    }
}
