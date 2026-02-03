//! Query DSL code generator for dibs.
//!
//! Parses `.styx` query files and generates Rust code + SQL.

// Error types
mod error;
pub use error::{QError, QErrorKind, QSource};

// Happy types;
pub use dibs_query_schema::*;

// Parse
mod parse;
pub use parse::parse_query_file;

// Query planner
mod planner;
pub(crate) use planner::{QueryPlan, QueryPlanner};

// SQL code generation
mod sqlgen;
pub use sqlgen::{
    GeneratedDelete, GeneratedInsert, GeneratedInsertMany, GeneratedSelect, GeneratedUpdate,
    GeneratedUpsert, GeneratedUpsertMany, SqlGenContext, generate_delete_sql,
    generate_insert_many_sql, generate_insert_sql, generate_select_sql, generate_update_sql,
    generate_upsert_many_sql, generate_upsert_sql,
};

// Rust code generation
mod rustgen;
pub use rustgen::{GeneratedCode, generate_rust_code};

// Filter argument parsing and validation
mod filter_spec;
pub use filter_spec::{
    ArgSpec, CONTAINS_SPEC, EQ_BARE_SPEC, EQ_SPEC, FilterArg, FunctionSpec, GT_SPEC, GTE_SPEC,
    ILIKE_SPEC, IN_SPEC, JSON_GET_SPEC, JSON_GET_TEXT_SPEC, KEY_EXISTS_SPEC, LIKE_SPEC, LT_SPEC,
    LTE_SPEC, NE_SPEC, validate_filter, validate_query_file, validate_relation_where,
    validate_where,
};
