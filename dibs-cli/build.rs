//! Build script for dibs-cli.
//!
//! Generates Styx schemas for the config and query DSL.

fn main() {
    // Re-run if schema source files change
    println!("cargo::rerun-if-changed=src/config.rs");
    println!("cargo::rerun-if-changed=src/query_schema.rs");

    // Generate config schema
    let config_schema = facet_styx::GenerateSchema::<crate::config::Config>::new()
        .crate_name("dibs")
        .version("1")
        .cli("dibs")
        .generate();

    // Generate query schema
    let query_schema = facet_styx::GenerateSchema::<crate::query_schema::QueryFile>::new()
        .crate_name("dibs-queries")
        .version("1")
        .cli("dibs")
        .generate();

    // Write both schemas to OUT_DIR for embedding
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    std::fs::write(
        std::path::Path::new(&out_dir).join("dibs-config.styx"),
        &config_schema,
    )
    .expect("Failed to write config schema");
    std::fs::write(
        std::path::Path::new(&out_dir).join("dibs-queries.styx"),
        &query_schema,
    )
    .expect("Failed to write query schema");

    // Write a combined file for embedding (styx-embed only supports single outdir file)
    let combined = format!(
        "# dibs-config.styx\n{}\n\n# dibs-queries.styx\n{}",
        config_schema, query_schema
    );
    std::fs::write(
        std::path::Path::new(&out_dir).join("dibs-schemas.styx"),
        combined,
    )
    .expect("Failed to write combined schema");
}

// Include the types for schema generation
mod config {
    use facet::Facet;

    /// Configuration loaded from `dibs.styx`.
    #[derive(Debug, Clone, Facet)]
    pub struct Config {
        /// Database crate configuration.
        #[facet(default)]
        pub db: DbConfig,
    }

    /// Database crate configuration.
    #[derive(Debug, Clone, Facet, Default)]
    pub struct DbConfig {
        /// Name of the crate containing schema definitions (e.g., "my-app-db").
        #[facet(rename = "crate")]
        pub crate_name: Option<String>,

        /// Path to a pre-built binary (for faster iteration).
        /// If not specified, we'll use `cargo run -p <crate_name>`.
        pub binary: Option<String>,
    }
}

mod query_schema {
    use facet::Facet;

    /// A file containing one or more query definitions.
    #[derive(Debug, Facet)]
    pub struct QueryFile {
        /// Query definitions in this file.
        #[facet(default)]
        pub queries: Vec<Query>,
    }

    /// A query definition.
    #[derive(Debug, Facet)]
    pub struct Query {
        /// Query parameters (inputs).
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

        /// LIMIT clause.
        #[facet(default)]
        pub limit: Option<LimitValue>,

        /// OFFSET clause.
        #[facet(default)]
        pub offset: Option<LimitValue>,

        /// Whether to return only the first result.
        #[facet(default)]
        pub first: Option<bool>,

        /// Fields to select from the query.
        pub select: SelectClause,

        /// Raw SQL escape hatch (heredoc).
        #[facet(default)]
        pub sql: Option<String>,

        /// Return type declaration for raw SQL queries.
        #[facet(default)]
        pub returns: Option<ReturnsClause>,
    }

    /// Query parameters block.
    #[derive(Debug, Facet)]
    pub struct Params {
        /// Individual parameter definitions.
        #[facet(flatten)]
        pub items: Vec<ParamDef>,
    }

    /// A single parameter definition.
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
        /// Optional parameter.
        #[facet(rename = "optional")]
        Optional(Box<ParamDef>),
    }

    /// WHERE clause.
    #[derive(Debug, Facet)]
    pub struct WhereClause {
        /// Filter conditions.
        #[facet(flatten)]
        pub filters: Vec<FilterCondition>,
    }

    /// A filter condition.
    #[derive(Debug, Facet)]
    #[facet(tag)]
    #[repr(u8)]
    pub enum FilterCondition {
        /// Equality comparison.
        #[facet(rename = "eq")]
        Eq { column: String, value: FilterValue },
        /// IS NULL check.
        #[facet(rename = "null")]
        IsNull { column: String },
        /// IS NOT NULL check.
        #[facet(rename = "not_null")]
        IsNotNull { column: String },
        /// ILIKE pattern matching.
        #[facet(rename = "ilike")]
        ILike { column: String, pattern: String },
        /// LIKE pattern matching.
        #[facet(rename = "like")]
        Like { column: String, pattern: String },
    }

    /// A value used in filter conditions.
    #[derive(Debug, Facet)]
    #[repr(u8)]
    pub enum FilterValue {
        /// Parameter reference.
        Param(String),
        /// String literal.
        String(String),
        /// Integer literal.
        Int(i64),
        /// Boolean literal.
        Bool(bool),
    }

    /// ORDER BY clause.
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

    /// Sort direction.
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
        /// Parameter reference.
        Param(String),
    }

    /// SELECT clause.
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
        #[facet(rename = "rel")]
        Relation {
            /// Relation name.
            name: String,
            /// Target table.
            #[facet(default)]
            from: Option<String>,
            /// Filter conditions.
            #[facet(default, rename = "where")]
            where_clause: Option<WhereClause>,
            /// ORDER BY for the relation.
            #[facet(default)]
            order_by: Option<OrderByClause>,
            /// Whether to return first row only.
            #[facet(default)]
            first: Option<bool>,
            /// Fields to select.
            select: SelectClause,
        },

        /// Count aggregate.
        #[facet(rename = "count")]
        Count {
            /// Field name for the count.
            name: String,
            /// Table to count from.
            table: String,
        },
    }

    /// Returns clause for raw SQL queries.
    #[derive(Debug, Facet)]
    pub struct ReturnsClause {
        /// Return field definitions.
        #[facet(flatten)]
        pub fields: Vec<ReturnField>,
    }

    /// A return field definition.
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
}
