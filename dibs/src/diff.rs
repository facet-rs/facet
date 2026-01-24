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

use crate::{CheckConstraint, Column, ForeignKey, Index, PgType, Schema, Table, quote_ident};
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
    /// Rename a column.
    RenameColumn { from: String, to: String },
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
    /// Add a CHECK constraint.
    AddCheck(CheckConstraint),
    /// Drop a CHECK constraint (by name).
    DropCheck(String),
}

impl Change {
    /// Generate SQL statement for this change.
    ///
    /// The `table_name` is required for column-level changes.
    pub fn to_sql(&self, table_name: &str) -> String {
        let qt = quote_ident(table_name);
        match self {
            Change::AddTable(t) => t.to_create_table_sql(),
            Change::DropTable(name) => format!("DROP TABLE {};", quote_ident(name)),
            Change::RenameTable { from, to } => {
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quote_ident(from),
                    quote_ident(to)
                )
            }
            Change::AddColumn(col) => {
                let not_null = if col.nullable { "" } else { " NOT NULL" };
                let default = col
                    .default
                    .as_ref()
                    .map(|d| format!(" DEFAULT {}", d))
                    .unwrap_or_default();
                format!(
                    "ALTER TABLE {} ADD COLUMN {} {}{}{};",
                    qt,
                    quote_ident(&col.name),
                    col.pg_type,
                    not_null,
                    default
                )
            }
            Change::DropColumn(name) => {
                format!("ALTER TABLE {} DROP COLUMN {};", qt, quote_ident(name))
            }
            Change::RenameColumn { from, to } => {
                format!(
                    "ALTER TABLE {} RENAME COLUMN {} TO {};",
                    qt,
                    quote_ident(from),
                    quote_ident(to)
                )
            }
            Change::AlterColumnType { name, to, .. } => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::{};",
                    qt,
                    quote_ident(name),
                    to,
                    quote_ident(name),
                    to
                )
            }
            Change::AlterColumnNullable { name, to, .. } => {
                if *to {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL;",
                        qt,
                        quote_ident(name)
                    )
                } else {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                        qt,
                        quote_ident(name)
                    )
                }
            }
            Change::AlterColumnDefault { name, to, .. } => {
                if let Some(default) = to {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {};",
                        qt,
                        quote_ident(name),
                        default
                    )
                } else {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT;",
                        qt,
                        quote_ident(name)
                    )
                }
            }
            Change::AddPrimaryKey(cols) => {
                let quoted_cols: Vec<_> = cols.iter().map(|c| quote_ident(c)).collect();
                format!(
                    "ALTER TABLE {} ADD PRIMARY KEY ({});",
                    qt,
                    quoted_cols.join(", ")
                )
            }
            Change::DropPrimaryKey => {
                let constraint_name = format!("{}_pkey", table_name);
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    qt,
                    quote_ident(&constraint_name)
                )
            }
            Change::AddForeignKey(fk) => {
                let constraint_name = format!("{}_{}_fkey", table_name, fk.columns.join("_"));
                let quoted_cols: Vec<_> = fk.columns.iter().map(|c| quote_ident(c)).collect();
                let quoted_ref_cols: Vec<_> = fk
                    .references_columns
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect();
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});",
                    qt,
                    quote_ident(&constraint_name),
                    quoted_cols.join(", "),
                    quote_ident(&fk.references_table),
                    quoted_ref_cols.join(", ")
                )
            }
            Change::DropForeignKey(fk) => {
                let constraint_name = format!("{}_{}_fkey", table_name, fk.columns.join("_"));
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    qt,
                    quote_ident(&constraint_name)
                )
            }
            Change::AddIndex(idx) => {
                let unique = if idx.unique { "UNIQUE " } else { "" };
                let quoted_cols: Vec<_> = idx.columns.iter().map(|c| c.to_sql()).collect();
                let where_clause = idx
                    .where_clause
                    .as_ref()
                    .map(|w| format!(" WHERE {}", w))
                    .unwrap_or_default();
                format!(
                    "CREATE {}INDEX {} ON {} ({}){};",
                    unique,
                    quote_ident(&idx.name),
                    qt,
                    quoted_cols.join(", "),
                    where_clause
                )
            }
            Change::DropIndex(name) => {
                format!("DROP INDEX {};", quote_ident(name))
            }
            Change::AddUnique(col) => {
                let constraint_name = format!("{}_{}_key", table_name, col);
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({});",
                    qt,
                    quote_ident(&constraint_name),
                    quote_ident(col)
                )
            }
            Change::DropUnique(col) => {
                let constraint_name = format!("{}_{}_key", table_name, col);
                format!(
                    "ALTER TABLE {} DROP CONSTRAINT {};",
                    qt,
                    quote_ident(&constraint_name)
                )
            }
            Change::AddCheck(check) => format!(
                "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({});",
                qt,
                quote_ident(&check.name),
                check.expr
            ),
            Change::DropCheck(name) => {
                format!("ALTER TABLE {} DROP CONSTRAINT {};", qt, quote_ident(name))
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
            Change::RenameColumn { from, to } => write!(f, "~ rename column {} -> {}", from, to),
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
                let where_clause = idx
                    .where_clause
                    .as_ref()
                    .map(|w| format!(" WHERE {}", w))
                    .unwrap_or_default();
                let cols: Vec<String> = idx
                    .columns
                    .iter()
                    .map(|c| format!("{}{}{}", c.name, c.order.to_sql(), c.nulls.to_sql()))
                    .collect();
                write!(
                    f,
                    "+ {}INDEX {} ({}){}",
                    unique,
                    idx.name,
                    cols.join(", "),
                    where_clause
                )
            }
            Change::DropIndex(name) => write!(f, "- INDEX {}", name),
            Change::AddUnique(col) => write!(f, "+ UNIQUE ({})", col),
            Change::DropUnique(col) => write!(f, "- UNIQUE ({})", col),
            Change::AddCheck(check) => write!(f, "+ CHECK {}: {}", check.name, check.expr),
            Change::DropCheck(name) => write!(f, "- CHECK {}", name),
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

        // Build a map of old_name -> new_name for FK comparison.
        // This is used to account for the fact that Postgres automatically updates
        // FK references when a table is renamed.
        let table_renames: std::collections::HashMap<String, String> =
            renames.iter().cloned().collect();

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
                let column_changes = diff_table(new_table, old_table, &table_renames);
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
                    changes: table_creation_changes(table),
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
                let changes = diff_table(desired_table, current_table, &table_renames);
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
///
/// The `table_renames` map contains old_name -> new_name mappings for tables being
/// renamed in this migration. This is needed so FK comparisons can account for
/// automatic reference updates.
fn diff_table(
    desired: &Table,
    current: &Table,
    table_renames: &std::collections::HashMap<String, String>,
) -> Vec<Change> {
    let mut changes = Vec::new();

    // Diff columns
    changes.extend(diff_columns(&desired.columns, &current.columns));

    // Diff CHECK constraints
    changes.extend(diff_check_constraints(
        &desired.check_constraints,
        &current.check_constraints,
    ));

    // Diff foreign keys
    changes.extend(diff_foreign_keys(
        &desired.foreign_keys,
        &current.foreign_keys,
        table_renames,
    ));

    // Diff indices
    changes.extend(diff_indices(&desired.indices, &current.indices));

    changes
}

fn diff_check_constraints(desired: &[CheckConstraint], current: &[CheckConstraint]) -> Vec<Change> {
    let mut changes = Vec::new();

    fn normalize_check_expr(expr: &str) -> String {
        let mut s = expr.trim().to_string();

        // Strip redundant outer parentheses.
        loop {
            let t = s.trim();
            if t.starts_with('(') && t.ends_with(')') {
                let inner = &t[1..t.len() - 1];
                let mut depth = 0i32;
                let mut ok = true;
                for ch in inner.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth < 0 {
                                ok = false;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if ok && depth == 0 {
                    s = inner.to_string();
                    continue;
                }
            }
            break;
        }

        // Normalize casts that Postgres tends to insert in deparsed expressions.
        for cast in [
            "::text",
            "::character varying",
            "::varchar",
            "::bpchar",
            "::int",
            "::int4",
            "::integer",
            "::bigint",
            "::int8",
        ] {
            s = s.replace(cast, "");
        }

        // Normalize Postgres' `= ANY (ARRAY[...])` back to `IN (...)` for stable diffing.
        //
        // Example: `status = ANY (ARRAY['reserved', 'applied'])` ≈ `status IN ('reserved', 'applied')`
        // This is not a full SQL parser; it's just enough to make round-trips stable.
        s = s.replace(" = ANY (ARRAY[", " IN (");
        s = s.replace("= ANY (ARRAY[", "IN (");
        s = s.replace("])", ")");

        fn strip_simple_group_parens(input: &str) -> String {
            fn is_group_prefix(ch: char) -> bool {
                ch.is_whitespace()
                    || matches!(ch, '(' | '!' | '=' | '<' | '>' | '+' | '-' | '*' | '/')
            }

            let mut s = input.to_string();
            loop {
                let bytes = s.as_bytes();
                let mut out = String::with_capacity(s.len());
                let mut changed = false;
                let mut i = 0usize;
                while i < bytes.len() {
                    if bytes[i] == b'(' {
                        let prev = if i == 0 {
                            None
                        } else {
                            Some(bytes[i - 1] as char)
                        };
                        if prev.is_none() || prev.is_some_and(is_group_prefix) {
                            // Only consider non-nested (...) groups.
                            if let Some(close) = s[i + 1..].find(')') {
                                let j = i + 1 + close;
                                let inner = &s[i + 1..j];
                                if !inner.contains('(') && !inner.contains(')') {
                                    let upper = inner.to_uppercase();
                                    // Don't strip IN lists or boolean compositions.
                                    if !inner.contains(',')
                                        && !upper.contains(" OR ")
                                        && !upper.contains(" AND ")
                                    {
                                        out.push_str(inner.trim());
                                        i = j + 1;
                                        changed = true;
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    out.push(bytes[i] as char);
                    i += 1;
                }

                if !changed {
                    return s;
                }
                s = out;
            }
        }

        s = strip_simple_group_parens(&s);

        // Normalize whitespace.
        let mut out = String::with_capacity(s.len());
        let mut pending_space = false;
        for ch in s.chars() {
            if ch.is_whitespace() {
                pending_space = true;
                continue;
            }
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }

        out.trim().to_string()
    }

    let desired_by_name: std::collections::HashMap<&str, &CheckConstraint> =
        desired.iter().map(|c| (c.name.as_str(), c)).collect();
    let current_by_name: std::collections::HashMap<&str, &CheckConstraint> =
        current.iter().map(|c| (c.name.as_str(), c)).collect();

    // Drops
    for c in current {
        if !desired_by_name.contains_key(c.name.as_str()) {
            changes.push(Change::DropCheck(c.name.clone()));
        }
    }

    // Adds / modifications
    for d in desired {
        match current_by_name.get(d.name.as_str()) {
            None => changes.push(Change::AddCheck(d.clone())),
            Some(c) if normalize_check_expr(&c.expr) != normalize_check_expr(&d.expr) => {
                changes.push(Change::DropCheck(d.name.clone()));
                changes.push(Change::AddCheck(d.clone()));
            }
            Some(_) => {}
        }
    }

    changes
}

/// Generate all changes needed to create a new table.
///
/// This is the single source of truth for table creation. It includes:
/// - The CREATE TABLE statement
/// - All foreign key constraints (as ALTER TABLE ADD CONSTRAINT)
/// - All indices (as CREATE INDEX)
///
/// By centralizing this logic, we prevent bugs where new table features
/// (like FKs or indices) are forgotten when adding tables.
fn table_creation_changes(table: &Table) -> Vec<Change> {
    let mut changes = Vec::with_capacity(1 + table.foreign_keys.len() + table.indices.len());

    // The table itself
    changes.push(Change::AddTable(table.clone()));

    // All foreign keys
    for fk in &table.foreign_keys {
        changes.push(Change::AddForeignKey(fk.clone()));
    }

    // All indices
    for idx in &table.indices {
        changes.push(Change::AddIndex(idx.clone()));
    }

    changes
}

/// Calculate similarity score between two columns for rename detection.
///
/// Returns a score from 0.0 to 1.0 based on:
/// - Type match (50%): Must match exactly for any score
/// - Nullability match (15%): Same nullability
/// - Name similarity (35%): Similar names suggest rename
fn column_similarity(a: &Column, b: &Column) -> f64 {
    // Type must match - this is the strongest signal
    if a.pg_type != b.pg_type {
        return 0.0;
    }

    let mut score = 0.5; // Type match base score

    // Nullability match
    if a.nullable == b.nullable {
        score += 0.15;
    }

    // Name similarity - check for common rename patterns
    let name_sim = column_name_similarity(&a.name, &b.name);
    score += 0.35 * name_sim;

    score
}

/// Calculate name similarity between two column names.
///
/// Returns 1.0 for exact match, high score for similar names, 0.0 for unrelated.
fn column_name_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }

    // Check for common transformations
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Remove underscores and compare (user_name vs username)
    let a_no_underscore: String = a_lower.chars().filter(|c| *c != '_').collect();
    let b_no_underscore: String = b_lower.chars().filter(|c| *c != '_').collect();
    if a_no_underscore == b_no_underscore {
        return 0.9;
    }

    // Check if one is prefix/suffix of the other with common additions
    // e.g., "created" vs "created_at", "name" vs "user_name"
    if a_lower.contains(&b_lower) || b_lower.contains(&a_lower) {
        return 0.7;
    }

    // Check common prefixes (at least 3 chars)
    let common_prefix_len = a_lower
        .chars()
        .zip(b_lower.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    if common_prefix_len >= 3 {
        let max_len = a.len().max(b.len());
        return (common_prefix_len as f64 / max_len as f64) * 0.5;
    }

    0.0
}

/// Detect likely column renames from lists of added and dropped columns.
fn detect_column_renames(added: &[&Column], dropped: &[&Column]) -> Vec<(String, String)> {
    const RENAME_THRESHOLD: f64 = 0.65;

    let mut renames = Vec::new();
    let mut used_added: HashSet<&str> = HashSet::new();
    let mut used_dropped: HashSet<&str> = HashSet::new();

    // Find best matches
    let mut candidates: Vec<(f64, &str, &str)> = Vec::new();

    for dropped_col in dropped {
        for added_col in added {
            let sim = column_similarity(dropped_col, added_col);
            if sim >= RENAME_THRESHOLD {
                candidates.push((sim, &dropped_col.name, &added_col.name));
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

/// Diff columns between desired and current state.
fn diff_columns(desired: &[Column], current: &[Column]) -> Vec<Change> {
    let mut changes = Vec::new();

    let desired_names: HashSet<&str> = desired.iter().map(|c| c.name.as_str()).collect();
    let current_names: HashSet<&str> = current.iter().map(|c| c.name.as_str()).collect();

    // Find columns only in desired (candidates for add or rename target)
    let added_cols: Vec<&Column> = desired
        .iter()
        .filter(|c| !current_names.contains(c.name.as_str()))
        .collect();

    // Find columns only in current (candidates for drop or rename source)
    let dropped_cols: Vec<&Column> = current
        .iter()
        .filter(|c| !desired_names.contains(c.name.as_str()))
        .collect();

    // Detect likely renames
    let renames = detect_column_renames(&added_cols, &dropped_cols);
    let renamed_from: HashSet<&str> = renames.iter().map(|(from, _)| from.as_str()).collect();
    let renamed_to: HashSet<&str> = renames.iter().map(|(_, to)| to.as_str()).collect();

    // Generate rename changes
    for (from, to) in &renames {
        changes.push(Change::RenameColumn {
            from: from.clone(),
            to: to.clone(),
        });

        // Also check if other properties changed after rename
        if let (Some(current_col), Some(desired_col)) = (
            current.iter().find(|c| &c.name == from),
            desired.iter().find(|c| &c.name == to),
        ) {
            // Nullability change
            if desired_col.nullable != current_col.nullable {
                changes.push(Change::AlterColumnNullable {
                    name: to.clone(),
                    from: current_col.nullable,
                    to: desired_col.nullable,
                });
            }
            // Default change
            if desired_col.default != current_col.default {
                changes.push(Change::AlterColumnDefault {
                    name: to.clone(),
                    from: current_col.default.clone(),
                    to: desired_col.default.clone(),
                });
            }
            // Unique constraint change
            if desired_col.unique != current_col.unique {
                if desired_col.unique {
                    changes.push(Change::AddUnique(to.clone()));
                } else {
                    changes.push(Change::DropUnique(to.clone()));
                }
            }
        }
    }

    // Columns to add (not involved in a rename)
    for col in &added_cols {
        if !renamed_to.contains(col.name.as_str()) {
            changes.push(Change::AddColumn((*col).clone()));
        }
    }

    // Columns to drop (not involved in a rename)
    for col in &dropped_cols {
        if !renamed_from.contains(col.name.as_str()) {
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
///
/// The `table_renames` map contains old_name -> new_name mappings for tables being
/// renamed in this migration. When comparing FKs, current FKs that reference a renamed
/// table are treated as if they already reference the new name, since Postgres
/// automatically updates FK references when a table is renamed.
fn diff_foreign_keys(
    desired: &[ForeignKey],
    current: &[ForeignKey],
    table_renames: &std::collections::HashMap<String, String>,
) -> Vec<Change> {
    let mut changes = Vec::new();

    // Transform current FKs to account for table renames.
    // If a current FK references a table that's being renamed, transform it
    // to reference the new name for comparison purposes.
    let transformed_current: Vec<ForeignKey> = current
        .iter()
        .map(|fk| {
            if let Some(new_name) = table_renames.get(&fk.references_table) {
                ForeignKey {
                    columns: fk.columns.clone(),
                    references_table: new_name.clone(),
                    references_columns: fk.references_columns.clone(),
                }
            } else {
                fk.clone()
            }
        })
        .collect();

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
    let transformed_current_keys: HashSet<String> =
        transformed_current.iter().map(fk_key).collect();

    // FKs to add (in desired but not in transformed current)
    for fk in desired {
        if !transformed_current_keys.contains(&fk_key(fk)) {
            changes.push(Change::AddForeignKey(fk.clone()));
        }
    }

    // FKs to drop (in current but not in desired, after accounting for renames)
    // We compare transformed current against desired to see what's truly missing
    for fk in &transformed_current {
        if !desired_keys.contains(&fk_key(fk)) {
            // Find the original FK to drop (with original references_table)
            let original = current
                .iter()
                .find(|orig| {
                    orig.columns == fk.columns && orig.references_columns == fk.references_columns
                })
                .unwrap_or(fk);
            changes.push(Change::DropForeignKey(original.clone()));
        }
    }

    changes
}

/// Diff indices.
fn diff_indices(desired: &[Index], current: &[Index]) -> Vec<Change> {
    let mut changes = Vec::new();

    fn normalize_where_clause(where_clause: &str) -> String {
        let mut s = where_clause.trim().to_string();

        // Strip redundant outer parentheses.
        loop {
            let t = s.trim();
            if t.starts_with('(') && t.ends_with(')') {
                let inner = &t[1..t.len() - 1];
                // Only strip if the inner string doesn't obviously unbalance parens.
                // (Cheap guard; this is normalization, not a parser.)
                let mut depth = 0i32;
                let mut ok = true;
                for ch in inner.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth < 0 {
                                ok = false;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if ok && depth == 0 {
                    s = inner.to_string();
                    continue;
                }
            }
            break;
        }

        // PostgreSQL often inserts casts in stored index predicates (e.g. "'applied'::text").
        // Strip the most common ones so round-tripping doesn't cause diff churn.
        for cast in ["::text", "::character varying", "::varchar", "::bpchar"] {
            s = s.replace(cast, "");
        }

        // Normalize whitespace.
        let mut out = String::with_capacity(s.len());
        let mut pending_space = false;
        for ch in s.chars() {
            if ch.is_whitespace() {
                pending_space = true;
                continue;
            }
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
        out.trim().to_string()
    }

    // Compare by columns (with order and nulls), uniqueness, and where_clause (not name, since names may differ)
    // Note: column order matters for indexes, so we don't sort them
    let idx_key = |idx: &Index| -> String {
        let cols: Vec<String> = idx
            .columns
            .iter()
            .map(|c| format!("{}{}{}", c.name, c.order.to_sql(), c.nulls.to_sql()))
            .collect();
        let where_part = idx
            .where_clause
            .as_deref()
            .map(normalize_where_clause)
            .unwrap_or_default();
        format!(
            "{}:{}:{}",
            if idx.unique { "U" } else { "" },
            cols.join(","),
            where_part
        )
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
    use crate::{IndexColumn, SourceLocation};

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
            check_constraints: Vec::new(),
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

    fn make_column_with_default(
        name: &str,
        pg_type: PgType,
        nullable: bool,
        default: &str,
    ) -> Column {
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
            check_constraints: Vec::new(),
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
            check_constraints: Vec::new(),
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
            check_constraints: Vec::new(),
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
            check_constraints: Vec::new(),
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
                    check_constraints: Vec::new(),
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
                    check_constraints: Vec::new(),
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
                    check_constraints: Vec::new(),
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
        assert!(super::is_plural_singular_pair(
            "user_follows",
            "user_follow"
        ));
        assert!(super::is_plural_singular_pair("post_likes", "post_like"));
        assert!(super::is_plural_singular_pair(
            "post_categories",
            "post_category"
        ));

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
        assert!(
            sim_different < 0.5,
            "Expected low similarity, got {}",
            sim_different
        );
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

    #[test]
    fn test_rename_table_preserves_fk_references() {
        // When a table is renamed, FKs that reference it should NOT generate
        // add/drop changes - Postgres automatically updates them.

        fn make_table_with_fks(name: &str, columns: Vec<Column>, fks: Vec<ForeignKey>) -> Table {
            Table {
                name: name.to_string(),
                columns,
                check_constraints: Vec::new(),
                foreign_keys: fks,
                indices: Vec::new(),
                source: SourceLocation::default(),
                doc: None,
                icon: None,
            }
        }

        // Current: categories with self-ref FK
        let current = Schema {
            tables: vec![make_table_with_fks(
                "categories",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("parent_id", PgType::BigInt, true),
                ],
                vec![ForeignKey {
                    columns: vec!["parent_id".to_string()],
                    references_table: "categories".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            )],
        };

        // Desired: same table renamed to category, FK references category
        let desired = Schema {
            tables: vec![make_table_with_fks(
                "category",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("parent_id", PgType::BigInt, true),
                ],
                vec![ForeignKey {
                    columns: vec!["parent_id".to_string()],
                    references_table: "category".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            )],
        };

        let diff = desired.diff(&current);

        // Should only have ONE table diff with ONE change (the rename)
        assert_eq!(
            diff.table_diffs.len(),
            1,
            "Should have exactly one table diff"
        );
        assert_eq!(
            diff.table_diffs[0].changes.len(),
            1,
            "Should have exactly one change (rename), not add/drop FK. Changes: {:?}",
            diff.table_diffs[0].changes
        );
        assert!(
            matches!(
                &diff.table_diffs[0].changes[0],
                Change::RenameTable { from, to } if from == "categories" && to == "category"
            ),
            "The one change should be a rename"
        );
    }

    #[test]
    fn test_rename_table_with_external_fk_references() {
        // When a table is renamed, FKs from OTHER tables that reference it
        // should also NOT generate add/drop changes.

        fn make_table_with_fks(name: &str, columns: Vec<Column>, fks: Vec<ForeignKey>) -> Table {
            Table {
                name: name.to_string(),
                columns,
                check_constraints: Vec::new(),
                foreign_keys: fks,
                indices: Vec::new(),
                source: SourceLocation::default(),
                doc: None,
                icon: None,
            }
        }

        // Current: users and posts (posts has FK to users)
        let current = Schema {
            tables: vec![
                make_table("users", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "posts",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                    vec![ForeignKey {
                        columns: vec!["author_id".to_string()],
                        references_table: "users".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                ),
            ],
        };

        // Desired: users renamed to user, posts FK now references user
        let desired = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "posts",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                    vec![ForeignKey {
                        columns: vec!["author_id".to_string()],
                        references_table: "user".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                ),
            ],
        };

        let diff = desired.diff(&current);

        // Should only have ONE table diff (the user rename)
        // The posts table should NOT have any changes since its FK reference
        // will be automatically updated by Postgres
        assert_eq!(
            diff.table_diffs.len(),
            1,
            "Should only have one table diff (rename). Got: {:?}",
            diff.table_diffs
        );
        assert!(
            matches!(
                &diff.table_diffs[0].changes[0],
                Change::RenameTable { from, to } if from == "users" && to == "user"
            ),
            "The change should be the user rename"
        );
    }

    // ===== Column rename detection tests =====

    #[test]
    fn test_column_name_similarity() {
        // Exact match
        assert_eq!(column_name_similarity("email", "email"), 1.0);

        // Underscore variations
        assert!(column_name_similarity("user_name", "username") > 0.8);

        // Prefix/suffix containment
        assert!(column_name_similarity("created", "created_at") > 0.6);
        assert!(column_name_similarity("name", "user_name") > 0.6);

        // Common prefix
        assert!(column_name_similarity("user_id", "user_name") > 0.2);

        // Unrelated
        assert_eq!(column_name_similarity("foo", "bar"), 0.0);
    }

    #[test]
    fn test_column_similarity() {
        let col_a = make_column("email", PgType::Text, false);
        let col_b = make_column("user_email", PgType::Text, false);
        let col_c = make_column("email", PgType::Integer, false);
        let col_d = make_column("email", PgType::Text, true);

        // Same type + similar name = high similarity
        let sim = column_similarity(&col_a, &col_b);
        assert!(
            sim > 0.65,
            "Expected high similarity for similar columns, got {}",
            sim
        );

        // Different type = 0 (disqualified)
        assert_eq!(column_similarity(&col_a, &col_c), 0.0);

        // Same type, same name, different nullability
        let sim_nullable = column_similarity(&col_a, &col_d);
        assert!(
            sim_nullable > 0.5,
            "Expected medium similarity, got {}",
            sim_nullable
        );
    }

    #[test]
    fn test_diff_detects_column_rename() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("user_email", PgType::Text, false),
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

        // Should detect a column rename, not add + drop
        assert_eq!(diff.table_diffs.len(), 1);
        assert!(
            matches!(
                &diff.table_diffs[0].changes[0],
                Change::RenameColumn { from, to } if from == "email" && to == "user_email"
            ),
            "Expected RenameColumn, got {:?}",
            diff.table_diffs[0].changes
        );
    }

    #[test]
    fn test_column_rename_with_property_changes() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("user_email", PgType::Text, true), // Now nullable
                ],
            )],
        };

        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false), // Was not nullable
                ],
            )],
        };

        let diff = desired.diff(&current);

        // Should detect rename AND nullability change
        assert_eq!(diff.table_diffs.len(), 1);
        let changes = &diff.table_diffs[0].changes;

        // First should be rename
        assert!(
            matches!(
                &changes[0],
                Change::RenameColumn { from, to } if from == "email" && to == "user_email"
            ),
            "Expected RenameColumn, got {:?}",
            changes[0]
        );

        // Second should be nullability change
        assert!(
            matches!(
                &changes[1],
                Change::AlterColumnNullable { name, from: false, to: true } if name == "user_email"
            ),
            "Expected AlterColumnNullable, got {:?}",
            changes[1]
        );
    }

    #[test]
    fn test_no_false_positive_rename() {
        // Columns with different types should not be detected as renames
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("count", PgType::BigInt, false),
                ],
            )],
        };

        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("total", PgType::Text, false), // Different type!
                ],
            )],
        };

        let diff = desired.diff(&current);
        let changes = &diff.table_diffs[0].changes;

        // Should NOT detect rename - types don't match
        // Should see add + drop instead
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, Change::AddColumn(col) if col.name == "count"))
        );
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, Change::DropColumn(name) if name == "total"))
        );
    }

    #[test]
    fn snapshot_rename_column_sql() {
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("user_email", PgType::Text, false),
                    make_column("full_name", PgType::Text, true),
                ],
            )],
        };

        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                    make_column("name", PgType::Text, true),
                ],
            )],
        };

        let diff = desired.diff(&current);
        insta::assert_snapshot!(diff.to_sql());
    }

    #[test]
    fn snapshot_new_table_with_foreign_key() {
        // When creating a new table with an FK, the migration should include
        // both CREATE TABLE and ALTER TABLE ADD CONSTRAINT for the FK.
        fn make_table_with_fks(name: &str, columns: Vec<Column>, fks: Vec<ForeignKey>) -> Table {
            Table {
                name: name.to_string(),
                columns,
                check_constraints: Vec::new(),
                foreign_keys: fks,
                indices: Vec::new(),
                source: SourceLocation::default(),
                doc: None,
                icon: None,
            }
        }

        let desired = Schema {
            tables: vec![
                make_table("shop", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "category",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("shop_id", PgType::BigInt, false),
                        make_column("parent_id", PgType::BigInt, true),
                    ],
                    vec![
                        ForeignKey {
                            columns: vec!["shop_id".to_string()],
                            references_table: "shop".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                        ForeignKey {
                            columns: vec!["parent_id".to_string()],
                            references_table: "category".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                    ],
                ),
            ],
        };

        // Current: only shop table exists, no category
        let current = Schema {
            tables: vec![make_table(
                "shop",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };

        let diff = desired.diff(&current);
        insta::assert_snapshot!(diff.to_sql());
    }

    #[test]
    fn test_diff_partial_index() {
        // Test that partial indexes (with WHERE clause) are detected correctly
        let desired = vec![Index {
            name: "uq_product_primary".to_string(),
            columns: vec![IndexColumn::new("product_id")],
            unique: true,
            where_clause: Some("is_primary = true".to_string()),
        }];

        let current = vec![Index {
            name: "idx_product_product_id".to_string(),
            columns: vec![IndexColumn::new("product_id")],
            unique: true,
            where_clause: None, // No WHERE clause - different index
        }];

        let changes = diff_indices(&desired, &current);

        // Should have 2 changes: drop the old index, add the new one
        assert_eq!(changes.len(), 2);
        assert!(
            matches!(&changes[0], Change::AddIndex(idx) if idx.where_clause == Some("is_primary = true".to_string()))
        );
        assert!(matches!(&changes[1], Change::DropIndex(name) if name == "idx_product_product_id"));
    }

    #[test]
    fn test_diff_same_partial_index() {
        // Same partial index should produce no changes
        let desired = vec![Index {
            name: "uq_product_primary".to_string(),
            columns: vec![IndexColumn::new("product_id")],
            unique: true,
            where_clause: Some("is_primary = true".to_string()),
        }];

        let current = vec![Index {
            name: "uq_product_primary".to_string(),
            columns: vec![IndexColumn::new("product_id")],
            unique: true,
            where_clause: Some("is_primary = true".to_string()),
        }];

        let changes = diff_indices(&desired, &current);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_partial_index_sql_generation() {
        let idx = Index {
            name: "uq_product_primary".to_string(),
            columns: vec![IndexColumn::new("product_id")],
            unique: true,
            where_clause: Some("is_primary = true".to_string()),
        };

        let change = Change::AddIndex(idx);
        let sql = change.to_sql("product_category");

        assert_eq!(
            sql,
            r#"CREATE UNIQUE INDEX "uq_product_primary" ON "product_category" ("product_id") WHERE is_primary = true;"#
        );
    }
}
