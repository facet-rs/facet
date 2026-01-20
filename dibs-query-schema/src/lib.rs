//! Facet types for the dibs query DSL schema.
//!
//! These types define the structure of `.styx` query files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation

use facet::Facet;
use std::collections::HashMap;

/// A query file - top level is a map of declaration names to declarations.
#[derive(Debug, Facet)]
pub struct QueryFile {
    #[facet(flatten)]
    pub decls: HashMap<String, Decl>,
}

/// A declaration in a query file.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum Decl {
    /// A query declaration.
    Query(Query),
}

/// A query definition.
///
/// Can be either a structured query (with `from` and `select`) or a raw SQL query
/// (with `sql` and `returns`).
#[derive(Debug, Facet)]
pub struct Query {
    /// Query parameters.
    pub params: Option<Params>,

    /// Source table to query from (for structured queries).
    pub from: Option<String>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Return only the first result.
    pub first: Option<bool>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Limit clause (number or param reference like $limit).
    pub limit: Option<String>,

    /// Offset clause (number or param reference like $offset).
    pub offset: Option<String>,

    /// Fields to select (for structured queries).
    pub select: Option<Select>,

    /// Raw SQL query string (for raw SQL queries).
    pub sql: Option<String>,

    /// Return type specification (for raw SQL queries).
    pub returns: Option<Returns>,
}

/// Return type specification for raw SQL queries.
#[derive(Debug, Facet)]
pub struct Returns {
    #[facet(flatten)]
    pub fields: HashMap<String, ParamType>,
}

/// ORDER BY clause.
#[derive(Debug, Facet)]
pub struct OrderBy {
    /// Column name -> direction ("asc" or "desc", None means asc)
    #[facet(flatten)]
    pub columns: HashMap<String, Option<String>>,
}

/// WHERE clause - filter conditions.
#[derive(Debug, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: HashMap<String, FilterValue>,
}

/// A filter value - tagged operators or bare scalars for where clauses.
///
/// Tagged operators:
/// - `@null` for IS NULL
/// - `@ilike($param)` or `@ilike("pattern")` for case-insensitive LIKE
/// - `@like`, `@gt`, `@lt` for other operators
///
/// Bare scalars (like `$handle`) are treated as equality filters via `#[facet(other)]`.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum FilterValue {
    /// NULL check (@null)
    Null,
    /// ILIKE pattern matching (@ilike($param) or @ilike("pattern"))
    Ilike(Vec<String>),
    /// LIKE pattern matching (@like($param) or @like("pattern"))
    Like(Vec<String>),
    /// Greater than (@gt($param) or @gt(value))
    Gt(Vec<String>),
    /// Less than (@lt($param) or @lt(value))
    Lt(Vec<String>),
    /// Equality - bare scalar fallback (e.g., `$handle` or `"value"`)
    #[facet(other)]
    Eq(String),
}

/// Query parameters.
#[derive(Debug, Facet)]
pub struct Params {
    #[facet(flatten)]
    pub params: HashMap<String, ParamType>,
}

/// Parameter type.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum ParamType {
    String,
    Int,
    Bool,
    Uuid,
    Decimal,
    Timestamp,
    /// Optional type: @optional(@string) -> Optional(vec![String])
    Optional(Vec<ParamType>),
}

/// SELECT clause.
#[derive(Debug, Facet)]
pub struct Select {
    #[facet(flatten)]
    pub fields: HashMap<String, Option<FieldDef>>,
}

/// A field definition - tagged values in select.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum FieldDef {
    /// A relation field (`@rel{...}`).
    Rel(Relation),
    /// A count aggregation (`@count(table_name)`).
    Count(Vec<String>),
}

/// A relation definition (nested query on related table).
#[derive(Debug, Facet)]
pub struct Relation {
    /// Optional explicit table name.
    pub from: Option<String>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Return only the first result.
    pub first: Option<bool>,

    /// Fields to select from the relation.
    pub select: Option<Select>,
}
