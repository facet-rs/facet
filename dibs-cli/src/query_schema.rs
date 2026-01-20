//! Query DSL schema types for Styx schema generation.
//!
//! These types are used to generate a Styx schema that describes the query DSL.
//! They mirror the runtime AST types but use Facet derives for schema generation.

use facet::Facet;

/// A file containing one or more query definitions.
///
/// Each top-level node should be a query with a `@query` tag.
#[derive(Debug, Facet)]
pub struct QueryFile {
    /// Query definitions in this file.
    #[facet(default)]
    pub queries: Vec<Query>,
}

/// A query definition.
///
/// Queries define how to fetch data from the database, including
/// filtering, ordering, pagination, and nested relations.
///
/// # Example
///
/// ```styx
/// ProductByHandle @query{
///     params{ handle @string }
///     from product
///     where{ handle $handle }
///     first true
///     select{ id, handle, status }
/// }
/// ```
#[derive(Debug, Facet)]
pub struct Query {
    /// Query parameters (inputs).
    ///
    /// Parameters are referenced in `where` clauses using `$name` syntax.
    #[facet(default)]
    pub params: Option<Params>,

    /// Source table to query from.
    pub from: String,

    /// WHERE clause conditions.
    #[facet(default, rename = "where")]
    pub where_clause: Option<WhereClause>,

    /// ORDER BY clause.
    #[facet(default)]
    pub order_by: Option<OrderByClause>,

    /// LIMIT clause (number or parameter reference).
    #[facet(default)]
    pub limit: Option<LimitValue>,

    /// OFFSET clause (number or parameter reference).
    #[facet(default)]
    pub offset: Option<LimitValue>,

    /// Whether to return only the first result (`Option<T>`) instead of all results (`Vec<T>`).
    #[facet(default)]
    pub first: Option<bool>,

    /// Fields to select from the query.
    pub select: SelectClause,

    /// Raw SQL escape hatch (heredoc).
    ///
    /// When using raw SQL, you must also specify `returns` to define the output type.
    #[facet(default)]
    pub sql: Option<String>,

    /// Return type declaration for raw SQL queries.
    #[facet(default)]
    pub returns: Option<ReturnsClause>,
}

/// Query parameters block.
///
/// Each parameter has a name and a type tag.
///
/// # Example
///
/// ```styx
/// params{ handle @string, limit @int }
/// ```
#[derive(Debug, Facet)]
pub struct Params {
    /// Individual parameter definitions.
    #[facet(flatten)]
    pub items: Vec<ParamDef>,
}

/// A single parameter definition.
///
/// The type is specified using a tag like `@string`, `@int`, `@bool`, etc.
#[derive(Debug, Facet)]
#[facet(tag)]
#[repr(u8)]
pub enum ParamDef {
    /// String parameter.
    #[facet(rename = "string")]
    String,
    /// Integer parameter.
    #[facet(rename = "int")]
    Int,
    /// Boolean parameter.
    #[facet(rename = "bool")]
    Bool,
    /// UUID parameter.
    #[facet(rename = "uuid")]
    Uuid,
    /// Decimal parameter.
    #[facet(rename = "decimal")]
    Decimal,
    /// Timestamp parameter.
    #[facet(rename = "timestamp")]
    Timestamp,
    /// Optional parameter (wraps another type).
    #[facet(rename = "optional")]
    Optional(Box<ParamDef>),
}

/// WHERE clause containing filter conditions.
///
/// Each filter specifies a column, operator, and value.
///
/// # Example
///
/// ```styx
/// where{ status "active", deleted_at @null }
/// ```
#[derive(Debug, Facet)]
pub struct WhereClause {
    /// Filter conditions.
    #[facet(flatten)]
    pub filters: Vec<FilterCondition>,
}

/// A filter condition in a WHERE clause.
#[derive(Debug, Facet)]
#[facet(tag)]
#[repr(u8)]
pub enum FilterCondition {
    /// Equality comparison: `column value` or `column $param`.
    #[facet(rename = "eq")]
    Eq { column: String, value: FilterValue },

    /// IS NULL check: `column @null`.
    #[facet(rename = "null")]
    IsNull { column: String },

    /// IS NOT NULL check: `column @not_null`.
    #[facet(rename = "not_null")]
    IsNotNull { column: String },

    /// ILIKE pattern matching: `column @ilike($param)`.
    #[facet(rename = "ilike")]
    ILike { column: String, pattern: String },

    /// LIKE pattern matching: `column @like($param)`.
    #[facet(rename = "like")]
    Like { column: String, pattern: String },
}

/// A value used in filter conditions.
#[derive(Debug, Facet)]
#[repr(u8)]
pub enum FilterValue {
    /// Parameter reference (`$name`).
    Param(String),
    /// String literal.
    String(String),
    /// Integer literal.
    Int(i64),
    /// Boolean literal.
    Bool(bool),
}

/// ORDER BY clause.
///
/// # Example
///
/// ```styx
/// order_by{ created_at desc, name asc }
/// ```
#[derive(Debug, Facet)]
pub struct OrderByClause {
    /// Ordering specifications.
    #[facet(flatten)]
    pub items: Vec<OrderByItem>,
}

/// A single ORDER BY item.
#[derive(Debug, Facet)]
pub struct OrderByItem {
    /// Column to order by.
    pub column: String,
    /// Sort direction.
    pub direction: SortDirection,
}

/// Sort direction for ORDER BY.
#[derive(Debug, Facet)]
#[repr(u8)]
pub enum SortDirection {
    /// Ascending order.
    #[facet(rename = "asc")]
    Asc,
    /// Descending order.
    #[facet(rename = "desc")]
    Desc,
}

/// A limit or offset value.
#[derive(Debug, Facet)]
#[repr(u8)]
pub enum LimitValue {
    /// Literal number.
    Literal(i64),
    /// Parameter reference (`$name`).
    Param(String),
}

/// SELECT clause containing fields to retrieve.
///
/// # Example
///
/// ```styx
/// select{ id, handle, status }
/// ```
#[derive(Debug, Facet)]
pub struct SelectClause {
    /// Fields to select.
    #[facet(flatten)]
    pub fields: Vec<SelectField>,
}

/// A field in the SELECT clause.
#[derive(Debug, Facet)]
#[facet(tag)]
#[repr(u8)]
pub enum SelectField {
    /// Simple column reference.
    Column { name: String },

    /// Relation (nested query via foreign key).
    ///
    /// # Example
    ///
    /// ```styx
    /// translation @rel{
    ///     from product_translation
    ///     first true
    ///     select{ locale, title }
    /// }
    /// ```
    #[facet(rename = "rel")]
    Relation {
        /// Relation name (becomes the field name in the result struct).
        name: String,
        /// Target table (if different from relation name).
        #[facet(default)]
        from: Option<String>,
        /// Filter conditions for the relation.
        #[facet(default, rename = "where")]
        where_clause: Option<WhereClause>,
        /// ORDER BY for the relation.
        #[facet(default)]
        order_by: Option<OrderByClause>,
        /// Whether to return first row only (`Option<T>` vs `Vec<T>`).
        #[facet(default)]
        first: Option<bool>,
        /// Fields to select from the related table.
        select: SelectClause,
    },

    /// Count aggregate.
    ///
    /// # Example
    ///
    /// ```styx
    /// variant_count @count(product_variant)
    /// ```
    #[facet(rename = "count")]
    Count {
        /// Field name for the count.
        name: String,
        /// Table to count from.
        table: String,
    },
}

/// Returns clause for raw SQL queries.
///
/// # Example
///
/// ```styx
/// returns{ id @int, title @string }
/// ```
#[derive(Debug, Facet)]
pub struct ReturnsClause {
    /// Return field definitions.
    #[facet(flatten)]
    pub fields: Vec<ReturnField>,
}

/// A return field definition for raw SQL queries.
#[derive(Debug, Facet)]
#[facet(tag)]
#[repr(u8)]
pub enum ReturnField {
    /// String return field.
    #[facet(rename = "string")]
    String { name: String },
    /// Integer return field.
    #[facet(rename = "int")]
    Int { name: String },
    /// Boolean return field.
    #[facet(rename = "bool")]
    Bool { name: String },
    /// UUID return field.
    #[facet(rename = "uuid")]
    Uuid { name: String },
    /// Decimal return field.
    #[facet(rename = "decimal")]
    Decimal { name: String },
    /// Timestamp return field.
    #[facet(rename = "timestamp")]
    Timestamp { name: String },
}
