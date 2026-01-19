//! SQL query building.
//!
//! Converts AST types to parameterized SQL strings for Postgres.

use super::{DeleteQuery, Expr, InsertQuery, SelectQuery, SortDir, UpdateQuery, Value};

/// Result of building a query: SQL string and parameter values.
#[derive(Debug)]
pub struct BuiltQuery {
    /// The SQL string with $1, $2, etc. placeholders
    pub sql: String,
    /// The parameter values in order
    pub params: Vec<Value>,
}

/// Builds SQL from expressions, tracking parameter indices.
struct SqlBuilder {
    sql: String,
    params: Vec<Value>,
}

impl SqlBuilder {
    fn new() -> Self {
        Self {
            sql: String::new(),
            params: Vec::new(),
        }
    }

    fn push(&mut self, s: &str) {
        self.sql.push_str(s);
    }

    fn push_param(&mut self, value: Value) {
        self.params.push(value);
        self.sql.push('$');
        self.sql.push_str(&self.params.len().to_string());
    }

    fn push_ident(&mut self, name: &str) {
        // Quote identifier to handle reserved words and special chars
        self.sql.push('"');
        // Escape any double quotes in the identifier
        for c in name.chars() {
            if c == '"' {
                self.sql.push('"');
            }
            self.sql.push(c);
        }
        self.sql.push('"');
    }

    fn build_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Eq(col, val) => {
                self.push_ident(col);
                self.push(" = ");
                self.push_param(val.clone());
            }
            Expr::Ne(col, val) => {
                self.push_ident(col);
                self.push(" != ");
                self.push_param(val.clone());
            }
            Expr::Lt(col, val) => {
                self.push_ident(col);
                self.push(" < ");
                self.push_param(val.clone());
            }
            Expr::Lte(col, val) => {
                self.push_ident(col);
                self.push(" <= ");
                self.push_param(val.clone());
            }
            Expr::Gt(col, val) => {
                self.push_ident(col);
                self.push(" > ");
                self.push_param(val.clone());
            }
            Expr::Gte(col, val) => {
                self.push_ident(col);
                self.push(" >= ");
                self.push_param(val.clone());
            }
            Expr::Like(col, pattern) => {
                self.push_ident(col);
                self.push(" LIKE ");
                self.push_param(Value::String(pattern.clone()));
            }
            Expr::ILike(col, pattern) => {
                self.push_ident(col);
                self.push(" ILIKE ");
                self.push_param(Value::String(pattern.clone()));
            }
            Expr::IsNull(col) => {
                self.push_ident(col);
                self.push(" IS NULL");
            }
            Expr::IsNotNull(col) => {
                self.push_ident(col);
                self.push(" IS NOT NULL");
            }
            Expr::In(col, values) => {
                self.push_ident(col);
                self.push(" IN (");
                for (i, val) in values.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push_param(val.clone());
                }
                self.push(")");
            }
            Expr::And(exprs) => {
                if exprs.is_empty() {
                    self.push("TRUE");
                } else {
                    self.push("(");
                    for (i, e) in exprs.iter().enumerate() {
                        if i > 0 {
                            self.push(" AND ");
                        }
                        self.build_expr(e);
                    }
                    self.push(")");
                }
            }
            Expr::Or(exprs) => {
                if exprs.is_empty() {
                    self.push("FALSE");
                } else {
                    self.push("(");
                    for (i, e) in exprs.iter().enumerate() {
                        if i > 0 {
                            self.push(" OR ");
                        }
                        self.build_expr(e);
                    }
                    self.push(")");
                }
            }
            Expr::Not(e) => {
                self.push("NOT (");
                self.build_expr(e);
                self.push(")");
            }
        }
    }

    fn build_where(&mut self, filters: &[Expr]) {
        if filters.is_empty() {
            return;
        }
        self.push(" WHERE ");
        for (i, expr) in filters.iter().enumerate() {
            if i > 0 {
                self.push(" AND ");
            }
            self.build_expr(expr);
        }
    }

    fn build_returning(&mut self, returning: &[String]) {
        if returning.is_empty() {
            return;
        }
        self.push(" RETURNING ");
        for (i, col) in returning.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            if col == "*" {
                self.push("*");
            } else {
                self.push_ident(col);
            }
        }
    }

    fn finish(self) -> BuiltQuery {
        BuiltQuery {
            sql: self.sql,
            params: self.params,
        }
    }
}

impl SelectQuery {
    /// Build the SELECT query.
    pub fn build(&self) -> BuiltQuery {
        let mut b = SqlBuilder::new();

        b.push("SELECT ");
        if self.columns.is_empty() {
            b.push("*");
        } else {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    b.push(", ");
                }
                b.push_ident(col);
            }
        }

        b.push(" FROM ");
        b.push_ident(&self.table);

        b.build_where(&self.filters);

        if !self.order.is_empty() {
            b.push(" ORDER BY ");
            for (i, (col, dir)) in self.order.iter().enumerate() {
                if i > 0 {
                    b.push(", ");
                }
                b.push_ident(col);
                match dir {
                    SortDir::Asc => b.push(" ASC"),
                    SortDir::Desc => b.push(" DESC"),
                }
            }
        }

        if let Some(limit) = self.limit {
            b.push(" LIMIT ");
            b.push(&limit.to_string());
        }

        if let Some(offset) = self.offset {
            b.push(" OFFSET ");
            b.push(&offset.to_string());
        }

        b.finish()
    }

    /// Build a COUNT(*) query (ignores columns, order, limit, offset).
    pub fn build_count(&self) -> BuiltQuery {
        let mut b = SqlBuilder::new();

        b.push("SELECT COUNT(*) FROM ");
        b.push_ident(&self.table);

        b.build_where(&self.filters);

        b.finish()
    }
}

impl InsertQuery {
    /// Build the INSERT query.
    pub fn build(&self) -> BuiltQuery {
        let mut b = SqlBuilder::new();

        b.push("INSERT INTO ");
        b.push_ident(&self.table);

        if !self.columns.is_empty() {
            b.push(" (");
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    b.push(", ");
                }
                b.push_ident(col);
            }
            b.push(") VALUES (");
            for (i, val) in self.values.iter().enumerate() {
                if i > 0 {
                    b.push(", ");
                }
                b.push_param(val.clone());
            }
            b.push(")");
        } else {
            b.push(" DEFAULT VALUES");
        }

        b.build_returning(&self.returning);

        b.finish()
    }
}

impl UpdateQuery {
    /// Build the UPDATE query.
    pub fn build(&self) -> BuiltQuery {
        let mut b = SqlBuilder::new();

        b.push("UPDATE ");
        b.push_ident(&self.table);
        b.push(" SET ");

        for (i, (col, val)) in self.changes.iter().enumerate() {
            if i > 0 {
                b.push(", ");
            }
            b.push_ident(col);
            b.push(" = ");
            b.push_param(val.clone());
        }

        b.build_where(&self.filters);
        b.build_returning(&self.returning);

        b.finish()
    }
}

impl DeleteQuery {
    /// Build the DELETE query.
    pub fn build(&self) -> BuiltQuery {
        let mut b = SqlBuilder::new();

        b.push("DELETE FROM ");
        b.push_ident(&self.table);

        b.build_where(&self.filters);
        b.build_returning(&self.returning);

        b.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_simple() {
        let q = SelectQuery::new("users").build();
        assert_eq!(q.sql, r#"SELECT * FROM "users""#);
        assert!(q.params.is_empty());
    }

    #[test]
    fn test_select_with_columns() {
        let q = SelectQuery::new("users")
            .columns(["id", "name", "email"])
            .build();
        assert_eq!(q.sql, r#"SELECT "id", "name", "email" FROM "users""#);
    }

    #[test]
    fn test_select_with_filter() {
        let q = SelectQuery::new("users")
            .filter(Expr::eq("status", "active"))
            .build();
        assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE "status" = $1"#);
        assert_eq!(q.params, vec![Value::String("active".into())]);
    }

    #[test]
    fn test_select_with_multiple_filters() {
        let q = SelectQuery::new("users")
            .filter(Expr::eq("status", "active"))
            .filter(Expr::gte("age", 18i32))
            .build();
        assert_eq!(
            q.sql,
            r#"SELECT * FROM "users" WHERE "status" = $1 AND "age" >= $2"#
        );
        assert_eq!(q.params.len(), 2);
    }

    #[test]
    fn test_select_with_order_and_limit() {
        let q = SelectQuery::new("users")
            .order_by("created_at", SortDir::Desc)
            .limit(10)
            .offset(20)
            .build();
        assert_eq!(
            q.sql,
            r#"SELECT * FROM "users" ORDER BY "created_at" DESC LIMIT 10 OFFSET 20"#
        );
    }

    #[test]
    fn test_insert() {
        let q = InsertQuery::new("users")
            .values([("name", "Alice"), ("email", "alice@example.com")])
            .returning_all()
            .build();
        assert_eq!(
            q.sql,
            r#"INSERT INTO "users" ("name", "email") VALUES ($1, $2) RETURNING *"#
        );
        assert_eq!(q.params.len(), 2);
    }

    #[test]
    fn test_update() {
        let q = UpdateQuery::new("users")
            .set([("name", Value::String("Bob".into()))])
            .filter(Expr::eq("id", 42i64))
            .returning_all()
            .build();
        assert_eq!(
            q.sql,
            r#"UPDATE "users" SET "name" = $1 WHERE "id" = $2 RETURNING *"#
        );
    }

    #[test]
    fn test_delete() {
        let q = DeleteQuery::new("users")
            .filter(Expr::eq("id", 42i64))
            .build();
        assert_eq!(q.sql, r#"DELETE FROM "users" WHERE "id" = $1"#);
    }

    #[test]
    fn test_or_expression() {
        let q = SelectQuery::new("users")
            .filter(Expr::or([
                Expr::eq("status", "active"),
                Expr::eq("status", "pending"),
            ]))
            .build();
        assert_eq!(
            q.sql,
            r#"SELECT * FROM "users" WHERE ("status" = $1 OR "status" = $2)"#
        );
    }

    #[test]
    fn test_in_expression() {
        let q = SelectQuery::new("users")
            .filter(Expr::is_in("id", [1i64, 2i64, 3i64]))
            .build();
        assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE "id" IN ($1, $2, $3)"#);
    }
}
