//! Facet types for the dibs query DSL schema.
//!
//! These types define the structure of `.styx` query files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation

use facet::Facet;
pub use facet_styx::Documented;
use indexmap::IndexMap;

/// A query file - top level is a map of declaration names to declarations.
/// Uses `Documented<String>` as keys to capture doc comments from the styx file.
#[derive(Debug, Facet)]
#[facet(transparent)]
pub struct QueryFile(pub IndexMap<Documented<String>, Decl>);

/// A declaration in a query file.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum Decl {
    /// A SELECT query declaration.
    Query(Query),
    /// An INSERT declaration.
    Insert(Insert),
    /// An UPSERT declaration.
    Upsert(Upsert),
    /// An UPDATE declaration.
    Update(Update),
    /// A DELETE declaration.
    Delete(Delete),
}

/// A query definition.
///
/// Can be either a structured query (with `from` and `select`) or a raw SQL query
/// (with `sql` and `returns`).
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
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

    /// Use DISTINCT to return only unique rows.
    pub distinct: Option<bool>,

    /// DISTINCT ON clause (PostgreSQL-specific) - return first row of each group.
    pub distinct_on: Option<DistinctOn>,

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
    pub fields: IndexMap<String, ParamType>,
}

/// DISTINCT ON clause (PostgreSQL-specific) - a sequence of column names.
#[derive(Debug, Facet)]
#[facet(transparent)]
pub struct DistinctOn(pub Vec<String>);

/// ORDER BY clause.
#[derive(Debug, Facet)]
pub struct OrderBy {
    /// Column name -> direction ("asc" or "desc", None means asc)
    #[facet(flatten)]
    pub columns: IndexMap<String, Option<String>>,
}

/// WHERE clause - filter conditions.
#[derive(Debug, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: IndexMap<String, FilterValue>,
}

/// A filter value - tagged operators or bare scalars for where clauses.
///
/// Tagged operators:
/// - `@null` for IS NULL
/// - `@not_null` for IS NOT NULL
/// - `@ilike($param)` or `@ilike("pattern")` for case-insensitive LIKE
/// - `@like`, `@gt`, `@lt`, `@gte`, `@lte`, `@ne` for comparison operators
/// - `@in($param)` for `= ANY($1)` (array containment)
/// - `@json-get($param)` for JSONB `->` operator (get JSON object)
/// - `@json-get-text($param)` for JSONB `->>` operator (get JSON value as text)
/// - `@contains($param)` for `@>` operator (contains, typically JSONB)
/// - `@key-exists($param)` for `?` operator (key exists, typically JSONB)
///
/// Bare scalars (like `$handle`) are treated as equality filters via `#[facet(other)]`.
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum FilterValue {
    /// NULL check (@null)
    Null,
    /// NOT NULL check (@not-null)
    #[facet(rename = "not-null")]
    NotNull,
    /// ILIKE pattern matching (@ilike($param) or @ilike("pattern"))
    Ilike(Vec<String>),
    /// LIKE pattern matching (@like($param) or @like("pattern"))
    Like(Vec<String>),
    /// Greater than (@gt($param) or @gt(value))
    Gt(Vec<String>),
    /// Less than (@lt($param) or @lt(value))
    Lt(Vec<String>),
    /// Greater than or equal (@gte($param) or @gte(value))
    Gte(Vec<String>),
    /// Less than or equal (@lte($param) or @lte(value))
    Lte(Vec<String>),
    /// Not equal (@ne($param) or @ne(value))
    Ne(Vec<String>),
    /// IN array check (@in($param)) - param should be an array type
    In(Vec<String>),
    /// JSONB get object operator (@json_get($param)) -> `column -> $param`
    JsonGet(Vec<String>),
    /// JSONB get text operator (@json_get_text($param)) -> `column ->> $param`
    JsonGetText(Vec<String>),
    /// Contains operator (@contains($param)) -> `column @> $param`
    Contains(Vec<String>),
    /// Key exists operator (@key_exists($param)) -> `column ? $param`
    KeyExists(Vec<String>),
    /// Equality - bare scalar fallback (e.g., `$handle` or `"value"`)
    #[facet(other)]
    Eq(String),
}

/// Query parameters.
#[derive(Debug, Facet)]
pub struct Params {
    #[facet(flatten)]
    pub params: IndexMap<String, ParamType>,
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
    Bytes,
    /// Optional type: @optional(@string) -> Optional(vec![String])
    Optional(Vec<ParamType>),
}

/// SELECT clause.
#[derive(Debug, Facet)]
pub struct Select {
    #[facet(flatten)]
    pub fields: IndexMap<String, Option<FieldDef>>,
}

/// A field definition - tagged values in select.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
#[allow(clippy::large_enum_variant)]
pub enum FieldDef {
    /// A relation field (`@rel{...}`).
    Rel(Relation),
    /// A count aggregation (`@count(table_name)`).
    Count(Vec<String>),
}

/// A relation definition (nested query on related table).
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct Relation {
    /// Optional explicit table name.
    pub from: Option<String>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Return only the first result.
    pub first: Option<bool>,

    /// Fields to select from the relation.
    pub select: Option<Select>,
}

/// An INSERT declaration.
#[derive(Debug, Facet)]
pub struct Insert {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub into: String,
    /// Values to insert (column -> value expression).
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// An UPSERT declaration (INSERT ... ON CONFLICT ... DO UPDATE).
#[derive(Debug, Facet)]
pub struct Upsert {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub into: String,
    /// ON CONFLICT clause.
    #[facet(rename = "on-conflict")]
    pub on_conflict: OnConflict,
    /// Values to insert (column -> value expression).
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// An UPDATE declaration.
#[derive(Debug, Facet)]
pub struct Update {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub table: String,
    /// Values to set (column -> value expression).
    pub set: Values,
    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// A DELETE declaration.
#[derive(Debug, Facet)]
pub struct Delete {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub from: String,
    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// Values clause for INSERT/UPDATE.
#[derive(Debug, Facet)]
pub struct Values {
    /// Column name -> value expression. None means use param with same name ($column_name).
    #[facet(flatten)]
    pub columns: IndexMap<String, Option<ValueExpr>>,
}

/// Payload of a value expression - can be scalar or sequence.
#[derive(Debug, Facet)]
#[facet(untagged)]
#[repr(u8)]
pub enum Payload {
    /// Scalar payload (for bare values like $name)
    Scalar(String),
    /// Sequence payload (for functions with args like @coalesce($a $b))
    Seq(Vec<ValueExpr>),
}

/// A value expression in INSERT/UPDATE.
///
/// Special cases:
/// - `@default` - the DEFAULT keyword
/// - `@funcname` or `@funcname(args...)` - SQL function calls like NOW(), COALESCE(), etc.
/// - Bare scalars - parameter references ($name) or literals
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum ValueExpr {
    /// Default value (@default).
    Default,
    /// Everything else: functions and bare scalars.
    /// - Bare scalars: tag=None, content=Some(Scalar(...))
    /// - Nullary functions: tag=Some("name"), content=None
    /// - Functions with args: tag=Some("name"), content=Some(Seq(...))
    #[facet(other)]
    Other {
        #[facet(tag)]
        tag: Option<String>,
        #[facet(content)]
        content: Option<Payload>,
    },
}

/// ON CONFLICT clause for UPSERT.
#[derive(Debug, Facet)]
pub struct OnConflict {
    /// Target columns for conflict detection.
    pub target: ConflictTarget,
    /// Columns to update on conflict.
    pub update: ConflictUpdate,
}

/// Conflict target columns.
#[derive(Debug, Facet)]
pub struct ConflictTarget {
    #[facet(flatten)]
    pub columns: IndexMap<String, ()>,
}

/// Columns to update on conflict.
#[derive(Debug, Facet)]
pub struct ConflictUpdate {
    #[facet(flatten)]
    pub columns: IndexMap<String, Option<UpdateValue>>,
}

/// Value for an update column - mirrors `ValueExpr`.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum UpdateValue {
    /// Default value (@default).
    Default,
    /// Everything else: functions and bare scalars.
    #[facet(other)]
    Other {
        #[facet(tag)]
        tag: Option<String>,
        #[facet(content)]
        content: Option<Payload>,
    },
}

/// RETURNING clause.
#[derive(Debug, Facet)]
pub struct Returning {
    #[facet(flatten)]
    pub columns: IndexMap<String, ()>,
}
