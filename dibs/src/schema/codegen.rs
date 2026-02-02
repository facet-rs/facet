use super::{Schema, Table};
use crate::schema::{
    create_index_sql, create_table_sql, create_trigger_check_function_sql, create_trigger_check_sql,
};

/// Generate SQL to create all tables, foreign keys, and indices.
///
/// Returns a complete SQL script that can be executed to create the schema.
/// Tables are created first, then foreign keys (as ALTER TABLE), then indices.
pub fn schema_to_sql(schema: &Schema) -> String {
    let mut sql = String::new();

    // Create tables (without foreign keys to avoid dependency issues)
    for table in schema.tables.values() {
        sql.push_str(&create_table_sql(table));
        sql.push_str("\n\n");
    }

    // Add foreign keys
    for table in schema.tables.values() {
        for fk in &table.foreign_keys {
            let constraint_name = format!("fk_{}_{}", table.name, fk.columns.join("_"));
            let quoted_cols: Vec<_> = fk.columns.iter().map(|c| crate::quote_ident(c)).collect();
            let quoted_ref_cols: Vec<_> = fk
                .references_columns
                .iter()
                .map(|c| crate::quote_ident(c))
                .collect();
            sql.push_str(&format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({});\n",
                crate::quote_ident(&table.name),
                crate::quote_ident(&constraint_name),
                quoted_cols.join(", "),
                crate::quote_ident(&fk.references_table),
                quoted_ref_cols.join(", ")
            ));
        }
    }

    if schema.tables.values().any(|t| !t.foreign_keys.is_empty()) {
        sql.push('\n');
    }

    // Create indices
    for table in schema.tables.values() {
        for idx in &table.indices {
            sql.push_str(&create_index_sql(table, idx));
            sql.push('\n');
        }
    }

    // Create trigger checks
    for table in schema.tables.values() {
        for trig in &table.trigger_checks {
            sql.push_str(&create_trigger_check_function_sql(trig));
            sql.push('\n');
            sql.push_str(&create_trigger_check_sql(table, trig));
            sql.push('\n');
        }
    }

    sql.trim_end().to_string()
}
