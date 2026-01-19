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
        }
    }
}

impl std::error::Error for SolverError {}

/// Virtual representation of a table for simulation.
#[derive(Debug, Clone)]
struct VirtualTable {
    columns: HashSet<String>,
    foreign_keys: Vec<ForeignKey>,
    indices: HashSet<String>,
    unique_constraints: HashSet<String>,
}

/// Virtual schema state for simulating migrations.
#[derive(Debug, Clone)]
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
                    foreign_keys: Vec::new(),
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
                    foreign_keys: table.foreign_keys.clone(),
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
                        foreign_keys: t.foreign_keys.clone(),
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
                    table.foreign_keys.push(fk.clone());
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
                    table.foreign_keys.retain(|f| f != fk);
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
/// Returns an error if the changes cannot be ordered (cycle) or would fail
/// (precondition not satisfiable).
pub fn order_changes(
    diff: &SchemaDiff,
    existing_tables: &HashSet<String>,
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

    // Initialize virtual schema with existing tables
    let mut schema = VirtualSchema::from_existing(existing_tables);

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

    Ok(OrderedChanges { changes: ordered })
}

impl SchemaDiff {
    /// Generate SQL statements with proper dependency ordering.
    ///
    /// Unlike `to_sql()`, this method analyzes dependencies between changes
    /// and orders them so that preconditions are satisfied. For example,
    /// table renames happen before FK constraints that reference the new names.
    ///
    /// Returns an error if the migration cannot be ordered (e.g., circular
    /// dependencies) or would fail (e.g., FK references non-existent table).
    pub fn to_ordered_sql(
        &self,
        existing_tables: &HashSet<String>,
    ) -> Result<String, SolverError> {
        let ordered = order_changes(self, existing_tables)?;

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
        let existing: HashSet<String> = ["posts", "comment"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = order_changes(&diff, &existing);
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
        let existing: HashSet<String> = ["users", "posts", "comment"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = order_changes(&diff, &existing);
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
        let existing: HashSet<String> =
            ["post", "comment"].iter().map(|s| s.to_string()).collect();

        let result = order_changes(&diff, &existing);
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

        let existing: HashSet<String> = ["posts"].iter().map(|s| s.to_string()).collect();

        let result = order_changes(&diff, &existing);
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

        let existing: HashSet<String> = HashSet::new();

        let result = order_changes(&diff, &existing);
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
                changes: vec![Change::AddTable(table)],
            }],
        };

        // Table already exists
        let existing: HashSet<String> = ["users"].iter().map(|s| s.to_string()).collect();

        let result = order_changes(&diff, &existing);
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
        let existing: HashSet<String> = ["posts", "comment"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let sql = diff.to_ordered_sql(&existing);
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

        let existing: HashSet<String> = ["posts"].iter().map(|s| s.to_string()).collect();

        let result = diff.to_ordered_sql(&existing);
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
        let existing: HashSet<String> = ["users", "categories", "posts", "comments"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = order_changes(&diff, &existing);
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
        let mut test_schema = VirtualSchema::from_existing(&existing);
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
        let existing: HashSet<String> = ["users"].iter().map(|s| s.to_string()).collect();

        let result = order_changes(&diff, &existing);
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
}
