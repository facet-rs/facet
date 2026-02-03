//! Types for query planning.

use indexmap::IndexMap;

use dibs_sql::{ColumnName, TableName};

/// A planned query with JOINs resolved.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// The base table
    pub from_table: TableName,
    /// Alias for the base table
    pub from_alias: String,
    /// JOIN clauses
    pub joins: Vec<JoinClause>,
    /// Column selections with their aliases
    pub select_columns: Vec<SelectColumn>,
    /// COUNT subqueries
    pub count_subqueries: Vec<CountSubquery>,
    /// Mapping from result columns to nested struct paths
    pub result_mapping: ResultMapping,
    /// Counter for generating unique table aliases
    alias_counter: usize,
}

impl QueryPlan {
    /// Create a new QueryPlan with the given base table.
    pub fn new(from_table: TableName) -> Self {
        Self {
            from_table,
            from_alias: "t0".to_string(),
            joins: Vec::new(),
            select_columns: Vec::new(),
            count_subqueries: Vec::new(),
            result_mapping: ResultMapping::default(),
            alias_counter: 1, // t0 is already used for the base table
        }
    }

    /// Generate the next unique table alias (t1, t2, ...).
    pub fn next_alias(&mut self) -> String {
        let alias = format!("t{}", self.alias_counter);
        self.alias_counter += 1;
        alias
    }

    /// Add a column to the SELECT clause.
    pub fn add_column(
        &mut self,
        table_alias: &str,
        column: &ColumnName,
        result_alias: ColumnName,
        path: Vec<ColumnName>,
    ) {
        self.select_columns.push(SelectColumn {
            table_alias: table_alias.to_string(),
            column: column.clone(),
            result_alias: result_alias.clone(),
        });
        self.result_mapping.columns.insert(result_alias, path);
    }

    /// Add a JOIN clause and return its alias.
    pub fn add_join(&mut self, join: JoinClause) -> String {
        let alias = join.alias.clone();
        self.joins.push(join);
        alias
    }

    /// Add a COUNT subquery.
    pub fn add_count(&mut self, subquery: CountSubquery, path: Vec<ColumnName>) {
        let alias = subquery.result_alias.clone();
        self.count_subqueries.push(subquery);
        self.result_mapping.columns.insert(alias, path);
    }

    /// Add a relation mapping at the top level.
    pub fn add_relation(&mut self, name: ColumnName, mapping: RelationMapping) {
        self.result_mapping.relations.insert(name, mapping);
    }
}

/// A JOIN clause in the query plan.
#[derive(Debug, Clone)]
pub struct JoinClause {
    /// JOIN type (LEFT, INNER)
    pub join_type: JoinType,
    /// Table to join
    pub table: TableName,
    /// Alias for the joined table
    pub alias: String,
    /// ON condition: (left_col, right_col)
    pub on_condition: (String, String),
    /// Additional conditions for the ON clause (from relation-level WHERE)
    pub extra_conditions: Vec<JoinCondition>,
    /// Whether this is a first:true relation (affects LATERAL generation)
    pub first: bool,
    /// Columns selected from this join (needed for LATERAL subquery)
    pub select_columns: Vec<ColumnName>,
}

/// An extra condition in a JOIN ON clause.
#[derive(Debug, Clone)]
pub struct JoinCondition {
    /// The column being filtered
    pub column: ColumnName,
    /// The value or parameter (e.g., "'en'" or "$1")
    pub value: JoinConditionValue,
}

/// Value for a JOIN condition.
#[derive(Debug, Clone)]
pub enum JoinConditionValue {
    /// A literal string value (will be quoted)
    Literal(String),
    /// A parameter reference (e.g., $1)
    Param(dibs_sql::ParamName),
}

/// JOIN type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Left,
    Inner,
}

/// A column in the SELECT clause.
#[derive(Debug, Clone)]
pub struct SelectColumn {
    /// Table alias
    pub table_alias: String,
    /// Column name
    pub column: ColumnName,
    /// Result alias (for AS clause)
    pub result_alias: ColumnName,
}

/// A COUNT subquery in the SELECT clause.
#[derive(Debug, Clone)]
pub struct CountSubquery {
    /// Result alias (e.g., "variant_count")
    pub result_alias: ColumnName,
    /// Table to count from (e.g., "product_variant")
    pub count_table: TableName,
    /// FK column in the count table (e.g., "product_id")
    pub fk_column: ColumnName,
    /// Parent table alias (e.g., "t0")
    pub parent_alias: String,
    /// Parent key column (e.g., "id")
    pub parent_key: ColumnName,
}

/// Mapping of result columns to nested struct paths.
#[derive(Debug, Clone, Default)]
pub struct ResultMapping {
    /// Map from result alias to struct path (e.g., "t_title" -> ["translation", "title"])
    pub columns: IndexMap<ColumnName, Vec<ColumnName>>,
    /// Nested relations and their mappings
    pub relations: IndexMap<ColumnName, RelationMapping>,
}

/// Mapping for a single relation.
#[derive(Debug, Clone)]
pub struct RelationMapping {
    /// Relation name
    pub name: ColumnName,
    /// Whether it's first (`Option<T>`) or many (`Vec<T>`)
    pub first: bool,
    /// Column mappings within this relation
    pub columns: IndexMap<ColumnName, ColumnName>,
    /// Parent's primary key column name (used for grouping Vec relations)
    pub parent_key_column: Option<ColumnName>,
    /// Table alias for this relation (e.g., "t1", "t2")
    pub table_alias: String,
    /// Nested relations within this relation
    pub nested_relations: IndexMap<ColumnName, RelationMapping>,
}

/// Error type for query planning.
#[derive(Debug)]
pub enum PlanError {
    /// Table not found in schema
    TableNotFound { table: String },
    /// No FK relationship found between tables
    NoForeignKey { from: String, to: String },
    /// Relation requires explicit 'from' clause
    #[allow(dead_code)]
    RelationNeedsFrom { relation: String },
}

/// Direction of FK relationship.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FkDirection {
    /// FK is in from_table pointing to to_table (belongs-to)
    Forward,
    /// FK is in to_table pointing to from_table (has-many)
    Reverse,
}

/// Result of FK resolution.
#[derive(Debug, Clone)]
pub struct FkResolution {
    /// The JOIN clause
    pub join_clause: JoinClause,
    /// Direction of the relationship (reserved for future use)
    pub _direction: FkDirection,
    /// Parent's primary key column (used for grouping Vec relations)
    pub parent_key_column: ColumnName,
}
