//! SQL generation from query schema types.

mod common;
mod delete;
mod insert;
mod insert_many;
mod select;
mod update;
mod upsert;
mod upsert_many;

// Common helpers are used by submodules via `super::common::`
pub use delete::{GeneratedDelete, generate_delete_sql};
use indexmap::IndexMap;
pub use insert::{GeneratedInsert, generate_insert_sql};
pub use insert_many::{GeneratedInsertMany, generate_insert_many_sql};
pub use select::{GeneratedSelect, generate_select_sql};
pub use update::{GeneratedUpdate, generate_update_sql};
pub use upsert::{GeneratedUpsert, generate_upsert_sql};
pub use upsert_many::{GeneratedUpsertMany, generate_upsert_many_sql};

#[allow(unused_imports)]
use dibs_query_schema::{ParamType, ValueExpr};

use crate::QueryPlan;

/// Generated SQL with parameter placeholders.
#[derive(Debug, Clone)]
pub struct GeneratedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,

    /// Parameter names in order (maps to $1, $2, etc.).
    pub param_order: Vec<String>,

    /// Query plan (if JOINs are involved).
    pub plan: Option<QueryPlan>,

    /// Column names in SELECT order (for index-based access).
    /// Maps column names to their index in the result set.
    pub column_order: IndexMap<String, usize>,
}

/// Format a single filter condition from a column name and FilterValue.
///
/// Returns the SQL condition string and the updated param index.
fn format_filter(
    column: &str,
    filter_value: &dibs_query_schema::FilterValue,
    mut param_idx: usize,
    param_order: &mut Vec<dibs_sql::ParamName>,
) -> (String, usize) {
    use crate::FilterArg;
    use dibs_query_schema::FilterValue;

    // Handle dotted column names (e.g., "t0.column") by quoting each part
    let col = if column.contains('.') {
        column
            .split('.')
            .map(|part| format!("\"{part}\""))
            .collect::<Vec<_>>()
            .join(".")
    } else {
        format!("\"{column}\"")
    };

    /// Format a comparison operator with a single argument.
    fn format_comparison(
        col: &str,
        op: &str,
        args: &[dibs_query_schema::Meta<String>],
        param_idx: &mut usize,
        param_order: &mut Vec<dibs_sql::ParamName>,
    ) -> String {
        use crate::FilterArg;

        if let Some(arg) = args.first() {
            match FilterArg::parse(&arg.value) {
                FilterArg::Variable(name) => {
                    param_order.push(name.into());
                    let s = format!("{col} {op} ${}", *param_idx);
                    *param_idx += 1;
                    s
                }
                FilterArg::Literal(value) => {
                    let escaped = value.replace('\'', "''");
                    format!("{col} {op} '{escaped}'")
                }
            }
        } else {
            format!("{col} {op} NULL")
        }
    }

    let result = match filter_value {
        FilterValue::Null => format!("{col} IS NULL"),
        FilterValue::NotNull => format!("{col} IS NOT NULL"),
        FilterValue::Eq(args) => format_comparison(&col, "=", args, &mut param_idx, param_order),
        FilterValue::EqBare(opt_meta) => {
            if let Some(meta) = opt_meta {
                match FilterArg::parse(&meta.value) {
                    FilterArg::Variable(name) => {
                        param_order.push(name.into());
                        let s = format!("{col} = ${param_idx}");
                        param_idx += 1;
                        s
                    }
                    FilterArg::Literal(value) => {
                        let escaped = value.replace('\'', "''");
                        format!("{col} = '{escaped}'")
                    }
                }
            } else {
                format!("{col} IS NULL")
            }
        }
        FilterValue::Ne(args) => format_comparison(&col, "!=", args, &mut param_idx, param_order),
        FilterValue::Lt(args) => format_comparison(&col, "<", args, &mut param_idx, param_order),
        FilterValue::Lte(args) => format_comparison(&col, "<=", args, &mut param_idx, param_order),
        FilterValue::Gt(args) => format_comparison(&col, ">", args, &mut param_idx, param_order),
        FilterValue::Gte(args) => format_comparison(&col, ">=", args, &mut param_idx, param_order),
        FilterValue::Like(args) => {
            format_comparison(&col, "LIKE", args, &mut param_idx, param_order)
        }
        FilterValue::Ilike(args) => {
            format_comparison(&col, "ILIKE", args, &mut param_idx, param_order)
        }
        FilterValue::In(args) => {
            if let Some(arg) = args.first() {
                match FilterArg::parse(&arg.value) {
                    FilterArg::Variable(name) => {
                        param_order.push(name.into());
                        let s = format!("{col} = ANY(${param_idx})");
                        param_idx += 1;
                        s
                    }
                    FilterArg::Literal(value) => {
                        format!("{col} = ANY(ARRAY[{value}])")
                    }
                }
            } else {
                format!("{col} = ANY(ARRAY[])")
            }
        }
        FilterValue::JsonGet(args) => {
            format_comparison(&col, "->", args, &mut param_idx, param_order)
        }
        FilterValue::JsonGetText(args) => {
            format_comparison(&col, "->>", args, &mut param_idx, param_order)
        }
        FilterValue::Contains(args) => {
            format_comparison(&col, "@>", args, &mut param_idx, param_order)
        }
        FilterValue::KeyExists(args) => {
            format_comparison(&col, "?", args, &mut param_idx, param_order)
        }
    };

    (result, param_idx)
}

#[cfg(test)]
mod tests;
