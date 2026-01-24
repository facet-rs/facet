//! Database introspection - read schema from a live Postgres database.
//!
//! This module queries `information_schema` and `pg_catalog` to build a [`Schema`]
//! from the current state of a database.

use crate::{Column, ForeignKey, Index, PgType, Result, Schema, SourceLocation, Table};
use tokio_postgres::Client;

impl Schema {
    /// Introspect a live Postgres database and build a Schema from it.
    ///
    /// This queries `information_schema` to discover tables, columns, constraints,
    /// and indices in the `public` schema.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let schema = Schema::from_database(&client).await?;
    /// for table in &schema.tables {
    ///     println!("Found table: {}", table.name);
    /// }
    /// ```
    pub async fn from_database(client: &Client) -> Result<Self> {
        let tables = introspect_tables(client).await?;
        Ok(Self { tables })
    }
}

/// Introspect all tables in the public schema.
async fn introspect_tables(client: &Client) -> Result<Vec<Table>> {
    // Get all base tables in public schema, excluding dibs meta tables
    let rows = client
        .query(
            r#"
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_type = 'BASE TABLE'
              AND table_name NOT LIKE '_dibs_%'
              AND table_name NOT LIKE '__dibs_%'
            ORDER BY table_name
            "#,
            &[],
        )
        .await?;

    let mut tables = Vec::new();
    for row in rows {
        let table_name: String = row.get(0);
        let table = introspect_table(client, &table_name).await?;
        tables.push(table);
    }

    Ok(tables)
}

/// Introspect a single table.
async fn introspect_table(client: &Client, table_name: &str) -> Result<Table> {
    let columns = introspect_columns(client, table_name).await?;
    let primary_keys = introspect_primary_keys(client, table_name).await?;
    let unique_columns = introspect_unique_constraints(client, table_name).await?;
    let foreign_keys = introspect_foreign_keys(client, table_name).await?;
    let indices = introspect_indices(client, table_name).await?;

    // Mark columns with PK and unique flags
    let columns: Vec<Column> = columns
        .into_iter()
        .map(|mut col| {
            col.primary_key = primary_keys.contains(&col.name);
            col.unique = unique_columns.contains(&col.name);
            col
        })
        .collect();

    Ok(Table {
        name: table_name.to_string(),
        columns,
        foreign_keys,
        indices,
        source: SourceLocation::default(), // DB tables don't have Rust source
        doc: None,
        icon: None, // Not available from introspection
    })
}

/// Introspect columns for a table.
async fn introspect_columns(client: &Client, table_name: &str) -> Result<Vec<Column>> {
    let rows = client
        .query(
            r#"
            SELECT
                column_name,
                data_type,
                udt_name,
                is_nullable,
                column_default
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = $1
            ORDER BY ordinal_position
            "#,
            &[&table_name],
        )
        .await?;

    let mut columns = Vec::new();
    for row in rows {
        let name: String = row.get(0);
        let data_type: String = row.get(1);
        let udt_name: String = row.get(2);
        let is_nullable: String = row.get(3);
        let column_default: Option<String> = row.get(4);

        let pg_type = pg_type_from_info_schema(&data_type, &udt_name);
        let nullable = is_nullable == "YES";

        // Clean up default value (remove type casts like ::text)
        let default = column_default.map(|d| clean_default_value(&d));

        // Detect auto-generated columns (serial, identity, uuid default, etc.)
        let auto_generated = is_auto_generated(&default);

        columns.push(Column {
            name,
            pg_type,
            rust_type: None, // Not available from introspection
            nullable,
            default,
            primary_key: false, // Set later
            unique: false,      // Set later
            auto_generated,
            long: false,           // Not available from introspection
            label: false,          // Not available from introspection
            enum_variants: vec![], // TODO: fetch from pg_enum if pg_type is USER-DEFINED
            doc: None,             // Not available from introspection
            lang: None,            // Not available from introspection
            icon: None,            // Not available from introspection
            subtype: None,         // Not available from introspection
        });
    }

    Ok(columns)
}

/// Introspect primary key columns for a table.
async fn introspect_primary_keys(client: &Client, table_name: &str) -> Result<Vec<String>> {
    let rows = client
        .query(
            r#"
            SELECT kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'PRIMARY KEY'
                AND tc.table_schema = 'public'
                AND tc.table_name = $1
            ORDER BY kcu.ordinal_position
            "#,
            &[&table_name],
        )
        .await?;

    Ok(rows.iter().map(|r| r.get(0)).collect())
}

/// Introspect unique constraint columns for a table.
async fn introspect_unique_constraints(client: &Client, table_name: &str) -> Result<Vec<String>> {
    let rows = client
        .query(
            r#"
            SELECT kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'UNIQUE'
                AND tc.table_schema = 'public'
                AND tc.table_name = $1
            "#,
            &[&table_name],
        )
        .await?;

    Ok(rows.iter().map(|r| r.get(0)).collect())
}

/// Introspect foreign keys for a table.
async fn introspect_foreign_keys(client: &Client, table_name: &str) -> Result<Vec<ForeignKey>> {
    let rows = client
        .query(
            r#"
            SELECT
                tc.constraint_name,
                kcu.column_name,
                ccu.table_name AS foreign_table,
                ccu.column_name AS foreign_column,
                kcu.ordinal_position
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
                ON tc.constraint_name = ccu.constraint_name
                AND tc.table_schema = ccu.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = 'public'
                AND tc.table_name = $1
            ORDER BY tc.constraint_name, kcu.ordinal_position
            "#,
            &[&table_name],
        )
        .await?;

    // Group by constraint name (handles composite FKs correctly)
    let mut fk_map: std::collections::HashMap<String, (ForeignKey, Vec<(i32, String, String)>)> =
        std::collections::HashMap::new();

    for row in rows {
        let constraint_name: String = row.get(0);
        let column: String = row.get(1);
        let foreign_table: String = row.get(2);
        let foreign_column: String = row.get(3);
        let ordinal: i32 = row.get(4);

        fk_map
            .entry(constraint_name)
            .or_insert_with(|| {
                (
                    ForeignKey {
                        columns: Vec::new(),
                        references_table: foreign_table,
                        references_columns: Vec::new(),
                    },
                    Vec::new(),
                )
            })
            .1
            .push((ordinal, column, foreign_column));
    }

    // Sort columns by ordinal position and build final FK
    Ok(fk_map
        .into_values()
        .map(|(mut fk, mut cols)| {
            cols.sort_by_key(|(ord, _, _)| *ord);
            for (_, col, ref_col) in cols {
                fk.columns.push(col);
                if !fk.references_columns.contains(&ref_col) {
                    fk.references_columns.push(ref_col);
                }
            }
            fk
        })
        .collect())
}

/// Introspect indices for a table.
async fn introspect_indices(client: &Client, table_name: &str) -> Result<Vec<Index>> {
    // Use pg_indexes view, but exclude primary key and unique constraint indices
    // (those are handled separately as constraints)
    let rows = client
        .query(
            r#"
            SELECT
                i.indexname,
                i.indexdef
            FROM pg_indexes i
            WHERE i.schemaname = 'public'
              AND i.tablename = $1
              AND NOT EXISTS (
                  SELECT 1 FROM information_schema.table_constraints tc
                  WHERE tc.constraint_name = i.indexname
                    AND tc.table_schema = 'public'
              )
            "#,
            &[&table_name],
        )
        .await?;

    let mut indices = Vec::new();
    for row in rows {
        let name: String = row.get(0);
        let indexdef: String = row.get(1);

        // Parse columns from indexdef
        // Example: "CREATE INDEX idx_users_name ON public.users USING btree (name)"
        // Example: "CREATE UNIQUE INDEX idx_users_email ON public.users USING btree (email)"
        let unique = indexdef.to_uppercase().contains("UNIQUE");
        let columns = parse_index_columns(&indexdef);

        indices.push(Index {
            name,
            columns,
            unique,
        });
    }

    Ok(indices)
}

/// Parse column names from an index definition.
fn parse_index_columns(indexdef: &str) -> Vec<String> {
    // Find the part between the last ( and )
    if let Some(start) = indexdef.rfind('(')
        && let Some(end) = indexdef.rfind(')')
    {
        let cols_str = &indexdef[start + 1..end];
        return cols_str.split(',').map(|s| s.trim().to_string()).collect();
    }
    Vec::new()
}

/// Map Postgres information_schema types to our PgType enum.
fn pg_type_from_info_schema(data_type: &str, udt_name: &str) -> PgType {
    // data_type is the SQL standard name, udt_name is the Postgres internal name
    match data_type.to_uppercase().as_str() {
        "SMALLINT" => PgType::SmallInt,
        "INTEGER" => PgType::Integer,
        "BIGINT" => PgType::BigInt,
        "REAL" => PgType::Real,
        "DOUBLE PRECISION" => PgType::DoublePrecision,
        "NUMERIC" | "DECIMAL" => PgType::Numeric,
        "BOOLEAN" => PgType::Boolean,
        "TEXT" => PgType::Text,
        "BYTEA" => PgType::Bytea,
        "DATE" => PgType::Date,
        "TIME WITHOUT TIME ZONE" | "TIME" => PgType::Time,
        "TIMESTAMP WITH TIME ZONE" | "TIMESTAMP WITHOUT TIME ZONE" | "TIMESTAMP" => {
            PgType::Timestamptz
        }
        "UUID" => PgType::Uuid,
        "JSONB" => PgType::Jsonb,
        "USER-DEFINED" => {
            // Check udt_name for custom types
            match udt_name {
                "uuid" => PgType::Uuid,
                "jsonb" => PgType::Jsonb,
                _ => PgType::Text, // Fallback
            }
        }
        "CHARACTER VARYING" | "VARCHAR" | "CHAR" | "CHARACTER" => PgType::Text,
        "ARRAY" => {
            // udt_name for arrays is the element type prefixed with underscore
            match udt_name {
                "_text" | "_varchar" => PgType::TextArray,
                "_int8" => PgType::BigIntArray,
                "_int4" => PgType::IntegerArray,
                _ => PgType::Jsonb, // Fallback for unsupported array types
            }
        }
        _ => {
            // Fallback - check udt_name
            match udt_name {
                "int2" => PgType::SmallInt,
                "int4" => PgType::Integer,
                "int8" => PgType::BigInt,
                "float4" => PgType::Real,
                "float8" => PgType::DoublePrecision,
                "numeric" => PgType::Numeric,
                "bool" => PgType::Boolean,
                "text" | "varchar" | "bpchar" => PgType::Text,
                "bytea" => PgType::Bytea,
                "timestamptz" | "timestamp" => PgType::Timestamptz,
                "date" => PgType::Date,
                "time" => PgType::Time,
                "uuid" => PgType::Uuid,
                "jsonb" => PgType::Jsonb,
                _ => PgType::Text, // Ultimate fallback
            }
        }
    }
}

/// Clean up a default value from information_schema.
///
/// Postgres stores defaults with type casts like `'foo'::text` or `0::bigint`.
/// We want to normalize these for comparison.
fn clean_default_value(default: &str) -> String {
    let s = default.trim();

    // Remove type casts like ::text, ::bigint, etc.
    if let Some(idx) = s.find("::") {
        return s[..idx].to_string();
    }

    s.to_string()
}

/// Check if a default value indicates an auto-generated column.
///
/// Detects:
/// - Serial/BigSerial/SmallSerial: `nextval('table_column_seq'::regclass)`
/// - UUID generation: `gen_random_uuid()`, `uuid_generate_v4()`
/// - Timestamps: `now()`, `CURRENT_TIMESTAMP`
fn is_auto_generated(default: &Option<String>) -> bool {
    let Some(def) = default else {
        return false;
    };

    let lower = def.to_lowercase();

    // Serial/identity columns use nextval
    if lower.contains("nextval(") {
        return true;
    }

    // UUID generation functions
    if lower.contains("gen_random_uuid()") || lower.contains("uuid_generate_v") {
        return true;
    }

    // Timestamp defaults
    if lower.contains("now()") || lower.contains("current_timestamp") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index_columns() {
        assert_eq!(
            parse_index_columns("CREATE INDEX idx_users_name ON public.users USING btree (name)"),
            vec!["name"]
        );
        assert_eq!(
            parse_index_columns(
                "CREATE UNIQUE INDEX idx_users_email ON public.users USING btree (email)"
            ),
            vec!["email"]
        );
        assert_eq!(
            parse_index_columns(
                "CREATE INDEX idx_posts_author ON public.posts USING btree (author_id, created_at)"
            ),
            vec!["author_id", "created_at"]
        );
    }

    #[test]
    fn test_clean_default_value() {
        assert_eq!(clean_default_value("'foo'::text"), "'foo'");
        assert_eq!(clean_default_value("0::bigint"), "0");
        assert_eq!(clean_default_value("now()"), "now()");
        assert_eq!(clean_default_value("  42  "), "42");
    }

    #[test]
    fn test_pg_type_from_info_schema() {
        assert_eq!(pg_type_from_info_schema("BIGINT", "int8"), PgType::BigInt);
        assert_eq!(pg_type_from_info_schema("TEXT", "text"), PgType::Text);
        assert_eq!(pg_type_from_info_schema("BOOLEAN", "bool"), PgType::Boolean);
        assert_eq!(
            pg_type_from_info_schema("USER-DEFINED", "uuid"),
            PgType::Uuid
        );
        assert_eq!(
            pg_type_from_info_schema("CHARACTER VARYING", "varchar"),
            PgType::Text
        );
    }
}
