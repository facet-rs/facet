//! Meta tables for schema provenance tracking.
//!
//! dibs maintains `__dibs_*` tables in the database to track:
//! - Source locations of schema elements (file, line, column)
//! - Doc comments from Rust code
//! - Migration history (which migration created/modified each element)

use crate::Schema;

/// SQL to create the __dibs_migrations table.
pub const CREATE_MIGRATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS __dibs_migrations (
    name TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    checksum TEXT,
    execution_time_ms INTEGER
);
"#;

/// SQL to create the __dibs_tables table.
pub const CREATE_TABLES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS __dibs_tables (
    table_name TEXT PRIMARY KEY,
    source_file TEXT,
    source_line INTEGER,
    source_column INTEGER,
    doc_comment TEXT,
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#;

/// SQL to create the __dibs_columns table.
pub const CREATE_COLUMNS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS __dibs_columns (
    table_name TEXT NOT NULL,
    column_name TEXT NOT NULL,
    source_file TEXT,
    source_line INTEGER,
    source_column INTEGER,
    doc_comment TEXT,
    rust_type TEXT,
    sql_type TEXT,
    is_nullable BOOLEAN NOT NULL DEFAULT false,
    default_value TEXT,
    is_primary_key BOOLEAN NOT NULL DEFAULT false,
    is_unique BOOLEAN NOT NULL DEFAULT false,
    is_indexed BOOLEAN NOT NULL DEFAULT false,
    fk_references_table TEXT,
    fk_references_column TEXT,
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (table_name, column_name)
);
"#;

/// SQL to create the __dibs_indices table.
pub const CREATE_INDICES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS __dibs_indices (
    table_name TEXT NOT NULL,
    index_name TEXT NOT NULL,
    source_file TEXT,
    source_line INTEGER,
    source_column INTEGER,
    columns TEXT[] NOT NULL,
    is_unique BOOLEAN NOT NULL DEFAULT false,
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (table_name, index_name)
);
"#;

/// Generate SQL to create all meta tables.
pub fn create_meta_tables_sql() -> String {
    format!(
        "{}\n{}\n{}\n{}",
        CREATE_MIGRATIONS_TABLE.trim(),
        CREATE_TABLES_TABLE.trim(),
        CREATE_COLUMNS_TABLE.trim(),
        CREATE_INDICES_TABLE.trim()
    )
}

/// Generate SQL to upsert table metadata from the current schema.
pub fn sync_tables_sql(schema: &Schema, migration_name: Option<&str>) -> String {
    let mut sql = String::new();

    for table in &schema.tables {
        let source_file = table
            .source
            .file
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());
        let source_line = table
            .source
            .line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "NULL".to_string());
        let source_column = table
            .source
            .column
            .map(|n| n.to_string())
            .unwrap_or_else(|| "NULL".to_string());
        let doc_comment = table
            .doc
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());
        let migration = migration_name
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        sql.push_str(&format!(
            r#"
INSERT INTO __dibs_tables (table_name, source_file, source_line, source_column, doc_comment, created_by_migration, modified_by_migration)
VALUES ('{}', {}, {}, {}, {}, {}, {})
ON CONFLICT (table_name) DO UPDATE SET
    source_file = EXCLUDED.source_file,
    source_line = EXCLUDED.source_line,
    source_column = EXCLUDED.source_column,
    doc_comment = EXCLUDED.doc_comment,
    modified_by_migration = EXCLUDED.modified_by_migration,
    modified_at = now();
"#,
            table.name.replace('\'', "''"),
            source_file,
            source_line,
            source_column,
            doc_comment,
            migration,
            migration
        ));

        // Sync columns
        for col in &table.columns {
            let col_doc = "NULL"; // TODO: field-level doc comments when facet supports them
            let fk_table = "NULL";
            let fk_col = "NULL";

            // Check if this column has a foreign key
            let fk = table
                .foreign_keys
                .iter()
                .find(|fk| fk.columns.len() == 1 && fk.columns[0] == col.name);

            let (fk_table_val, fk_col_val) = if let Some(fk) = fk {
                (
                    format!("'{}'", fk.references_table.replace('\'', "''")),
                    format!(
                        "'{}'",
                        fk.references_columns
                            .first()
                            .map(|s| s.replace('\'', "''"))
                            .unwrap_or_default()
                    ),
                )
            } else {
                (fk_table.to_string(), fk_col.to_string())
            };

            // Check if column is indexed
            let is_indexed = table
                .indices
                .iter()
                .any(|idx| idx.columns.len() == 1 && idx.columns[0] == col.name);

            sql.push_str(&format!(
                r#"
INSERT INTO __dibs_columns (table_name, column_name, source_file, source_line, source_column, doc_comment, sql_type, is_nullable, default_value, is_primary_key, is_unique, is_indexed, fk_references_table, fk_references_column, created_by_migration, modified_by_migration)
VALUES ('{}', '{}', {}, {}, {}, {}, '{}', {}, {}, {}, {}, {}, {}, {}, {}, {})
ON CONFLICT (table_name, column_name) DO UPDATE SET
    source_file = EXCLUDED.source_file,
    source_line = EXCLUDED.source_line,
    source_column = EXCLUDED.source_column,
    doc_comment = EXCLUDED.doc_comment,
    sql_type = EXCLUDED.sql_type,
    is_nullable = EXCLUDED.is_nullable,
    default_value = EXCLUDED.default_value,
    is_primary_key = EXCLUDED.is_primary_key,
    is_unique = EXCLUDED.is_unique,
    is_indexed = EXCLUDED.is_indexed,
    fk_references_table = EXCLUDED.fk_references_table,
    fk_references_column = EXCLUDED.fk_references_column,
    modified_by_migration = EXCLUDED.modified_by_migration,
    modified_at = now();
"#,
                table.name.replace('\'', "''"),
                col.name.replace('\'', "''"),
                source_file, // Use table's source for now
                source_line,
                source_column,
                col_doc,
                col.pg_type,
                col.nullable,
                col.default
                    .as_ref()
                    .map(|s| format!("'{}'", s.replace('\'', "''")))
                    .unwrap_or_else(|| "NULL".to_string()),
                col.primary_key,
                col.unique,
                is_indexed,
                fk_table_val,
                fk_col_val,
                migration,
                migration
            ));
        }

        // Sync indices
        for idx in &table.indices {
            let columns_array = format!(
                "ARRAY[{}]",
                idx.columns
                    .iter()
                    .map(|c| format!("'{}'", c.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            sql.push_str(&format!(
                r#"
INSERT INTO __dibs_indices (table_name, index_name, source_file, source_line, source_column, columns, is_unique, created_by_migration, modified_by_migration)
VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {})
ON CONFLICT (table_name, index_name) DO UPDATE SET
    source_file = EXCLUDED.source_file,
    source_line = EXCLUDED.source_line,
    source_column = EXCLUDED.source_column,
    columns = EXCLUDED.columns,
    is_unique = EXCLUDED.is_unique,
    modified_by_migration = EXCLUDED.modified_by_migration,
    modified_at = now();
"#,
                table.name.replace('\'', "''"),
                idx.name.replace('\'', "''"),
                source_file,
                source_line,
                source_column,
                columns_array,
                idx.unique,
                migration,
                migration
            ));
        }
    }

    sql
}

/// Generate SQL to record a migration.
pub fn record_migration_sql(
    name: &str,
    checksum: Option<&str>,
    execution_time_ms: Option<i64>,
) -> String {
    let checksum_val = checksum
        .map(|s| format!("'{}'", s.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());
    let time_val = execution_time_ms
        .map(|t| t.to_string())
        .unwrap_or_else(|| "NULL".to_string());

    format!(
        r#"
INSERT INTO __dibs_migrations (name, checksum, execution_time_ms)
VALUES ('{}', {}, {})
ON CONFLICT (name) DO NOTHING;
"#,
        name.replace('\'', "''"),
        checksum_val,
        time_val
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_meta_tables_sql() {
        let sql = create_meta_tables_sql();
        assert!(sql.contains("__dibs_migrations"));
        assert!(sql.contains("__dibs_tables"));
        assert!(sql.contains("__dibs_columns"));
        assert!(sql.contains("__dibs_indices"));
    }
}
