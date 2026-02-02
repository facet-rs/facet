//! SQL generation for DELETE statements.

use super::common::filter_value_to_expr;
use dibs_query_schema::{Delete, Where};
use dibs_sql::{ColumnName, DeleteStmt, Expr, ParamName, render};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedDelete {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,

    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for a DELETE statement.
pub fn generate_delete_sql(delete: &Delete) -> GeneratedDelete {
    let mut stmt = DeleteStmt::new(delete.from.value.clone());

    // WHERE clause
    if let Some(where_clause) = &delete.where_clause {
        if let Some(expr) = where_to_expr(where_clause) {
            stmt = stmt.where_(expr);
        }
    }

    // RETURNING clause
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &delete.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    for col in &returning_columns {
        stmt = stmt.returning([col.clone()]);
    }

    let rendered = render(&stmt);

    GeneratedDelete {
        sql: rendered.sql,
        params: rendered.params,
        returning_columns,
    }
}

/// Convert a WHERE clause to a dibs_sql::Expr.
fn where_to_expr(where_clause: &Where) -> Option<Expr> {
    let mut exprs: Vec<Expr> = vec![];

    for (col_meta, filter_value) in &where_clause.filters {
        let col_name = &col_meta.value;
        if let Some(expr) = filter_value_to_expr(col_name, filter_value) {
            exprs.push(expr);
        }
    }

    // AND all expressions together
    let mut iter = exprs.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, expr| acc.and(expr)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;

    fn get_first_delete(source: &str) -> Delete {
        let file = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Delete(d) = decl {
                return d.clone();
            }
        }
        panic!("No delete found in source");
    }

    #[test]
    fn test_simple_delete() {
        let source = r#"
DeleteUser @delete{
    params {id @int}
    from users
    where {id $id}
    returning {id}
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_delete_no_returning() {
        let source = r#"
DeleteOldSessions @delete{
    from sessions
    where {expired_at @lt($now)}
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_delete_multiple_conditions() {
        let source = r#"
DeleteUserPosts @delete{
    params {user_id @int, status @string}
    from posts
    where {user_id $user_id, status $status, deleted_at @null}
    returning {id, title}
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_snapshot!(result.sql);
    }
}
