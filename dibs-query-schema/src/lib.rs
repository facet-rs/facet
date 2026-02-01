//! Facet types for the dibs query DSL schema.
//!
//! These types define the structure of `.styx` query files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation
//! - Used by the LSP extension for diagnostics, hover, go-to-definition

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
    Query(Query),
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
    pub from: Option<Meta<String>>,

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
    pub select: Option<Select>,

    /// Raw SQL query string (for raw SQL queries).
    pub sql: Option<Meta<String>>,

    /// Return type specification (for raw SQL queries).
    pub returns: Option<Returns>,
}

/// Return type specification for raw SQL queries.
#[derive(Debug, Facet)]
pub struct Returns {
    #[facet(flatten)]
    pub fields: IndexMap<Meta<String>, ParamType>,
}

/// DISTINCT ON clause (PostgreSQL-specific) - a sequence of column names.
#[derive(Debug, Facet)]
#[facet(transparent)]
pub struct DistinctOn(pub Vec<Meta<String>>);

/// ORDER BY clause.
#[derive(Debug, Facet)]
pub struct OrderBy {
    /// Column name -> direction ("asc" or "desc", None means asc)
    #[facet(flatten)]
    pub columns: IndexMap<Meta<String>, Option<Meta<String>>>,
}

/// WHERE clause - filter conditions.
#[derive(Debug, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: IndexMap<Meta<String>, FilterValue>,
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
#[derive(Debug, Facet)]
pub struct Params {
    #[facet(flatten)]
    pub params: IndexMap<Meta<String>, ParamType>,
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
#[facet(metadata_container)]
pub struct Select {
    /// Source span of the select block.
    #[facet(metadata = "span")]
    pub span: Span,

    #[facet(flatten)]
    pub fields: IndexMap<Meta<String>, Option<FieldDef>>,
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
    Count(Vec<Meta<String>>),
}

/// A relation definition (nested query on related table).
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct Relation {
    /// Optional explicit table name.
    pub from: Option<Meta<String>>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Return only the first result.
    pub first: Option<Meta<bool>>,

    /// Fields to select from the relation.
    pub select: Option<Select>,
}

/// An INSERT declaration.
#[derive(Debug, Facet)]
pub struct Insert {
    /// Query parameters.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<String>,
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
    pub into: Meta<String>,
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
#[derive(Debug, Facet)]
pub struct InsertMany {
    /// Query parameters - each becomes an array parameter.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<String>,
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
#[derive(Debug, Facet)]
pub struct UpsertMany {
    /// Query parameters - each becomes an array parameter.
    pub params: Option<Params>,
    /// Target table.
    pub into: Meta<String>,
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
    pub table: Meta<String>,
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
    pub from: Meta<String>,
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
    pub columns: IndexMap<Meta<String>, Option<ValueExpr>>,
}

/// Payload of a value expression - can be scalar or sequence.
#[derive(Debug, Facet)]
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
    pub columns: IndexMap<Meta<String>, ()>,
}

/// Columns to update on conflict.
#[derive(Debug, Facet)]
pub struct ConflictUpdate {
    #[facet(flatten)]
    pub columns: IndexMap<Meta<String>, Option<UpdateValue>>,
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
    pub columns: IndexMap<Meta<String>, ()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_styx::RenderError;

    /// Test that Spanned<String> works as a map key with facet-styx.
    #[test]
    fn spanned_string_as_map_key() {
        #[derive(Debug, Facet)]
        struct TestMap {
            #[facet(flatten)]
            items: IndexMap<Meta<String>, String>,
        }

        let source = r#"{foo bar, baz qux}"#;
        let result: Result<TestMap, _> = facet_styx::from_str(source);

        match result {
            Ok(map) => {
                assert_eq!(map.items.len(), 2);
                let keys: Vec<_> = map.items.keys().map(|k| k.value.as_str()).collect();
                assert!(keys.contains(&"foo"));
                assert!(keys.contains(&"baz"));
            }
            Err(e) => {
                panic!("Failed to parse: {}", e.render("<test>", source));
            }
        }
    }

    /// Test that Where clause parses correctly with Spanned keys.
    #[test]
    fn where_clause_with_spanned_keys() {
        let source = r#"{deleted_at @null}"#;
        let result: Result<Where, _> = facet_styx::from_str(source);

        match result {
            Ok(where_clause) => {
                assert_eq!(where_clause.filters.len(), 1);
                let key = where_clause.filters.keys().next().unwrap();
                assert_eq!(key.value, "deleted_at");
            }
            Err(e) => {
                panic!("Failed to parse: {}", e.render("<test>", source));
            }
        }
    }

    /// Test that FilterValue::EqBare works with Meta<String>.
    #[test]
    fn filter_value_eq() {
        let source = r#"{id $id}"#;
        let result: Result<Where, _> = facet_styx::from_str(source);

        match result {
            Ok(where_clause) => {
                assert_eq!(where_clause.filters.len(), 1);
                let (key, value) = where_clause.filters.iter().next().unwrap();
                assert_eq!(key.value, "id");
                match value {
                    FilterValue::EqBare(Some(meta)) => {
                        assert_eq!(meta.as_str(), "$id");
                        // Verify span is captured (offset 4, len 3 for "$id")
                        assert_eq!(meta.span.offset, 4);
                        assert_eq!(meta.span.len, 3);
                    }
                    _ => panic!("Expected EqBare variant, got {:?}", value),
                }
            }
            Err(e) => {
                panic!("Failed to parse: {}", e.render("<test>", source));
            }
        }
    }

    /// Test that FilterValue::EqBare works with shorthand (no value).
    #[test]
    fn filter_value_eq_shorthand() {
        let source = r#"{id}"#;
        let result: Result<Where, _> = facet_styx::from_str(source);

        match result {
            Ok(where_clause) => {
                assert_eq!(where_clause.filters.len(), 1);
                let (key, value) = where_clause.filters.iter().next().unwrap();
                assert_eq!(key.value, "id");
                match value {
                    FilterValue::EqBare(None) => {
                        // Success - shorthand syntax where {id} means {id $id}
                    }
                    FilterValue::EqBare(Some(meta)) => {
                        panic!(
                            "Expected EqBare(None) for shorthand, got EqBare(Some({}))",
                            meta.as_str()
                        );
                    }
                    _ => panic!("Expected EqBare variant, got {:?}", value),
                }
            }
            Err(e) => {
                panic!("Failed to parse: {}", e.render("<test>", source));
            }
        }
    }
}
