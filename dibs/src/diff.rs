//! Schema diffing - compare Rust-defined schema against database schema.
//!
//! This module compares two [`Schema`] instances and produces a list of changes
//! needed to transform one into the other.

use crate::{Column, ForeignKey, Index, PgType, Schema, Table};
use std::collections::HashSet;

/// A diff between two schemas.
#[derive(Debug, Clone, Default)]
pub struct SchemaDiff {
    /// Changes organized by table.
    pub table_diffs: Vec<TableDiff>,
}

impl SchemaDiff {
    /// Returns true if there are no differences.
    pub fn is_empty(&self) -> bool {
        self.table_diffs.is_empty()
    }

    /// Count total number of changes.
    pub fn change_count(&self) -> usize {
        self.table_diffs.iter().map(|t| t.changes.len()).sum()
    }
}

/// Changes for a single table.
#[derive(Debug, Clone)]
pub struct TableDiff {
    /// Table name.
    pub table: String,
    /// List of changes.
    pub changes: Vec<Change>,
}

/// A single schema change.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Add a new table.
    AddTable(Table),
    /// Drop an existing table.
    DropTable(String),
    /// Add a new column.
    AddColumn(Column),
    /// Drop an existing column.
    DropColumn(String),
    /// Change a column's type.
    AlterColumnType {
        name: String,
        from: PgType,
        to: PgType,
    },
    /// Change a column's nullability.
    AlterColumnNullable { name: String, from: bool, to: bool },
    /// Change a column's default value.
    AlterColumnDefault {
        name: String,
        from: Option<String>,
        to: Option<String>,
    },
    /// Add a primary key.
    AddPrimaryKey(Vec<String>),
    /// Drop a primary key.
    DropPrimaryKey,
    /// Add a foreign key.
    AddForeignKey(ForeignKey),
    /// Drop a foreign key.
    DropForeignKey(ForeignKey),
    /// Add an index.
    AddIndex(Index),
    /// Drop an index.
    DropIndex(String),
    /// Add a unique constraint.
    AddUnique(String),
    /// Drop a unique constraint.
    DropUnique(String),
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Change::AddTable(t) => write!(f, "+ table {}", t.name),
            Change::DropTable(name) => write!(f, "- table {}", name),
            Change::AddColumn(col) => {
                let nullable = if col.nullable { " (nullable)" } else { "" };
                write!(f, "+ {}: {}{}", col.name, col.pg_type, nullable)
            }
            Change::DropColumn(name) => write!(f, "- {}", name),
            Change::AlterColumnType { name, from, to } => {
                write!(f, "~ {}: {} -> {}", name, from, to)
            }
            Change::AlterColumnNullable { name, from, to } => {
                let from_str = if *from { "nullable" } else { "not null" };
                let to_str = if *to { "nullable" } else { "not null" };
                write!(f, "~ {}: {} -> {}", name, from_str, to_str)
            }
            Change::AlterColumnDefault { name, from, to } => {
                let from_str = from.as_deref().unwrap_or("(none)");
                let to_str = to.as_deref().unwrap_or("(none)");
                write!(f, "~ {} default: {} -> {}", name, from_str, to_str)
            }
            Change::AddPrimaryKey(cols) => write!(f, "+ PRIMARY KEY ({})", cols.join(", ")),
            Change::DropPrimaryKey => write!(f, "- PRIMARY KEY"),
            Change::AddForeignKey(fk) => {
                write!(
                    f,
                    "+ FOREIGN KEY ({}) -> {}.{}",
                    fk.columns.join(", "),
                    fk.references_table,
                    fk.references_columns.join(", ")
                )
            }
            Change::DropForeignKey(fk) => {
                write!(
                    f,
                    "- FOREIGN KEY ({}) -> {}.{}",
                    fk.columns.join(", "),
                    fk.references_table,
                    fk.references_columns.join(", ")
                )
            }
            Change::AddIndex(idx) => {
                let unique = if idx.unique { "UNIQUE " } else { "" };
                write!(
                    f,
                    "+ {}INDEX {} ({})",
                    unique,
                    idx.name,
                    idx.columns.join(", ")
                )
            }
            Change::DropIndex(name) => write!(f, "- INDEX {}", name),
            Change::AddUnique(col) => write!(f, "+ UNIQUE ({})", col),
            Change::DropUnique(col) => write!(f, "- UNIQUE ({})", col),
        }
    }
}

impl Schema {
    /// Compare this schema (desired/Rust) against another schema (current/database).
    ///
    /// Returns the changes needed to transform `db_schema` into `self`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rust_schema = Schema::collect();
    /// let db_schema = Schema::from_database(&client).await?;
    /// let diff = rust_schema.diff(&db_schema);
    ///
    /// if diff.is_empty() {
    ///     println!("Schemas match!");
    /// } else {
    ///     for table_diff in &diff.table_diffs {
    ///         println!("{}:", table_diff.table);
    ///         for change in &table_diff.changes {
    ///             println!("  {}", change);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn diff(&self, db_schema: &Schema) -> SchemaDiff {
        let mut table_diffs = Vec::new();

        let desired_tables: HashSet<&str> = self.tables.iter().map(|t| t.name.as_str()).collect();
        let current_tables: HashSet<&str> =
            db_schema.tables.iter().map(|t| t.name.as_str()).collect();

        // Tables to add (in desired but not in current)
        for table in &self.tables {
            if !current_tables.contains(table.name.as_str()) {
                table_diffs.push(TableDiff {
                    table: table.name.clone(),
                    changes: vec![Change::AddTable(table.clone())],
                });
            }
        }

        // Tables to drop (in current but not in desired)
        for table in &db_schema.tables {
            if !desired_tables.contains(table.name.as_str()) {
                table_diffs.push(TableDiff {
                    table: table.name.clone(),
                    changes: vec![Change::DropTable(table.name.clone())],
                });
            }
        }

        // Tables in both - diff columns and constraints
        for desired_table in &self.tables {
            if let Some(current_table) = db_schema
                .tables
                .iter()
                .find(|t| t.name == desired_table.name)
            {
                let changes = diff_table(desired_table, current_table);
                if !changes.is_empty() {
                    table_diffs.push(TableDiff {
                        table: desired_table.name.clone(),
                        changes,
                    });
                }
            }
        }

        // Sort by table name for consistent output
        table_diffs.sort_by(|a, b| a.table.cmp(&b.table));

        SchemaDiff { table_diffs }
    }
}

/// Diff two tables with the same name.
fn diff_table(desired: &Table, current: &Table) -> Vec<Change> {
    let mut changes = Vec::new();

    // Diff columns
    changes.extend(diff_columns(&desired.columns, &current.columns));

    // Diff foreign keys
    changes.extend(diff_foreign_keys(
        &desired.foreign_keys,
        &current.foreign_keys,
    ));

    // Diff indices
    changes.extend(diff_indices(&desired.indices, &current.indices));

    changes
}

/// Diff columns between desired and current state.
fn diff_columns(desired: &[Column], current: &[Column]) -> Vec<Change> {
    let mut changes = Vec::new();

    let desired_names: HashSet<&str> = desired.iter().map(|c| c.name.as_str()).collect();
    let current_names: HashSet<&str> = current.iter().map(|c| c.name.as_str()).collect();

    // Columns to add
    for col in desired {
        if !current_names.contains(col.name.as_str()) {
            changes.push(Change::AddColumn(col.clone()));
        }
    }

    // Columns to drop
    for col in current {
        if !desired_names.contains(col.name.as_str()) {
            changes.push(Change::DropColumn(col.name.clone()));
        }
    }

    // Columns in both - check for changes
    for desired_col in desired {
        if let Some(current_col) = current.iter().find(|c| c.name == desired_col.name) {
            // Type change
            if desired_col.pg_type != current_col.pg_type {
                changes.push(Change::AlterColumnType {
                    name: desired_col.name.clone(),
                    from: current_col.pg_type,
                    to: desired_col.pg_type,
                });
            }

            // Nullability change
            if desired_col.nullable != current_col.nullable {
                changes.push(Change::AlterColumnNullable {
                    name: desired_col.name.clone(),
                    from: current_col.nullable,
                    to: desired_col.nullable,
                });
            }

            // Default change
            if desired_col.default != current_col.default {
                changes.push(Change::AlterColumnDefault {
                    name: desired_col.name.clone(),
                    from: current_col.default.clone(),
                    to: desired_col.default.clone(),
                });
            }

            // Unique change
            if desired_col.unique != current_col.unique {
                if desired_col.unique {
                    changes.push(Change::AddUnique(desired_col.name.clone()));
                } else {
                    changes.push(Change::DropUnique(desired_col.name.clone()));
                }
            }

            // Primary key changes are handled at table level (composite PKs)
        }
    }

    changes
}

/// Diff foreign keys.
fn diff_foreign_keys(desired: &[ForeignKey], current: &[ForeignKey]) -> Vec<Change> {
    let mut changes = Vec::new();

    // Use a simple key for comparison
    let fk_key = |fk: &ForeignKey| -> String {
        format!(
            "{}->{}({})",
            fk.columns.join(","),
            fk.references_table,
            fk.references_columns.join(",")
        )
    };

    let desired_keys: HashSet<String> = desired.iter().map(fk_key).collect();
    let current_keys: HashSet<String> = current.iter().map(fk_key).collect();

    // FKs to add
    for fk in desired {
        if !current_keys.contains(&fk_key(fk)) {
            changes.push(Change::AddForeignKey(fk.clone()));
        }
    }

    // FKs to drop
    for fk in current {
        if !desired_keys.contains(&fk_key(fk)) {
            changes.push(Change::DropForeignKey(fk.clone()));
        }
    }

    changes
}

/// Diff indices.
fn diff_indices(desired: &[Index], current: &[Index]) -> Vec<Change> {
    let mut changes = Vec::new();

    // Compare by columns (not name, since names may differ)
    let idx_key = |idx: &Index| -> String {
        let mut cols = idx.columns.clone();
        cols.sort();
        format!("{}:{}", if idx.unique { "U" } else { "" }, cols.join(","))
    };

    let desired_keys: HashSet<String> = desired.iter().map(idx_key).collect();
    let current_keys: HashSet<String> = current.iter().map(idx_key).collect();

    // Indices to add
    for idx in desired {
        if !current_keys.contains(&idx_key(idx)) {
            changes.push(Change::AddIndex(idx.clone()));
        }
    }

    // Indices to drop
    for idx in current {
        if !desired_keys.contains(&idx_key(idx)) {
            changes.push(Change::DropIndex(idx.name.clone()));
        }
    }

    changes
}

impl std::fmt::Display for SchemaDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            writeln!(f, "No changes detected.")?;
        } else {
            writeln!(f, "Changes detected:\n")?;
            for table_diff in &self.table_diffs {
                writeln!(f, "  {}:", table_diff.table)?;
                for change in &table_diff.changes {
                    writeln!(f, "    {}", change)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceLocation;

    fn make_column(name: &str, pg_type: PgType, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            pg_type,
            nullable,
            default: None,
            primary_key: false,
            unique: false,
        }
    }

    fn make_table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            name: name.to_string(),
            columns,
            foreign_keys: Vec::new(),
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
        }
    }

    #[test]
    fn test_diff_empty_schemas() {
        let a = Schema::new();
        let b = Schema::new();
        let diff = a.diff(&b);
        assert!(diff.is_empty());
    }

    #[test]
    fn test_diff_add_table() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };
        let current = Schema::new();

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::AddTable(_)
        ));
    }

    #[test]
    fn test_diff_drop_table() {
        let desired = Schema::new();
        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::DropTable(name) if name == "users"
        ));
    }

    #[test]
    fn test_diff_add_column() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                ],
            )],
        };
        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::AddColumn(col) if col.name == "email"
        ));
    }

    #[test]
    fn test_diff_drop_column() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };
        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                ],
            )],
        };

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::DropColumn(name) if name == "email"
        ));
    }

    #[test]
    fn test_diff_alter_column_type() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("age", PgType::BigInt, false)],
            )],
        };
        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("age", PgType::Integer, false)],
            )],
        };

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::AlterColumnType { name, from: PgType::Integer, to: PgType::BigInt } if name == "age"
        ));
    }

    #[test]
    fn test_diff_alter_column_nullable() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("bio", PgType::Text, true)],
            )],
        };
        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("bio", PgType::Text, false)],
            )],
        };

        let diff = desired.diff(&current);
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::AlterColumnNullable { name, from: false, to: true } if name == "bio"
        ));
    }

    #[test]
    fn test_diff_no_changes() {
        let schema = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                ],
            )],
        };

        let diff = schema.diff(&schema);
        assert!(diff.is_empty());
    }
}
