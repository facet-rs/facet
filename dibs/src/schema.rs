//! Schema definition and introspection.
//!
//! ## Naming Convention
//!
//! **Table names use singular form** (e.g., `user`, `post`, `comment`).
//!
//! This convention treats each table as a definition of what a single record
//! represents, rather than a container of multiple records.
//!
//! ## Example
//!
//! ```ignore
//! use dibs::prelude::*;
//! use facet::Facet;
//!
//! #[derive(Facet)]
//! #[facet(dibs::table = "user")]
//! pub struct User {
//!     #[facet(dibs::pk)]
//!     pub id: i64,
//!
//!     #[facet(dibs::unique)]
//!     pub email: String,
//!
//!     pub name: String,
//! }
//! ```

pub mod codegen;

pub use dibs_db_schema::{
    CheckConstraint, Column, ForeignKey, Index, IndexColumn, NullsOrder, PgType, Schema, SortOrder,
    SourceLocation, Table, TableDef, TriggerCheckConstraint,
};

/// Extension trait for Schema to add SQL generation.
pub trait SchemaCodegen {
    /// Generate SQL to create all tables, foreign keys, and indices.
    fn to_sql(&self) -> String;
}

impl SchemaCodegen for Schema {
    fn to_sql(&self) -> String {
        codegen::schema_to_sql(self)
    }
}

/// Generate CREATE TABLE SQL statement.
///
/// Does not include foreign key constraints (those should be added
/// separately to handle table creation order).
pub fn create_table_sql(table: &Table) -> String {
    let mut sql = format!("CREATE TABLE {} (\n", crate::quote_ident(&table.name));

    // Collect primary key columns
    let pk_columns: Vec<&str> = table
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.as_str())
        .collect();

    // If there's more than one PK column, we need a table constraint
    let use_table_pk_constraint = pk_columns.len() > 1;

    let mut parts: Vec<String> = table
        .columns
        .iter()
        .map(|col| {
            let mut def = format!("    {} {}", crate::quote_ident(&col.name), col.pg_type);

            // Only add inline PRIMARY KEY for single-column PKs
            if col.primary_key && !use_table_pk_constraint {
                def.push_str(" PRIMARY KEY");
            }

            // NOT NULL: PK columns are implicitly NOT NULL, but for composite PKs
            // we need to add it explicitly since we're not using inline PRIMARY KEY
            if !col.nullable && (!col.primary_key || use_table_pk_constraint) {
                def.push_str(" NOT NULL");
            }

            if col.unique && !col.primary_key {
                def.push_str(" UNIQUE");
            }

            if let Some(default) = &col.default {
                def.push_str(&format!(" DEFAULT {}", default));
            }

            def
        })
        .collect();

    // Add composite primary key constraint if needed
    if use_table_pk_constraint {
        let quoted_pk_cols: Vec<_> = pk_columns.iter().map(|c| crate::quote_ident(c)).collect();
        parts.push(format!("    PRIMARY KEY ({})", quoted_pk_cols.join(", ")));
    }

    // Add CHECK constraints
    for check in &table.check_constraints {
        parts.push(format!(
            "    CONSTRAINT {} CHECK ({})",
            crate::quote_ident(&check.name),
            check.expr
        ));
    }

    sql.push_str(&parts.join(",\n"));
    sql.push_str("\n);");

    sql
}

/// Generate CREATE INDEX SQL statement for a given index.
pub fn create_index_sql(table: &Table, idx: &Index) -> String {
    let unique = if idx.unique { "UNIQUE " } else { "" };
    let quoted_cols: Vec<_> = idx.columns.iter().map(index_column_to_sql).collect();
    let where_clause = idx
        .where_clause
        .as_ref()
        .map(|w| format!(" WHERE {}", w))
        .unwrap_or_default();
    format!(
        "CREATE {}INDEX {} ON {} ({}){};",
        unique,
        crate::quote_ident(&idx.name),
        crate::quote_ident(&table.name),
        quoted_cols.join(", "),
        where_clause
    )
}

/// Generate CREATE FUNCTION SQL for a trigger check.
pub fn create_trigger_check_function_sql(trig: &TriggerCheckConstraint) -> String {
    let fn_name = crate::trigger_check_function_name(&trig.name);
    let message = trig
        .message
        .as_deref()
        .unwrap_or("trigger check failed")
        .replace('\'', "''");
    format!(
        "CREATE OR REPLACE FUNCTION {}() RETURNS trigger LANGUAGE plpgsql AS $$\n\
         BEGIN\n\
             IF NOT ({}) THEN\n\
                 RAISE EXCEPTION '{}' USING ERRCODE = '23514';\n\
             END IF;\n\
             RETURN NEW;\n\
         END;\n\
         $$;",
        crate::quote_ident(&fn_name),
        trig.expr,
        message
    )
}

/// Generate CREATE TRIGGER SQL for a trigger check.
pub fn create_trigger_check_sql(table: &Table, trig: &TriggerCheckConstraint) -> String {
    let fn_name = crate::trigger_check_function_name(&trig.name);
    format!(
        "CREATE TRIGGER {} BEFORE INSERT OR UPDATE ON {} FOR EACH ROW EXECUTE FUNCTION {}();",
        crate::quote_ident(&trig.name),
        crate::quote_ident(&table.name),
        crate::quote_ident(&fn_name)
    )
}

/// Returns the SQL fragment for an index column (name + order + nulls).
pub fn index_column_to_sql(col: &IndexColumn) -> String {
    format!(
        "{}{}{}",
        crate::quote_ident(&col.name),
        col.order.to_sql(),
        col.nulls.to_sql()
    )
}

/// Collect schema from all registered table types.
///
/// This uses facet reflection to inspect types marked with `#[facet(dibs::table)]`.
pub fn collect_schema() -> Schema {
    let tables = inventory::iter::<TableDef>
        .into_iter()
        .filter_map(|def| def.to_table())
        .map(|t| (t.name.clone(), t))
        .collect();

    Schema { tables }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_column_to_sql() {
        // Simple column
        let col = IndexColumn::new("name");
        assert_eq!(index_column_to_sql(&col), "\"name\"");

        // DESC
        let col = IndexColumn::desc("created_at");
        assert_eq!(index_column_to_sql(&col), "\"created_at\" DESC");

        // NULLS FIRST
        let col = IndexColumn::nulls_first("reminder_sent_at");
        assert_eq!(
            index_column_to_sql(&col),
            "\"reminder_sent_at\" NULLS FIRST"
        );

        // DESC NULLS LAST
        let col = IndexColumn {
            name: "priority".to_string(),
            order: SortOrder::Desc,
            nulls: NullsOrder::Last,
        };
        assert_eq!(index_column_to_sql(&col), "\"priority\" DESC NULLS LAST");
    }
}
