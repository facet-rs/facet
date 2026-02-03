//! SQL generation for UPDATE statements.

use super::SqlGenContext;
use super::common::{value_expr_to_expr, where_to_expr_validated};
use crate::QError;
use dibs_query_schema::Update;
use dibs_sql::{ColumnName, ParamName, UpdateStmt, render};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedUpdate {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,

    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for an UPDATE statement.
pub fn generate_update_sql(
    ctx: &SqlGenContext,
    update: &Update,
) -> Result<GeneratedUpdate, QError> {
    let mut stmt = UpdateStmt::new(update.table.value.clone());

    // SET clause
    for (col_meta, value_expr) in &update.set.columns {
        let col_name = &col_meta.value;
        let expr = value_expr_to_expr(col_name, value_expr);
        stmt = stmt.set(col_name.clone(), expr);
    }

    // WHERE clause
    if let Some(where_clause) = &update.where_clause
        && let Some(expr) = where_to_expr_validated(ctx, where_clause)?
    {
        stmt = stmt.where_(expr);
    }

    // RETURNING clause
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &update.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    for col in &returning_columns {
        stmt = stmt.returning([col.clone()]);
    }

    let rendered = render(&stmt);

    Ok(GeneratedUpdate {
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

    fn get_first_update(source: &str) -> (Update, crate::QSource) {
        let (file, qsource) = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Update(u) = decl {
                return (u.clone(), (*qsource).clone());
            }
        }
        panic!("No update found in source");
    }

    #[test]
    fn test_simple_update() {
        let source = r#"
UpdateUserEmail @update{
    params {id @uuid, email @string}
    table users
    set {email $email}
    where {id $id}
    returning {id, email}
}
"#;
        let (update, qsource) = get_first_update(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_update_sql(&ctx, &update).unwrap();
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_update_with_function() {
        let source = r#"
UpdateUser @update{
    params {id @uuid, name @string}
    table users
    set {name $name, updated_at @now}
    where {id $id}
    returning {id, name, updated_at}
}
"#;
        let (update, qsource) = get_first_update(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_update_sql(&ctx, &update).unwrap();
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_update_multiple_conditions() {
        let source = r#"
UpdateProductStatus @update{
    params {user_id @uuid, old_status @string, new_status @string}
    table products
    set {status $new_status, updated_at @now}
    where {user_id $user_id, status $old_status}
    returning {id, status}
}
"#;
        let (update, qsource) = get_first_update(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_update_sql(&ctx, &update).unwrap();
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_update_shorthand_params() {
        let source = r#"
UpdateUser @update{
    params {id @uuid, name @string, email @string}
    table users
    set {name, email}
    where {id}
    returning {id}
}
"#;
        let (update, qsource) = get_first_update(source);
        let schema = Schema::default();
        let ctx = SqlGenContext::new(&schema, std::sync::Arc::new(qsource));
        let result = generate_update_sql(&ctx, &update).unwrap();
        insta::assert_snapshot!(result.sql);
    }
}
