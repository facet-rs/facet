//! Query AST types.

use super::{Expr, Value};

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// A SELECT query.
#[derive(Debug, Clone)]
pub struct SelectQuery {
    /// Table name
    pub table: String,
    /// Columns to select (empty = *)
    pub columns: Vec<String>,
    /// WHERE conditions (ANDed together)
    pub filters: Vec<Expr>,
    /// ORDER BY clauses
    pub order: Vec<(String, SortDir)>,
    /// LIMIT
    pub limit: Option<u32>,
    /// OFFSET
    pub offset: Option<u32>,
}

impl SelectQuery {
    /// Create a new SELECT query for a table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            columns: Vec::new(),
            filters: Vec::new(),
            order: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Select specific columns.
    pub fn columns(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns = cols.into_iter().map(Into::into).collect();
        self
    }

    /// Add a filter condition.
    pub fn filter(mut self, expr: Expr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Add an ORDER BY clause.
    pub fn order_by(mut self, column: impl Into<String>, dir: SortDir) -> Self {
        self.order.push((column.into(), dir));
        self
    }

    /// Set LIMIT.
    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }

    /// Set OFFSET.
    pub fn offset(mut self, n: u32) -> Self {
        self.offset = Some(n);
        self
    }
}

/// An INSERT query.
#[derive(Debug, Clone)]
pub struct InsertQuery {
    /// Table name
    pub table: String,
    /// Column names
    pub columns: Vec<String>,
    /// Values to insert
    pub values: Vec<Value>,
    /// Columns to return (RETURNING clause)
    pub returning: Vec<String>,
}

impl InsertQuery {
    /// Create a new INSERT query for a table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            columns: Vec::new(),
            values: Vec::new(),
            returning: Vec::new(),
        }
    }

    /// Set the columns and values to insert.
    pub fn values(
        mut self,
        data: impl IntoIterator<Item = (impl Into<String>, impl Into<Value>)>,
    ) -> Self {
        let (cols, vals): (Vec<_>, Vec<_>) =
            data.into_iter().map(|(c, v)| (c.into(), v.into())).unzip();
        self.columns = cols;
        self.values = vals;
        self
    }

    /// Set RETURNING columns.
    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = cols.into_iter().map(Into::into).collect();
        self
    }

    /// Return all columns.
    pub fn returning_all(mut self) -> Self {
        self.returning = vec!["*".to_string()];
        self
    }
}

/// An UPDATE query.
#[derive(Debug, Clone)]
pub struct UpdateQuery {
    /// Table name
    pub table: String,
    /// Columns and new values
    pub changes: Vec<(String, Value)>,
    /// WHERE conditions
    pub filters: Vec<Expr>,
    /// Columns to return (RETURNING clause)
    pub returning: Vec<String>,
}

impl UpdateQuery {
    /// Create a new UPDATE query for a table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            changes: Vec::new(),
            filters: Vec::new(),
            returning: Vec::new(),
        }
    }

    /// Set the columns and values to update.
    pub fn set(
        mut self,
        data: impl IntoIterator<Item = (impl Into<String>, impl Into<Value>)>,
    ) -> Self {
        self.changes = data
            .into_iter()
            .map(|(c, v)| (c.into(), v.into()))
            .collect();
        self
    }

    /// Add a filter condition.
    pub fn filter(mut self, expr: Expr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Set RETURNING columns.
    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = cols.into_iter().map(Into::into).collect();
        self
    }

    /// Return all columns.
    pub fn returning_all(mut self) -> Self {
        self.returning = vec!["*".to_string()];
        self
    }
}

/// A DELETE query.
#[derive(Debug, Clone)]
pub struct DeleteQuery {
    /// Table name
    pub table: String,
    /// WHERE conditions
    pub filters: Vec<Expr>,
    /// Columns to return (RETURNING clause)
    pub returning: Vec<String>,
}

impl DeleteQuery {
    /// Create a new DELETE query for a table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            filters: Vec::new(),
            returning: Vec::new(),
        }
    }

    /// Add a filter condition.
    pub fn filter(mut self, expr: Expr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Set RETURNING columns.
    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = cols.into_iter().map(Into::into).collect();
        self
    }

    /// Return all columns.
    pub fn returning_all(mut self) -> Self {
        self.returning = vec!["*".to_string()];
        self
    }
}
