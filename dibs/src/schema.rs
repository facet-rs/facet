//! Schema definition and introspection.
//!
//! ## Naming Convention
//!
//! **Table names use singular form** (e.g., `user`, `post`, `comment`).
//!
//! This convention treats each table as a definition of what a single record
//! represents, rather than a container of multiple records.
//!
//! ## Example
//!
//! ```ignore
//! use dibs::prelude::*;
//! use facet::Facet;
//!
//! #[derive(Facet)]
//! #[facet(dibs::table = "user")]
//! pub struct User {
//!     #[facet(dibs::pk)]
//!     pub id: i64,
//!
//!     #[facet(dibs::unique)]
//!     pub email: String,
//!
//!     pub name: String,
//! }
//! ```

use crate::Jsonb;
use facet::{Facet, Shape, Type, UserType};

// Define the dibs attribute grammar using facet's macro.
// This generates:
// - `Attr` enum with all attribute variants
// - `__attr!` macro for parsing attributes
// - Re-exports for use as `dibs::table`, `dibs::pk`, etc.
facet::define_attr_grammar! {
    ns "dibs";
    crate_path ::dibs;

    /// Dibs schema attribute types.
    pub enum Attr {
        /// Marks a struct as a database table.
        ///
        /// Usage: `#[facet(dibs::table = "table_name")]`
        Table(&'static str),

        /// Marks a field as the primary key.
        ///
        /// Usage: `#[facet(dibs::pk)]`
        Pk,

        /// Marks a field as having a unique constraint.
        ///
        /// Usage: `#[facet(dibs::unique)]`
        Unique,

        /// Marks a field as a foreign key reference.
        ///
        /// Usage: `#[facet(dibs::fk = "other_table.column")]`
        Fk(&'static str),

        /// Marks a field as not null (explicit, inferred for non-Option types).
        ///
        /// Usage: `#[facet(dibs::not_null)]`
        NotNull,

        /// Sets a default value expression for the column.
        ///
        /// Usage: `#[facet(dibs::default = "now()")]`
        Default(&'static str),

        /// Overrides the column name (default: snake_case of field name).
        ///
        /// Usage: `#[facet(dibs::column = "column_name")]`
        Column(&'static str),

        /// Creates an index on a single column (field-level).
        ///
        /// Usage: `#[facet(dibs::index)]` or `#[facet(dibs::index = "index_name")]`
        Index(Option<&'static str>),

        /// Creates an index on one or more columns (container-level).
        ///
        /// Usage:
        /// - `#[facet(dibs::index(columns = "col1,col2"))]` - auto-named composite index
        /// - `#[facet(dibs::index(name = "idx_foo", columns = "col1,col2"))]` - named composite index
        CompositeIndex(CompositeIndex),

        /// Creates a unique constraint on one or more columns (container-level).
        ///
        /// Usage:
        /// - `#[facet(dibs::composite_unique(columns = "col1,col2"))]` - auto-named unique constraint
        /// - `#[facet(dibs::composite_unique(name = "uq_foo", columns = "col1,col2"))]` - named constraint
        CompositeUnique(CompositeUnique),

        /// Marks a field as auto-generated (e.g., SERIAL, sequences).
        ///
        /// Usage: `#[facet(dibs::auto)]`
        Auto,

        /// Marks a text field as "long" (renders as textarea in admin UI).
        ///
        /// Usage: `#[facet(dibs::long)]`
        Long,

        /// Marks a field as the display label for the row (used in FK references).
        ///
        /// Usage: `#[facet(dibs::label)]`
        Label,

        /// Specifies the language/format of a text field (e.g., "markdown", "json").
        /// Implies `long` - will render with a code editor in admin UI.
        ///
        /// Usage: `#[facet(dibs::lang = "markdown")]`
        Lang(&'static str),

        /// Specifies a Lucide icon name for display in the admin UI.
        /// Can be used on fields or containers (tables).
        ///
        /// Usage: `#[facet(dibs::icon = "user")]`
        Icon(&'static str),

        /// Specifies the semantic subtype of a column.
        /// Sets a default icon (can be overridden with explicit `dibs::icon`).
        ///
        /// Supported subtypes:
        /// - Contact: `email`, `phone`, `url`, `website`, `username`
        /// - Media: `image`, `avatar`, `file`, `video`
        /// - Money: `currency`, `money`, `price`, `percent`
        /// - Security: `password`, `secret`, `token`
        /// - Code: `code`, `json`, `markdown`, `html`
        /// - Location: `address`, `country`, `ip`
        /// - Content: `slug`, `color`, `tag`
        ///
        /// Usage: `#[facet(dibs::subtype = "email")]`
        Subtype(&'static str),
    }

    /// Composite index definition for multi-column indices.
    pub struct CompositeIndex {
        /// Optional index name (auto-generated if not provided)
        pub name: Option<&'static str>,
        /// Comma-separated column names
        pub columns: &'static str,
        /// Optional WHERE clause for partial index (PostgreSQL-specific)
        ///
        /// Example: `filter = "is_active = true"` creates `CREATE INDEX ... WHERE is_active = true`
        pub filter: Option<&'static str>,
    }

    /// Composite unique constraint for multi-column uniqueness.
    ///
    /// Usage:
    /// - `#[facet(dibs::composite_unique(columns = "col1,col2"))]` - auto-named unique constraint
    /// - `#[facet(dibs::composite_unique(name = "uq_foo", columns = "col1,col2"))]` - named constraint
    /// - `#[facet(dibs::composite_unique(columns = "col", filter = "is_primary = true"))]` - partial unique
    pub struct CompositeUnique {
        /// Optional constraint name (auto-generated if not provided)
        pub name: Option<&'static str>,
        /// Comma-separated column names
        pub columns: &'static str,
        /// Optional WHERE clause for partial unique index (PostgreSQL-specific)
        ///
        /// Example: `filter = "is_active = true"` creates `CREATE UNIQUE INDEX ... WHERE is_active = true`
        pub filter: Option<&'static str>,
    }
}

/// Postgres column types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgType {
    /// SMALLINT (2 bytes)
    SmallInt,
    /// INTEGER (4 bytes)
    Integer,
    /// BIGINT (8 bytes)
    BigInt,
    /// REAL (4 bytes floating point)
    Real,
    /// DOUBLE PRECISION (8 bytes floating point)
    DoublePrecision,
    /// NUMERIC (arbitrary precision)
    Numeric,
    /// BOOLEAN
    Boolean,
    /// TEXT
    Text,
    /// BYTEA (binary)
    Bytea,
    /// TIMESTAMPTZ
    Timestamptz,
    /// DATE
    Date,
    /// TIME
    Time,
    /// UUID
    Uuid,
    /// JSONB
    Jsonb,
    /// TEXT[] (array of text)
    TextArray,
    /// BIGINT[] (array of bigint)
    BigIntArray,
    /// INTEGER[] (array of integer)
    IntegerArray,
}

impl std::fmt::Display for PgType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgType::SmallInt => write!(f, "SMALLINT"),
            PgType::Integer => write!(f, "INTEGER"),
            PgType::BigInt => write!(f, "BIGINT"),
            PgType::Real => write!(f, "REAL"),
            PgType::DoublePrecision => write!(f, "DOUBLE PRECISION"),
            PgType::Numeric => write!(f, "NUMERIC"),
            PgType::Boolean => write!(f, "BOOLEAN"),
            PgType::Text => write!(f, "TEXT"),
            PgType::Bytea => write!(f, "BYTEA"),
            PgType::Timestamptz => write!(f, "TIMESTAMPTZ"),
            PgType::Date => write!(f, "DATE"),
            PgType::Time => write!(f, "TIME"),
            PgType::Uuid => write!(f, "UUID"),
            PgType::Jsonb => write!(f, "JSONB"),
            PgType::TextArray => write!(f, "TEXT[]"),
            PgType::BigIntArray => write!(f, "BIGINT[]"),
            PgType::IntegerArray => write!(f, "INTEGER[]"),
        }
    }
}

/// A database column definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    /// Column name
    pub name: String,
    /// Postgres type
    pub pg_type: PgType,
    /// Rust type name (if known, e.g., from reflection)
    pub rust_type: Option<String>,
    /// Whether the column allows NULL
    pub nullable: bool,
    /// Default value expression (if any)
    pub default: Option<String>,
    /// Whether this is a primary key
    pub primary_key: bool,
    /// Whether this has a unique constraint
    pub unique: bool,
    /// Whether this column is auto-generated (serial, identity, uuid default, etc.)
    pub auto_generated: bool,
    /// Whether this is a long text field (use textarea)
    pub long: bool,
    /// Whether this column should be used as the display label
    pub label: bool,
    /// Enum variants (if this is an enum type)
    pub enum_variants: Vec<String>,
    /// Doc comment (if any)
    pub doc: Option<String>,
    /// Language/format for code editor (e.g., "markdown", "json")
    pub lang: Option<String>,
    /// Lucide icon name for display in admin UI (explicit or derived from subtype)
    pub icon: Option<String>,
    /// Semantic subtype of the column (e.g., "email", "url", "password")
    pub subtype: Option<String>,
}

/// Get the default Lucide icon name for a subtype.
fn subtype_default_icon(subtype: &str) -> Option<&'static str> {
    match subtype {
        // Contact/Identity
        "email" => Some("mail"),
        "phone" => Some("phone"),
        "url" | "website" => Some("link"),
        "username" => Some("at-sign"),

        // Media
        "image" | "avatar" | "photo" => Some("image"),
        "file" => Some("file"),
        "video" => Some("video"),
        "audio" => Some("music"),

        // Money
        "currency" | "money" | "price" => Some("coins"),
        "percent" | "percentage" => Some("percent"),

        // Security
        "password" => Some("lock"),
        "secret" | "token" | "api_key" => Some("key"),

        // Code/Technical
        "code" => Some("code"),
        "json" => Some("braces"),
        "markdown" | "md" => Some("file-text"),
        "html" => Some("code"),
        "regex" => Some("asterisk"),

        // Location
        "address" => Some("map-pin"),
        "city" => Some("building-2"),
        "country" => Some("flag"),
        "zip" | "postal_code" => Some("hash"),
        "ip" | "ip_address" => Some("globe"),
        "coordinates" | "geo" => Some("map"),

        // Content
        "slug" => Some("link-2"),
        "color" | "hex_color" => Some("palette"),
        "tag" | "tags" => Some("tag"),

        // Identifiers
        "uuid" => Some("fingerprint"),
        "sku" | "barcode" => Some("scan-barcode"),
        "version" => Some("git-branch"),

        // Time
        "duration" => Some("timer"),

        _ => None,
    }
}

/// A foreign key constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeignKey {
    /// Column(s) in this table
    pub columns: Vec<String>,
    /// Referenced table
    pub references_table: String,
    /// Referenced column(s)
    pub references_columns: Vec<String>,
}

/// Sort order for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// Ascending order (default)
    #[default]
    Asc,
    /// Descending order
    Desc,
}

impl SortOrder {
    /// Returns the SQL keyword for this sort order, or empty string for ASC (default).
    pub fn to_sql(&self) -> &'static str {
        match self {
            SortOrder::Asc => "",
            SortOrder::Desc => " DESC",
        }
    }
}

/// Nulls ordering for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NullsOrder {
    /// Use database default (NULLS LAST for ASC, NULLS FIRST for DESC)
    #[default]
    Default,
    /// Sort nulls before non-null values
    First,
    /// Sort nulls after non-null values
    Last,
}

impl NullsOrder {
    /// Returns the SQL clause for this nulls ordering, or empty string for default.
    pub fn to_sql(&self) -> &'static str {
        match self {
            NullsOrder::Default => "",
            NullsOrder::First => " NULLS FIRST",
            NullsOrder::Last => " NULLS LAST",
        }
    }
}

/// A column in an index with optional sort order and nulls ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexColumn {
    /// Column name
    pub name: String,
    /// Sort order (ASC or DESC)
    pub order: SortOrder,
    /// Nulls ordering (NULLS FIRST, NULLS LAST, or default)
    pub nulls: NullsOrder,
}

impl IndexColumn {
    /// Create a new index column with default (ASC) ordering and default nulls.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Asc,
            nulls: NullsOrder::Default,
        }
    }

    /// Create a new index column with DESC ordering and default nulls.
    pub fn desc(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Desc,
            nulls: NullsOrder::Default,
        }
    }

    /// Create a new index column with NULLS FIRST ordering.
    pub fn nulls_first(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Asc,
            nulls: NullsOrder::First,
        }
    }

    /// Returns the SQL fragment for this column (name + order + nulls).
    pub fn to_sql(&self) -> String {
        format!(
            "{}{}{}",
            crate::quote_ident(&self.name),
            self.order.to_sql(),
            self.nulls.to_sql()
        )
    }

    /// Parse a column specification like "col_name", "col_name DESC", or "col_name DESC NULLS FIRST".
    pub fn parse(spec: &str) -> Self {
        let spec = spec.trim();
        let upper = spec.to_uppercase();

        // Parse nulls ordering first (it comes at the end)
        let (spec_without_nulls, nulls) = if upper.ends_with(" NULLS FIRST") {
            (&spec[..spec.len() - 12], NullsOrder::First)
        } else if upper.ends_with(" NULLS LAST") {
            (&spec[..spec.len() - 11], NullsOrder::Last)
        } else {
            (spec, NullsOrder::Default)
        };

        let trimmed = spec_without_nulls.trim();
        let upper_trimmed = trimmed.to_uppercase();

        // Parse sort order
        let (name, order) = if upper_trimmed.ends_with(" DESC") {
            (
                trimmed[..trimmed.len() - 5].trim().to_string(),
                SortOrder::Desc,
            )
        } else if upper_trimmed.ends_with(" ASC") {
            (
                trimmed[..trimmed.len() - 4].trim().to_string(),
                SortOrder::Asc,
            )
        } else {
            (trimmed.to_string(), SortOrder::Asc)
        };

        Self { name, order, nulls }
    }
}

/// A database index.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    /// Index name
    pub name: String,
    /// Column(s) in the index with sort order
    pub columns: Vec<IndexColumn>,
    /// Whether this is a unique index
    pub unique: bool,
    /// Optional WHERE clause for partial indexes (PostgreSQL-specific)
    ///
    /// Example: `"is_primary = true"` creates `CREATE INDEX ... WHERE is_primary = true`
    pub where_clause: Option<String>,
}

/// Source location of a schema element.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceLocation {
    /// Source file path
    pub file: Option<String>,
    /// Line number (1-indexed)
    pub line: Option<u32>,
    /// Column number (1-indexed)
    pub column: Option<u32>,
}

impl SourceLocation {
    /// Check if we have any source location info.
    pub fn is_known(&self) -> bool {
        self.file.is_some()
    }

    /// Format as "file:line" or "file:line:column"
    pub fn to_string_short(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        match (self.line, self.column) {
            (Some(line), Some(col)) => Some(format!("{}:{}:{}", file, line, col)),
            (Some(line), None) => Some(format!("{}:{}", file, line)),
            _ => Some(file.clone()),
        }
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_string_short() {
            Some(s) => write!(f, "{}", s),
            None => write!(f, "<unknown>"),
        }
    }
}

/// A database table definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Table name
    pub name: String,
    /// Columns
    pub columns: Vec<Column>,
    /// Foreign keys
    pub foreign_keys: Vec<ForeignKey>,
    /// Indices
    pub indices: Vec<Index>,
    /// Source location of the Rust struct
    pub source: SourceLocation,
    /// Doc comment from the Rust struct
    pub doc: Option<String>,
    /// Lucide icon name for display in admin UI
    pub icon: Option<String>,
}

/// A complete database schema.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// Tables in the schema
    pub tables: Vec<Table>,
}

impl Schema {
    /// Create a new empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect schema from all registered table types.
    ///
    /// This uses facet reflection to inspect types marked with `#[facet(dibs::table)]`.
    pub fn collect() -> Self {
        let tables = inventory::iter::<TableDef>
            .into_iter()
            .filter_map(|def| def.to_table())
            .collect();

        Self { tables }
    }

    /// Generate SQL to create all tables, foreign keys, and indices.
    ///
    /// Returns a complete SQL script that can be executed to create the schema.
    /// Tables are created first, then foreign keys (as ALTER TABLE), then indices.
    pub fn to_sql(&self) -> String {
        let mut sql = String::new();

        // Create tables (without foreign keys to avoid dependency issues)
        for table in &self.tables {
            sql.push_str(&table.to_create_table_sql());
            sql.push_str("\n\n");
        }

        // Add foreign keys
        for table in &self.tables {
            for fk in &table.foreign_keys {
                let constraint_name = format!("fk_{}_{}", table.name, fk.columns.join("_"));
                let quoted_cols: Vec<_> =
                    fk.columns.iter().map(|c| crate::quote_ident(c)).collect();
                let quoted_ref_cols: Vec<_> = fk
                    .references_columns
                    .iter()
                    .map(|c| crate::quote_ident(c))
                    .collect();
                sql.push_str(&format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({});\n",
                    crate::quote_ident(&table.name),
                    crate::quote_ident(&constraint_name),
                    quoted_cols.join(", "),
                    crate::quote_ident(&fk.references_table),
                    quoted_ref_cols.join(", ")
                ));
            }
        }

        if self.tables.iter().any(|t| !t.foreign_keys.is_empty()) {
            sql.push('\n');
        }

        // Create indices
        for table in &self.tables {
            for idx in &table.indices {
                sql.push_str(&table.to_create_index_sql(idx));
                sql.push('\n');
            }
        }

        sql.trim_end().to_string()
    }
}

impl Table {
    /// Generate CREATE TABLE SQL statement.
    ///
    /// Does not include foreign key constraints (those should be added
    /// separately to handle table creation order).
    pub fn to_create_table_sql(&self) -> String {
        let mut sql = format!("CREATE TABLE {} (\n", crate::quote_ident(&self.name));

        // Collect primary key columns
        let pk_columns: Vec<&str> = self
            .columns
            .iter()
            .filter(|c| c.primary_key)
            .map(|c| c.name.as_str())
            .collect();

        // If there's more than one PK column, we need a table constraint
        let use_table_pk_constraint = pk_columns.len() > 1;

        let col_defs: Vec<String> = self
            .columns
            .iter()
            .map(|col| {
                let mut def = format!("    {} {}", crate::quote_ident(&col.name), col.pg_type);

                // Only add inline PRIMARY KEY for single-column PKs
                if col.primary_key && !use_table_pk_constraint {
                    def.push_str(" PRIMARY KEY");
                }

                // NOT NULL: PK columns are implicitly NOT NULL, but for composite PKs
                // we need to add it explicitly since we're not using inline PRIMARY KEY
                if !col.nullable && (!col.primary_key || use_table_pk_constraint) {
                    def.push_str(" NOT NULL");
                }

                if col.unique && !col.primary_key {
                    def.push_str(" UNIQUE");
                }

                if let Some(default) = &col.default {
                    def.push_str(&format!(" DEFAULT {}", default));
                }

                def
            })
            .collect();

        sql.push_str(&col_defs.join(",\n"));

        // Add composite primary key constraint if needed
        if use_table_pk_constraint {
            let quoted_pk_cols: Vec<_> = pk_columns.iter().map(|c| crate::quote_ident(c)).collect();
            sql.push_str(",\n");
            sql.push_str(&format!("    PRIMARY KEY ({})", quoted_pk_cols.join(", ")));
        }

        sql.push_str("\n);");

        sql
    }

    /// Generate CREATE INDEX SQL statement for a given index.
    pub fn to_create_index_sql(&self, idx: &Index) -> String {
        let unique = if idx.unique { "UNIQUE " } else { "" };
        let quoted_cols: Vec<_> = idx.columns.iter().map(|c| c.to_sql()).collect();
        let where_clause = idx
            .where_clause
            .as_ref()
            .map(|w| format!(" WHERE {}", w))
            .unwrap_or_default();
        format!(
            "CREATE {}INDEX {} ON {} ({}){};",
            unique,
            crate::quote_ident(&idx.name),
            crate::quote_ident(&self.name),
            quoted_cols.join(", "),
            where_clause
        )
    }
}

/// Parse a foreign key reference string.
///
/// Supports two formats:
/// - `table.column` (dot-separated)
/// - `table(column)` (parentheses)
///
/// Returns `Some((table, column))` on success, `None` on parse failure.
pub fn parse_fk_reference(fk_ref: &str) -> Option<(&str, &str)> {
    // Try "table.column" format first
    if let Some((table, col)) = fk_ref.split_once('.')
        && !table.is_empty()
        && !col.is_empty()
    {
        return Some((table, col));
    }

    // Try "table(column)" format
    if let Some(paren_idx) = fk_ref.find('(')
        && fk_ref.ends_with(')')
    {
        let table = &fk_ref[..paren_idx];
        let col = &fk_ref[paren_idx + 1..fk_ref.len() - 1];
        if !table.is_empty() && !col.is_empty() {
            return Some((table, col));
        }
    }

    None
}

/// Map a Rust type to a Postgres type.
///
/// Takes a Shape to properly handle generic types like `Vec<u8>` and `Jsonb<T>`.
pub fn shape_to_pg_type(shape: &Shape) -> Option<PgType> {
    // Check for Jsonb<T> using decl_id (works for all generic instantiations)
    if shape.decl_id == Jsonb::<()>::SHAPE.decl_id {
        return Some(PgType::Jsonb);
    }

    // Check for Vec<T> types - shape.def is List
    if matches!(&shape.def, facet::Def::List(_)) {
        if let Some(inner) = shape.inner {
            return match inner.type_identifier {
                "u8" => Some(PgType::Bytea),
                "String" => Some(PgType::TextArray),
                "i64" => Some(PgType::BigIntArray),
                "i32" => Some(PgType::IntegerArray),
                _ => None,
            };
        }
        return None;
    }

    // Check for slice &[u8] (bytea)
    if matches!(&shape.def, facet::Def::Slice(_)) {
        if shape
            .inner
            .is_some_and(|inner| inner.type_identifier == "u8")
        {
            return Some(PgType::Bytea);
        }
        return None;
    }

    // Fall back to type name matching
    rust_type_to_pg(shape.type_identifier)
}

/// Map a Rust type name to a Postgres type.
pub fn rust_type_to_pg(type_name: &str) -> Option<PgType> {
    match type_name {
        "i16" => Some(PgType::SmallInt),
        "i32" => Some(PgType::Integer),
        "i64" => Some(PgType::BigInt),
        "f32" => Some(PgType::Real),
        "f64" => Some(PgType::DoublePrecision),
        // Decimal/Numeric
        "Decimal" | "rust_decimal::Decimal" => Some(PgType::Numeric),
        "bool" => Some(PgType::Boolean),
        "String" | "&str" => Some(PgType::Text),
        // Datetime types
        "Timestamp" | "jiff::Timestamp" | "jiff::tz::Timestamp" => Some(PgType::Timestamptz),
        "Zoned" | "jiff::Zoned" | "jiff::tz::Zoned" => Some(PgType::Timestamptz),
        "DateTime" | "chrono::DateTime" | "chrono::DateTime<Utc>" | "chrono::DateTime<Local>" => {
            Some(PgType::Timestamptz)
        }
        "NaiveDateTime" | "chrono::NaiveDateTime" => Some(PgType::Timestamptz),
        "Date" | "jiff::civil::Date" | "chrono::NaiveDate" => Some(PgType::Date),
        "Time" | "jiff::civil::Time" | "chrono::NaiveTime" => Some(PgType::Time),
        // UUID
        "Uuid" | "uuid::Uuid" => Some(PgType::Uuid),
        _ => None,
    }
}

// =============================================================================
// Attribute helpers
// =============================================================================

/// Get a string value from a dibs attribute on a shape.
fn shape_get_dibs_attr_str(shape: &Shape, key: &str) -> Option<&'static str> {
    shape.attributes.iter().find_map(|attr| {
        if attr.ns == Some("dibs") && attr.key == key {
            attr.get_as::<&str>().copied()
        } else {
            None
        }
    })
}

/// Check if a field has a dibs attribute.
fn field_has_dibs_attr(field: &facet::Field, key: &str) -> bool {
    field
        .attributes
        .iter()
        .any(|attr| attr.ns == Some("dibs") && attr.key == key)
}

/// Get a string value from a dibs attribute on a field.
fn field_get_dibs_attr_str(field: &facet::Field, key: &str) -> Option<&'static str> {
    field.attributes.iter().find_map(|attr| {
        if attr.ns == Some("dibs") && attr.key == key {
            attr.get_as::<&str>().copied()
        } else {
            None
        }
    })
}

/// Check if a default value indicates an auto-generated column.
fn is_auto_generated_default(default: &Option<String>) -> bool {
    let Some(def) = default else {
        return false;
    };

    let lower = def.to_lowercase();

    // Serial/identity columns use nextval
    if lower.contains("nextval(") {
        return true;
    }

    // UUID generation functions
    if lower.contains("gen_random_uuid()") || lower.contains("uuid_generate_v") {
        return true;
    }

    // Timestamp defaults
    if lower.contains("now()") || lower.contains("current_timestamp") {
        return true;
    }

    false
}

/// Extract enum variants from a shape if it's an enum type.
///
/// Currently returns empty vec - enum support requires either:
/// 1. Facet enum reflection (when available)
/// 2. PostgreSQL enum introspection
/// 3. Manual #[facet(dibs::enum_variants = "A,B,C")] annotation
fn extract_enum_variants(_shape: &'static Shape) -> Vec<String> {
    // TODO: Implement when facet adds enum variant reflection
    // For now, enums are stored as TEXT and variants can be added via annotation
    Vec::new()
}

// =============================================================================
// Table definition registration
// =============================================================================

/// A registered table definition.
///
/// This is submitted to inventory by types marked with `#[facet(dibs::table)]`.
pub struct TableDef {
    /// The facet shape of the table struct.
    pub shape: &'static Shape,
}

impl TableDef {
    /// Create a new table definition from a Facet type.
    pub const fn new<T: Facet<'static>>() -> Self {
        Self { shape: T::SHAPE }
    }

    /// Get the table name from the `dibs::table` attribute.
    pub fn table_name(&self) -> Option<&'static str> {
        shape_get_dibs_attr_str(self.shape, "table")
    }

    /// Convert this definition to a Table struct.
    pub fn to_table(&self) -> Option<Table> {
        let table_name = self.table_name()?.to_string();

        // Get the struct type to access fields
        let struct_type = match &self.shape.ty {
            Type::User(UserType::Struct(s)) => s,
            _ => return None,
        };

        let mut columns = Vec::new();
        let mut foreign_keys = Vec::new();
        let mut indices = Vec::new();

        // Collect container-level composite indices
        for attr in self.shape.attributes.iter() {
            if attr.ns == Some("dibs")
                && attr.key == "composite_index"
                && let Some(Attr::CompositeIndex(composite)) = attr.get_as::<Attr>()
            {
                let cols: Vec<IndexColumn> = composite
                    .columns
                    .split(',')
                    .map(IndexColumn::parse)
                    .collect();
                let col_names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
                let idx_name = composite
                    .name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| crate::index_name(&table_name, &col_names));
                indices.push(Index {
                    name: idx_name,
                    columns: cols,
                    unique: false,
                    where_clause: composite.filter.map(|s| s.to_string()),
                });
            }
            // Collect container-level composite unique constraints
            if attr.ns == Some("dibs")
                && attr.key == "composite_unique"
                && let Some(Attr::CompositeUnique(composite)) = attr.get_as::<Attr>()
            {
                let cols: Vec<IndexColumn> = composite
                    .columns
                    .split(',')
                    .map(IndexColumn::parse)
                    .collect();
                let col_names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
                let idx_name = composite
                    .name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| crate::unique_index_name(&table_name, &col_names));
                indices.push(Index {
                    name: idx_name,
                    columns: cols,
                    unique: true,
                    where_clause: composite.filter.map(|s| s.to_string()),
                });
            }
        }

        for field in struct_type.fields {
            let field_shape = field.shape.get();

            // Determine column name
            let col_name = field_get_dibs_attr_str(field, "column")
                .map(|s| s.to_string())
                .unwrap_or_else(|| field.name.to_string());

            // Determine if nullable (Option<T> types)
            let (inner_shape, nullable) = unwrap_option(field_shape);

            // Map type to Postgres
            let pg_type = match shape_to_pg_type(inner_shape) {
                Some(pg_type) => pg_type,
                None => {
                    eprintln!(
                        "dibs: unsupported type '{}' for column '{}' in table '{}' ({})",
                        inner_shape.type_identifier,
                        field.name,
                        table_name,
                        self.shape.source_file.unwrap_or("<unknown>")
                    );
                    return None;
                }
            };

            // Check for primary key
            let primary_key = field_has_dibs_attr(field, "pk");

            // Check for unique
            let unique = field_has_dibs_attr(field, "unique");

            // Check for default
            let default = field_get_dibs_attr_str(field, "default").map(|s| s.to_string());

            // Extract doc comment from field
            let doc = if field.doc.is_empty() {
                None
            } else {
                Some(field.doc.join("\n"))
            };

            // Detect auto-generated columns from default or annotation
            let auto_generated =
                is_auto_generated_default(&default) || field_has_dibs_attr(field, "auto");

            // Check for lang annotation (implies long)
            let lang = field_get_dibs_attr_str(field, "lang").map(|s| s.to_string());

            // Check for long text annotation (or implied by lang)
            let long = field_has_dibs_attr(field, "long") || lang.is_some();

            // Check for label annotation
            let label = field_has_dibs_attr(field, "label");

            // Check for subtype annotation
            let subtype = field_get_dibs_attr_str(field, "subtype").map(|s| s.to_string());

            // Check for explicit icon annotation, or derive from subtype
            let explicit_icon = field_get_dibs_attr_str(field, "icon").map(|s| s.to_string());
            let icon = explicit_icon.or_else(|| {
                subtype
                    .as_ref()
                    .and_then(|st| subtype_default_icon(st).map(|s| s.to_string()))
            });

            // Check for enum variants
            let enum_variants = extract_enum_variants(inner_shape);

            columns.push(Column {
                name: col_name.clone(),
                pg_type,
                rust_type: Some(inner_shape.type_identifier.to_string()),
                nullable,
                default,
                primary_key,
                unique,
                auto_generated,
                long,
                label,
                enum_variants,
                doc,
                lang,
                icon,
                subtype,
            });

            // Check for foreign key
            if let Some(fk_ref) = field_get_dibs_attr_str(field, "fk") {
                // Parse FK reference - supports both "table.column" and "table(column)" formats
                let parsed = parse_fk_reference(fk_ref);
                match parsed {
                    Some((ref_table, ref_col)) => {
                        foreign_keys.push(ForeignKey {
                            columns: vec![field.name.to_string()],
                            references_table: ref_table.to_string(),
                            references_columns: vec![ref_col.to_string()],
                        });
                    }
                    None => {
                        eprintln!(
                            "dibs: invalid FK format '{}' for field '{}' in table '{}' - expected 'table.column' or 'table(column)' ({})",
                            fk_ref,
                            field.name,
                            table_name,
                            self.shape.source_file.unwrap_or("<unknown>")
                        );
                    }
                }
            }

            // Check for field-level index
            if field_has_dibs_attr(field, "index") {
                let idx_name = field_get_dibs_attr_str(field, "index")
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| crate::index_name(&table_name, &[&col_name]));
                indices.push(Index {
                    name: idx_name,
                    columns: vec![IndexColumn::new(col_name.clone())],
                    unique: false,
                    where_clause: None, // Field-level indexes don't support WHERE clause
                });
            }
        }

        // Extract source location from Shape
        let source = SourceLocation {
            file: self.shape.source_file.map(|s| s.to_string()),
            line: self.shape.source_line,
            column: self.shape.source_column,
        };

        // Extract doc comment from Shape
        let doc = if self.shape.doc.is_empty() {
            None
        } else {
            Some(self.shape.doc.join("\n"))
        };

        // Extract container-level icon
        let icon = shape_get_dibs_attr_str(self.shape, "icon").map(|s| s.to_string());

        Some(Table {
            name: table_name,
            columns,
            foreign_keys,
            indices,
            source,
            doc,
            icon,
        })
    }
}

/// Unwrap Option<T> to get the inner type and nullability.
fn unwrap_option(shape: &'static Shape) -> (&'static Shape, bool) {
    // Check if this is an Option type by looking at the type identifier
    // Note: type_identifier is just the type name without generics, e.g., "Option"
    if shape.type_identifier == "Option"
        || shape.type_identifier == "core::option::Option"
        || shape.type_identifier == "std::option::Option"
    {
        // Get the inner shape from the Option's inner field
        if let Some(inner) = shape.inner {
            return (inner, true);
        }
    }
    (shape, false)
}

// Register TableDef with inventory
inventory::collect!(TableDef);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fk_reference_dot_format() {
        assert_eq!(parse_fk_reference("users.id"), Some(("users", "id")));
        assert_eq!(parse_fk_reference("shop.id"), Some(("shop", "id")));
        assert_eq!(
            parse_fk_reference("category.parent_id"),
            Some(("category", "parent_id"))
        );
    }

    #[test]
    fn test_parse_fk_reference_paren_format() {
        assert_eq!(parse_fk_reference("users(id)"), Some(("users", "id")));
        assert_eq!(parse_fk_reference("shop(id)"), Some(("shop", "id")));
        assert_eq!(
            parse_fk_reference("category(parent_id)"),
            Some(("category", "parent_id"))
        );
    }

    #[test]
    fn test_parse_fk_reference_invalid() {
        assert_eq!(parse_fk_reference(""), None);
        assert_eq!(parse_fk_reference("users"), None);
        assert_eq!(parse_fk_reference(".id"), None);
        assert_eq!(parse_fk_reference("users."), None);
        assert_eq!(parse_fk_reference("(id)"), None);
        assert_eq!(parse_fk_reference("users("), None);
        assert_eq!(parse_fk_reference("users()"), None);
        assert_eq!(parse_fk_reference("()"), None);
    }

    #[test]
    fn test_index_column_parse_simple() {
        let col = IndexColumn::parse("name");
        assert_eq!(col.name, "name");
        assert_eq!(col.order, SortOrder::Asc);
        assert_eq!(col.nulls, NullsOrder::Default);
    }

    #[test]
    fn test_index_column_parse_desc() {
        let col = IndexColumn::parse("created_at DESC");
        assert_eq!(col.name, "created_at");
        assert_eq!(col.order, SortOrder::Desc);
        assert_eq!(col.nulls, NullsOrder::Default);
    }

    #[test]
    fn test_index_column_parse_asc() {
        let col = IndexColumn::parse("id ASC");
        assert_eq!(col.name, "id");
        assert_eq!(col.order, SortOrder::Asc);
        assert_eq!(col.nulls, NullsOrder::Default);
    }

    #[test]
    fn test_index_column_parse_nulls_first() {
        let col = IndexColumn::parse("reminder_sent_at NULLS FIRST");
        assert_eq!(col.name, "reminder_sent_at");
        assert_eq!(col.order, SortOrder::Asc);
        assert_eq!(col.nulls, NullsOrder::First);
    }

    #[test]
    fn test_index_column_parse_nulls_last() {
        let col = IndexColumn::parse("score NULLS LAST");
        assert_eq!(col.name, "score");
        assert_eq!(col.order, SortOrder::Asc);
        assert_eq!(col.nulls, NullsOrder::Last);
    }

    #[test]
    fn test_index_column_parse_desc_nulls_first() {
        let col = IndexColumn::parse("priority DESC NULLS FIRST");
        assert_eq!(col.name, "priority");
        assert_eq!(col.order, SortOrder::Desc);
        assert_eq!(col.nulls, NullsOrder::First);
    }

    #[test]
    fn test_index_column_parse_desc_nulls_last() {
        let col = IndexColumn::parse("updated_at DESC NULLS LAST");
        assert_eq!(col.name, "updated_at");
        assert_eq!(col.order, SortOrder::Desc);
        assert_eq!(col.nulls, NullsOrder::Last);
    }

    #[test]
    fn test_index_column_parse_asc_nulls_first() {
        let col = IndexColumn::parse("nullable_col ASC NULLS FIRST");
        assert_eq!(col.name, "nullable_col");
        assert_eq!(col.order, SortOrder::Asc);
        assert_eq!(col.nulls, NullsOrder::First);
    }

    #[test]
    fn test_index_column_to_sql() {
        // Simple column
        let col = IndexColumn::new("name");
        assert_eq!(col.to_sql(), "\"name\"");

        // DESC
        let col = IndexColumn::desc("created_at");
        assert_eq!(col.to_sql(), "\"created_at\" DESC");

        // NULLS FIRST
        let col = IndexColumn::nulls_first("reminder_sent_at");
        assert_eq!(col.to_sql(), "\"reminder_sent_at\" NULLS FIRST");

        // DESC NULLS LAST
        let col = IndexColumn {
            name: "priority".to_string(),
            order: SortOrder::Desc,
            nulls: NullsOrder::Last,
        };
        assert_eq!(col.to_sql(), "\"priority\" DESC NULLS LAST");
    }
}
