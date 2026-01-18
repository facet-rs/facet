//! Schema definition and introspection.
//!
//! Define tables using facet attributes:
//!
//! ```ignore
//! use dibs::prelude::*;
//! use facet::Facet;
//!
//! #[derive(Facet)]
//! #[facet(dibs::table = "users")]
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
    }

    /// Composite index definition for multi-column indices.
    pub struct CompositeIndex {
        /// Optional index name (auto-generated if not provided)
        pub name: Option<&'static str>,
        /// Comma-separated column names
        pub columns: &'static str,
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
}

impl std::fmt::Display for PgType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgType::SmallInt => write!(f, "SMALLINT"),
            PgType::Integer => write!(f, "INTEGER"),
            PgType::BigInt => write!(f, "BIGINT"),
            PgType::Real => write!(f, "REAL"),
            PgType::DoublePrecision => write!(f, "DOUBLE PRECISION"),
            PgType::Boolean => write!(f, "BOOLEAN"),
            PgType::Text => write!(f, "TEXT"),
            PgType::Bytea => write!(f, "BYTEA"),
            PgType::Timestamptz => write!(f, "TIMESTAMPTZ"),
            PgType::Date => write!(f, "DATE"),
            PgType::Time => write!(f, "TIME"),
            PgType::Uuid => write!(f, "UUID"),
            PgType::Jsonb => write!(f, "JSONB"),
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
    /// Doc comment (if any)
    pub doc: Option<String>,
}

/// A foreign key constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignKey {
    /// Column(s) in this table
    pub columns: Vec<String>,
    /// Referenced table
    pub references_table: String,
    /// Referenced column(s)
    pub references_columns: Vec<String>,
}

/// A database index.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    /// Index name
    pub name: String,
    /// Column(s) in the index
    pub columns: Vec<String>,
    /// Whether this is a unique index
    pub unique: bool,
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
                sql.push_str(&format!(
                    "ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({});\n",
                    table.name,
                    table.name,
                    fk.columns.join("_"),
                    fk.columns.join(", "),
                    fk.references_table,
                    fk.references_columns.join(", ")
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
        let mut sql = format!("CREATE TABLE {} (\n", self.name);

        let col_defs: Vec<String> = self
            .columns
            .iter()
            .map(|col| {
                let mut def = format!("    {} {}", col.name, col.pg_type);

                if col.primary_key {
                    def.push_str(" PRIMARY KEY");
                }

                if !col.nullable && !col.primary_key {
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
        sql.push_str("\n);");

        sql
    }

    /// Generate CREATE INDEX SQL statement for a given index.
    pub fn to_create_index_sql(&self, idx: &Index) -> String {
        let unique = if idx.unique { "UNIQUE " } else { "" };
        format!(
            "CREATE {}INDEX {} ON {} ({});",
            unique,
            idx.name,
            self.name,
            idx.columns.join(", ")
        )
    }
}

/// Map a Rust type name to a Postgres type.
pub fn rust_type_to_pg(type_name: &str) -> Option<PgType> {
    match type_name {
        "i16" => Some(PgType::SmallInt),
        "i32" => Some(PgType::Integer),
        "i64" => Some(PgType::BigInt),
        "f32" => Some(PgType::Real),
        "f64" => Some(PgType::DoublePrecision),
        "bool" => Some(PgType::Boolean),
        "String" | "&str" => Some(PgType::Text),
        "Vec<u8>" | "&[u8]" => Some(PgType::Bytea),
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
            if attr.ns == Some("dibs") && attr.key == "composite_index" {
                // The attribute data is Attr::CompositeIndex(CompositeIndex{...})
                if let Some(Attr::CompositeIndex(composite)) = attr.get_as::<Attr>() {
                    let cols: Vec<String> = composite
                        .columns
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    let idx_name = composite
                        .name
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("idx_{}_{}", table_name, cols.join("_")));
                    indices.push(Index {
                        name: idx_name,
                        columns: cols,
                        unique: false,
                    });
                }
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
            let pg_type = rust_type_to_pg(inner_shape.type_identifier)?;

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

            columns.push(Column {
                name: col_name.clone(),
                pg_type,
                rust_type: Some(inner_shape.type_identifier.to_string()),
                nullable,
                default,
                primary_key,
                unique,
                doc,
            });

            // Check for foreign key
            if let Some(fk_ref) = field_get_dibs_attr_str(field, "fk") {
                // Parse "table.column" format
                if let Some((ref_table, ref_col)) = fk_ref.split_once('.') {
                    foreign_keys.push(ForeignKey {
                        columns: vec![field.name.to_string()],
                        references_table: ref_table.to_string(),
                        references_columns: vec![ref_col.to_string()],
                    });
                }
            }

            // Check for field-level index
            if field_has_dibs_attr(field, "index") {
                let idx_name = field_get_dibs_attr_str(field, "index")
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("idx_{}_{}", table_name, col_name));
                indices.push(Index {
                    name: idx_name,
                    columns: vec![col_name.clone()],
                    unique: false,
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

        Some(Table {
            name: table_name,
            columns,
            foreign_keys,
            indices,
            source,
            doc,
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
