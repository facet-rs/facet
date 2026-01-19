//! Schema diffing - compare Rust-defined schema against database schema.
//!
//! This module compares two [`Schema`] instances and produces a list of changes
//! needed to transform one into the other.
//!
//! ## Rename Detection
//!
//! The diff algorithm automatically detects likely table renames instead of
//! generating separate drop + add operations. This is particularly useful when
//! migrating from plural to singular table names (e.g., `users` → `user`).
//!
//! Detection is based on a similarity score combining:
//! - **Name similarity (30%)**: Recognizes plural/singular patterns like
//!   `users`→`user`, `categories`→`category`, `post_tags`→`post_tag`
//! - **Column overlap (70%)**: Uses Jaccard similarity to compare column sets
//!
//! Tables with similarity ≥ 0.6 are considered rename candidates. The algorithm
//! greedily assigns the best matches (highest similarity first) to avoid
//! ambiguous many-to-many mappings.
//!
//! ### Example
//!
//! ```text
//! // Instead of:
//! categories:
//!   - table categories
//! category:
//!   + table category
//!
//! // You'll see:
//! category:
//!   ~ rename categories -> category
//! ```
//!
//! The generated SQL uses `ALTER TABLE ... RENAME TO`:
//!
//! ```sql
//! ALTER TABLE categories RENAME TO category;
//! ```

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

    /// Generate SQL statements for all changes in this diff.
    pub fn to_sql(&self) -> String {
        let mut sql = String::new();
        for table_diff in &self.table_diffs {
            sql.push_str(&format!("-- Table: {}\n", table_diff.table));
            for change in &table_diff.changes {
                sql.push_str(&change.to_sql(&table_diff.table));
                sql.push('\n');
            }
            sql.push('\n');
        }
        sql
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
    /// Rename a table.
    RenameTable { from: String, to: String },
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

impl Change {
    /// Generate SQL statement for this change.
    ///
    /// The `table_name` is required for column-level changes.
    pub fn to_sql(&self, table_name: &str) -> String {
        match self {
            Change::AddTable(t) => t.to_create_table_sql(),
            Change::DropTable(name) => format!("DROP TABLE {};", name),
            Change::RenameTable { from, to } => format!("ALTER TABLE {} RENAME TO {};", from, to),
            Change::AddColumn(col) => {
                let not_null = if col.nullable { "" } else { " NOT NULL" };
                let default = col
                    .default
                    .as_ref()
                    .map(|d| format!(" DEFAULT {}", d))
                    .unwrap_or_default();
                format!(
                    "ALTER TABLE {} ADD COLUMN {} {}{}{};",
                    table_name, col.name, col.pg_type, not_null, default
                )
            }
            Change::DropColumn(name) => {
                format!("ALTER TABLE {} DROP COLUMN {};", table_name, name)
            }
            Change::AlterColumnType { name, to, .. } => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::{};",
                    table_name, name, to, name, to
                )
            }
            Change::AlterColumnNullable { name, to, .. } => {
                if *to {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL;",
                        table_name, name
                    )
                } else {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                        table_name, name
                    )
                }
            }
            Change::AlterColumnDefault { name, to, .. } => {
                if let Some(default) = to {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {};",
                        table_name, name, default
                    )
                } else {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT;",
                        table_name, name
                    )
                }
            }
            Change::AddPrimaryKey(cols) => {
                format!(
                    "ALTER TABLE {} ADD PRIMARY KEY ({});",
                    table_name,
                    cols.join(", ")
                )
            }
            Change::DropPrimaryKey => {
                format!("ALTER TABLE {} DROP CONSTRAINT {}_pkey;", table_name, table_name)
            }
            Change::AddForeignKey(fk) => {
                let constraint_name = format!(
                    "{}_{}_fkey",
                    table_name,
                    fk.columns.join("_")
                );
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});",
                    table_name,
                    constraint_name,
                    fk.columns.join(", "),
                    fk.references_table,
                    fk.references_columns.join(", ")
                )
            }
            Change::DropForeignKey(fk) => {
                let constraint_name = format!(
                    "{}_{}_fkey",
                    table_name,
                    fk.columns.join("_")
                );
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    table_name, constraint_name
                )
            }
            Change::AddIndex(idx) => {
                let unique = if idx.unique { "UNIQUE " } else { "" };
                format!(
                    "CREATE {}INDEX {} ON {} ({});",
                    unique,
                    idx.name,
                    table_name,
                    idx.columns.join(", ")
                )
            }
            Change::DropIndex(name) => {
                format!("DROP INDEX {};", name)
            }
            Change::AddUnique(col) => {
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {}_{}_key UNIQUE ({});",
                    table_name, table_name, col, col
                )
            }
            Change::DropUnique(col) => {
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}_{}_key;",
                    table_name, table_name, col
                )
            }
        }
    }
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Change::AddTable(t) => write!(f, "+ table {}", t.name),
            Change::DropTable(name) => write!(f, "- table {}", name),
            Change::RenameTable { from, to } => write!(f, "~ rename {} -> {}", from, to),
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

/// Check if two names are likely plural/singular variants of each other.
///
/// Recognizes common English plural patterns:
/// - Basic 's' suffix: `users` ↔ `user`, `posts` ↔ `post`
/// - 'ies' → 'y': `categories` ↔ `category`, `entries` ↔ `entry`
/// - Compound names: `post_tags` ↔ `post_tag`, `user_follows` ↔ `user_follow`
/// - Compound with 'ies': `post_categories` ↔ `post_category`
///
/// Note: This is intentionally simple and covers the most common cases.
/// Irregular plurals (e.g., `people`/`person`) are not detected.
fn is_plural_singular_pair(a: &str, b: &str) -> bool {
    // Ensure a is the longer one (likely plural)
    let (plural, singular) = if a.len() > b.len() { (a, b) } else { (b, a) };

    // Common plural patterns
    // "users" -> "user" (remove trailing 's')
    if plural == format!("{}s", singular) {
        return true;
    }

    // "categories" -> "category" (ies -> y)
    if plural.ends_with("ies") && singular.ends_with('y') {
        let plural_stem = &plural[..plural.len() - 3];
        let singular_stem = &singular[..singular.len() - 1];
        if plural_stem == singular_stem {
            return true;
        }
    }

    // "post_tags" -> "post_tag", "user_follows" -> "user_follow"
    // Check if the last segment differs by 's'
    if let (Some(plural_last), Some(singular_last)) =
        (plural.rsplit('_').next(), singular.rsplit('_').next())
    {
        if plural_last == format!("{}s", singular_last) {
            let plural_prefix = &plural[..plural.len() - plural_last.len()];
            let singular_prefix = &singular[..singular.len() - singular_last.len()];
            if plural_prefix == singular_prefix {
                return true;
            }
        }
        // "post_likes" -> "post_like" already covered above
        // "categories" case for compound: "post_categories" -> "post_category"
        if plural_last.ends_with("ies") && singular_last.ends_with('y') {
            let plural_stem = &plural_last[..plural_last.len() - 3];
            let singular_stem = &singular_last[..singular_last.len() - 1];
            if plural_stem == singular_stem {
                let plural_prefix = &plural[..plural.len() - plural_last.len()];
                let singular_prefix = &singular[..singular.len() - singular_last.len()];
                if plural_prefix == singular_prefix {
                    return true;
                }
            }
        }
    }

    false
}

/// Calculate similarity score between two tables (0.0 to 1.0).
///
/// The score combines two factors:
///
/// - **Name similarity (30% weight)**: Adds 0.3 if the table names are
///   plural/singular variants of each other (see [`is_plural_singular_pair`]).
///
/// - **Column overlap (70% weight)**: Uses Jaccard similarity (intersection/union)
///   on the column name sets. Identical column sets score 0.7, no overlap scores 0.
///
/// A score of 1.0 means identical columns + matching plural/singular names.
/// A score of 0.6 or higher typically indicates a likely rename.
fn table_similarity(a: &Table, b: &Table) -> f64 {
    let mut score = 0.0;

    // Name similarity (0.3 weight)
    if is_plural_singular_pair(&a.name, &b.name) {
        score += 0.3;
    }

    // Column overlap (0.7 weight)
    let a_cols: HashSet<&str> = a.columns.iter().map(|c| c.name.as_str()).collect();
    let b_cols: HashSet<&str> = b.columns.iter().map(|c| c.name.as_str()).collect();

    let intersection = a_cols.intersection(&b_cols).count();
    let union = a_cols.union(&b_cols).count();

    if union > 0 {
        let jaccard = intersection as f64 / union as f64;
        score += 0.7 * jaccard;
    }

    score
}

/// Detect likely table renames from lists of added and dropped tables.
///
/// Given tables that appear only in the desired schema (added) and tables that
/// appear only in the current schema (dropped), this function identifies pairs
/// that are likely renames rather than independent add/drop operations.
///
/// ## Algorithm
///
/// 1. Calculate similarity scores for all (dropped, added) table pairs
/// 2. Filter pairs with similarity ≥ `RENAME_THRESHOLD` (0.6)
/// 3. Sort by similarity descending (best matches first)
/// 4. Greedily assign matches, ensuring each table is used at most once
///
/// ## Returns
///
/// A list of `(old_name, new_name)` pairs representing detected renames.
/// Tables not involved in a rename will be handled as regular add/drop operations.
fn detect_renames(added: &[&Table], dropped: &[&Table]) -> Vec<(String, String)> {
    const RENAME_THRESHOLD: f64 = 0.6;

    let mut renames = Vec::new();
    let mut used_added: HashSet<&str> = HashSet::new();
    let mut used_dropped: HashSet<&str> = HashSet::new();

    // Find best matches
    let mut candidates: Vec<(f64, &str, &str)> = Vec::new();

    for dropped_table in dropped {
        for added_table in added {
            let sim = table_similarity(dropped_table, added_table);
            if sim >= RENAME_THRESHOLD {
                candidates.push((sim, &dropped_table.name, &added_table.name));
            }
        }
    }

    // Sort by similarity descending
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Greedily assign renames
    for (_, from, to) in candidates {
        if !used_dropped.contains(from) && !used_added.contains(to) {
            renames.push((from.to_string(), to.to_string()));
            used_dropped.insert(from);
            used_added.insert(to);
        }
    }

    renames
}

impl Schema {
    /// Compare this schema (desired/Rust) against another schema (current/database).
    ///
    /// Returns the changes needed to transform `db_schema` into `self`.
    ///
    /// ## Rename Detection
    ///
    /// This method automatically detects likely table renames based on column
    /// similarity and plural/singular name patterns. Instead of generating
    /// separate `DropTable` + `AddTable` changes, it produces a single
    /// `RenameTable` change with the appropriate `ALTER TABLE ... RENAME TO` SQL.
    ///
    /// See the module-level documentation for details on how rename detection works.
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

        // Find tables only in desired (candidates for add or rename target)
        let added_tables: Vec<&Table> = self
            .tables
            .iter()
            .filter(|t| !current_tables.contains(t.name.as_str()))
            .collect();

        // Find tables only in current (candidates for drop or rename source)
        let dropped_tables: Vec<&Table> = db_schema
            .tables
            .iter()
            .filter(|t| !desired_tables.contains(t.name.as_str()))
            .collect();

        // Detect likely renames
        let renames = detect_renames(&added_tables, &dropped_tables);
        let renamed_from: HashSet<&str> = renames.iter().map(|(from, _)| from.as_str()).collect();
        let renamed_to: HashSet<&str> = renames.iter().map(|(_, to)| to.as_str()).collect();

        // Generate rename changes
        for (from, to) in &renames {
            table_diffs.push(TableDiff {
                table: to.clone(),
                changes: vec![Change::RenameTable {
                    from: from.clone(),
                    to: to.clone(),
                }],
            });

            // Also diff the columns between old and new
            if let (Some(old_table), Some(new_table)) = (
                db_schema.tables.iter().find(|t| &t.name == from),
                self.tables.iter().find(|t| &t.name == to),
            ) {
                let column_changes = diff_table(new_table, old_table);
                if !column_changes.is_empty() {
                    // Add column changes to the same table diff
                    if let Some(td) = table_diffs.iter_mut().find(|td| &td.table == to) {
                        td.changes.extend(column_changes);
                    }
                }
            }
        }

        // Tables to add (not involved in a rename)
        for table in &added_tables {
            if !renamed_to.contains(table.name.as_str()) {
                table_diffs.push(TableDiff {
                    table: table.name.clone(),
                    changes: vec![Change::AddTable((*table).clone())],
                });
            }
        }

        // Tables to drop (not involved in a rename)
        for table in &dropped_tables {
            if !renamed_from.contains(table.name.as_str()) {
                table_diffs.push(TableDiff {
                    table: table.name.clone(),
                    changes: vec![Change::DropTable(table.name.clone())],
                });
            }
        }

        // Tables in both (not renamed) - diff columns and constraints
        for desired_table in &self.tables {
            if renamed_to.contains(desired_table.name.as_str()) {
                continue; // Already handled above
            }
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
            rust_type: None,
            nullable,
            default: None,
            primary_key: false,
            unique: false,
            auto_generated: false,
            long: false,
            label: false,
            enum_variants: vec![],
            doc: None,
            icon: None,
            lang: None,
            subtype: None,
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
            icon: None,
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

    // ===== Snapshot tests for SQL generation =====

    fn make_pk_column(name: &str, pg_type: PgType) -> Column {
        Column {
            name: name.to_string(),
            pg_type,
            rust_type: None,
            nullable: false,
            default: None,
            primary_key: true,
            unique: false,
            auto_generated: false,
            long: false,
            label: false,
            enum_variants: vec![],
            doc: None,
            icon: None,
            lang: None,
            subtype: None,
        }
    }

    fn make_column_with_default(name: &str, pg_type: PgType, nullable: bool, default: &str) -> Column {
        Column {
            name: name.to_string(),
            pg_type,
            rust_type: None,
            nullable,
            default: Some(default.to_string()),
            primary_key: false,
            unique: false,
            auto_generated: false,
            long: false,
            label: false,
            enum_variants: vec![],
            doc: None,
            icon: None,
            lang: None,
            subtype: None,
        }
    }

    fn make_unique_column(name: &str, pg_type: PgType, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            pg_type,
            rust_type: None,
            nullable,
            default: None,
            primary_key: false,
            unique: true,
            auto_generated: false,
            long: false,
            label: false,
            enum_variants: vec![],
            doc: None,
            icon: None,
            lang: None,
            subtype: None,
        }
    }

    #[test]
    fn snapshot_simple_table() {
        let table = Table {
            name: "users".to_string(),
            columns: vec![
                make_pk_column("id", PgType::BigInt),
                make_unique_column("email", PgType::Text, false),
                make_column("name", PgType::Text, false),
                make_column("bio", PgType::Text, true),
                make_column_with_default("created_at", PgType::Timestamptz, false, "now()"),
            ],
            foreign_keys: Vec::new(),
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
            icon: None,
        };

        insta::assert_snapshot!(table.to_create_table_sql());
    }

    #[test]
    fn snapshot_composite_primary_key() {
        // This is the case that was broken - composite PK should use table constraint
        let table = Table {
            name: "post_likes".to_string(),
            columns: vec![
                make_pk_column("user_id", PgType::BigInt),
                make_pk_column("post_id", PgType::BigInt),
                make_column_with_default("created_at", PgType::Timestamptz, false, "now()"),
            ],
            foreign_keys: Vec::new(),
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
            icon: None,
        };

        insta::assert_snapshot!(table.to_create_table_sql());
    }

    #[test]
    fn snapshot_table_with_foreign_keys() {
        let table = Table {
            name: "posts".to_string(),
            columns: vec![
                make_pk_column("id", PgType::BigInt),
                make_column("author_id", PgType::BigInt, false),
                make_column("category_id", PgType::BigInt, true),
                make_column("title", PgType::Text, false),
                make_column("body", PgType::Text, false),
            ],
            foreign_keys: vec![
                ForeignKey {
                    columns: vec!["author_id".to_string()],
                    references_table: "users".to_string(),
                    references_columns: vec!["id".to_string()],
                },
                ForeignKey {
                    columns: vec!["category_id".to_string()],
                    references_table: "categories".to_string(),
                    references_columns: vec!["id".to_string()],
                },
            ],
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
            icon: None,
        };

        // Note: to_create_table_sql doesn't include FKs (they're added separately)
        insta::assert_snapshot!(table.to_create_table_sql());
    }

    #[test]
    fn snapshot_junction_table() {
        // Many-to-many junction table with composite PK and FKs
        let table = Table {
            name: "post_tags".to_string(),
            columns: vec![
                make_pk_column("post_id", PgType::BigInt),
                make_pk_column("tag_id", PgType::BigInt),
            ],
            foreign_keys: vec![
                ForeignKey {
                    columns: vec!["post_id".to_string()],
                    references_table: "posts".to_string(),
                    references_columns: vec!["id".to_string()],
                },
                ForeignKey {
                    columns: vec!["tag_id".to_string()],
                    references_table: "tags".to_string(),
                    references_columns: vec!["id".to_string()],
                },
            ],
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
            icon: None,
        };

        insta::assert_snapshot!(table.to_create_table_sql());
    }

    #[test]
    fn snapshot_full_diff_sql() {
        // Test the full diff SQL output
        let desired = Schema {
            tables: vec![
                Table {
                    name: "users".to_string(),
                    columns: vec![
                        make_pk_column("id", PgType::BigInt),
                        make_unique_column("email", PgType::Text, false),
                        make_column("name", PgType::Text, false),
                    ],
                    foreign_keys: Vec::new(),
                    indices: Vec::new(),
                    source: SourceLocation::default(),
                    doc: None,
                    icon: None,
                },
                Table {
                    name: "posts".to_string(),
                    columns: vec![
                        make_pk_column("id", PgType::BigInt),
                        make_column("author_id", PgType::BigInt, false),
                        make_column("title", PgType::Text, false),
                    ],
                    foreign_keys: vec![ForeignKey {
                        columns: vec!["author_id".to_string()],
                        references_table: "users".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                    indices: Vec::new(),
                    source: SourceLocation::default(),
                    doc: None,
                    icon: None,
                },
                Table {
                    name: "post_likes".to_string(),
                    columns: vec![
                        make_pk_column("user_id", PgType::BigInt),
                        make_pk_column("post_id", PgType::BigInt),
                    ],
                    foreign_keys: vec![
                        ForeignKey {
                            columns: vec!["user_id".to_string()],
                            references_table: "users".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                        ForeignKey {
                            columns: vec!["post_id".to_string()],
                            references_table: "posts".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                    ],
                    indices: Vec::new(),
                    source: SourceLocation::default(),
                    doc: None,
                    icon: None,
                },
            ],
        };

        let current = Schema::new();
        let diff = desired.diff(&current);

        insta::assert_snapshot!(diff.to_sql());
    }

    // ===== Rename detection tests =====

    #[test]
    fn test_plural_singular_detection() {
        // Basic 's' suffix
        assert!(super::is_plural_singular_pair("users", "user"));
        assert!(super::is_plural_singular_pair("posts", "post"));
        assert!(super::is_plural_singular_pair("tags", "tag"));

        // 'ies' -> 'y'
        assert!(super::is_plural_singular_pair("categories", "category"));
        assert!(super::is_plural_singular_pair("entries", "entry"));

        // Compound names
        assert!(super::is_plural_singular_pair("post_tags", "post_tag"));
        assert!(super::is_plural_singular_pair("user_follows", "user_follow"));
        assert!(super::is_plural_singular_pair("post_likes", "post_like"));
        assert!(super::is_plural_singular_pair("post_categories", "post_category"));

        // Non-matches
        assert!(!super::is_plural_singular_pair("users", "posts"));
        assert!(!super::is_plural_singular_pair("user", "category"));
        assert!(!super::is_plural_singular_pair("foo", "bar"));
    }

    #[test]
    fn test_table_similarity() {
        let users_plural = make_table(
            "users",
            vec![
                make_column("id", PgType::BigInt, false),
                make_column("email", PgType::Text, false),
                make_column("name", PgType::Text, false),
            ],
        );

        let user_singular = make_table(
            "user",
            vec![
                make_column("id", PgType::BigInt, false),
                make_column("email", PgType::Text, false),
                make_column("name", PgType::Text, false),
            ],
        );

        let posts = make_table(
            "posts",
            vec![
                make_column("id", PgType::BigInt, false),
                make_column("title", PgType::Text, false),
            ],
        );

        // Same columns + plural/singular name = high similarity
        let sim = super::table_similarity(&users_plural, &user_singular);
        assert!(sim > 0.9, "Expected high similarity, got {}", sim);

        // Different tables = low similarity
        let sim_different = super::table_similarity(&users_plural, &posts);
        assert!(sim_different < 0.5, "Expected low similarity, got {}", sim_different);
    }

    #[test]
    fn test_diff_detects_rename() {
        let desired = Schema {
            tables: vec![make_table(
                "user",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                ],
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

        // Should detect a rename, not add + drop
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(matches!(
            &diff.table_diffs[0].changes[0],
            Change::RenameTable { from, to } if from == "users" && to == "user"
        ));
    }

    #[test]
    fn snapshot_rename_table_sql() {
        let desired = Schema {
            tables: vec![
                make_table(
                    "user",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("email", PgType::Text, false),
                    ],
                ),
                make_table(
                    "category",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("name", PgType::Text, false),
                    ],
                ),
            ],
        };

        let current = Schema {
            tables: vec![
                make_table(
                    "users",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("email", PgType::Text, false),
                    ],
                ),
                make_table(
                    "categories",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("name", PgType::Text, false),
                    ],
                ),
            ],
        };

        let diff = desired.diff(&current);
        insta::assert_snapshot!(diff.to_sql());
    }
}
