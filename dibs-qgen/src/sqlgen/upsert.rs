//! SQL generation for UPSERT statements (INSERT ... ON CONFLICT ... DO UPDATE).

use super::common::{update_value_to_expr, value_expr_to_expr};
use dibs_query_schema::Upsert;
use dibs_sql::{
    ColumnName, ConflictAction, InsertStmt, OnConflict, ParamName, UpdateAssignment, render,
};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedUpsert {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,

    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for an UPSERT statement.
pub fn generate_upsert_sql(upsert: &Upsert) -> GeneratedUpsert {
    let mut stmt = InsertStmt::new(upsert.into.value.clone());

    // VALUES clause
    for (col_meta, value_expr) in &upsert.values.columns {
        let col_name = &col_meta.value;
        let expr = value_expr_to_expr(col_name, value_expr);
        stmt = stmt.column(col_name.clone(), expr);
    }

    // ON CONFLICT clause
    let conflict_columns: Vec<ColumnName> = upsert
        .on_conflict
        .target
        .columns
        .keys()
        .map(|k| k.value.clone())
        .collect();

    // Build update assignments from on_conflict.update
    let update_assignments: Vec<UpdateAssignment> = upsert
        .on_conflict
        .update
        .columns
        .iter()
        .map(|(col_meta, update_value)| {
            let col_name = &col_meta.value;
            let expr = update_value_to_expr(col_name, update_value);
            UpdateAssignment::new(col_name.clone(), expr)
        })
        .collect();

    stmt = stmt.on_conflict(OnConflict {
        columns: conflict_columns,
        action: ConflictAction::DoUpdate(update_assignments),
    });

    // RETURNING clause
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &upsert.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    for col in &returning_columns {
        stmt = stmt.returning([col.clone()]);
    }

    let rendered = render(&stmt);

    GeneratedUpsert {
        sql: rendered.sql,
        params: rendered.params,
        returning_columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;

    fn get_first_upsert(source: &str) -> Upsert {
        let (file, _source) = parse_query_file(camino::Utf8Path::new("<test>"), source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Upsert(u) = decl {
                return u.clone();
            }
        }
        panic!("No upsert found in source");
    }

    #[test]
    fn test_simple_upsert() {
        let source = r#"
UpsertProduct @upsert{
    params {id @uuid, name @string, price @decimal}
    into products
    on-conflict {
        target {id}
        update {name, price}
    }
    values {id $id, name $name, price $price}
    returning {id, name, price}
}
"#;
        let upsert = get_first_upsert(source);
        let result = generate_upsert_sql(&upsert);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_upsert_with_function_in_update() {
        let source = r#"
UpsertProduct @upsert{
    params {handle @string, name @string}
    into products
    on-conflict {
        target {handle}
        update {name, updated_at @now}
    }
    values {handle $handle, name $name, created_at @now}
    returning {id, handle, name, updated_at}
}
"#;
        let upsert = get_first_upsert(source);
        let result = generate_upsert_sql(&upsert);
        insta::assert_snapshot!(result.sql);
    }

    #[test]
    fn test_upsert_multiple_conflict_columns() {
        let source = r#"
UpsertTranslation @upsert{
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
        let upsert = get_first_upsert(source);
        let result = generate_upsert_sql(&upsert);
        insta::assert_snapshot!(result.sql);
    }
}
