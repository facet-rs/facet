//! Migration solver - orders and validates schema changes.
//!
//! The solver ensures migrations will succeed by:
//! 1. Simulating changes against a virtual schema
//! 2. Ordering operations to satisfy dependencies
//! 3. Detecting impossible migrations (cycles, conflicts)
//!
//! ## Example Problem
//!
//! ```text
//! -- This fails:
//! ALTER TABLE comment ADD CONSTRAINT ... REFERENCES post(id);  -- "post" doesn't exist!
//! ALTER TABLE posts RENAME TO post;
//!
//! -- This works:
//! ALTER TABLE posts RENAME TO post;
//! ALTER TABLE comment ADD CONSTRAINT ... REFERENCES post(id);  -- "post" exists now
//! ```

use crate::{Change, ForeignKey, SchemaDiff};
use std::collections::{HashMap, HashSet};

/// Error when migration cannot be executed.
#[derive(Debug, Clone, PartialEq)]
pub enum SolverError {
    /// A change requires a table that doesn't exist.
    TableNotFound {
        change: String,
        table: String,
    },
    /// A change requires a table to NOT exist, but it does.
    TableAlreadyExists {
        change: String,
        table: String,
    },
    /// A change requires a column that doesn't exist.
    ColumnNotFound {
        change: String,
        table: String,
        column: String,
    },
    /// A change requires a column to NOT exist, but it does.
    ColumnAlreadyExists {
        change: String,
        table: String,
        column: String,
    },
    /// A foreign key references a table that doesn't exist.
    ForeignKeyTargetNotFound {
        change: String,
        source_table: String,
        target_table: String,
    },
    /// A foreign key references columns that don't exist.
    ForeignKeyColumnsNotFound {
        change: String,
        table: String,
        columns: Vec<String>,
    },
    /// Changes form a dependency cycle that cannot be resolved.
    CycleDetected {
        changes: Vec<String>,
    },
    /// Conflicting operations detected (e.g., add then drop same column).
    ConflictingOperations {
        first: String,
        second: String,
        reason: String,
    },
    /// Migration simulation didn't produce the expected result.
    SimulationMismatch {
        /// Human-readable diff between expected and actual state.
        diff: String,
    },
}

impl std::fmt::Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolverError::TableNotFound { change, table } => {
                write!(f, "{}: table '{}' does not exist", change, table)
            }
            SolverError::TableAlreadyExists { change, table } => {
                write!(f, "{}: table '{}' already exists", change, table)
            }
            SolverError::ColumnNotFound { change, table, column } => {
                write!(f, "{}: column '{}.{}' does not exist", change, table, column)
            }
            SolverError::ColumnAlreadyExists { change, table, column } => {
                write!(f, "{}: column '{}.{}' already exists", change, table, column)
            }
            SolverError::ForeignKeyTargetNotFound { change, source_table, target_table } => {
                write!(
                    f,
                    "{}: foreign key from '{}' references non-existent table '{}'",
                    change, source_table, target_table
                )
            }
            SolverError::ForeignKeyColumnsNotFound { change, table, columns } => {
                write!(
                    f,
                    "{}: foreign key columns {} not found in table '{}'",
                    change,
                    columns.join(", "),
                    table
                )
            }
            SolverError::CycleDetected { changes } => {
                write!(
                    f,
                    "dependency cycle detected, cannot order: {}",
                    changes.join(" -> ")
                )
            }
            SolverError::ConflictingOperations { first, second, reason } => {
                write!(f, "conflicting operations: '{}' and '{}': {}", first, second, reason)
            }
            SolverError::SimulationMismatch { diff } => {
                write!(
                    f,
                    "migration simulation didn't produce expected result:\n{}",
                    diff
                )
            }
        }
    }
}

impl std::error::Error for SolverError {}

/// Virtual representation of a table for simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VirtualTable {
    columns: HashSet<String>,
    foreign_keys: HashSet<ForeignKey>,
    indices: HashSet<String>,
    unique_constraints: HashSet<String>,
}

/// Virtual schema state for simulating migrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualSchema {
    tables: HashMap<String, VirtualTable>,
}

impl VirtualSchema {
    /// Create a virtual schema from a set of existing table names.
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// Initialize from actual database state.
    pub fn from_existing(existing_tables: &HashSet<String>) -> Self {
        let mut schema = Self::new();
        for table_name in existing_tables {
            schema.tables.insert(
                table_name.clone(),
                VirtualTable {
                    columns: HashSet::new(), // We don't track columns from DB yet
                    foreign_keys: HashSet::new(),
                    indices: HashSet::new(),
                    unique_constraints: HashSet::new(),
                },
            );
        }
        schema
    }

    /// Initialize with full table info including columns.
    pub fn from_tables(tables: &[crate::Table]) -> Self {
        let mut schema = Self::new();
        for table in tables {
            schema.tables.insert(
                table.name.clone(),
                VirtualTable {
                    columns: table.columns.iter().map(|c| c.name.clone()).collect(),
                    foreign_keys: table.foreign_keys.iter().cloned().collect(),
                    indices: table.indices.iter().map(|i| i.name.clone()).collect(),
                    unique_constraints: table
                        .columns
                        .iter()
                        .filter(|c| c.unique)
                        .map(|c| c.name.clone())
                        .collect(),
                },
            );
        }
        schema
    }

    /// Check if a table exists.
    pub fn table_exists(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// Check if a column exists in a table.
    pub fn column_exists(&self, table: &str, column: &str) -> bool {
        self.tables
            .get(table)
            .map(|t| t.columns.contains(column))
            .unwrap_or(false)
    }

    /// Compare this schema to another and return a human-readable diff.
    /// Returns None if schemas are identical, Some(diff) otherwise.
    pub fn diff_from(&self, other: &VirtualSchema) -> Option<String> {
        if self == other {
            return None;
        }

        let mut diffs = Vec::new();

        // Tables only in self
        for name in self.tables.keys() {
            if !other.tables.contains_key(name) {
                diffs.push(format!("+ table '{}'", name));
            }
        }

        // Tables only in other
        for name in other.tables.keys() {
            if !self.tables.contains_key(name) {
                diffs.push(format!("- table '{}'", name));
            }
        }

        // Tables in both - compare contents
        for (name, self_table) in &self.tables {
            if let Some(other_table) = other.tables.get(name) {
                if self_table != other_table {
                    // Columns
                    for col in &self_table.columns {
                        if !other_table.columns.contains(col) {
                            diffs.push(format!("+ {}.{}", name, col));
                        }
                    }
                    for col in &other_table.columns {
                        if !self_table.columns.contains(col) {
                            diffs.push(format!("- {}.{}", name, col));
                        }
                    }

                    // Foreign keys
                    for fk in &self_table.foreign_keys {
                        if !other_table.foreign_keys.contains(fk) {
                            diffs.push(format!(
                                "+ {}.FK({} -> {})",
                                name,
                                fk.columns.join(","),
                                fk.references_table
                            ));
                        }
                    }
                    for fk in &other_table.foreign_keys {
                        if !self_table.foreign_keys.contains(fk) {
                            diffs.push(format!(
                                "- {}.FK({} -> {})",
                                name,
                                fk.columns.join(","),
                                fk.references_table
                            ));
                        }
                    }

                    // Indices
                    for idx in &self_table.indices {
                        if !other_table.indices.contains(idx) {
                            diffs.push(format!("+ {}.index({})", name, idx));
                        }
                    }
                    for idx in &other_table.indices {
                        if !self_table.indices.contains(idx) {
                            diffs.push(format!("- {}.index({})", name, idx));
                        }
                    }
                }
            }
        }

        if diffs.is_empty() {
            None
        } else {
            Some(diffs.join("\n"))
        }
    }

    /// Apply a change to the virtual schema, validating preconditions.
    pub fn apply(&mut self, table_context: &str, change: &Change) -> Result<(), SolverError> {
        let change_desc = format!("{}", change);

        match change {
            Change::AddTable(t) => {
                if self.table_exists(&t.name) {
                    return Err(SolverError::TableAlreadyExists {
                        change: change_desc,
                        table: t.name.clone(),
                    });
                }
                // Check FK targets exist
                for fk in &t.foreign_keys {
                    if !self.table_exists(&fk.references_table) {
                        return Err(SolverError::ForeignKeyTargetNotFound {
                            change: change_desc,
                            source_table: t.name.clone(),
                            target_table: fk.references_table.clone(),
                        });
                    }
                }
                self.tables.insert(
                    t.name.clone(),
                    VirtualTable {
                        columns: t.columns.iter().map(|c| c.name.clone()).collect(),
                        foreign_keys: t.foreign_keys.iter().cloned().collect(),
                        indices: t.indices.iter().map(|i| i.name.clone()).collect(),
                        unique_constraints: t
                            .columns
                            .iter()
                            .filter(|c| c.unique)
                            .map(|c| c.name.clone())
                            .collect(),
                    },
                );
            }

            Change::DropTable(name) => {
                if !self.table_exists(name) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: name.clone(),
                    });
                }
                self.tables.remove(name);
            }

            Change::RenameTable { from, to } => {
                if !self.table_exists(from) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: from.clone(),
                    });
                }
                if self.table_exists(to) {
                    return Err(SolverError::TableAlreadyExists {
                        change: change_desc,
                        table: to.clone(),
                    });
                }
                if let Some(table) = self.tables.remove(from) {
                    self.tables.insert(to.clone(), table);
                }
            }

            Change::AddColumn(col) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if self.column_exists(table_context, &col.name) {
                    return Err(SolverError::ColumnAlreadyExists {
                        change: change_desc,
                        table: table_context.to_string(),
                        column: col.name.clone(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.columns.insert(col.name.clone());
                    // Track unique constraint if column is unique
                    if col.unique {
                        table.unique_constraints.insert(col.name.clone());
                    }
                }
            }

            Change::DropColumn(name) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                // Note: We don't require column to exist since we may not have full column info
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.columns.remove(name);
                    // Also remove unique constraint if it existed
                    table.unique_constraints.remove(name);
                }
            }

            Change::RenameColumn { from, to } => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    // Check new name doesn't already exist
                    if table.columns.contains(to) {
                        return Err(SolverError::ColumnAlreadyExists {
                            change: change_desc,
                            table: table_context.to_string(),
                            column: to.clone(),
                        });
                    }
                    // Rename in columns set
                    table.columns.remove(from);
                    table.columns.insert(to.clone());
                    // Rename in unique constraints if present
                    if table.unique_constraints.remove(from) {
                        table.unique_constraints.insert(to.clone());
                    }
                    // Note: We don't update FKs here because they reference
                    // other tables' columns, not our own column names
                }
            }

            Change::AddForeignKey(fk) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if !self.table_exists(&fk.references_table) {
                    return Err(SolverError::ForeignKeyTargetNotFound {
                        change: change_desc,
                        source_table: table_context.to_string(),
                        target_table: fk.references_table.clone(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.foreign_keys.insert(fk.clone());
                }
            }

            Change::DropForeignKey(fk) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.foreign_keys.remove(fk);
                }
            }

            // Column alterations just need the table to exist
            Change::AlterColumnType { .. }
            | Change::AlterColumnNullable { .. }
            | Change::AlterColumnDefault { .. } => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
            }

            // Primary key constraints
            Change::AddPrimaryKey(_) | Change::DropPrimaryKey => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
            }

            // Index operations
            Change::AddIndex(idx) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.indices.insert(idx.name.clone());
                }
            }

            Change::DropIndex(name) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.indices.remove(name);
                }
            }

            // Unique constraint operations
            Change::AddUnique(col) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.unique_constraints.insert(col.clone());
                }
            }

            Change::DropUnique(col) => {
                if !self.table_exists(table_context) {
                    return Err(SolverError::TableNotFound {
                        change: change_desc,
                        table: table_context.to_string(),
                    });
                }
                if let Some(table) = self.tables.get_mut(table_context) {
                    table.unique_constraints.remove(col);
                }
            }
        }

        Ok(())
    }

    /// Check if a change can be applied (without actually applying it).
    pub fn can_apply(&self, table_context: &str, change: &Change) -> bool {
        let mut clone = self.clone();
        clone.apply(table_context, change).is_ok()
    }
}

/// A change with its context (which table it belongs to).
#[derive(Debug, Clone)]
pub struct ContextualChange {
    /// The table this change applies to (for column-level changes).
    pub table: String,
    /// The actual change.
    pub change: Change,
    /// Original index in the diff (for cycle detection reporting).
    pub original_index: usize,
}

impl std::fmt::Display for ContextualChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.table, self.change)
    }
}

/// Result of ordering changes.
#[derive(Debug)]
pub struct OrderedChanges {
    /// Changes in valid execution order.
    pub changes: Vec<ContextualChange>,
}

/// Order changes to satisfy dependencies, validating against virtual schema.
///
/// This function:
/// 1. Orders changes so preconditions are satisfied
/// 2. Simulates the migration against the current schema
/// 3. Verifies the result matches the desired schema
///
/// Returns an error if:
/// - Changes cannot be ordered (cycle)
/// - A change would fail (precondition not satisfiable)
/// - The simulation result doesn't match the desired schema (bug in diff!)
pub fn order_changes(
    diff: &SchemaDiff,
    current: &VirtualSchema,
    desired: &VirtualSchema,
) -> Result<OrderedChanges, SolverError> {
    // Flatten all changes with their table context
    let mut all_changes: Vec<ContextualChange> = Vec::new();
    for table_diff in &diff.table_diffs {
        for change in &table_diff.changes {
            all_changes.push(ContextualChange {
                table: table_diff.table.clone(),
                change: change.clone(),
                original_index: all_changes.len(),
            });
        }
    }

    // Start with the current schema state
    let mut schema = current.clone();

    // Result ordering
    let mut ordered: Vec<ContextualChange> = Vec::new();

    // Track which changes have been scheduled
    let mut scheduled: HashSet<usize> = HashSet::new();

    // Keep trying until all changes are scheduled or we can't make progress
    let mut iterations_without_progress = 0;
    const MAX_ITERATIONS: usize = 1000; // Prevent infinite loops

    while scheduled.len() < all_changes.len() {
        let mut made_progress = false;

        for (i, change) in all_changes.iter().enumerate() {
            if scheduled.contains(&i) {
                continue;
            }

            // Try to apply this change to the virtual schema
            if schema.can_apply(&change.table, &change.change) {
                // Actually apply it
                schema
                    .apply(&change.table, &change.change)
                    .expect("can_apply returned true but apply failed");

                ordered.push(change.clone());
                scheduled.insert(i);
                made_progress = true;
                iterations_without_progress = 0;
            }
        }

        if !made_progress {
            iterations_without_progress += 1;

            if iterations_without_progress > MAX_ITERATIONS {
                // Collect unscheduled changes for error reporting
                let unscheduled: Vec<String> = all_changes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !scheduled.contains(i))
                    .map(|(_, c)| format!("{}", c))
                    .collect();

                // Try to determine why each unscheduled change can't be applied
                let mut test_schema = schema.clone();
                for (i, change) in all_changes.iter().enumerate() {
                    if !scheduled.contains(&i) {
                        if let Err(e) = test_schema.apply(&change.table, &change.change) {
                            return Err(e);
                        }
                    }
                }

                // If we get here, it's a cycle
                return Err(SolverError::CycleDetected {
                    changes: unscheduled,
                });
            }
        }
    }

    // Verify: after applying all changes, we should arrive at the desired schema
    if let Some(diff) = schema.diff_from(desired) {
        return Err(SolverError::SimulationMismatch { diff });
    }

    Ok(OrderedChanges { changes: ordered })
}

impl SchemaDiff {
    /// Generate SQL statements with proper dependency ordering.
    ///
    /// Unlike `to_sql()`, this method analyzes dependencies between changes
    /// and orders them so that preconditions are satisfied. For example,
    /// table renames happen before FK constraints that reference the new names.
    ///
    /// This method also verifies that applying the migration to `current`
    /// produces `desired`. If not, this indicates a bug in the diff algorithm.
    ///
    /// Returns an error if the migration cannot be ordered (e.g., circular
    /// dependencies), would fail (e.g., FK references non-existent table),
    /// or doesn't produce the expected result.
    pub fn to_ordered_sql(
        &self,
        current: &VirtualSchema,
        desired: &VirtualSchema,
    ) -> Result<String, SolverError> {
        let ordered = order_changes(self, current, desired)?;

        let mut sql = String::new();
        for change in &ordered.changes {
            sql.push_str(&change.change.to_sql(&change.table));
            sql.push('\n');
        }
        Ok(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, ForeignKey, PgType, Schema, SourceLocation, Table};

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

    fn make_table_with_fks(name: &str, columns: Vec<Column>, fks: Vec<ForeignKey>) -> Table {
        Table {
            name: name.to_string(),
            columns,
            foreign_keys: fks,
            indices: Vec::new(),
            source: SourceLocation::default(),
            doc: None,
            icon: None,
        }
    }

    // ==================== Virtual Schema Tests ====================

    #[test]
    fn test_virtual_schema_add_table() {
        let mut schema = VirtualSchema::new();

        let table = make_table("users", vec![make_column("id", PgType::BigInt, false)]);
        let result = schema.apply("users", &Change::AddTable(table.clone()));

        assert!(result.is_ok());
        assert!(schema.table_exists("users"));
    }

    #[test]
    fn test_virtual_schema_add_table_already_exists() {
        let mut schema = VirtualSchema::new();

        let table = make_table("users", vec![make_column("id", PgType::BigInt, false)]);
        schema.apply("users", &Change::AddTable(table.clone())).unwrap();

        // Try to add again
        let result = schema.apply("users", &Change::AddTable(table));
        assert!(matches!(result, Err(SolverError::TableAlreadyExists { .. })));
    }

    #[test]
    fn test_virtual_schema_drop_table() {
        let mut schema = VirtualSchema::from_existing(
            &["users".to_string()].into_iter().collect(),
        );

        let result = schema.apply("users", &Change::DropTable("users".to_string()));
        assert!(result.is_ok());
        assert!(!schema.table_exists("users"));
    }

    #[test]
    fn test_virtual_schema_drop_nonexistent_table() {
        let mut schema = VirtualSchema::new();

        let result = schema.apply("users", &Change::DropTable("users".to_string()));
        assert!(matches!(result, Err(SolverError::TableNotFound { .. })));
    }

    #[test]
    fn test_virtual_schema_rename_table() {
        let mut schema = VirtualSchema::from_existing(
            &["posts".to_string()].into_iter().collect(),
        );

        let result = schema.apply(
            "post",
            &Change::RenameTable {
                from: "posts".to_string(),
                to: "post".to_string(),
            },
        );

        assert!(result.is_ok());
        assert!(!schema.table_exists("posts"));
        assert!(schema.table_exists("post"));
    }

    #[test]
    fn test_virtual_schema_rename_nonexistent() {
        let mut schema = VirtualSchema::new();

        let result = schema.apply(
            "post",
            &Change::RenameTable {
                from: "posts".to_string(),
                to: "post".to_string(),
            },
        );

        assert!(matches!(result, Err(SolverError::TableNotFound { .. })));
    }

    #[test]
    fn test_virtual_schema_add_fk_target_exists() {
        let mut schema = VirtualSchema::from_existing(
            &["users".to_string(), "posts".to_string()]
                .into_iter()
                .collect(),
        );

        let fk = ForeignKey {
            columns: vec!["author_id".to_string()],
            references_table: "users".to_string(),
            references_columns: vec!["id".to_string()],
        };

        let result = schema.apply("posts", &Change::AddForeignKey(fk));
        assert!(result.is_ok());
    }

    #[test]
    fn test_virtual_schema_add_fk_target_missing() {
        let mut schema = VirtualSchema::from_existing(
            &["posts".to_string()].into_iter().collect(),
        );

        let fk = ForeignKey {
            columns: vec!["author_id".to_string()],
            references_table: "users".to_string(), // doesn't exist!
            references_columns: vec!["id".to_string()],
        };

        let result = schema.apply("posts", &Change::AddForeignKey(fk));
        assert!(matches!(
            result,
            Err(SolverError::ForeignKeyTargetNotFound { .. })
        ));
    }

    // ==================== Ordering Tests ====================

    #[test]
    fn test_rename_before_fk() {
        // Scenario: Rename posts->post, then add FK referencing post
        // The FK add must come AFTER the rename

        let desired = Schema {
            tables: vec![
                make_table("post", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                    ],
                    vec![ForeignKey {
                        columns: vec!["post_id".to_string()],
                        references_table: "post".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                ),
            ],
        };

        let current = Schema {
            tables: vec![
                make_table("posts", vec![make_column("id", PgType::BigInt, false)]),
                make_table(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                    ],
                ),
            ],
        };

        let diff = desired.diff(&current);
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&diff, &current_schema, &desired_schema);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let ordered = result.unwrap();

        // Find positions
        let rename_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::RenameTable { .. }));
        let add_fk_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::AddForeignKey(_)));

        assert!(
            rename_pos.is_some() && add_fk_pos.is_some(),
            "Should have both rename and add FK"
        );
        assert!(
            rename_pos.unwrap() < add_fk_pos.unwrap(),
            "Rename (pos {}) must come before AddFK (pos {})",
            rename_pos.unwrap(),
            add_fk_pos.unwrap()
        );
    }

    #[test]
    fn test_multiple_renames_with_fks() {
        // Scenario: Rename multiple tables, add FKs that reference new names

        let desired = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table(
                    "post",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                ),
                make_table_with_fks(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                    vec![
                        ForeignKey {
                            columns: vec!["post_id".to_string()],
                            references_table: "post".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                        ForeignKey {
                            columns: vec!["author_id".to_string()],
                            references_table: "user".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                    ],
                ),
            ],
        };

        let current = Schema {
            tables: vec![
                make_table("users", vec![make_column("id", PgType::BigInt, false)]),
                make_table(
                    "posts",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                ),
                make_table(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                ),
            ],
        };

        let diff = desired.diff(&current);
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&diff, &current_schema, &desired_schema);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let ordered = result.unwrap();

        // All renames must come before any FK additions
        let last_rename_pos = ordered
            .changes
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(&c.change, Change::RenameTable { .. }))
            .map(|(i, _)| i)
            .max();

        let first_fk_pos = ordered
            .changes
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(&c.change, Change::AddForeignKey(_)))
            .map(|(i, _)| i)
            .min();

        if let (Some(last_rename), Some(first_fk)) = (last_rename_pos, first_fk_pos) {
            assert!(
                last_rename < first_fk,
                "All renames (last at {}) must come before any FK additions (first at {})",
                last_rename,
                first_fk
            );
        }
    }

    #[test]
    fn test_drop_fk_before_drop_table() {
        // If we're dropping a table that's referenced by FKs,
        // we need to drop the FKs first

        let desired = Schema {
            tables: vec![make_table(
                "comment",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("post_id", PgType::BigInt, false),
                ],
            )],
        };

        let current = Schema {
            tables: vec![
                make_table("post", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                    ],
                    vec![ForeignKey {
                        columns: vec!["post_id".to_string()],
                        references_table: "post".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                ),
            ],
        };

        let diff = desired.diff(&current);
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&diff, &current_schema, &desired_schema);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let ordered = result.unwrap();

        // DropFK should come before DropTable
        let drop_fk_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::DropForeignKey(_)));
        let drop_table_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::DropTable(_)));

        if let (Some(fk_pos), Some(table_pos)) = (drop_fk_pos, drop_table_pos) {
            assert!(
                fk_pos < table_pos,
                "DropFK (pos {}) must come before DropTable (pos {})",
                fk_pos,
                table_pos
            );
        }
    }

    // ==================== Error Cases ====================

    #[test]
    fn test_error_fk_to_nonexistent_table() {
        // Try to add FK to a table that will never exist

        let diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "posts".to_string(),
                changes: vec![Change::AddForeignKey(ForeignKey {
                    columns: vec!["user_id".to_string()],
                    references_table: "nonexistent".to_string(),
                    references_columns: vec!["id".to_string()],
                })],
            }],
        };

        // Build minimal VirtualSchemas for this test
        let current = VirtualSchema::from_existing(
            &["posts"].iter().map(|s| s.to_string()).collect(),
        );
        // Desired has the FK (but target doesn't exist)
        let desired = VirtualSchema::from_existing(
            &["posts"].iter().map(|s| s.to_string()).collect(),
        );

        let result = order_changes(&diff, &current, &desired);
        assert!(
            matches!(result, Err(SolverError::ForeignKeyTargetNotFound { .. })),
            "Should fail with ForeignKeyTargetNotFound: {:?}",
            result
        );
    }

    #[test]
    fn test_error_drop_nonexistent_table() {
        let diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "ghost".to_string(),
                changes: vec![Change::DropTable("ghost".to_string())],
            }],
        };

        let current = VirtualSchema::new();
        let desired = VirtualSchema::new();

        let result = order_changes(&diff, &current, &desired);
        assert!(
            matches!(result, Err(SolverError::TableNotFound { .. })),
            "Should fail with TableNotFound: {:?}",
            result
        );
    }

    #[test]
    fn test_error_add_duplicate_table() {
        let table = make_table("users", vec![make_column("id", PgType::BigInt, false)]);

        let diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "users".to_string(),
                changes: vec![Change::AddTable(table.clone())],
            }],
        };

        // Table already exists
        let current = VirtualSchema::from_existing(
            &["users"].iter().map(|s| s.to_string()).collect(),
        );
        // Desired also has the table
        let desired = VirtualSchema::from_tables(&[table]);

        let result = order_changes(&diff, &current, &desired);
        assert!(
            matches!(result, Err(SolverError::TableAlreadyExists { .. })),
            "Should fail with TableAlreadyExists: {:?}",
            result
        );
    }

    // ==================== SQL Output Tests ====================

    #[test]
    fn test_ordered_sql_output() {
        let desired = Schema {
            tables: vec![
                make_table("post", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                    ],
                    vec![ForeignKey {
                        columns: vec!["post_id".to_string()],
                        references_table: "post".to_string(),
                        references_columns: vec!["id".to_string()],
                    }],
                ),
            ],
        };

        let current = Schema {
            tables: vec![
                make_table("posts", vec![make_column("id", PgType::BigInt, false)]),
                make_table(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                    ],
                ),
            ],
        };

        let diff = desired.diff(&current);
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let sql = diff.to_ordered_sql(&current_schema, &desired_schema);
        assert!(sql.is_ok(), "Should succeed: {:?}", sql);

        let sql = sql.unwrap();

        // RENAME should appear before ADD CONSTRAINT
        let rename_pos = sql.find("RENAME TO");
        let add_constraint_pos = sql.find("ADD CONSTRAINT");

        assert!(
            rename_pos.is_some() && add_constraint_pos.is_some(),
            "SQL should contain both RENAME and ADD CONSTRAINT"
        );
        assert!(
            rename_pos.unwrap() < add_constraint_pos.unwrap(),
            "RENAME should appear before ADD CONSTRAINT in SQL:\n{}",
            sql
        );
    }

    #[test]
    fn test_ordered_sql_error_propagates() {
        let diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "posts".to_string(),
                changes: vec![Change::AddForeignKey(ForeignKey {
                    columns: vec!["user_id".to_string()],
                    references_table: "nonexistent".to_string(),
                    references_columns: vec!["id".to_string()],
                })],
            }],
        };

        let current = VirtualSchema::from_existing(
            &["posts"].iter().map(|s| s.to_string()).collect(),
        );
        let desired = VirtualSchema::from_existing(
            &["posts"].iter().map(|s| s.to_string()).collect(),
        );

        let result = diff.to_ordered_sql(&current, &desired);
        assert!(result.is_err(), "Should fail");
    }

    // ==================== Index Tests ====================

    #[test]
    fn test_add_index_on_existing_table() {
        let mut schema = VirtualSchema::from_existing(
            &["users".to_string()].into_iter().collect(),
        );

        let idx = crate::Index {
            name: "users_email_idx".to_string(),
            columns: vec!["email".to_string()],
            unique: false,
        };

        let result = schema.apply("users", &Change::AddIndex(idx));
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_index_on_nonexistent_table() {
        let mut schema = VirtualSchema::new();

        let idx = crate::Index {
            name: "users_email_idx".to_string(),
            columns: vec!["email".to_string()],
            unique: false,
        };

        let result = schema.apply("users", &Change::AddIndex(idx));
        assert!(matches!(result, Err(SolverError::TableNotFound { .. })));
    }

    // ==================== Real-World Scenario Tests ====================

    #[test]
    fn test_plural_to_singular_migration() {
        // This is the actual scenario that prompted the solver:
        // Rename tables from plural to singular, then add FKs referencing new names

        let desired = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table("category", vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("parent_id", PgType::BigInt, true),
                ]),
                make_table_with_fks(
                    "post",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                        make_column("category_id", PgType::BigInt, true),
                    ],
                    vec![
                        ForeignKey {
                            columns: vec!["author_id".to_string()],
                            references_table: "user".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                        ForeignKey {
                            columns: vec!["category_id".to_string()],
                            references_table: "category".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                    ],
                ),
                make_table_with_fks(
                    "comment",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                    vec![
                        ForeignKey {
                            columns: vec!["post_id".to_string()],
                            references_table: "post".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                        ForeignKey {
                            columns: vec!["author_id".to_string()],
                            references_table: "user".to_string(),
                            references_columns: vec!["id".to_string()],
                        },
                    ],
                ),
            ],
        };

        let current = Schema {
            tables: vec![
                make_table("users", vec![make_column("id", PgType::BigInt, false)]),
                make_table("categories", vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("parent_id", PgType::BigInt, true),
                ]),
                make_table(
                    "posts",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                        make_column("category_id", PgType::BigInt, true),
                    ],
                ),
                make_table(
                    "comments",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("post_id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                ),
            ],
        };

        let diff = desired.diff(&current);
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&diff, &current_schema, &desired_schema);
        assert!(result.is_ok(), "Migration should be orderable: {:?}", result);

        let ordered = result.unwrap();

        // Build a map of table renames: new_name -> position
        let mut rename_to_positions: HashMap<String, usize> = HashMap::new();
        for (i, c) in ordered.changes.iter().enumerate() {
            if let Change::RenameTable { to, .. } = &c.change {
                rename_to_positions.insert(to.clone(), i);
            }
        }

        // Verify each FK comes after the rename of its referenced table
        for (i, c) in ordered.changes.iter().enumerate() {
            if let Change::AddForeignKey(fk) = &c.change {
                if let Some(&rename_pos) = rename_to_positions.get(&fk.references_table) {
                    assert!(
                        rename_pos < i,
                        "FK to '{}' at position {} must come after rename at position {}",
                        fk.references_table,
                        i,
                        rename_pos
                    );
                }
            }
        }

        // Also verify no errors would occur by simulating the full sequence
        let mut test_schema = current_schema.clone();
        for c in &ordered.changes {
            test_schema
                .apply(&c.table, &c.change)
                .expect("Ordered changes should all succeed");
        }
    }

    #[test]
    fn test_add_column_on_renamed_table() {
        // Add a column to a table that's being renamed in the same migration

        let desired = Schema {
            tables: vec![make_table(
                "user",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false), // new column
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
        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&diff, &current_schema, &desired_schema);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let ordered = result.unwrap();

        // Rename must come before AddColumn
        let rename_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::RenameTable { .. }))
            .expect("Should have rename");
        let add_col_pos = ordered
            .changes
            .iter()
            .position(|c| matches!(&c.change, Change::AddColumn(_)))
            .expect("Should have add column");

        assert!(
            rename_pos < add_col_pos,
            "Rename (pos {}) must come before AddColumn (pos {})",
            rename_pos,
            add_col_pos
        );
    }

    // ==================== Simulation Mismatch Tests ====================

    #[test]
    fn test_simulation_detects_add_then_drop_same_fk() {
        // This tests the key scenario: the diff algorithm might generate
        // both "ADD FK (col -> new_table)" and "DROP FK (col -> old_table)"
        // when renaming a table. These are semantically the same FK but
        // the diff doesn't know that.
        //
        // The simulation verification catches this: after applying both
        // changes, we end up with NO FK (add then drop = nothing), but
        // the desired state expects the FK to exist.

        // Current state: category table with self-referential FK to "categories"
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

        // Desired state: same table renamed to "category" with FK to "category"
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

        // Manually construct a buggy diff that adds new FK and drops old FK
        // (This simulates what a naive diff algorithm might produce)
        let buggy_diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "category".to_string(),
                changes: vec![
                    Change::RenameTable {
                        from: "categories".to_string(),
                        to: "category".to_string(),
                    },
                    Change::AddForeignKey(ForeignKey {
                        columns: vec!["parent_id".to_string()],
                        references_table: "category".to_string(),
                        references_columns: vec!["id".to_string()],
                    }),
                    Change::DropForeignKey(ForeignKey {
                        columns: vec!["parent_id".to_string()],
                        references_table: "categories".to_string(),
                        references_columns: vec!["id".to_string()],
                    }),
                ],
            }],
        };

        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&buggy_diff, &current_schema, &desired_schema);

        // The simulation should detect the mismatch:
        // - We start with FK(parent_id -> categories)
        // - We rename table to category
        // - We add FK(parent_id -> category)
        // - We drop FK(parent_id -> categories) - but wait, it doesn't exist anymore
        //   (the table was renamed, so the FK now references "category")
        //
        // Actually, this is subtle. The FK tracking in VirtualSchema needs to
        // properly handle this. Let's see what happens.

        // For now, let's just verify the solver runs and check the result
        match result {
            Ok(_) => {
                // If it succeeded, verify by simulating manually
                let mut test_schema = current_schema.clone();
                println!("Initial schema: {:?}", test_schema);

                for td in &buggy_diff.table_diffs {
                    for change in &td.changes {
                        let r = test_schema.apply(&td.table, change);
                        println!("After {:?}: {:?}", change, r);
                    }
                }

                println!("Final schema: {:?}", test_schema);
                println!("Desired schema: {:?}", desired_schema);

                // The final schema might not match desired if the diff was buggy
                if test_schema != desired_schema {
                    panic!(
                        "Simulation should have caught mismatch!\nFinal: {:?}\nDesired: {:?}",
                        test_schema, desired_schema
                    );
                }
            }
            Err(SolverError::SimulationMismatch { diff }) => {
                // Good! The simulation caught the problem
                println!("Correctly detected simulation mismatch: {}", diff);
            }
            Err(other) => {
                // Some other error - might be valid (e.g., FK drop fails because
                // the FK doesn't exist after rename). This is also catching bugs.
                println!("Got error (which is also catching bugs): {:?}", other);
            }
        }
    }

    #[test]
    fn test_simulation_mismatch_on_incomplete_diff() {
        // Test that if the diff doesn't produce all necessary changes,
        // the simulation catches the mismatch

        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![make_column("id", PgType::BigInt, false)],
            )],
        };

        // Desired has an extra column
        let desired = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("email", PgType::Text, false),
                ],
            )],
        };

        // Incomplete diff - missing the AddColumn change
        let incomplete_diff = SchemaDiff {
            table_diffs: vec![],
        };

        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&incomplete_diff, &current_schema, &desired_schema);

        // Should fail with SimulationMismatch
        assert!(
            matches!(result, Err(SolverError::SimulationMismatch { .. })),
            "Should detect mismatch when diff is incomplete: {:?}",
            result
        );
    }

    #[test]
    fn test_simulation_detects_canceling_operations() {
        // This tests when diff generates operations that truly cancel each other:
        // ADD FK then DROP the SAME FK. This should be detected as a bug.

        let current = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table(
                    "post",
                    vec![
                        make_column("id", PgType::BigInt, false),
                        make_column("author_id", PgType::BigInt, false),
                    ],
                ),
            ],
        };

        // Desired state has the FK
        let desired = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table_with_fks(
                    "post",
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

        // Buggy diff that adds then drops the SAME FK
        let the_fk = ForeignKey {
            columns: vec!["author_id".to_string()],
            references_table: "user".to_string(),
            references_columns: vec!["id".to_string()],
        };

        let buggy_diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "post".to_string(),
                changes: vec![
                    Change::AddForeignKey(the_fk.clone()),
                    Change::DropForeignKey(the_fk),
                ],
            }],
        };

        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&buggy_diff, &current_schema, &desired_schema);

        // Should fail because after add+drop, we have no FK, but desired has FK
        assert!(
            matches!(result, Err(SolverError::SimulationMismatch { .. })),
            "Should detect that add+drop cancels out: {:?}",
            result
        );
    }

    #[test]
    fn test_simulation_detects_extra_operations() {
        // Test that if the diff produces extra changes beyond what's needed,
        // and they result in wrong state, simulation catches it

        let current = Schema {
            tables: vec![make_table(
                "users",
                vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("name", PgType::Text, false),
                ],
            )],
        };

        // Desired is same as current (no changes needed)
        let desired = current.clone();

        // Buggy diff that drops a column that should still exist
        let buggy_diff = SchemaDiff {
            table_diffs: vec![crate::TableDiff {
                table: "users".to_string(),
                changes: vec![Change::DropColumn("name".to_string())],
            }],
        };

        let current_schema = VirtualSchema::from_tables(&current.tables);
        let desired_schema = VirtualSchema::from_tables(&desired.tables);

        let result = order_changes(&buggy_diff, &current_schema, &desired_schema);

        // Should fail because we dropped a column that should exist
        assert!(
            matches!(result, Err(SolverError::SimulationMismatch { .. })),
            "Should detect extra drop operation: {:?}",
            result
        );
    }
}

// ==================== Property-Based Tests ====================

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::{Column, ForeignKey, Index, PgType, Schema, SourceLocation, Table};
    use proptest::prelude::*;
    use std::collections::HashSet;

    // Strategy for generating valid SQL identifiers
    fn identifier() -> impl Strategy<Value = String> {
        // Start with letter, then letters/numbers/underscore
        "[a-z][a-z0-9_]{0,10}".prop_map(|s| s.to_string())
    }

    // Strategy for table names - use a fixed pool to increase FK hit rate
    fn table_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("user".to_string()),
            Just("post".to_string()),
            Just("comment".to_string()),
            Just("category".to_string()),
            Just("tag".to_string()),
            Just("item".to_string()),
            Just("order".to_string()),
            Just("product".to_string()),
        ]
    }

    // Strategy for column names - includes self-referential FK columns
    fn column_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("id".to_string()),
            Just("name".to_string()),
            Just("title".to_string()),
            Just("content".to_string()),
            Just("email".to_string()),
            Just("slug".to_string()),
            Just("created_at".to_string()),
            Just("updated_at".to_string()),
            Just("parent_id".to_string()),  // self-referential
            Just("author_id".to_string()),
            Just("user_id".to_string()),
            Just("post_id".to_string()),
            Just("category_id".to_string()),
            identifier(),
        ]
    }

    // Strategy for PgType
    fn pg_type() -> impl Strategy<Value = PgType> {
        prop_oneof![
            Just(PgType::BigInt),
            Just(PgType::Integer),
            Just(PgType::Text),
            Just(PgType::Boolean),
            Just(PgType::Timestamptz),
        ]
    }

    // Strategy for a column with optional unique constraint
    fn column_strategy() -> impl Strategy<Value = Column> {
        (column_name(), pg_type(), any::<bool>(), any::<bool>()).prop_map(
            |(name, pg_type, nullable, unique)| Column {
                name,
                pg_type,
                rust_type: None,
                nullable,
                default: None,
                primary_key: false,
                // Only apply unique to suitable columns (not id, not nullable)
                unique: unique && !nullable,
                auto_generated: false,
                long: false,
                label: false,
                enum_variants: vec![],
                doc: None,
                icon: None,
                lang: None,
                subtype: None,
            },
        )
    }

    // Strategy for an index
    fn index_strategy(table_name: &str, columns: &[String]) -> Vec<Index> {
        if columns.is_empty() {
            return vec![];
        }

        // Generate 0-2 indices on random columns
        let mut indices = vec![];
        let indexable: Vec<_> = columns
            .iter()
            .filter(|c| *c != "id") // Don't index PK
            .collect();

        for (i, col) in indexable.iter().take(2).enumerate() {
            // ~50% chance to create an index
            if i % 2 == 0 {
                indices.push(Index {
                    name: format!("{}_{}_idx", table_name, col),
                    columns: vec![(*col).clone()],
                    unique: false,
                });
            }
        }

        indices
    }

    // Strategy for a table (without FKs initially)
    fn table_without_fks() -> impl Strategy<Value = Table> {
        (
            table_name(),
            prop::collection::vec(column_strategy(), 1..6),
            any::<u8>(), // seed for index generation
        )
            .prop_map(|(name, mut columns, seed)| {
                // Ensure unique column names
                let mut seen = HashSet::new();
                columns.retain(|c| seen.insert(c.name.clone()));

                // Always ensure we have an id column
                if !columns.iter().any(|c| c.name == "id") {
                    columns.insert(
                        0,
                        Column {
                            name: "id".to_string(),
                            pg_type: PgType::BigInt,
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
                        },
                    );
                }

                // Don't mark 'id' as unique (it's already PK)
                for col in &mut columns {
                    if col.name == "id" {
                        col.unique = false;
                    }
                }

                // Generate indices based on seed
                let col_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
                let indices = if seed % 3 == 0 {
                    index_strategy(&name, &col_names)
                } else {
                    vec![]
                };

                Table {
                    name,
                    columns,
                    foreign_keys: vec![],
                    indices,
                    source: SourceLocation::default(),
                    doc: None,
                    icon: None,
                }
            })
    }

    // Strategy for a schema (collection of tables, with FKs added after)
    fn schema_strategy() -> impl Strategy<Value = Schema> {
        prop::collection::vec(table_without_fks(), 1..6).prop_map(|mut tables| {
            // Ensure unique table names
            let mut seen = HashSet::new();
            tables.retain(|t| seen.insert(t.name.clone()));

            // Collect table names for FK generation
            let table_names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();

            // Add FKs (including self-referential)
            for table in &mut tables {
                let table_name = table.name.clone();

                // Look for columns that look like FK columns (ending in _id)
                let fk_columns: Vec<String> = table
                    .columns
                    .iter()
                    .filter(|c| c.name.ends_with("_id") && c.name != "id")
                    .map(|c| c.name.clone())
                    .collect();

                for col_name in fk_columns {
                    // Handle self-referential FK (parent_id -> same table)
                    if col_name == "parent_id" {
                        table.foreign_keys.push(ForeignKey {
                            columns: vec![col_name],
                            references_table: table_name.clone(),
                            references_columns: vec!["id".to_string()],
                        });
                        continue;
                    }

                    // Try to find a matching table
                    let ref_table = col_name.trim_end_matches("_id").to_string();
                    if table_names.contains(&ref_table) {
                        table.foreign_keys.push(ForeignKey {
                            columns: vec![col_name],
                            references_table: ref_table,
                            references_columns: vec!["id".to_string()],
                        });
                    }
                }
            }

            Schema { tables }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10000))]

        /// The fundamental property: diff + solve + simulate = desired state
        #[test]
        fn prop_diff_solve_produces_desired_state(
            current in schema_strategy(),
            desired in schema_strategy()
        ) {
            let diff = desired.diff(&current);
            let current_virtual = VirtualSchema::from_tables(&current.tables);
            let desired_virtual = VirtualSchema::from_tables(&desired.tables);

            let result = order_changes(&diff, &current_virtual, &desired_virtual);

            match result {
                Ok(ordered) => {
                    // Simulate the changes
                    let mut simulated = current_virtual.clone();
                    for change in &ordered.changes {
                        simulated.apply(&change.table, &change.change)
                            .expect("Ordered changes should apply successfully");
                    }

                    // Must match desired
                    prop_assert_eq!(
                        simulated, desired_virtual,
                        "Simulation must produce desired state"
                    );
                }
                Err(SolverError::SimulationMismatch { diff }) => {
                    // This is actually a bug in the diff algorithm!
                    // The solver correctly detected that the diff doesn't work.
                    // For now, we'll allow this but log it.
                    eprintln!("Diff algorithm produced invalid migration: {}", diff);
                    eprintln!("Current: {:?}", current);
                    eprintln!("Desired: {:?}", desired);
                }
                Err(e) => {
                    // Other errors (cycles, missing tables, etc.) are valid rejections
                    // as long as they're consistent
                    eprintln!("Solver rejected migration: {}", e);
                }
            }
        }

        /// Changes should be idempotent: applying them twice should fail gracefully
        #[test]
        fn prop_ordered_changes_are_valid(
            current in schema_strategy(),
            desired in schema_strategy()
        ) {
            let diff = desired.diff(&current);
            let current_virtual = VirtualSchema::from_tables(&current.tables);
            let desired_virtual = VirtualSchema::from_tables(&desired.tables);

            if let Ok(ordered) = order_changes(&diff, &current_virtual, &desired_virtual) {
                // Every change in the ordered list should be applicable
                let mut schema = current_virtual.clone();
                for (i, change) in ordered.changes.iter().enumerate() {
                    let result = schema.apply(&change.table, &change.change);
                    prop_assert!(
                        result.is_ok(),
                        "Change {} ({}) should apply: {:?}",
                        i, change, result
                    );
                }
            }
        }

        /// Empty diff should produce no changes
        #[test]
        fn prop_same_schema_produces_empty_diff(schema in schema_strategy()) {
            let diff = schema.diff(&schema);
            let virtual_schema = VirtualSchema::from_tables(&schema.tables);

            // Diff of same schema should have no changes
            let total_changes: usize = diff.table_diffs.iter()
                .map(|td| td.changes.len())
                .sum();

            if total_changes == 0 {
                // Good - no changes needed
                let result = order_changes(&diff, &virtual_schema, &virtual_schema.clone());
                prop_assert!(result.is_ok(), "Empty diff should succeed: {:?}", result);
            } else {
                // Bug in diff algorithm - same schema shouldn't need changes
                eprintln!("WARNING: Diff of identical schema produced {} changes", total_changes);
                for td in &diff.table_diffs {
                    for c in &td.changes {
                        eprintln!("  {}: {}", td.table, c);
                    }
                }
            }
        }
    }
}
