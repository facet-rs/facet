//! SQL generation for bulk UPSERT statements using UNNEST.
//!
//! Generates SQL like:
//! ```sql
//! INSERT INTO products (handle, status, created_at)
//! SELECT handle, status, NOW()
//! FROM UNNEST($1::text[], $2::text[]) AS t(handle, status)
//! ON CONFLICT (handle) DO UPDATE SET status = EXCLUDED.status, updated_at = NOW()
//! RETURNING id, handle, status
//! ```

use dibs_query_schema::{ParamType, Payload, UpdateValue, UpsertMany, ValueExpr};
use dibs_sql::{ColumnName, ParamName};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedUpsertMany {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,

    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for a bulk UPSERT statement.
pub fn generate_upsert_many_sql(upsert: &UpsertMany) -> GeneratedUpsertMany {
    let mut sql = String::new();
    let mut params: Vec<ParamName> = Vec::new();

    // Collect param names for UNNEST
    let param_names: Vec<&str> = upsert
        .params
        .as_ref()
        .map(|p| p.params.keys().map(|k| k.value.as_str()).collect())
        .unwrap_or_default();

    // INSERT INTO table (columns)
    sql.push_str("INSERT INTO \"");
    sql.push_str(upsert.into.value.as_str());
    sql.push_str("\" (");

    let columns: Vec<&str> = upsert
        .values
        .columns
        .keys()
        .map(|k| k.value.as_str())
        .collect();
    sql.push_str(
        &columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push(')');

    // SELECT expressions FROM UNNEST
    sql.push_str(" SELECT ");

    let select_exprs: Vec<String> = upsert
        .values
        .columns
        .iter()
        .map(|(col_meta, expr)| value_expr_to_unnest_select(&col_meta.value, expr, &param_names))
        .collect();
    sql.push_str(&select_exprs.join(", "));

    // FROM UNNEST($1::type[], $2::type[], ...) AS t(col1, col2, ...)
    sql.push_str(" FROM UNNEST(");

    if let Some(params_def) = &upsert.params {
        let unnest_params: Vec<String> = params_def
            .params
            .iter()
            .enumerate()
            .map(|(i, (name_meta, ty))| {
                params.push(name_meta.value.clone());
                let pg_type = param_type_to_pg_array(ty);
                format!("${}::{}", i + 1, pg_type)
            })
            .collect();
        sql.push_str(&unnest_params.join(", "));
    }

    sql.push_str(") AS t(");
    sql.push_str(&param_names.join(", "));
    sql.push(')');

    // ON CONFLICT (columns) DO UPDATE SET ...
    let conflict_columns: Vec<&str> = upsert
        .on_conflict
        .target
        .columns
        .keys()
        .map(|k| k.value.as_str())
        .collect();

    sql.push_str(" ON CONFLICT (");
    sql.push_str(
        &conflict_columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push_str(") DO UPDATE SET ");

    // Build update assignments
    let update_assignments: Vec<String> = upsert
        .on_conflict
        .update
        .columns
        .iter()
        .map(|(col_meta, update_value)| {
            let col = &col_meta.value;
            let value = update_value_to_excluded(col, update_value, &param_names);
            format!("\"{}\" = {}", col, value)
        })
        .collect();
    sql.push_str(&update_assignments.join(", "));

    // RETURNING
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &upsert.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    if !returning_columns.is_empty() {
        sql.push_str(" RETURNING ");
        sql.push_str(
            &returning_columns
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    GeneratedUpsertMany {
        sql,
        params,
        returning_columns,
    }
}

/// Convert a ValueExpr to a SELECT expression for UNNEST queries.
fn value_expr_to_unnest_select(
    col: &ColumnName,
    expr: &Option<ValueExpr>,
    param_names: &[&str],
) -> String {
    match expr {
        None => {
            // Shorthand: {col} means reference the UNNEST column
            col.to_string()
        }
        Some(ValueExpr::Default) => "DEFAULT".to_string(),
        Some(ValueExpr::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name)
            (None, Some(Payload::Scalar(s))) => {
                let val = &s.value;
                if let Some(name) = val.strip_prefix('$') {
                    if param_names.contains(&name) {
                        name.to_string()
                    } else {
                        format!("${}", name)
                    }
                } else {
                    // Literal value
                    if val.parse::<i64>().is_ok() || val == "true" || val == "false" {
                        val.clone()
                    } else {
                        format!("'{}'", val.replace('\'', "''"))
                    }
                }
            }
            // Nullary function like @now
            (Some(name), None) => format!("{}()", name.to_uppercase()),
            // Function with args
            (Some(name), Some(Payload::Seq(args))) => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| value_expr_to_unnest_select(col, &Some(a.clone()), param_names))
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => {
                let arg = value_expr_to_unnest_select(
                    col,
                    &Some(ValueExpr::Other {
                        tag: None,
                        content: Some(Payload::Scalar(s.clone())),
                    }),
                    param_names,
                );
                format!("{}({})", name.to_uppercase(), arg)
            }
            (None, None) => "NULL".to_string(),
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => "NULL".to_string(),
        },
    }
}

/// Convert an UpdateValue to an EXCLUDED reference for ON CONFLICT DO UPDATE.
fn update_value_to_excluded(
    col: &ColumnName,
    expr: &Option<UpdateValue>,
    param_names: &[&str],
) -> String {
    match expr {
        None => {
            // Shorthand: {col} means use EXCLUDED.col
            format!("EXCLUDED.\"{}\"", col)
        }
        Some(UpdateValue::Default) => "DEFAULT".to_string(),
        Some(UpdateValue::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name)
            (None, Some(Payload::Scalar(s))) => {
                let val = &s.value;
                if let Some(name) = val.strip_prefix('$') {
                    if param_names.contains(&name) {
                        format!("EXCLUDED.\"{}\"", col)
                    } else {
                        format!("${}", name)
                    }
                } else {
                    // Literal value
                    if val.parse::<i64>().is_ok() || val == "true" || val == "false" {
                        val.clone()
                    } else {
                        format!("'{}'", val.replace('\'', "''"))
                    }
                }
            }
            // Nullary function like @now
            (Some(name), None) => format!("{}()", name.to_uppercase()),
            // Function with args
            (Some(name), Some(Payload::Seq(args))) => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| {
                        // Convert ValueExpr args
                        value_expr_to_unnest_select(col, &Some(a.clone()), param_names)
                    })
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => {
                let arg = value_expr_to_unnest_select(
                    col,
                    &Some(ValueExpr::Other {
                        tag: None,
                        content: Some(Payload::Scalar(s.clone())),
                    }),
                    param_names,
                );
                format!("{}({})", name.to_uppercase(), arg)
            }
            (None, None) => "NULL".to_string(),
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => "NULL".to_string(),
        },
    }
}

/// Convert a ParamType to PostgreSQL array type.
fn param_type_to_pg_array(ty: &ParamType) -> &'static str {
    match ty {
        ParamType::String => "text[]",
        ParamType::Int => "bigint[]",
        ParamType::Bool => "boolean[]",
        ParamType::Uuid => "uuid[]",
        ParamType::Decimal => "numeric[]",
        ParamType::Timestamp => "timestamptz[]",
        ParamType::Bytes => "bytea[]",
        ParamType::Optional(inner_vec) => {
            if let Some(inner) = inner_vec.first() {
                param_type_to_pg_array(inner)
            } else {
                "text[]"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;

    fn get_first_upsert_many(source: &str) -> UpsertMany {
        let (file, _source) = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::UpsertMany(um) = decl {
                return um.clone();
            }
        }
        panic!("No upsert-many found in source");
    }

    #[test]
    fn test_simple_upsert_many() {
        let source = r#"
BulkUpsertProducts @upsert-many{
    params {handle @string, status @string}
    into products
    on-conflict {
        target {handle}
        update {status}
    }
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
        let upsert_many = get_first_upsert_many(source);
        let result = generate_upsert_many_sql(&upsert_many);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_upsert_many_with_functions() {
        let source = r#"
BulkUpsertProducts @upsert-many{
    params {handle @string, status @string}
    into products
    on-conflict {
        target {handle}
        update {status, updated_at @now}
    }
    values {handle $handle, status $status, created_at @now}
    returning {id, handle, status}
}
"#;
        let upsert_many = get_first_upsert_many(source);
        let result = generate_upsert_many_sql(&upsert_many);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_upsert_many_multiple_conflict_columns() {
        let source = r#"
BulkUpsertTranslations @upsert-many{
    params {product_id @uuid, locale @string, title @string}
    into product_translations
    on-conflict {
        target {product_id, locale}
        update {title}
    }
    values {product_id $product_id, locale $locale, title $title}
    returning {id}
}
"#;
        let upsert_many = get_first_upsert_many(source);
        let result = generate_upsert_many_sql(&upsert_many);
        insta::assert_snapshot!(result.sql);
    }
}
