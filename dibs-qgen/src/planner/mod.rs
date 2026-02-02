//! Query planner for JOIN resolution.
//!
//! This module handles:
//! - FK relationship resolution between tables
//! - JOIN clause generation
//! - Column aliasing to avoid collisions
//! - Result assembly mapping

mod types;

use dibs_db_schema::Schema;
use dibs_sql::{ColumnName, TableName};
pub use types::*;

use crate::{Select, SelectFields};
use indexmap::IndexMap;

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::TableNotFound { table } => write!(f, "table not found: {}", table),
            PlanError::NoForeignKey { from, to } => {
                write!(f, "no FK relationship between {} and {}", from, to)
            }
            PlanError::RelationNeedsFrom { relation } => {
                write!(f, "relation '{}' requires explicit 'from' clause", relation)
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// Query planner that resolves JOINs.
pub struct QueryPlanner<'a> {
    schema: &'a Schema,
}

impl<'a> QueryPlanner<'a> {
    pub fn new(schema: &'a Schema) -> Self {
        Self { schema }
    }

    /// Plan a query, resolving all relations to JOINs.
    pub fn plan(&self, query: &Select) -> Result<QueryPlan, PlanError> {
        let from_table_meta = query
            .from
            .as_ref()
            .ok_or_else(|| PlanError::TableNotFound {
                table: "<unknown>".to_string(),
            })?;
        let from_table = from_table_meta.value.clone();

        let mut plan = QueryPlan::new(from_table.clone());

        // Process top-level fields (columns and relations)
        if let Some(select) = &query.fields {
            self.process_select(
                select,
                &from_table,
                &plan.from_alias.clone(),
                &[],
                &mut plan,
            )?;
        }

        Ok(plan)
    }

    /// Process select fields recursively, handling nested relations.
    fn process_select(
        &self,
        select: &SelectFields,
        parent_table: &TableName,
        parent_alias: &str,
        path: &[ColumnName], // path to this relation (e.g., ["variants", "prices"])
        plan: &mut QueryPlan,
    ) -> Result<(), PlanError> {
        // Process simple columns
        for (name_meta, _field_def) in select.columns() {
            let name = &name_meta.value;
            // Build result alias: for nested relations, prefix with path
            let result_alias: ColumnName = if path.is_empty() {
                name.clone()
            } else {
                let path_str: Vec<&str> = path.iter().map(|c| c.as_str()).collect();
                format!("{}_{}", path_str.join("_"), name).into()
            };

            // Build full path for column mapping
            let mut full_path = path.to_vec();
            full_path.push(name.clone());

            plan.add_column(parent_alias, name, result_alias, full_path);
        }

        // Process relations
        for (name_meta, relation) in select.relations() {
            let name = &name_meta.value;

            // Resolve the relation table name
            let relation_table: TableName = relation
                .from
                .as_ref()
                .map(|m| m.value.clone())
                .unwrap_or_else(|| name.as_str().into());

            // Find FK relationship
            let relation_alias = plan.next_alias();
            let fk_resolution =
                self.resolve_fk(parent_table, &relation_table, &relation_alias, parent_alias)?;

            // Collect column names for the join (only direct columns, not nested relations)
            let join_select_columns: Vec<ColumnName> = relation
                .fields
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            // Build join with proper ON condition
            let mut join = fk_resolution.join_clause;
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            // Add extra conditions from relation-level WHERE clause
            if let Some(where_clause) = &relation.where_clause {
                for (col_meta, filter_value) in &where_clause.filters {
                    if let Some(condition) =
                        Self::filter_to_join_condition(&col_meta.value, filter_value)
                    {
                        join.extra_conditions.push(condition);
                    }
                }
            }

            plan.add_join(join);

            // Build path for nested fields
            let mut nested_path = path.to_vec();
            nested_path.push(name.clone());

            // Process nested columns and relations
            let mut relation_columns = IndexMap::new();
            let mut nested_relations = IndexMap::new();

            if let Some(nested_select) = &relation.fields {
                self.process_select_nested(
                    nested_select,
                    &relation_table,
                    &relation_alias,
                    &nested_path,
                    plan,
                    &mut relation_columns,
                    &mut nested_relations,
                )?;
            }

            // For Vec relations (first=false), store parent key for grouping
            let parent_key_column = if relation.is_first() {
                None
            } else {
                Some(fk_resolution.parent_key_column)
            };

            plan.add_relation(
                name.clone(),
                RelationMapping {
                    name: name.clone(),
                    first: relation.is_first(),
                    columns: relation_columns,
                    parent_key_column,
                    table_alias: relation_alias,
                    nested_relations,
                },
            );
        }

        // Process count aggregations
        for (name_meta, tables) in select.counts() {
            let name = &name_meta.value;
            // Use the first table from the count annotation, or derive from parent
            let count_table: TableName = tables
                .first()
                .map(|t| t.value.clone())
                .unwrap_or_else(|| format!("{}_count", parent_table).into());
            let subquery = CountSubquery {
                result_alias: name.clone(),
                count_table,
                fk_column: format!("{}_id", parent_table).into(), // placeholder
                parent_alias: parent_alias.to_string(),
                parent_key: "id".into(), // placeholder
            };
            plan.add_count(subquery, vec![name.clone()]);
        }

        Ok(())
    }

    /// Process nested select fields (used for relations).
    fn process_select_nested(
        &self,
        select: &SelectFields,
        parent_table: &TableName,
        parent_alias: &str,
        path: &[ColumnName],
        plan: &mut QueryPlan,
        column_mappings: &mut IndexMap<ColumnName, ColumnName>,
        relation_mappings: &mut IndexMap<ColumnName, RelationMapping>,
    ) -> Result<(), PlanError> {
        // Process simple columns in nested select
        for (name_meta, _field_def) in select.columns() {
            let col_name = &name_meta.value;
            let path_str: Vec<&str> = path.iter().map(|c| c.as_str()).collect();
            let result_alias: ColumnName = format!("{}_{}", path_str.join("_"), col_name).into();

            plan.select_columns.push(SelectColumn {
                table_alias: parent_alias.to_string(),
                column: col_name.clone(),
                result_alias: result_alias.clone(),
            });
            column_mappings.insert(col_name.clone(), result_alias);
        }

        // Process relations in nested select
        for (name_meta, relation) in select.relations() {
            let name = &name_meta.value;

            let relation_table: TableName = relation
                .from
                .as_ref()
                .map(|m| m.value.clone())
                .unwrap_or_else(|| name.as_str().into());

            let relation_alias = plan.next_alias();
            let fk_resolution =
                self.resolve_fk(parent_table, &relation_table, &relation_alias, parent_alias)?;

            let join_select_columns: Vec<ColumnName> = relation
                .fields
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            let mut join = fk_resolution.join_clause;
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            // Add extra conditions from relation-level WHERE clause
            if let Some(where_clause) = &relation.where_clause {
                for (col_meta, filter_value) in &where_clause.filters {
                    if let Some(condition) =
                        Self::filter_to_join_condition(&col_meta.value, filter_value)
                    {
                        join.extra_conditions.push(condition);
                    }
                }
            }

            plan.add_join(join);

            let mut nested_path = path.to_vec();
            nested_path.push(name.clone());

            let mut relation_columns = IndexMap::new();
            let mut nested_relations = IndexMap::new();

            if let Some(nested_select) = &relation.fields {
                self.process_select_nested(
                    nested_select,
                    &relation_table,
                    &relation_alias,
                    &nested_path,
                    plan,
                    &mut relation_columns,
                    &mut nested_relations,
                )?;
            }

            let parent_key_column = if relation.is_first() {
                None
            } else {
                Some(fk_resolution.parent_key_column)
            };

            relation_mappings.insert(
                name.clone(),
                RelationMapping {
                    name: name.clone(),
                    first: relation.is_first(),
                    columns: relation_columns,
                    parent_key_column,
                    table_alias: relation_alias,
                    nested_relations,
                },
            );
        }

        Ok(())
    }

    /// Convert a filter value to a JoinCondition for relation-level WHERE.
    /// Only supports simple equality filters (bare scalars).
    fn filter_to_join_condition(
        column: &ColumnName,
        filter_value: &crate::FilterValue,
    ) -> Option<JoinCondition> {
        use crate::{FilterArg, FilterValue};

        match filter_value {
            FilterValue::EqBare(Some(meta)) => {
                let value = match FilterArg::parse(&meta.value) {
                    FilterArg::Variable(name) => JoinConditionValue::Param(name.into()),
                    FilterArg::Literal(lit) => JoinConditionValue::Literal(lit),
                };
                Some(JoinCondition {
                    column: column.clone(),
                    value,
                })
            }
            // TODO: Support other filter types if needed
            _ => None,
        }
    }

    /// Resolve FK relationship between two tables.
    /// Returns the FkResolution with JoinClause, direction, and parent key column.
    fn resolve_fk(
        &self,
        from_table: &TableName,
        to_table: &TableName,
        alias: &str,
        parent_alias: &str,
    ) -> Result<FkResolution, PlanError> {
        let to_table_info =
            self.schema
                .tables
                .get(to_table.as_str())
                .ok_or_else(|| PlanError::TableNotFound {
                    table: to_table.to_string(),
                })?;

        // Check if to_table has FK pointing to from_table (reverse/has-many)
        for fk in &to_table_info.foreign_keys {
            if fk.references_table == from_table.as_str() {
                // Found: to_table.fk_col -> from_table.ref_col
                // JOIN to_table ON from_table.ref_col = to_table.fk_col
                let parent_key_column: ColumnName = fk.references_columns[0].clone().into();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.clone(),
                        alias: alias.to_string(),
                        on_condition: (
                            format!("{}.{}", parent_alias, parent_key_column),
                            format!("{}.{}", alias, fk.columns[0]),
                        ),
                        extra_conditions: vec![],
                        first: false,
                        select_columns: vec![],
                    },
                    direction: FkDirection::Reverse,
                    parent_key_column,
                });
            }
        }

        // Check if from_table has FK pointing to to_table (forward/belongs-to)
        let from_table_info = self.schema.tables.get(from_table.as_str()).ok_or_else(|| {
            PlanError::TableNotFound {
                table: from_table.to_string(),
            }
        })?;

        for fk in &from_table_info.foreign_keys {
            if fk.references_table == to_table.as_str() {
                // Found: from_table.fk_col -> to_table.ref_col
                // JOIN to_table ON from_table.fk_col = to_table.ref_col
                // For forward (belongs-to), parent key is the FK column in from_table
                let parent_key_column: ColumnName = fk.columns[0].clone().into();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.clone(),
                        alias: alias.to_string(),
                        on_condition: (
                            format!("{}.{}", parent_alias, parent_key_column),
                            format!("{}.{}", alias, fk.references_columns[0]),
                        ),
                        extra_conditions: vec![],
                        first: false,
                        select_columns: vec![],
                    },
                    direction: FkDirection::Forward,
                    parent_key_column,
                });
            }
        }

        Err(PlanError::NoForeignKey {
            from: from_table.to_string(),
            to: to_table.to_string(),
        })
    }
}

impl QueryPlan {
    /// Generate SQL SELECT clause.
    pub fn select_sql(&self) -> String {
        let mut parts: Vec<String> = self
            .select_columns
            .iter()
            .map(|col| {
                format!(
                    "\"{}\".\"{}\" AS \"{}\"",
                    col.table_alias, col.column, col.result_alias
                )
            })
            .collect();

        // Add COUNT subqueries
        for count in &self.count_subqueries {
            parts.push(format!(
                "(SELECT COUNT(*) FROM \"{}\" WHERE \"{}\" = \"{}\".\"{}\" ) AS \"{}\"",
                count.count_table,
                count.fk_column,
                count.parent_alias,
                count.parent_key,
                count.result_alias
            ));
        }

        parts.join(", ")
    }

    /// Generate SQL FROM clause with JOINs.
    pub fn from_sql(&self) -> String {
        self.from_sql_with_params(&mut Vec::new(), &mut 1)
    }

    /// Generate SQL FROM clause with JOINs, tracking parameter order.
    ///
    /// Returns the SQL and appends any parameter names to `param_order`.
    /// `param_idx` is updated to track the next $N placeholder.
    pub fn from_sql_with_params(
        &self,
        param_order: &mut Vec<dibs_sql::ParamName>,
        param_idx: &mut usize,
    ) -> String {
        use types::JoinConditionValue;

        let mut sql = format!("\"{}\" AS \"{}\"", self.from_table, self.from_alias);

        for join in &self.joins {
            // Regular JOIN
            let join_type = match join.join_type {
                JoinType::Left => "LEFT JOIN",
                JoinType::Inner => "INNER JOIN",
            };

            // Build ON clause with base condition
            let mut on_parts = vec![format!("{} = {}", join.on_condition.0, join.on_condition.1)];

            // Add extra conditions from relation-level WHERE
            for cond in &join.extra_conditions {
                let value_sql = match &cond.value {
                    JoinConditionValue::Literal(lit) => format!("'{}'", lit),
                    JoinConditionValue::Param(param_name) => {
                        param_order.push(param_name.clone());
                        let placeholder = format!("${}", *param_idx);
                        *param_idx += 1;
                        placeholder
                    }
                };
                on_parts.push(format!(
                    "\"{}\".\"{}\" = {}",
                    join.alias, cond.column, value_sql
                ));
            }

            sql.push_str(&format!(
                " {} \"{}\" AS \"{}\" ON {}",
                join_type,
                join.table,
                join.alias,
                on_parts.join(" AND ")
            ));
        }

        sql
    }
}
