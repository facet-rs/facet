//! Migration solver - orders schema changes to satisfy dependencies.
//!
//! When generating migration SQL, operation order matters. For example:
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
//!
//! The solver analyzes dependencies between changes and produces a valid ordering.

use crate::{Change, SchemaDiff};
use std::collections::HashSet;

/// A change with its context (which table it belongs to).
#[derive(Debug, Clone)]
pub struct ContextualChange {
    /// The table this change applies to (for column-level changes).
    pub table: String,
    /// The actual change.
    pub change: Change,
}

/// What must be true for a change to execute.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Precondition {
    /// A table must exist with this name.
    TableExists(String),
    /// A table must NOT exist with this name (for creating new tables).
    TableNotExists(String),
}

/// What becomes true after a change executes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    /// A table now exists with this name.
    TableExists(String),
    /// A table no longer exists with this name.
    TableNotExists(String),
}

impl ContextualChange {
    /// Get the preconditions for this change.
    pub fn preconditions(&self) -> Vec<Precondition> {
        match &self.change {
            Change::AddTable(t) => {
                // Table must not already exist
                let mut preconds = vec![Precondition::TableNotExists(t.name.clone())];
                // Any FK references must exist
                for fk in &t.foreign_keys {
                    preconds.push(Precondition::TableExists(fk.references_table.clone()));
                }
                preconds
            }
            Change::DropTable(name) => {
                // Table must exist
                vec![Precondition::TableExists(name.clone())]
            }
            Change::RenameTable { from, .. } => {
                // Source table must exist
                vec![Precondition::TableExists(from.clone())]
            }
            Change::AddForeignKey(fk) => {
                // Both the table we're adding to and the referenced table must exist
                vec![
                    Precondition::TableExists(self.table.clone()),
                    Precondition::TableExists(fk.references_table.clone()),
                ]
            }
            Change::DropForeignKey(_) => {
                // Table must exist
                vec![Precondition::TableExists(self.table.clone())]
            }
            // Column-level changes just need the table to exist
            Change::AddColumn(_)
            | Change::DropColumn(_)
            | Change::AlterColumnType { .. }
            | Change::AlterColumnNullable { .. }
            | Change::AlterColumnDefault { .. }
            | Change::AddPrimaryKey(_)
            | Change::DropPrimaryKey
            | Change::AddIndex(_)
            | Change::DropIndex(_)
            | Change::AddUnique(_)
            | Change::DropUnique(_) => {
                vec![Precondition::TableExists(self.table.clone())]
            }
        }
    }

    /// Get the effects of this change.
    pub fn effects(&self) -> Vec<Effect> {
        match &self.change {
            Change::AddTable(t) => {
                vec![Effect::TableExists(t.name.clone())]
            }
            Change::DropTable(name) => {
                vec![Effect::TableNotExists(name.clone())]
            }
            Change::RenameTable { from, to } => {
                // Old name gone, new name exists
                vec![
                    Effect::TableNotExists(from.clone()),
                    Effect::TableExists(to.clone()),
                ]
            }
            // Other changes don't affect table existence
            _ => vec![],
        }
    }
}

/// Order changes to satisfy dependencies.
///
/// Returns changes in an order where each change's preconditions are satisfied
/// by the initial state plus the effects of all preceding changes.
pub fn order_changes(
    diff: &SchemaDiff,
    existing_tables: &HashSet<String>,
) -> Vec<ContextualChange> {
    // Flatten all changes with their table context
    let mut all_changes: Vec<ContextualChange> = Vec::new();
    for table_diff in &diff.table_diffs {
        for change in &table_diff.changes {
            all_changes.push(ContextualChange {
                table: table_diff.table.clone(),
                change: change.clone(),
            });
        }
    }

    // Track current state (what tables exist)
    let mut tables_exist: HashSet<String> = existing_tables.clone();

    // Result ordering
    let mut ordered: Vec<ContextualChange> = Vec::new();

    // Track which changes have been scheduled
    let mut scheduled: HashSet<usize> = HashSet::new();

    // Keep trying until all changes are scheduled or we can't make progress
    let mut made_progress = true;
    while made_progress && scheduled.len() < all_changes.len() {
        made_progress = false;

        for (i, change) in all_changes.iter().enumerate() {
            if scheduled.contains(&i) {
                continue;
            }

            // Check if all preconditions are satisfied
            let preconds_satisfied = change.preconditions().iter().all(|p| match p {
                Precondition::TableExists(name) => tables_exist.contains(name),
                Precondition::TableNotExists(name) => !tables_exist.contains(name),
            });

            if preconds_satisfied {
                // Schedule this change
                ordered.push(change.clone());
                scheduled.insert(i);
                made_progress = true;

                // Apply effects to state
                for effect in change.effects() {
                    match effect {
                        Effect::TableExists(name) => {
                            tables_exist.insert(name);
                        }
                        Effect::TableNotExists(name) => {
                            tables_exist.remove(&name);
                        }
                    }
                }
            }
        }
    }

    // If we couldn't schedule everything, there's a cycle or missing dependency
    if scheduled.len() < all_changes.len() {
        // For now, append remaining changes in original order with a warning
        // TODO: Better error handling - detect cycles, report unsatisfiable constraints
        for (i, change) in all_changes.iter().enumerate() {
            if !scheduled.contains(&i) {
                ordered.push(change.clone());
            }
        }
    }

    ordered
}

impl SchemaDiff {
    /// Generate SQL statements with proper dependency ordering.
    ///
    /// Unlike `to_sql()`, this method analyzes dependencies between changes
    /// and orders them so that preconditions are satisfied. For example,
    /// table renames happen before FK constraints that reference the new names.
    pub fn to_ordered_sql(&self, existing_tables: &HashSet<String>) -> String {
        let ordered = order_changes(self, existing_tables);

        let mut sql = String::new();
        for change in &ordered {
            sql.push_str(&change.change.to_sql(&change.table));
            sql.push('\n');
        }
        sql
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

    #[test]
    fn test_rename_before_fk() {
        // Scenario: Rename posts->post, then add FK referencing post
        // The FK add must come AFTER the rename

        let desired = Schema {
            tables: vec![
                make_table(
                    "post",
                    vec![make_column("id", PgType::BigInt, false)],
                ),
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
                make_table(
                    "posts",
                    vec![make_column("id", PgType::BigInt, false)],
                ),
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

        // Current tables in DB
        let existing: HashSet<String> = ["posts", "comment"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let ordered = order_changes(&diff, &existing);

        // Find positions
        let rename_pos = ordered
            .iter()
            .position(|c| matches!(&c.change, Change::RenameTable { .. }));
        let add_fk_pos = ordered
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
        // All renames must happen before FKs that reference the new names

        let desired = Schema {
            tables: vec![
                make_table("user", vec![make_column("id", PgType::BigInt, false)]),
                make_table("post", vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("author_id", PgType::BigInt, false),
                ]),
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
                make_table("posts", vec![
                    make_column("id", PgType::BigInt, false),
                    make_column("author_id", PgType::BigInt, false),
                ]),
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

        let ordered = order_changes(&diff, &existing);

        // All renames must come before any FK additions
        let last_rename_pos = ordered
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(&c.change, Change::RenameTable { .. }))
            .map(|(i, _)| i)
            .max();

        let first_fk_pos = ordered
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
        // we need to drop the FKs first (or use CASCADE)
        // For now this is a placeholder for that logic

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
        let existing: HashSet<String> = ["post", "comment"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let ordered = order_changes(&diff, &existing);

        // DropFK should come before DropTable
        let drop_fk_pos = ordered
            .iter()
            .position(|c| matches!(&c.change, Change::DropForeignKey(_)));
        let drop_table_pos = ordered
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

    #[test]
    fn test_ordered_sql_output() {
        // Test the actual SQL output is in correct order
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

        // RENAME should appear before ADD CONSTRAINT
        let rename_pos = sql.find("RENAME TO");
        let add_constraint_pos = sql.find("ADD CONSTRAINT");

        assert!(
            rename_pos.is_some() && add_constraint_pos.is_some(),
            "SQL should contain both RENAME and ADD CONSTRAINT"
        );
        assert!(
            rename_pos.unwrap() < add_constraint_pos.unwrap(),
            "RENAME should appear before ADD CONSTRAINT in SQL"
        );
    }
}
