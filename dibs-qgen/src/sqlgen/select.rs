//! SQL generation for SELECT statements.

use crate::{QueryPlan, QueryPlanner};
use dibs_db_schema::Schema;
use dibs_query_schema::Select;
use dibs_sql::{ColumnName, ParamName};
use std::collections::HashMap;

use super::format_filter;

/// Generated SQL with parameter placeholders.
#[derive(Debug, Clone)]
pub struct GeneratedSelect {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub param_order: Vec<ParamName>,
    /// Query plan (for JOINs and result assembly).
    pub plan: QueryPlan,
    /// Column names in SELECT order (for index-based access).
    /// Maps column names to their index in the result set.
    pub column_order: HashMap<ColumnName, usize>,
}

/// Generate SQL for a SELECT query using the planner.
pub fn generate_select_sql(
    query: &Select,
    schema: &Schema,
) -> Result<GeneratedSelect, crate::planner::PlanError> {
    // Plan the query
    let planner = QueryPlanner::new(schema);
    let plan = planner.plan(query)?;

    let mut sql = String::new();
    let mut param_order: Vec<ParamName> = Vec::new();
    let mut param_idx = 1;
    let mut column_order: HashMap<ColumnName, usize> = HashMap::new();

    // Build column_order from plan's select_columns and count_subqueries
    let mut col_idx = 0;
    for col in &plan.select_columns {
        column_order.insert(col.result_alias.clone(), col_idx);
        col_idx += 1;
    }
    for count in &plan.count_subqueries {
        column_order.insert(count.result_alias.clone(), col_idx);
        col_idx += 1;
    }

    // SELECT with aliased columns
    sql.push_str("SELECT ");

    // DISTINCT or DISTINCT ON
    if let Some(distinct_on) = &query.distinct_on {
        if !distinct_on.0.is_empty() {
            sql.push_str("DISTINCT ON (");
            let distinct_cols: Vec<_> = distinct_on
                .0
                .iter()
                .map(|col| format!("\"t0\".\"{}\"", col.value))
                .collect();
            sql.push_str(&distinct_cols.join(", "));
            sql.push_str(") ");
        }
    } else if query.distinct.as_ref().map(|m| m.value).unwrap_or(false) {
        sql.push_str("DISTINCT ");
    }

    sql.push_str(&plan.select_sql());

    // FROM with JOINs (including relation filters in ON clauses)
    sql.push_str(" FROM ");
    sql.push_str(&plan.from_sql_with_params(&mut param_order, &mut param_idx));

    // WHERE
    if let Some(where_clause) = &query.where_clause {
        if !where_clause.filters.is_empty() {
            sql.push_str(" WHERE ");
            let conditions: Vec<_> = where_clause
                .filters
                .iter()
                .map(|(col_meta, filter_value)| {
                    // Prefix column with base table alias
                    let column = format!("t0.{}", col_meta.value);
                    let (cond, new_idx) =
                        format_filter(&column, filter_value, param_idx, &mut param_order);
                    param_idx = new_idx;
                    cond
                })
                .collect();
            sql.push_str(&conditions.join(" AND "));
        }
    }

    // ORDER BY
    if let Some(order_by) = &query.order_by {
        if !order_by.columns.is_empty() {
            sql.push_str(" ORDER BY ");
            let orders: Vec<_> = order_by
                .columns
                .iter()
                .map(|(col_meta, dir_opt)| {
                    let dir = dir_opt.as_ref().map(|d| d.value.as_str()).unwrap_or("asc");
                    let dir_sql = if dir.eq_ignore_ascii_case("desc") {
                        "DESC"
                    } else {
                        "ASC"
                    };
                    format!("\"t0\".\"{}\" {}", col_meta.value, dir_sql)
                })
                .collect();
            sql.push_str(&orders.join(", "));
        }
    }

    // LIMIT
    if let Some(limit) = &query.limit {
        sql.push_str(" LIMIT ");
        let limit_str = limit.value.as_str();
        if let Some(param_name) = limit_str.strip_prefix('$') {
            param_order.push(param_name.into());
            sql.push_str(&format!("${}", param_idx));
            param_idx += 1;
        } else {
            // Literal number
            sql.push_str(limit_str);
        }
    }

    // OFFSET
    if let Some(offset) = &query.offset {
        sql.push_str(" OFFSET ");
        let offset_str = offset.value.as_str();
        if let Some(param_name) = offset_str.strip_prefix('$') {
            param_order.push(param_name.into());
            sql.push_str(&format!("${}", param_idx));
            param_idx += 1;
        } else {
            // Literal number
            sql.push_str(offset_str);
        }
    }

    let _ = param_idx;

    Ok(GeneratedSelect {
        sql,
        param_order,
        plan,
        column_order,
    })
}
