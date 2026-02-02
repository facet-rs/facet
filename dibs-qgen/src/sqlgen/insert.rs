//! SQL generation for INSERT statements.

use super::common::value_expr_to_expr;
use dibs_query_schema::Insert;
use dibs_sql::{ColumnName, InsertStmt, ParamName, render};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedInsert {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,

    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for an INSERT statement.
pub fn generate_insert_sql(insert: &Insert) -> GeneratedInsert {
    let mut stmt = InsertStmt::new(insert.into.value.clone());

    // VALUES clause
    for (col_meta, value_expr) in &insert.values.columns {
        let col_name = &col_meta.value;
        let expr = value_expr_to_expr(col_name, value_expr);
        stmt = stmt.column(col_name.clone(), expr);
    }

    // RETURNING clause
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &insert.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    for col in &returning_columns {
        stmt = stmt.returning([col.clone()]);
    }

    let rendered = render(&stmt);

    GeneratedInsert {
        sql: rendered.sql,
        params: rendered.params,
        returning_columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;

    fn get_first_insert(source: &str) -> Insert {
        let file = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Insert(i) = decl {
                return i.clone();
            }
        }
        panic!("No insert found in source");
    }

    #[test]
    fn test_simple_insert() {
        let source = r#"
CreateUser @insert{
    params {name @string, email @string}
    into users
    values {name $name, email $email}
    returning {id}
}
"#;
        let insert = get_first_insert(source);
        let result = generate_insert_sql(&insert);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_insert_with_function() {
        let source = r#"
CreateUser @insert{
    params {name @string, email @string}
    into users
    values {name $name, email $email, created_at @now}
    returning {id, name, email, created_at}
}
"#;
        let insert = get_first_insert(source);
        let result = generate_insert_sql(&insert);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_insert_with_default() {
        let source = r#"
CreateProduct @insert{
    params {name @string}
    into products
    values {name $name, status @default}
    returning {id, name, status}
}
"#;
        let insert = get_first_insert(source);
        let result = generate_insert_sql(&insert);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_insert_shorthand_params() {
        let source = r#"
CreateUser @insert{
    params {name @string, email @string}
    into users
    values {name, email}
    returning {id}
}
"#;
        let insert = get_first_insert(source);
        let result = generate_insert_sql(&insert);
        insta::assert_snapshot!(result.sql);
    }
}
