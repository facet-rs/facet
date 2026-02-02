//! Database schema types for dibs.
//!
//! This crate contains the core schema types that are shared between
//! `dibs` (schema introspection) and `dibs-qgen` (query planning).

use dibs_sql::{check_constraint_name, index_name, trigger_check_name, unique_index_name};
use facet::{Facet, Shape, Type, UserType};
use indexmap::IndexMap;
use std::fmt;

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

        /// Creates a CHECK constraint (container-level).
        ///
        /// Usage:
        /// - `#[facet(dibs::check(expr = "foo IS NOT NULL"))]` - auto-named constraint
        /// - `#[facet(dibs::check(name = "ck_foo", expr = "foo IS NOT NULL"))]` - named constraint
        Check(Check),

        /// Creates a trigger-enforced invariant check (container-level).
        ///
        /// This is for cross-row or cross-table invariants that cannot be expressed as
        /// a SQL CHECK constraint (which is limited to the current row).
        ///
        /// Usage:
        /// - `#[facet(dibs::trigger_check(name = "trg_my_check", expr = "NEW.foo IS NULL OR EXISTS (...)"))]`
        TriggerCheck(TriggerCheck),

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

    /// CHECK constraint definition.
    pub struct Check {
        /// Optional constraint name (auto-generated if not provided)
        pub name: Option<&'static str>,
        /// SQL expression for CHECK(...)
        pub expr: &'static str,
    }

    /// Trigger-enforced check definition.
    pub struct TriggerCheck {
        /// Optional trigger name (auto-generated if not provided)
        pub name: Option<&'static str>,
        /// Boolean SQL expression evaluated in a `BEFORE INSERT OR UPDATE` trigger.
        ///
        /// Use `NEW.<column>` to reference the new row, and `OLD.<column>` for updates.
        pub expr: &'static str,
        /// Optional error message raised when the expression evaluates to false.
        pub message: Option<&'static str>,
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

impl PgType {
    /// Map this Postgres type to a Rust type string.
    ///
    /// These names match what's exported in `dibs_runtime::prelude`.
    pub fn to_rust_type(&self) -> &'static str {
        match self {
            PgType::SmallInt => "i16",
            PgType::Integer => "i32",
            PgType::BigInt => "i64",
            PgType::Real => "f32",
            PgType::DoublePrecision => "f64",
            PgType::Numeric => "Decimal",
            PgType::Boolean => "bool",
            PgType::Text => "String",
            PgType::Bytea => "Vec<u8>",
            PgType::Timestamptz => "Timestamp",
            PgType::Date => "Date",
            PgType::Time => "Time",
            PgType::Uuid => "Uuid",
            PgType::Jsonb => "Jsonb<facet_value::Value>",
            PgType::TextArray => "Vec<String>",
            PgType::BigIntArray => "Vec<i64>",
            PgType::IntegerArray => "Vec<i32>",
        }
    }
}

impl fmt::Display for PgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    pub fn to_sql(&self, quote_ident: impl Fn(&str) -> String) -> String {
        format!(
            "{}{}{}",
            quote_ident(&self.name),
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

        fn unquote_pg_ident_if_quoted(s: &str) -> String {
            let s = s.trim();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                let inner = &s[1..s.len() - 1];
                return inner.replace("\"\"", "\"");
            }
            s.to_string()
        }

        Self {
            name: unquote_pg_ident_if_quoted(&name),
            order,
            nulls,
        }
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

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_string_short() {
            Some(s) => write!(f, "{}", s),
            None => write!(f, "<unknown>"),
        }
    }
}

/// A table CHECK constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckConstraint {
    pub name: String,
    pub expr: String,
}

/// A trigger-enforced invariant check (BEFORE INSERT OR UPDATE).
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerCheckConstraint {
    pub name: String,
    pub expr: String,
    pub message: Option<String>,
}

/// A database table definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Table name
    pub name: String,
    /// Columns
    pub columns: Vec<Column>,
    /// CHECK constraints
    pub check_constraints: Vec<CheckConstraint>,
    /// Trigger-enforced checks
    pub trigger_checks: Vec<TriggerCheckConstraint>,
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
    /// Tables in the schema, indexed by name
    pub tables: IndexMap<String, Table>,
}

impl Schema {
    /// Create a new empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a table by name.
    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Iterate over all tables.
    pub fn iter_tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.values()
    }
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
        let mut check_constraints = Vec::new();
        let mut trigger_checks = Vec::new();
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
                    .unwrap_or_else(|| index_name(&table_name, &col_names));
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
                    .unwrap_or_else(|| unique_index_name(&table_name, &col_names));
                indices.push(Index {
                    name: idx_name,
                    columns: cols,
                    unique: true,
                    where_clause: composite.filter.map(|s| s.to_string()),
                });
            }

            // Collect container-level CHECK constraints
            if attr.ns == Some("dibs")
                && attr.key == "check"
                && let Some(Attr::Check(check)) = attr.get_as::<Attr>()
            {
                let expr = unescape_rust_string_escapes(check.expr);
                let name = check
                    .name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| check_constraint_name(&table_name, &expr));
                check_constraints.push(CheckConstraint { name, expr });
            }

            // Collect container-level trigger-enforced checks
            if attr.ns == Some("dibs")
                && attr.key == "trigger_check"
                && let Some(Attr::TriggerCheck(trig)) = attr.get_as::<Attr>()
            {
                let expr = unescape_rust_string_escapes(trig.expr);
                let name = trig
                    .name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| trigger_check_name(&table_name, &expr));
                trigger_checks.push(TriggerCheckConstraint {
                    name,
                    expr,
                    message: trig.message.map(unescape_rust_string_escapes),
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
                        inner_shape,
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

            // Use pg_type's rust representation for consistency
            let rust_type = pg_type.to_rust_type().to_string();

            columns.push(Column {
                name: col_name.clone(),
                pg_type,
                rust_type: Some(rust_type),
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
                        // FIXME: ... nice error handling you've got there
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
            check_constraints,
            trigger_checks,
            foreign_keys,
            indices,
            source,
            doc,
            icon,
        })
    }
}

/// Unwrap Option<T> to get the inner type and nullability.
fn unwrap_option(lhs: &'static Shape) -> (&'static Shape, bool) {
    let rhs = Option::<()>::SHAPE;

    if lhs.decl_id == rhs.decl_id {
        // Get the inner shape from the Option's inner field
        if let Some(inner) = lhs.inner {
            return (inner, true);
        }
    }
    (lhs, false)
}

#[test]
fn test_unwrap_option() {
    let (inner, success) = unwrap_option(Option::<dibs_jsonb::Jsonb<facet_value::Value>>::SHAPE);
    assert!(success);
    assert_eq!(inner, dibs_jsonb::Jsonb::<facet_value::Value>::SHAPE);
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
    // FIXME: this isn't rigorous at all

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
fn extract_enum_variants(shape: &'static Shape) -> Vec<String> {
    if let Type::User(UserType::Enum(enum_type)) = shape.ty {
        enum_type
            .variants
            .iter()
            .map(|v| v.name.to_string())
            .collect()
    } else {
        vec![]
    }
}

fn unescape_rust_string_escapes(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }

    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(other) => {
                // Unknown escape - keep it as-is.
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }

    out
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
    if shape.decl_id == dibs_jsonb::Jsonb::<()>::SHAPE.decl_id {
        return Some(PgType::Jsonb);
    }

    // Check for Vec<T> types - shape.def is List
    if matches!(&shape.def, facet::Def::List(_)) {
        if let Some(inner) = shape.inner {
            if inner == u8::SHAPE {
                return Some(PgType::Bytea);
            } else if inner == String::SHAPE {
                return Some(PgType::TextArray);
            } else if inner == i64::SHAPE {
                return Some(PgType::BigIntArray);
            } else if inner == i32::SHAPE {
                return Some(PgType::IntegerArray);
            }
        }
        return None;
    }

    // Check for slice &[u8] (bytea)
    if matches!(&shape.def, facet::Def::Slice(_)) {
        if let Some(inner) = shape.inner
            && inner == u8::SHAPE
        {
            return Some(PgType::Bytea);
        }
        return None;
    }

    // Fall back to type matching
    rust_type_to_pg(shape)
}

/// Map a Rust type name to a Postgres type.
pub fn rust_type_to_pg(shape: &Shape) -> Option<PgType> {
    // Integers: SmallInt (2 bytes)
    if shape == i8::SHAPE || shape == u8::SHAPE || shape == i16::SHAPE {
        Some(PgType::SmallInt)
    // Integers: Integer (4 bytes)
    } else if shape == u16::SHAPE || shape == i32::SHAPE {
        Some(PgType::Integer)
    // Integers: BigInt (8 bytes)
    } else if shape == u32::SHAPE
        || shape == i64::SHAPE
        || shape == u64::SHAPE
        || shape == isize::SHAPE
        || shape == usize::SHAPE
    {
        Some(PgType::BigInt)
    // Floats
    } else if shape == f32::SHAPE {
        Some(PgType::Real)
    } else if shape == f64::SHAPE {
        Some(PgType::DoublePrecision)
    } else if shape == bool::SHAPE {
        Some(PgType::Boolean)
    } else if shape == String::SHAPE {
        Some(PgType::Text)
    } else if shape == rust_decimal::Decimal::SHAPE {
        Some(PgType::Numeric)
    } else if shape == jiff::Timestamp::SHAPE || shape == jiff::Zoned::SHAPE {
        Some(PgType::Timestamptz)
    } else if shape == jiff::civil::Date::SHAPE {
        Some(PgType::Date)
    } else if shape == jiff::civil::Time::SHAPE {
        Some(PgType::Time)
    } else if shape == chrono::DateTime::<chrono::Utc>::SHAPE
        || shape == chrono::DateTime::<chrono::Local>::SHAPE
        || shape == chrono::NaiveDateTime::SHAPE
    {
        Some(PgType::Timestamptz)
    } else if shape == chrono::NaiveDate::SHAPE {
        Some(PgType::Date)
    } else if shape == chrono::NaiveTime::SHAPE {
        Some(PgType::Time)
    } else if shape == uuid::Uuid::SHAPE {
        Some(PgType::Uuid)
    } else {
        None
    }
}

// Register TableDef with inventory so it can be collected across crates
inventory::collect!(TableDef);

#[cfg(test)]
mod tests;
