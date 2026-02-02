//! Facet types for the dibs query DSL schema.
//!
//! These types define the structure of `.styx` query files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation
//! - Used by the LSP extension for diagnostics, hover, go-to-definition

use dibs_sql::{ColumnName, ParamName, TableName};
use facet::Facet;
pub use facet_reflect::Span;
use indexmap::IndexMap;
use std::{borrow::Borrow, hash::Hash, ops::Deref};

/// A value with source span and documentation.
///
/// This struct wraps a value along with:
/// - Source location (for diagnostics, go-to-definition)
/// - Doc comments (for hover info)
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct Meta<T> {
    /// The wrapped value.
    pub value: T,

    /// The tag associated to this value if any
    #[facet(metadata = "tag")]
    pub tag: Option<String>,

    /// The source span (offset and length).
    #[facet(metadata = "span")]
    pub span: Span,

    /// Documentation lines (each line is a separate string).
    #[facet(metadata = "doc")]
    pub doc: Option<Vec<String>>,
}

impl<T: Hash> Hash for Meta<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: PartialEq> PartialEq for Meta<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Meta<T> {}

impl Borrow<str> for Meta<String> {
    fn borrow(&self) -> &str {
        &self.value
    }
}

impl PartialEq<&str> for Meta<String> {
    fn eq(&self, other: &&str) -> bool {
        self.value == *other
    }
}

impl PartialEq<str> for Meta<String> {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}

impl<T> Meta<T> {
    /// Create a new spanned value with span information.
    pub fn with_span(value: T, span: Span) -> Self {
        Self {
            value,
            span,
            doc: None,
            tag: None,
        }
    }

    /// Get the documentation as a single joined string.
    pub fn doc_string(&self) -> Option<String> {
        self.doc.as_ref().map(|lines| lines.join("\n"))
    }
}

impl Meta<String> {
    /// Get the value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<'a> Meta<std::borrow::Cow<'a, str>> {
    /// Get the value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<T: Copy> Meta<T> {
    /// Get the inner value (for Copy types like bool).
    pub fn get(&self) -> T {
        self.value
    }
}

/// Extension trait for `Option<Meta<T>>` to make access more ergonomic.
pub trait OptionMetaExt<T> {
    /// Get the inner value by reference if present.
    fn inner(&self) -> Option<&T>;
    /// Get the span if the Meta is present.
    fn meta_span(&self) -> Option<Span>;
}

impl<T> OptionMetaExt<T> for Option<Meta<T>> {
    fn inner(&self) -> Option<&T> {
        self.as_ref().map(|m| &m.value)
    }

    fn meta_span(&self) -> Option<Span> {
        self.as_ref().map(|m| m.span)
    }
}

/// Extension trait for `Option<Meta<T>>` where T is Copy (like bool).
pub trait OptionMetaCopyExt<T: Copy> {
    /// Get the inner value if present.
    fn value(&self) -> Option<T>;
}

impl<T: Copy> OptionMetaCopyExt<T> for Option<Meta<T>> {
    fn value(&self) -> Option<T> {
        self.as_ref().map(|m| m.value)
    }
}

/// Extension trait for `Option<Meta<T>>` to get references to the inner value.
pub trait OptionMetaDerefExt<T> {
    /// Get the inner value by reference if present.
    fn value_as_ref(&self) -> Option<&T>;
    /// Get the inner value as its Deref target if present.
    fn value_as_deref(&self) -> Option<&<T as Deref>::Target>
    where
        T: Deref;
}

impl<T> OptionMetaDerefExt<T> for Option<Meta<T>> {
    fn value_as_ref(&self) -> Option<&T> {
        self.as_ref().map(|m| &m.value)
    }

    fn value_as_deref(&self) -> Option<&<T as Deref>::Target>
    where
        T: Deref,
    {
        self.as_ref().map(|m| m.value.deref())
    }
}

impl<T> OptionMetaDerefExt<T> for Option<&Meta<T>> {
    fn value_as_ref(&self) -> Option<&T> {
        self.map(|m| &m.value)
    }

    fn value_as_deref(&self) -> Option<&<T as Deref>::Target>
    where
        T: Deref,
    {
        self.map(|m| m.value.deref())
    }
}

impl<T> Deref for Meta<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// A query file - top level is a map of declaration names to declarations.
/// Uses `Meta<String>` as keys to capture doc comments from the styx file.
#[derive(Debug, Facet)]
#[facet(transparent)]
pub struct QueryFile(pub IndexMap<Meta<String>, Decl>);

/// A declaration in a query file.
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
#[allow(clippy::large_enum_variant)]
pub enum Decl {
    /// A SELECT query declaration.
    Select(Select),
    /// An INSERT declaration.
    Insert(Insert),
    /// A bulk INSERT declaration (insert multiple rows).
    InsertMany(InsertMany),
    /// An UPSERT declaration.
    Upsert(Upsert),
    /// A bulk UPSERT declaration (upsert multiple rows).
    UpsertMany(UpsertMany),
    /// An UPDATE declaration.
    Update(Update),
    /// A DELETE declaration.
    Delete(Delete),
}

/// A SELECT query definition.
///
/// Can be either a structured query (with `from` and `select`) or a raw SQL query
/// (with `sql` and `returns`).
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct Select {
    /// Query parameters.
    pub params: Option<Params>,

    /// Source table to query from (for structured queries).
    pub from: Option<Meta<TableName>>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Return only the first result.
    pub first: Option<Meta<bool>>,

    /// Use DISTINCT to return only unique rows.
    pub distinct: Option<Meta<bool>>,

    /// DISTINCT ON clause (PostgreSQL-specific) - return first row of each group.
    pub distinct_on: Option<DistinctOn>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Limit clause (number or param reference like $limit).
    pub limit: Option<Meta<String>>,

    /// Offset clause (number or param reference like $offset).
    pub offset: Option<Meta<String>>,

    /// Fields to select (for structured queries).
    pub fields: Option<SelectFields>,

    /// Raw SQL query string (for raw SQL queries).
    pub sql: Option<Meta<String>>,

    /// Return type specification (for raw SQL queries).
    pub returns: Option<Returns>,
}

/// Return type specification for raw SQL queries.
#[derive(Debug, Facet)]
pub struct Returns {
    #[facet(flatten)]
    pub fields: IndexMap<Meta<ColumnName>, ParamType>,
}

/// DISTINCT ON clause (PostgreSQL-specific) - a sequence of column names.
#[derive(Debug, Facet)]
#[facet(transparent)]
pub struct DistinctOn(pub Vec<Meta<ColumnName>>);

/// ORDER BY clause.
#[derive(Debug, Facet)]
pub struct OrderBy {
    /// Column name -> direction ("asc" or "desc", None means asc)
    #[facet(flatten)]
    pub columns: IndexMap<Meta<ColumnName>, Option<Meta<String>>>,
}

/// WHERE clause - filter conditions.
#[derive(Debug, Clone, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: IndexMap<Meta<ColumnName>, FilterValue>,
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
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum FilterValue {
    /// NULL check (@null)
    Null,
    /// NOT NULL check (@not-null)
    #[facet(rename = "not-null")]
    NotNull,
    /// ILIKE pattern matching (@ilike($param) or @ilike("pattern"))
    Ilike(Vec<Meta<String>>),
    /// LIKE pattern matching (@like($param) or @like("pattern"))
    Like(Vec<Meta<String>>),
    /// Greater than (@gt($param) or @gt(value))
    Gt(Vec<Meta<String>>),
    /// Less than (@lt($param) or @lt(value))
    Lt(Vec<Meta<String>>),
    /// Greater than or equal (@gte($param) or @gte(value))
    Gte(Vec<Meta<String>>),
    /// Less than or equal (@lte($param) or @lte(value))
    Lte(Vec<Meta<String>>),
    /// Not equal (@ne($param) or @ne(value))
    Ne(Vec<Meta<String>>),
    /// IN array check (@in($param)) - param should be an array type
    In(Vec<Meta<String>>),
    /// JSONB get object operator (@json_get($param)) -> `column -> $param`
    JsonGet(Vec<Meta<String>>),
    /// JSONB get text operator (@json_get_text($param)) -> `column ->> $param`
    JsonGetText(Vec<Meta<String>>),
    /// Contains operator (@contains($param)) -> `column @> $param`
    Contains(Vec<Meta<String>>),
    /// Key exists operator (@key_exists($param)) -> `column ? $param`
    KeyExists(Vec<Meta<String>>),
    /// Explicit equality (@eq($param) or @eq(value))
    Eq(Vec<Meta<String>>),
    /// Equality - bare scalar fallback (e.g., `$handle` or `"value"`)
    #[facet(other)]
    EqBare(Option<Meta<String>>),
}

/// Query parameters.
#[derive(Debug, Clone, Facet)]
pub struct Params {
    #[facet(flatten)]
    pub params: IndexMap<Meta<ParamName>, ParamType>,
}

/// Parameter type.
#[derive(Debug, Clone, Facet)]
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
#[facet(metadata_container)]
pub struct SelectFields {
    /// Source span of the select block.
    #[facet(metadata = "span")]
    pub span: Span,

    #[facet(flatten)]
    pub fields: IndexMap<Meta<ColumnName>, Option<FieldDef>>,
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
    Count(Vec<Meta<TableName>>),
}

/// A relation definition (nested query on related table).
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct Relation {
    /// Optional explicit table name.
    pub from: Option<Meta<TableName>>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Return only the first result.
    pub first: Option<Meta<bool>>,

    /// Fields to select from the relation.
    pub fields: Option<SelectFields>,
}

/// An INSERT declaration.
#[derive(Debug, Clone, Facet)]
pub struct Insert {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<TableName>,
    /// Values to insert (column -> value expression).
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// An UPSERT declaration (INSERT ... ON CONFLICT ... DO UPDATE).
#[derive(Debug, Clone, Facet)]
pub struct Upsert {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<TableName>,
    /// ON CONFLICT clause.
    #[facet(rename = "on-conflict")]
    pub on_conflict: OnConflict,
    /// Values to insert (column -> value expression).
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// A bulk INSERT declaration (insert multiple rows with a single query).
///
/// Uses PostgreSQL's UNNEST to insert multiple rows efficiently with constant SQL.
///
/// Example:
/// ```styx
/// BulkCreateProducts @insert-many{
///   params {handle @string, status @string}
///   into products
///   values {handle, status, created_at @now}
///   returning {id, handle, status}
/// }
/// ```
#[derive(Debug, Clone, Facet)]
pub struct InsertMany {
    /// Query parameters - each becomes an array parameter.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<TableName>,
    /// Values to insert (column -> value expression).
    /// Params become UNNEST columns, other expressions are applied to each row.
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// A bulk UPSERT declaration (upsert multiple rows with a single query).
///
/// Uses PostgreSQL's UNNEST with ON CONFLICT for efficient bulk upserts.
///
/// Example:
/// ```styx
/// BulkUpsertProducts @upsert-many{
///   params {handle @string, status @string}
///   into products
///   on-conflict {
///     target {handle}
///     update {status, updated_at @now}
///   }
///   values {handle, status, created_at @now}
///   returning {id, handle, status}
/// }
/// ```
#[derive(Debug, Clone, Facet)]
pub struct UpsertMany {
    /// Query parameters - each becomes an array parameter.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<TableName>,
    /// ON CONFLICT clause.
    #[facet(rename = "on-conflict")]
    pub on_conflict: OnConflict,
    /// Values to insert (column -> value expression).
    pub values: Values,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// An UPDATE declaration.
#[derive(Debug, Clone, Facet)]
pub struct Update {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub table: Meta<TableName>,
    /// Values to set (column -> value expression).
    pub set: Values,
    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// A DELETE declaration.
#[derive(Debug, Clone, Facet)]
pub struct Delete {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub from: Meta<TableName>,
    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,
    /// Columns to return.
    pub returning: Option<Returning>,
}

/// Values clause for INSERT/UPDATE.
#[derive(Debug, Clone, Facet)]
pub struct Values {
    /// Column name -> value expression. None means use param with same name ($column_name).
    #[facet(flatten)]
    pub columns: IndexMap<Meta<ColumnName>, Option<ValueExpr>>,
}

/// Payload of a value expression - can be scalar or sequence.
#[derive(Debug, Clone, Facet)]
#[facet(untagged)]
#[repr(u8)]
pub enum Payload {
    /// Scalar payload (for bare values like $name)
    Scalar(Meta<String>),
    /// Sequence payload (for functions with args like @coalesce($a $b))
    Seq(Vec<ValueExpr>),
}

/// A value expression in INSERT/UPDATE.
///
/// Special cases:
/// - `@default` - the DEFAULT keyword
/// - `@funcname` or `@funcname(args...)` - SQL function calls like NOW(), COALESCE(), etc.
/// - Bare scalars - parameter references ($name) or literals
#[derive(Debug, Clone, Facet)]
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
#[derive(Debug, Clone, Facet)]
pub struct OnConflict {
    /// Target columns for conflict detection.
    pub target: ConflictTarget,
    /// Columns to update on conflict.
    pub update: ConflictUpdate,
}

/// Conflict target columns.
#[derive(Debug, Clone, Facet)]
pub struct ConflictTarget {
    #[facet(flatten)]
    pub columns: IndexMap<Meta<ColumnName>, ()>,
}

/// Columns to update on conflict.
#[derive(Debug, Clone, Facet)]
pub struct ConflictUpdate {
    #[facet(flatten)]
    pub columns: IndexMap<Meta<ColumnName>, Option<UpdateValue>>,
}

/// Value for an update column - mirrors `ValueExpr`.
#[derive(Debug, Clone, Facet)]
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
#[derive(Debug, Clone, Facet)]
pub struct Returning {
    #[facet(flatten)]
    pub columns: IndexMap<Meta<ColumnName>, ()>,
}

// ============================================================================
// CONVENIENCE METHODS FOR SCHEMA TYPES
// ============================================================================

impl Select {
    /// Check if this query returns only the first result.
    pub fn is_first(&self) -> bool {
        self.first.is_some()
    }

    /// Check if this query has any relations in its select clause.
    pub fn has_relations(&self) -> bool {
        self.fields
            .as_ref()
            .map(|select| select.has_relations())
            .unwrap_or(false)
    }

    /// Check if this query has any Vec (has-many) relations.
    pub fn has_vec_relations(&self) -> bool {
        self.fields
            .as_ref()
            .map(|select| select.has_vec_relations())
            .unwrap_or(false)
    }

    /// Check if this query has nested Vec relations (Vec containing Vec).
    pub fn has_nested_vec_relations(&self) -> bool {
        self.fields
            .as_ref()
            .map(|select| select.has_nested_vec_relations())
            .unwrap_or(false)
    }
}

impl SelectFields {
    /// Check if this select has any relations.
    pub fn has_relations(&self) -> bool {
        self.fields
            .values()
            .any(|field_def| matches!(field_def, Some(FieldDef::Rel(_))))
    }

    /// Check if this select has any Vec (has-many) relations.
    pub fn has_vec_relations(&self) -> bool {
        self.fields.values().any(|field_def| {
            if let Some(FieldDef::Rel(rel)) = field_def {
                rel.first.is_none()
            } else {
                false
            }
        })
    }

    /// Check if this select has nested Vec relations.
    pub fn has_nested_vec_relations(&self) -> bool {
        for field_def in self.fields.values() {
            if let Some(FieldDef::Rel(rel)) = field_def
                && rel.first.is_none()
            {
                // This is a Vec relation
                if let Some(rel_select) = &rel.fields
                    && (rel_select.has_vec_relations() || rel_select.has_nested_vec_relations())
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if this select has any count aggregations.
    pub fn has_count(&self) -> bool {
        self.fields
            .values()
            .any(|field_def| matches!(field_def, Some(FieldDef::Count(_))))
    }

    /// Iterate over simple columns (fields with None FieldDef).
    pub fn columns(&self) -> impl Iterator<Item = (&Meta<ColumnName>, &Option<FieldDef>)> {
        self.fields
            .iter()
            .filter(|(_, field_def)| field_def.is_none())
    }

    /// Iterate over relations (fields with Some(FieldDef::Rel(_))).
    pub fn relations(&self) -> impl Iterator<Item = (&Meta<ColumnName>, &Relation)> {
        self.fields.iter().filter_map(|(name, field_def)| {
            if let Some(FieldDef::Rel(rel)) = field_def {
                Some((name, rel))
            } else {
                None
            }
        })
    }

    /// Iterate over count aggregations (fields with Some(FieldDef::Count(_))).
    pub fn counts(&self) -> impl Iterator<Item = (&Meta<ColumnName>, &Vec<Meta<TableName>>)> {
        self.fields.iter().filter_map(|(name, field_def)| {
            if let Some(FieldDef::Count(tables)) = field_def {
                Some((name, tables))
            } else {
                None
            }
        })
    }

    /// Get the first column name (first simple column, not a relation).
    /// Returns None if there are no simple columns.
    pub fn first_column(&self) -> Option<&ColumnName> {
        self.fields
            .iter()
            .find(|(_, field_def)| field_def.is_none())
            .map(|(name, _)| &name.value)
    }

    /// Get the ID column name (column named "id", or first column as fallback).
    /// Returns None if there are no simple columns.
    pub fn id_column(&self) -> Option<&ColumnName> {
        // First try to find a column named "id"
        self.fields
            .iter()
            .find(|(name, field_def)| field_def.is_none() && name.value.as_str() == "id")
            .map(|(name, _)| &name.value)
            .or_else(|| self.first_column())
    }
}

impl Relation {
    /// Get the table name for this relation.
    /// Returns the explicit `from` table if set, otherwise returns None
    /// (caller should use the relation field name as fallback).
    pub fn table_name(&self) -> Option<&str> {
        self.from.as_ref().map(|m| m.value.as_str())
    }

    /// Check if this relation is a single result (first).
    pub fn is_first(&self) -> bool {
        self.first.is_some()
    }

    /// Check if this relation has any nested relations.
    pub fn has_relations(&self) -> bool {
        self.fields
            .as_ref()
            .map(|select| select.has_relations())
            .unwrap_or(false)
    }

    /// Check if this relation has any Vec (has-many) nested relations.
    pub fn has_vec_relations(&self) -> bool {
        self.fields
            .as_ref()
            .map(|select| select.has_vec_relations())
            .unwrap_or(false)
    }
}

impl Params {
    /// Iterate over parameters by name and type.
    pub fn iter(&self) -> impl Iterator<Item = (&Meta<ParamName>, &ParamType)> {
        self.params.iter()
    }
}

#[cfg(test)]
mod tests;
