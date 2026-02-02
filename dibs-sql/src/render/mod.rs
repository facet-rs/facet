//! Render SQL AST to string.

use std::cell::RefCell;
use std::fmt;

use indexmap::IndexMap;

use crate::expr::{ColumnRef, Expr};
use crate::stmt::*;
use crate::{Ident, ParamName, RenderedSql, escape_string};

/// Mutable parameter tracking state.
struct ParamState {
    /// Named parameters mapped to their assigned positional index.
    params: IndexMap<ParamName, usize>,
    /// Next parameter index to assign (starts at 1 for `$1`).
    next_param_idx: usize,
}

impl ParamState {
    fn new() -> Self {
        Self {
            params: IndexMap::new(),
            next_param_idx: 1,
        }
    }

    /// Get or create a parameter index.
    fn get_or_insert(&mut self, name: &ParamName) -> usize {
        *self.params.entry(name.clone()).or_insert_with(|| {
            let idx = self.next_param_idx;
            self.next_param_idx += 1;
            idx
        })
    }
}

/// Rendering context that tracks parameter assignment.
///
/// Uses interior mutability (`RefCell`) so that `Render::render` can take `&self`,
/// enabling the `Fmt` wrapper to implement `Display`.
pub struct RenderContext {
    /// Parameter tracking state, wrapped for interior mutability.
    params: RefCell<ParamState>,
}

impl RenderContext {
    pub fn new() -> Self {
        Self {
            params: RefCell::new(ParamState::new()),
        }
    }

    /// Get or create a parameter placeholder index.
    fn param_idx(&self, name: &ParamName) -> usize {
        self.params.borrow_mut().get_or_insert(name)
    }

    /// Finish rendering and return the collected params.
    fn into_params(self) -> Vec<ParamName> {
        self.params.into_inner().params.into_keys().collect()
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for rendering a `Render` type via `Display`.
///
/// Allows using `write!(f, "{}", Fmt(ctx, &expr))` in format strings.
pub struct Fmt<'a, T: Render>(
    /// The rendering context for parameter tracking.
    &'a RenderContext,
    /// The value to render.
    &'a T,
);

impl<T: Render> fmt::Display for Fmt<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.render(self.0, f)
    }
}

// ============================================================================
// Render implementations
// ============================================================================

/// Trait for types that can be rendered to SQL.
pub trait Render {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl Render for Expr {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Param(name) => {
                let idx = ctx.param_idx(name);
                write!(f, "${idx}")
            }
            Expr::Column(col) => col.render(ctx, f),
            Expr::String(s) => {
                let escaped = escape_string(s);
                write!(f, "{escaped}")
            }
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Expr::Null => write!(f, "NULL"),
            Expr::Now => write!(f, "NOW()"),
            Expr::Default => write!(f, "DEFAULT"),
            Expr::BinOp { left, op, right } => {
                let left = Fmt(ctx, left.as_ref());
                let right = Fmt(ctx, right.as_ref());
                let op = op.as_str();
                write!(f, "{left} {op} {right}")
            }
            Expr::IsNull { expr, negated } => {
                let expr = Fmt(ctx, expr.as_ref());
                let suffix = if *negated { " IS NOT NULL" } else { " IS NULL" };
                write!(f, "{expr}{suffix}")
            }
            Expr::Like { expr, pattern } => {
                let expr = Fmt(ctx, expr.as_ref());
                let pattern = Fmt(ctx, pattern.as_ref());
                write!(f, "{expr} LIKE {pattern}")
            }
            Expr::ILike { expr, pattern } => {
                let expr = Fmt(ctx, expr.as_ref());
                let pattern = Fmt(ctx, pattern.as_ref());
                write!(f, "{expr} ILIKE {pattern}")
            }
            Expr::Any { expr, array } => {
                let expr = Fmt(ctx, expr.as_ref());
                let array = Fmt(ctx, array.as_ref());
                write!(f, "{expr} = ANY({array})")
            }
            Expr::JsonGet { expr, key } => {
                let expr = Fmt(ctx, expr.as_ref());
                let key = Fmt(ctx, key.as_ref());
                write!(f, "{expr} -> {key}")
            }
            Expr::JsonGetText { expr, key } => {
                let expr = Fmt(ctx, expr.as_ref());
                let key = Fmt(ctx, key.as_ref());
                write!(f, "{expr} ->> {key}")
            }
            Expr::Contains { expr, value } => {
                let expr = Fmt(ctx, expr.as_ref());
                let value = Fmt(ctx, value.as_ref());
                write!(f, "{expr} @> {value}")
            }
            Expr::KeyExists { expr, key } => {
                let expr = Fmt(ctx, expr.as_ref());
                let key = Fmt(ctx, key.as_ref());
                write!(f, "{expr} ? {key}")
            }
            Expr::Cast { expr, pg_type } => {
                let expr = Fmt(ctx, expr.as_ref());
                write!(f, "{expr}::{}", pg_type.as_str())
            }
            Expr::Excluded(column) => {
                let column = Ident(column.as_str());
                write!(f, "EXCLUDED.{column}")
            }
            Expr::FnCall { name, args } => {
                write!(f, "{name}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", Fmt(ctx, arg))?;
                }
                write!(f, ")")
            }
            Expr::Count { table } => {
                let table = Ident(table.as_str());
                write!(f, "COUNT({table}.*)")
            }
            Expr::Raw(s) => write!(f, "{s}"),
        }
    }
}

impl Render for ColumnRef {
    fn render(&self, _ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(table) = &self.table {
            let table = Ident(table.as_str());
            write!(f, "{table}.")?;
        }
        let column = Ident(self.column.as_str());
        write!(f, "{column}")
    }
}

impl Render for SelectStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SELECT")?;

        // DISTINCT ON (takes precedence over DISTINCT)
        if !self.distinct_on.is_empty() {
            write!(f, " DISTINCT ON (")?;
            for (i, expr) in self.distinct_on.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", Fmt(ctx, expr))?;
            }
            write!(f, ")")?;
        } else if self.distinct {
            write!(f, " DISTINCT")?;
        }

        // Columns
        if self.columns.is_empty() {
            write!(f, " *")?;
        } else {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, " {}", Fmt(ctx, col))?;
            }
        }

        // FROM
        if let Some(from) = &self.from {
            let table = Ident(from.table.as_str());
            write!(f, "\nFROM {table}")?;
            if let Some(alias) = &from.alias {
                let alias = Ident(alias.as_str());
                write!(f, " {alias}")?;
            }
        }

        // JOINs
        for join in &self.joins {
            let kind = join.kind.as_str();
            let table = Ident(join.table.as_str());
            write!(f, "\n{kind} {table}")?;
            if let Some(alias) = &join.alias {
                let alias = Ident(alias.as_str());
                write!(f, " {alias}")?;
            }
            let on = Fmt(ctx, &join.on);
            write!(f, " ON {on}")?;
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            write!(f, "\nORDER BY ")?;
            for (i, order) in self.order_by.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let expr = Fmt(ctx, &order.expr);
                let dir = if order.desc { " DESC" } else { " ASC" };
                write!(f, "{expr}{dir}")?;
                if let Some(nulls) = &order.nulls {
                    write!(
                        f,
                        "{}",
                        match nulls {
                            NullsOrder::First => " NULLS FIRST",
                            NullsOrder::Last => " NULLS LAST",
                        }
                    )?;
                }
            }
        }

        // LIMIT
        if let Some(limit) = &self.limit {
            let limit = Fmt(ctx, limit);
            write!(f, "\nLIMIT {limit}")?;
        }

        // OFFSET
        if let Some(offset) = &self.offset {
            let offset = Fmt(ctx, offset);
            write!(f, "\nOFFSET {offset}")?;
        }

        Ok(())
    }
}

impl Render for SelectColumn {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectColumn::Expr { expr, alias } => {
                let expr = Fmt(ctx, expr);
                write!(f, "{expr}")?;
                if let Some(alias) = alias {
                    let alias = Ident(alias.as_str());
                    write!(f, " AS {alias}")?;
                }
                Ok(())
            }
            SelectColumn::AllFrom(table) => {
                let table = Ident(table.as_str());
                write!(f, "{table}.*")
            }
        }
    }
}

impl Render for InsertStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "INSERT INTO {table} (")?;

        // Columns
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let col = Ident(col.as_str());
            write!(f, "{col}")?;
        }
        write!(f, ")")?;

        // VALUES
        write!(f, "\nVALUES (")?;
        for (i, val) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", Fmt(ctx, val))?;
        }
        write!(f, ")")?;

        // ON CONFLICT
        if let Some(conflict) = &self.on_conflict {
            write!(f, "\nON CONFLICT (")?;
            for (i, col) in conflict.columns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
            write!(f, ")")?;

            match &conflict.action {
                ConflictAction::DoNothing => {
                    write!(f, " DO NOTHING")?;
                }
                ConflictAction::DoUpdate(assignments) => {
                    write!(f, " DO UPDATE SET ")?;
                    for (i, assign) in assignments.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        let col = Ident(assign.column.as_str());
                        let val = Fmt(ctx, &assign.value);
                        write!(f, "{col} = {val}")?;
                    }
                }
            }
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for UpdateStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "UPDATE {table}")?;

        // SET
        write!(f, "\nSET ")?;
        for (i, assign) in self.assignments.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let col = Ident(assign.column.as_str());
            let val = Fmt(ctx, &assign.value);
            write!(f, "{col} = {val}")?;
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for DeleteStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "DELETE FROM {table}")?;

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for InsertSelectStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "INSERT INTO {table} (")?;

        // Columns
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let col = Ident(col.as_str());
            write!(f, "{col}")?;
        }
        write!(f, ")")?;

        // SELECT
        write!(f, "\nSELECT ")?;
        for (i, expr) in self.select_exprs.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", Fmt(ctx, expr))?;
        }

        // FROM UNNEST
        write!(f, "\nFROM UNNEST(")?;
        for (i, param) in self.unnest.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let idx = ctx.param_idx(&param.name.as_str().into());
            write!(f, "${}::{}", idx, param.pg_type.as_str())?;
        }
        let alias = Ident(self.unnest.alias.as_str());
        write!(f, ") AS {alias}(")?;
        for (i, param) in self.unnest.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param.name.as_str())?;
        }
        write!(f, ")")?;

        // ON CONFLICT
        if let Some(conflict) = &self.on_conflict {
            write!(f, "\nON CONFLICT (")?;
            for (i, col) in conflict.columns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
            write!(f, ")")?;

            match &conflict.action {
                ConflictAction::DoNothing => {
                    write!(f, " DO NOTHING")?;
                }
                ConflictAction::DoUpdate(assignments) => {
                    write!(f, " DO UPDATE SET ")?;
                    for (i, assign) in assignments.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        let col = Ident(assign.column.as_str());
                        let val = Fmt(ctx, &assign.value);
                        write!(f, "{col} = {val}")?;
                    }
                }
            }
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for Stmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Select(s) => s.render(ctx, f),
            Stmt::Insert(s) => s.render(ctx, f),
            Stmt::InsertSelect(s) => s.render(ctx, f),
            Stmt::Update(s) => s.render(ctx, f),
            Stmt::Delete(s) => s.render(ctx, f),
        }
    }
}

// ============================================================================
// Convenience methods
// ============================================================================

/// Render a statement to SQL.
pub fn render(stmt: &impl Render) -> RenderedSql {
    let ctx = RenderContext::new();
    let sql = format!("{}", Fmt(&ctx, stmt));
    RenderedSql {
        sql,
        params: ctx.into_params(),
    }
}

#[cfg(test)]
mod tests;
