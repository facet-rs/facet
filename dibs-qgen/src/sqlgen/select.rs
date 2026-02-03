//! SQL generation for SELECT statements.

use super::SqlGenContext;
use super::common::{filter_value_to_expr_validated, meta_string_to_expr};
use crate::{QError, QueryPlan, QueryPlanner};
use dibs_query_schema::{OrderBy as QueryOrderBy, Select, Where};
use dibs_sql::{
    ColumnName, Expr, FromClause, Join, JoinKind, OrderBy, ParamName, SelectColumn, SelectStmt,
    render,
};
use std::collections::HashMap;

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
pub fn generate_select_sql(ctx: &SqlGenContext, query: &Select) -> Result<GeneratedSelect, QError> {
    // Plan the query
    let planner = QueryPlanner::new(ctx.schema);
    let plan = planner.plan(query).map_err(|e| {
        use crate::{QErrorKind, planner::PlanError};
        let kind = match e {
            PlanError::TableNotFound { table } => QErrorKind::TableNotFound { table },
            PlanError::NoForeignKey { from, to } => QErrorKind::PlanMissing {
                reason: format!("no FK relationship between {from} and {to}"),
            },
            PlanError::RelationNeedsFrom { relation } => QErrorKind::PlanMissing {
                reason: format!("relation '{relation}' requires explicit 'from' clause"),
            },
        };
        crate::QError {
            source: ctx.source.clone(),
            span: query
                .from
                .as_ref()
                .map(|f| f.span)
                .unwrap_or(dibs_query_schema::Span { offset: 0, len: 0 }),
            kind,
        }
    })?;

    // Build column_order from plan's select_columns and count_subqueries
    let mut column_order: HashMap<ColumnName, usize> = HashMap::new();
    let mut col_idx = 0;
    for col in &plan.select_columns {
        column_order.insert(col.result_alias.clone(), col_idx);
        col_idx += 1;
    }
    for count in &plan.count_subqueries {
        column_order.insert(count.result_alias.clone(), col_idx);
        col_idx += 1;
    }

    // Build SelectStmt using builder API
    let mut stmt = SelectStmt::new();

    // DISTINCT / DISTINCT ON
    if let Some(distinct_on) = &query.distinct_on {
        if !distinct_on.0.is_empty() {
            let cols: Vec<Expr> = distinct_on
                .0
                .iter()
                .map(|col| Expr::qualified_column("t0".into(), col.value.clone()))
                .collect();
            stmt = stmt.distinct_on(cols);
        }
    } else if query.distinct.as_ref().map(|m| m.value).unwrap_or(false) {
        stmt = stmt.distinct();
    }

    // SELECT columns (from plan)
    for col in &plan.select_columns {
        stmt = stmt.column(SelectColumn::aliased(
            Expr::qualified_column(col.table_alias.as_str().into(), col.column.clone()),
            col.result_alias.clone(),
        ));
    }

    // COUNT subqueries (rendered as raw SQL for now - complex subqueries)
    for count in &plan.count_subqueries {
        let subquery = format!(
            "(SELECT COUNT(*) FROM \"{}\" WHERE \"{}\" = \"{}\".\"{}\")",
            count.count_table, count.fk_column, count.parent_alias, count.parent_key
        );
        stmt = stmt.column(SelectColumn::aliased(
            Expr::Raw(subquery),
            count.result_alias.clone(),
        ));
    }

    // FROM (from plan)
    stmt = stmt.from(FromClause::aliased(
        plan.from_table.clone(),
        plan.from_alias.as_str().into(),
    ));

    // JOINs (from plan)
    for join_clause in &plan.joins {
        // Parse ON condition: stored as "alias.column" strings
        let (left_alias, left_col) = parse_qualified_column(&join_clause.on_condition.0);
        let (right_alias, right_col) = parse_qualified_column(&join_clause.on_condition.1);

        let mut on_expr = Expr::qualified_column(left_alias.into(), left_col.into())
            .eq(Expr::qualified_column(right_alias.into(), right_col.into()));

        // Add extra conditions from relation-level WHERE
        for cond in &join_clause.extra_conditions {
            let value_expr = match &cond.value {
                crate::planner::JoinConditionValue::Param(p) => Expr::param(p.clone()),
                crate::planner::JoinConditionValue::Literal(lit) => Expr::string(lit),
            };
            on_expr = on_expr.and(
                Expr::qualified_column(join_clause.alias.as_str().into(), cond.column.clone())
                    .eq(value_expr),
            );
        }

        let kind = match join_clause.join_type {
            crate::planner::JoinType::Left => JoinKind::Left,
            crate::planner::JoinType::Inner => JoinKind::Inner,
        };

        stmt = stmt.join(Join {
            kind,
            table: join_clause.table.clone(),
            alias: Some(join_clause.alias.as_str().into()),
            on: on_expr,
        });
    }

    // WHERE
    if let Some(where_clause) = &query.where_clause
        && let Some(expr) = where_to_qualified_expr_validated(ctx, where_clause, "t0")?
    {
        stmt = stmt.where_(expr);
    }

    // ORDER BY
    if let Some(order_by) = &query.order_by {
        for order in order_by_to_ast(order_by, "t0") {
            stmt = stmt.order_by(order);
        }
    }

    // LIMIT
    if let Some(limit) = &query.limit {
        stmt = stmt.limit(meta_string_to_expr(limit));
    }

    // OFFSET
    if let Some(offset) = &query.offset {
        stmt = stmt.offset(meta_string_to_expr(offset));
    }

    // Render once
    let rendered = render(&stmt);

    Ok(GeneratedSelect {
        sql: rendered.sql,
        param_order: rendered.params,
        plan,
        column_order,
    })
}

/// Parse a qualified column string like "t0.id" into (alias, column).
fn parse_qualified_column(s: &str) -> (&str, &str) {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("", s)
    }
}

/// Convert a WHERE clause to a dibs_sql::Expr with qualified columns and validation.
fn where_to_qualified_expr_validated(
    ctx: &SqlGenContext,
    where_clause: &Where,
    table_alias: &str,
) -> Result<Option<Expr>, QError> {
    let mut exprs: Vec<Expr> = vec![];

    for (col_meta, filter_value) in &where_clause.filters {
        let col_name: ColumnName = col_meta.value.clone();
        // Create a qualified column name for the filter
        let qualified_col: ColumnName = format!("{}.{}", table_alias, col_name).into();
        if let Some(expr) =
            filter_value_to_expr_validated(ctx, &qualified_col, filter_value, col_meta.span)?
        {
            exprs.push(expr);
        }
    }

    // AND all expressions together
    let mut iter = exprs.into_iter();
    Ok(iter
        .next()
        .map(|first| iter.fold(first, |acc, expr| acc.and(expr))))
}

/// Convert ORDER BY clause to AST OrderBy items.
fn order_by_to_ast(order_by: &QueryOrderBy, table_alias: &str) -> Vec<OrderBy> {
    order_by
        .columns
        .iter()
        .map(|(col_meta, dir_opt)| {
            let expr = Expr::qualified_column(table_alias.into(), col_meta.value.clone());
            let dir = dir_opt.as_ref().map(|d| d.value.as_str()).unwrap_or("asc");
            if dir.eq_ignore_ascii_case("desc") {
                OrderBy::desc(expr)
            } else {
                OrderBy::asc(expr)
            }
        })
        .collect()
}
