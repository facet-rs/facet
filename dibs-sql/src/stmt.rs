//! SQL statements.

use crate::expr::Expr;

/// A SQL statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    Select(SelectStmt),
    Insert(InsertStmt),
    Update(UpdateStmt),
    Delete(DeleteStmt),
}

/// A SELECT statement.
#[derive(Debug, Clone, Default)]
pub struct SelectStmt {
    pub columns: Vec<SelectColumn>,
    pub from: Option<FromClause>,
    pub joins: Vec<Join>,
    pub where_: Option<Expr>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<Expr>,
    pub offset: Option<Expr>,
}

/// A column in a SELECT clause.
#[derive(Debug, Clone)]
pub enum SelectColumn {
    /// A simple column reference
    Expr { expr: Expr, alias: Option<String> },
    /// All columns from a table: table.*
    AllFrom(String),
}

impl SelectColumn {
    pub fn expr(expr: Expr) -> Self {
        SelectColumn::Expr { expr, alias: None }
    }

    pub fn aliased(expr: Expr, alias: impl Into<String>) -> Self {
        SelectColumn::Expr {
            expr,
            alias: Some(alias.into()),
        }
    }

    pub fn all_from(table: impl Into<String>) -> Self {
        SelectColumn::AllFrom(table.into())
    }
}

/// A FROM clause.
#[derive(Debug, Clone)]
pub struct FromClause {
    pub table: String,
    pub alias: Option<String>,
}

impl FromClause {
    pub fn table(name: impl Into<String>) -> Self {
        Self {
            table: name.into(),
            alias: None,
        }
    }

    pub fn aliased(name: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            table: name.into(),
            alias: Some(alias.into()),
        }
    }
}

/// A JOIN clause.
#[derive(Debug, Clone)]
pub struct Join {
    pub kind: JoinKind,
    pub table: String,
    pub alias: Option<String>,
    pub on: Expr,
}

/// Type of JOIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
}

impl JoinKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Full => "FULL JOIN",
        }
    }
}

/// ORDER BY clause.
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub expr: Expr,
    pub desc: bool,
    pub nulls: Option<NullsOrder>,
}

impl OrderBy {
    pub fn asc(expr: Expr) -> Self {
        Self {
            expr,
            desc: false,
            nulls: None,
        }
    }

    pub fn desc(expr: Expr) -> Self {
        Self {
            expr,
            desc: true,
            nulls: None,
        }
    }
}

/// NULLS FIRST / NULLS LAST
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullsOrder {
    First,
    Last,
}

// ============================================================================
// INSERT statement
// ============================================================================

/// An INSERT statement.
#[derive(Debug, Clone)]
pub struct InsertStmt {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<Expr>,
    pub on_conflict: Option<OnConflict>,
    pub returning: Vec<String>,
}

/// ON CONFLICT clause for upsert.
#[derive(Debug, Clone)]
pub struct OnConflict {
    /// Conflict target columns
    pub columns: Vec<String>,
    /// What to do on conflict
    pub action: ConflictAction,
}

/// What to do on conflict.
#[derive(Debug, Clone)]
pub enum ConflictAction {
    /// DO NOTHING
    DoNothing,
    /// DO UPDATE SET ...
    DoUpdate(Vec<UpdateAssignment>),
}

/// An assignment in UPDATE SET or ON CONFLICT DO UPDATE SET.
#[derive(Debug, Clone)]
pub struct UpdateAssignment {
    pub column: String,
    pub value: Expr,
}

impl UpdateAssignment {
    pub fn new(column: impl Into<String>, value: Expr) -> Self {
        Self {
            column: column.into(),
            value,
        }
    }
}

// ============================================================================
// UPDATE statement
// ============================================================================

/// An UPDATE statement.
#[derive(Debug, Clone)]
pub struct UpdateStmt {
    pub table: String,
    pub assignments: Vec<UpdateAssignment>,
    pub where_: Option<Expr>,
    pub returning: Vec<String>,
}

// ============================================================================
// DELETE statement
// ============================================================================

/// A DELETE statement.
#[derive(Debug, Clone)]
pub struct DeleteStmt {
    pub table: String,
    pub where_: Option<Expr>,
    pub returning: Vec<String>,
}

// ============================================================================
// Builder-style constructors
// ============================================================================

impl SelectStmt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn column(mut self, col: SelectColumn) -> Self {
        self.columns.push(col);
        self
    }

    pub fn columns(mut self, cols: impl IntoIterator<Item = SelectColumn>) -> Self {
        self.columns.extend(cols);
        self
    }

    pub fn from(mut self, from: FromClause) -> Self {
        self.from = Some(from);
        self
    }

    pub fn join(mut self, join: Join) -> Self {
        self.joins.push(join);
        self
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn order_by(mut self, order: OrderBy) -> Self {
        self.order_by.push(order);
        self
    }

    pub fn limit(mut self, expr: Expr) -> Self {
        self.limit = Some(expr);
        self
    }

    pub fn offset(mut self, expr: Expr) -> Self {
        self.offset = Some(expr);
        self
    }
}

impl InsertStmt {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            columns: Vec::new(),
            values: Vec::new(),
            on_conflict: None,
            returning: Vec::new(),
        }
    }

    pub fn column(mut self, name: impl Into<String>, value: Expr) -> Self {
        self.columns.push(name.into());
        self.values.push(value);
        self
    }

    pub fn on_conflict(mut self, conflict: OnConflict) -> Self {
        self.on_conflict = Some(conflict);
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning.extend(cols.into_iter().map(Into::into));
        self
    }
}

impl UpdateStmt {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            assignments: Vec::new(),
            where_: None,
            returning: Vec::new(),
        }
    }

    pub fn set(mut self, column: impl Into<String>, value: Expr) -> Self {
        self.assignments.push(UpdateAssignment::new(column, value));
        self
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning.extend(cols.into_iter().map(Into::into));
        self
    }
}

impl DeleteStmt {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            where_: None,
            returning: Vec::new(),
        }
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning.extend(cols.into_iter().map(Into::into));
        self
    }
}
