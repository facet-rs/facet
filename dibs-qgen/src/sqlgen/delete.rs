//! SQL generation for DELETE statements.

use super::SqlGenContext;
use super::common::where_to_expr_validated;
use crate::QError;
use dibs_query_schema::Delete;
use dibs_sql::{ColumnName, DeleteStmt, ParamName, render};

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
pub fn generate_delete_sql(
    ctx: &SqlGenContext,
    delete: &Delete,
) -> Result<GeneratedDelete, QError> {
    let mut stmt = DeleteStmt::new(delete.from.value.clone());

    // WHERE clause
    if let Some(where_clause) = &delete.where_clause
        && let Some(expr) = where_to_expr_validated(ctx, where_clause)?
    {
        stmt = stmt.where_(expr);
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

    Ok(GeneratedDelete {
        sql: rendered.sql,
        params: rendered.params,
        returning_columns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;
    use dibs_db_schema::Schema;

    fn get_first_delete(source: &str) -> (Delete, crate::QSource) {
        let (file, qsource) = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Delete(d) = decl {
                return (d.clone(), (*qsource).clone());
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
        let (delete, qsource) = get_first_delete(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_delete_sql(&ctx, &delete).unwrap();
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
        let (delete, qsource) = get_first_delete(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_delete_sql(&ctx, &delete).unwrap();
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
        let (delete, qsource) = get_first_delete(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_delete_sql(&ctx, &delete).unwrap();
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_delete_invalid_filter_args_produces_error() {
        // @lt requires exactly 1 argument, but we provide 2 (whitespace-separated in styx)
        let source = r#"
DeleteOld @delete{
    from records
    where {created_at @lt($a $b)}
}
"#;
        let (delete, qsource) = get_first_delete(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_delete_sql(&ctx, &delete);
        assert!(
            result.is_err(),
            "Should fail validation for @lt with 2 arguments"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("expected 1"),
            "Error should mention expected argument count: {}",
            err
        );
    }
}
