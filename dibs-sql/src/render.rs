//! Render SQL AST to string.

use indexmap::IndexMap;

use crate::expr::{ColumnRef, Expr};
use crate::stmt::*;
use crate::{RenderedSql, escape_string, quote_ident};

/// Rendering context that tracks parameters and formatting.
pub struct RenderContext {
    /// Named parameters -> their assigned index
    params: IndexMap<String, usize>,
    /// Next parameter index to assign
    next_param_idx: usize,
    /// The SQL being built
    sql: String,
    /// Current indentation level
    indent_level: usize,
    /// Whether we're at the start of a line
    at_line_start: bool,
    /// Whether to format with newlines/indentation
    pretty: bool,
}

impl RenderContext {
    pub fn new() -> Self {
        Self {
            params: IndexMap::new(),
            next_param_idx: 1,
            sql: String::new(),
            indent_level: 0,
            at_line_start: true,
            pretty: false,
        }
    }

    pub fn pretty() -> Self {
        Self {
            pretty: true,
            ..Self::new()
        }
    }

    /// Get or create a parameter placeholder.
    fn param(&mut self, name: &str) -> String {
        let idx = *self.params.entry(name.to_string()).or_insert_with(|| {
            let idx = self.next_param_idx;
            self.next_param_idx += 1;
            idx
        });
        format!("${}", idx)
    }

    fn write(&mut self, s: &str) {
        if self.pretty && self.at_line_start && self.indent_level > 0 {
            for _ in 0..self.indent_level {
                self.sql.push_str("    ");
            }
        }
        self.sql.push_str(s);
        self.at_line_start = false;
    }

    fn space(&mut self) {
        if !self.sql.is_empty() && !self.at_line_start {
            self.sql.push(' ');
        }
    }

    fn newline(&mut self) {
        if self.pretty {
            self.sql.push('\n');
            self.at_line_start = true;
        } else {
            self.space();
        }
    }

    #[allow(dead_code)]
    fn indent(&mut self) {
        self.indent_level += 1;
    }

    #[allow(dead_code)]
    fn dedent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    /// Finish rendering and return the result.
    pub fn finish(self) -> RenderedSql {
        RenderedSql {
            sql: self.sql,
            params: self.params.into_keys().collect(),
        }
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Render implementations
// ============================================================================

/// Trait for types that can be rendered to SQL.
pub trait Render {
    fn render(&self, ctx: &mut RenderContext);
}

impl Render for Expr {
    fn render(&self, ctx: &mut RenderContext) {
        match self {
            Expr::Param(name) => {
                let placeholder = ctx.param(name);
                ctx.write(&placeholder);
            }
            Expr::Column(col) => col.render(ctx),
            Expr::String(s) => ctx.write(&escape_string(s)),
            Expr::Int(n) => ctx.write(&n.to_string()),
            Expr::Bool(b) => ctx.write(if *b { "TRUE" } else { "FALSE" }),
            Expr::Null => ctx.write("NULL"),
            Expr::Now => ctx.write("NOW()"),
            Expr::Default => ctx.write("DEFAULT"),
            Expr::BinOp { left, op, right } => {
                left.render(ctx);
                ctx.space();
                ctx.write(op.as_str());
                ctx.space();
                right.render(ctx);
            }
            Expr::IsNull { expr, negated } => {
                expr.render(ctx);
                ctx.write(if *negated { " IS NOT NULL" } else { " IS NULL" });
            }
            Expr::ILike { expr, pattern } => {
                expr.render(ctx);
                ctx.write(" ILIKE ");
                pattern.render(ctx);
            }
            Expr::FnCall { name, args } => {
                ctx.write(name);
                ctx.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        ctx.write(", ");
                    }
                    arg.render(ctx);
                }
                ctx.write(")");
            }
            Expr::Count { table } => {
                ctx.write("COUNT(");
                ctx.write(&quote_ident(table));
                ctx.write(".*)");
            }
            Expr::Raw(s) => ctx.write(s),
        }
    }
}

impl Render for ColumnRef {
    fn render(&self, ctx: &mut RenderContext) {
        if let Some(table) = &self.table {
            ctx.write(&quote_ident(table));
            ctx.write(".");
        }
        ctx.write(&quote_ident(&self.column));
    }
}

impl Render for SelectStmt {
    fn render(&self, ctx: &mut RenderContext) {
        ctx.write("SELECT");

        // Columns
        if self.columns.is_empty() {
            ctx.write(" *");
        } else {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    ctx.write(",");
                }
                ctx.space();
                col.render(ctx);
            }
        }

        // FROM
        if let Some(from) = &self.from {
            ctx.newline();
            ctx.write("FROM ");
            ctx.write(&quote_ident(&from.table));
            if let Some(alias) = &from.alias {
                ctx.write(" ");
                ctx.write(&quote_ident(alias));
            }
        }

        // JOINs
        for join in &self.joins {
            ctx.newline();
            ctx.write(join.kind.as_str());
            ctx.write(" ");
            ctx.write(&quote_ident(&join.table));
            if let Some(alias) = &join.alias {
                ctx.write(" ");
                ctx.write(&quote_ident(alias));
            }
            ctx.write(" ON ");
            join.on.render(ctx);
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            ctx.newline();
            ctx.write("WHERE ");
            where_.render(ctx);
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            ctx.newline();
            ctx.write("ORDER BY ");
            for (i, order) in self.order_by.iter().enumerate() {
                if i > 0 {
                    ctx.write(", ");
                }
                order.expr.render(ctx);
                ctx.write(if order.desc { " DESC" } else { " ASC" });
                if let Some(nulls) = &order.nulls {
                    ctx.write(match nulls {
                        NullsOrder::First => " NULLS FIRST",
                        NullsOrder::Last => " NULLS LAST",
                    });
                }
            }
        }

        // LIMIT
        if let Some(limit) = &self.limit {
            ctx.newline();
            ctx.write("LIMIT ");
            limit.render(ctx);
        }

        // OFFSET
        if let Some(offset) = &self.offset {
            ctx.newline();
            ctx.write("OFFSET ");
            offset.render(ctx);
        }
    }
}

impl Render for SelectColumn {
    fn render(&self, ctx: &mut RenderContext) {
        match self {
            SelectColumn::Expr { expr, alias } => {
                expr.render(ctx);
                if let Some(alias) = alias {
                    ctx.write(" AS ");
                    ctx.write(&quote_ident(alias));
                }
            }
            SelectColumn::AllFrom(table) => {
                ctx.write(&quote_ident(table));
                ctx.write(".*");
            }
        }
    }
}

impl Render for InsertStmt {
    fn render(&self, ctx: &mut RenderContext) {
        ctx.write("INSERT INTO ");
        ctx.write(&quote_ident(&self.table));

        // Columns
        ctx.write(" (");
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                ctx.write(", ");
            }
            ctx.write(&quote_ident(col));
        }
        ctx.write(")");

        // VALUES
        ctx.newline();
        ctx.write("VALUES (");
        for (i, val) in self.values.iter().enumerate() {
            if i > 0 {
                ctx.write(", ");
            }
            val.render(ctx);
        }
        ctx.write(")");

        // ON CONFLICT
        if let Some(conflict) = &self.on_conflict {
            ctx.newline();
            ctx.write("ON CONFLICT (");
            for (i, col) in conflict.columns.iter().enumerate() {
                if i > 0 {
                    ctx.write(", ");
                }
                ctx.write(&quote_ident(col));
            }
            ctx.write(")");

            match &conflict.action {
                ConflictAction::DoNothing => {
                    ctx.write(" DO NOTHING");
                }
                ConflictAction::DoUpdate(assignments) => {
                    ctx.write(" DO UPDATE SET ");
                    for (i, assign) in assignments.iter().enumerate() {
                        if i > 0 {
                            ctx.write(", ");
                        }
                        ctx.write(&quote_ident(&assign.column));
                        ctx.write(" = ");
                        assign.value.render(ctx);
                    }
                }
            }
        }

        // RETURNING
        if !self.returning.is_empty() {
            ctx.newline();
            ctx.write("RETURNING ");
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    ctx.write(", ");
                }
                ctx.write(&quote_ident(col));
            }
        }
    }
}

impl Render for UpdateStmt {
    fn render(&self, ctx: &mut RenderContext) {
        ctx.write("UPDATE ");
        ctx.write(&quote_ident(&self.table));

        // SET
        ctx.newline();
        ctx.write("SET ");
        for (i, assign) in self.assignments.iter().enumerate() {
            if i > 0 {
                ctx.write(", ");
            }
            ctx.write(&quote_ident(&assign.column));
            ctx.write(" = ");
            assign.value.render(ctx);
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            ctx.newline();
            ctx.write("WHERE ");
            where_.render(ctx);
        }

        // RETURNING
        if !self.returning.is_empty() {
            ctx.newline();
            ctx.write("RETURNING ");
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    ctx.write(", ");
                }
                ctx.write(&quote_ident(col));
            }
        }
    }
}

impl Render for DeleteStmt {
    fn render(&self, ctx: &mut RenderContext) {
        ctx.write("DELETE FROM ");
        ctx.write(&quote_ident(&self.table));

        // WHERE
        if let Some(where_) = &self.where_ {
            ctx.newline();
            ctx.write("WHERE ");
            where_.render(ctx);
        }

        // RETURNING
        if !self.returning.is_empty() {
            ctx.newline();
            ctx.write("RETURNING ");
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    ctx.write(", ");
                }
                ctx.write(&quote_ident(col));
            }
        }
    }
}

impl Render for Stmt {
    fn render(&self, ctx: &mut RenderContext) {
        match self {
            Stmt::Select(s) => s.render(ctx),
            Stmt::Insert(s) => s.render(ctx),
            Stmt::Update(s) => s.render(ctx),
            Stmt::Delete(s) => s.render(ctx),
        }
    }
}

// ============================================================================
// Convenience methods
// ============================================================================

/// Render a statement to SQL with default (compact) formatting.
pub fn render(stmt: &impl Render) -> RenderedSql {
    let mut ctx = RenderContext::new();
    stmt.render(&mut ctx);
    ctx.finish()
}

/// Render a statement to SQL with pretty formatting (newlines, indentation).
pub fn render_pretty(stmt: &impl Render) -> RenderedSql {
    let mut ctx = RenderContext::pretty();
    stmt.render(&mut ctx);
    ctx.finish()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Expr;

    #[test]
    fn test_param_deduplication() {
        // Build: INSERT INTO t (a, b) VALUES ($a, $b) ON CONFLICT (a) DO UPDATE SET b = $b
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::param("status"))
            .on_conflict(OnConflict {
                columns: vec!["handle".into()],
                action: ConflictAction::DoUpdate(vec![UpdateAssignment::new(
                    "status",
                    Expr::param("status"), // same param, should be $2 not $3
                )]),
            })
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);

        // Key assertion: params should only have 2 entries
        assert_eq!(result.params, vec!["handle", "status"]);

        // SQL should reuse $2 for both VALUES and UPDATE SET
        assert!(result.sql.contains("VALUES ($1, $2)"));
        assert!(result.sql.contains("\"status\" = $2"));
    }

    #[test]
    fn test_simple_select() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::column("id")),
                SelectColumn::expr(Expr::column("name")),
            ])
            .from(FromClause::table("users"));

        let result = render(&stmt);
        assert_eq!(result.sql, "SELECT \"id\", \"name\" FROM \"users\"");
    }

    #[test]
    fn test_select_with_where() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("id").eq(Expr::param("id")));

        let result = render(&stmt);
        assert_eq!(result.sql, "SELECT \"id\" FROM \"users\" WHERE \"id\" = $1");
        assert_eq!(result.params, vec!["id"]);
    }

    #[test]
    fn test_insert() {
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::param("status"))
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "INSERT INTO \"products\" (\"handle\", \"status\") VALUES ($1, $2) RETURNING \"id\", \"handle\", \"status\""
        );
        assert_eq!(result.params, vec!["handle", "status"]);
    }

    #[test]
    fn test_insert_with_literals() {
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::Default)
            .column("created_at", Expr::Now);

        let result = render(&stmt);
        assert!(result.sql.contains("VALUES ($1, DEFAULT, NOW())"));
        assert_eq!(result.params, vec!["handle"]);
    }

    #[test]
    fn test_update() {
        let stmt = UpdateStmt::new("products")
            .set("status", Expr::param("status"))
            .where_(Expr::column("handle").eq(Expr::param("handle")))
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "UPDATE \"products\" SET \"status\" = $1 WHERE \"handle\" = $2 RETURNING \"id\", \"handle\", \"status\""
        );
        assert_eq!(result.params, vec!["status", "handle"]);
    }

    #[test]
    fn test_delete() {
        let stmt = DeleteStmt::new("products")
            .where_(Expr::column("id").eq(Expr::param("id")))
            .returning(["id", "handle"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "DELETE FROM \"products\" WHERE \"id\" = $1 RETURNING \"id\", \"handle\""
        );
        assert_eq!(result.params, vec!["id"]);
    }

    #[test]
    fn test_qualified_columns() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::qualified_column("t0", "id")),
                SelectColumn::expr(Expr::qualified_column("t1", "name")),
            ])
            .from(FromClause::aliased("users", "t0"))
            .join(Join {
                kind: JoinKind::Left,
                table: "profiles".into(),
                alias: Some("t1".into()),
                on: Expr::qualified_column("t1", "user_id").eq(Expr::qualified_column("t0", "id")),
            });

        let result = render(&stmt);
        assert!(result.sql.contains("\"t0\".\"id\""));
        assert!(result.sql.contains("\"t1\".\"name\""));
        assert!(result.sql.contains("LEFT JOIN \"profiles\" \"t1\" ON"));
    }

    #[test]
    fn test_pretty_formatting() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::column("id")),
                SelectColumn::expr(Expr::column("name")),
            ])
            .from(FromClause::table("users"))
            .where_(Expr::column("active").eq(Expr::Bool(true)))
            .order_by(OrderBy::desc(Expr::column("created_at")))
            .limit(Expr::Int(10));

        let result = render_pretty(&stmt);
        assert!(result.sql.contains("\n"), "Should have newlines");
        assert!(result.sql.contains("FROM"));
        assert!(result.sql.contains("WHERE"));
        assert!(result.sql.contains("ORDER BY"));
        assert!(result.sql.contains("LIMIT"));
    }

    #[test]
    fn test_is_null() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("deleted_at").is_null());

        let result = render(&stmt);
        assert!(result.sql.contains("\"deleted_at\" IS NULL"));
    }

    #[test]
    fn test_ilike() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("name").ilike(Expr::param("pattern")));

        let result = render(&stmt);
        assert!(result.sql.contains("\"name\" ILIKE $1"));
        assert_eq!(result.params, vec!["pattern"]);
    }
}
