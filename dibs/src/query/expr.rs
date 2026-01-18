//! Filter expressions for WHERE clauses.

use super::Value;

/// A filter expression.
///
/// Represents a condition in a WHERE clause. Can be composed with
/// `And`, `Or`, and `Not` for complex boolean logic.
#[derive(Debug, Clone)]
pub enum Expr {
    // Comparisons
    /// column = value
    Eq(String, Value),
    /// column != value
    Ne(String, Value),
    /// column < value
    Lt(String, Value),
    /// column <= value
    Lte(String, Value),
    /// column > value
    Gt(String, Value),
    /// column >= value
    Gte(String, Value),

    // Pattern matching
    /// column LIKE pattern
    Like(String, String),
    /// column ILIKE pattern (case-insensitive)
    ILike(String, String),

    // Nulls
    /// column IS NULL
    IsNull(String),
    /// column IS NOT NULL
    IsNotNull(String),

    // Lists
    /// column IN (values...)
    In(String, Vec<Value>),

    // Boolean logic
    /// expr AND expr AND ...
    And(Vec<Expr>),
    /// expr OR expr OR ...
    Or(Vec<Expr>),
    /// NOT expr
    Not(Box<Expr>),
}

impl Expr {
    /// Create an equality expression: column = value
    pub fn eq(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Eq(column.into(), value.into())
    }

    /// Create a not-equal expression: column != value
    pub fn ne(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Ne(column.into(), value.into())
    }

    /// Create a less-than expression: column < value
    pub fn lt(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Lt(column.into(), value.into())
    }

    /// Create a less-than-or-equal expression: column <= value
    pub fn lte(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Lte(column.into(), value.into())
    }

    /// Create a greater-than expression: column > value
    pub fn gt(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Gt(column.into(), value.into())
    }

    /// Create a greater-than-or-equal expression: column >= value
    pub fn gte(column: impl Into<String>, value: impl Into<Value>) -> Self {
        Expr::Gte(column.into(), value.into())
    }

    /// Create a LIKE expression: column LIKE pattern
    pub fn like(column: impl Into<String>, pattern: impl Into<String>) -> Self {
        Expr::Like(column.into(), pattern.into())
    }

    /// Create an ILIKE expression: column ILIKE pattern (case-insensitive)
    pub fn ilike(column: impl Into<String>, pattern: impl Into<String>) -> Self {
        Expr::ILike(column.into(), pattern.into())
    }

    /// Create an IS NULL expression
    pub fn is_null(column: impl Into<String>) -> Self {
        Expr::IsNull(column.into())
    }

    /// Create an IS NOT NULL expression
    pub fn is_not_null(column: impl Into<String>) -> Self {
        Expr::IsNotNull(column.into())
    }

    /// Create an IN expression: column IN (values...)
    pub fn is_in(
        column: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<Value>>,
    ) -> Self {
        Expr::In(column.into(), values.into_iter().map(Into::into).collect())
    }

    /// Combine expressions with AND
    pub fn and(exprs: impl IntoIterator<Item = Expr>) -> Self {
        Expr::And(exprs.into_iter().collect())
    }

    /// Combine expressions with OR
    pub fn or(exprs: impl IntoIterator<Item = Expr>) -> Self {
        Expr::Or(exprs.into_iter().collect())
    }

    /// Negate an expression
    pub fn not(expr: Expr) -> Self {
        Expr::Not(Box::new(expr))
    }
}
