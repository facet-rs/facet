//! Common SQL generation utilities shared across statement types.

use super::SqlGenContext;
use crate::filter_spec::{
    CONTAINS_SPEC, EQ_SPEC, GT_SPEC, GTE_SPEC, ILIKE_SPEC, IN_SPEC, JSON_GET_SPEC,
    JSON_GET_TEXT_SPEC, KEY_EXISTS_SPEC, LIKE_SPEC, LT_SPEC, LTE_SPEC, NE_SPEC,
};
use crate::{FilterArg, QError};
use dibs_query_schema::{FilterValue, Meta, Payload, Span, UpdateValue, ValueExpr, Where};
use dibs_sql::{BinOp, ColumnName, Expr, TableName};

/// Convert a column name (possibly qualified like "t0.id") to an Expr.
///
/// If the column name contains a dot, it's treated as "table.column".
/// Otherwise, it's an unqualified column reference.
fn column_name_to_expr(column: &ColumnName) -> Expr {
    let s = column.as_str();
    if let Some(dot_pos) = s.find('.') {
        let table: TableName = s[..dot_pos].into();
        let col: ColumnName = s[dot_pos + 1..].into();
        Expr::qualified_column(table, col)
    } else {
        Expr::column(column.clone())
    }
}

/// Extract the unqualified column name from a possibly qualified name.
///
/// "t0.id" -> "id"
/// "id" -> "id"
fn extract_column_name(column: &ColumnName) -> &str {
    let s = column.as_str();
    if let Some(dot_pos) = s.rfind('.') {
        &s[dot_pos + 1..]
    } else {
        s
    }
}

/// Convert a Meta<String> argument to an Expr.
///
/// If it starts with '$', it's a parameter reference.
/// Otherwise, it's a string literal, integer, or boolean.
pub fn meta_string_to_expr(meta: &Meta<String>) -> Expr {
    let s = &meta.value;
    if let Some(param_name) = s.strip_prefix('$') {
        Expr::param(param_name.into())
    } else {
        // Try to parse as integer
        if let Ok(n) = s.parse::<i64>() {
            Expr::int(n)
        } else if s == "true" {
            Expr::bool(true)
        } else if s == "false" {
            Expr::bool(false)
        } else {
            Expr::string(s)
        }
    }
}

/// Convert a FilterArg to an Expr.
fn filter_arg_to_expr(arg: &FilterArg) -> Expr {
    match arg {
        FilterArg::Variable(name) => Expr::param(name.as_str().into()),
        FilterArg::Literal(lit) => {
            // Try to parse as integer
            if let Ok(n) = lit.parse::<i64>() {
                Expr::int(n)
            } else if lit == "true" {
                Expr::bool(true)
            } else if lit == "false" {
                Expr::bool(false)
            } else {
                Expr::string(lit)
            }
        }
    }
}

/// Convert a FilterValue to a dibs_sql::Expr with validation.
///
/// Validates arguments using FunctionSpec::parse_args() and returns rich errors
/// with proper span information on validation failure.
pub fn filter_value_to_expr_validated(
    ctx: &SqlGenContext,
    column: &ColumnName,
    filter: &FilterValue,
    filter_span: Span,
) -> Result<Option<Expr>, QError> {
    let col = column_name_to_expr(column);
    let unqualified_name = extract_column_name(column);

    match filter {
        FilterValue::Null => Ok(Some(col.is_null())),
        FilterValue::NotNull => Ok(Some(col.is_not_null())),

        FilterValue::Eq(args) => {
            let parsed = EQ_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.eq(filter_arg_to_expr(arg))))
        }

        FilterValue::EqBare(opt_meta) => {
            if let Some(meta) = opt_meta {
                // Single argument - parse and validate
                let arg = FilterArg::parse(&meta.value);
                Ok(Some(col.eq(filter_arg_to_expr(&arg))))
            } else {
                // Shorthand: {id} means {id $id}
                Ok(Some(col.eq(Expr::param(unqualified_name.into()))))
            }
        }

        FilterValue::Ne(args) => {
            let parsed = NE_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Ne,
                right: Box::new(filter_arg_to_expr(arg)),
            }))
        }

        FilterValue::Lt(args) => {
            let parsed = LT_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Lt,
                right: Box::new(filter_arg_to_expr(arg)),
            }))
        }

        FilterValue::Lte(args) => {
            let parsed = LTE_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Le,
                right: Box::new(filter_arg_to_expr(arg)),
            }))
        }

        FilterValue::Gt(args) => {
            let parsed = GT_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Gt,
                right: Box::new(filter_arg_to_expr(arg)),
            }))
        }

        FilterValue::Gte(args) => {
            let parsed = GTE_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Ge,
                right: Box::new(filter_arg_to_expr(arg)),
            }))
        }

        FilterValue::Like(args) => {
            let parsed = LIKE_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.like(filter_arg_to_expr(arg))))
        }

        FilterValue::Ilike(args) => {
            let parsed = ILIKE_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.ilike(filter_arg_to_expr(arg))))
        }

        FilterValue::In(args) => {
            let parsed = IN_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.any(filter_arg_to_expr(arg))))
        }

        FilterValue::JsonGet(args) => {
            let parsed = JSON_GET_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.json_get(filter_arg_to_expr(arg))))
        }

        FilterValue::JsonGetText(args) => {
            let parsed = JSON_GET_TEXT_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.json_get_text(filter_arg_to_expr(arg))))
        }

        FilterValue::Contains(args) => {
            let parsed = CONTAINS_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.contains(filter_arg_to_expr(arg))))
        }

        FilterValue::KeyExists(args) => {
            let parsed = KEY_EXISTS_SPEC.parse_args(ctx.source.clone(), filter_span, args)?;
            let arg = &parsed[0];
            Ok(Some(col.key_exists(filter_arg_to_expr(arg))))
        }
    }
}

/// Convert a WHERE clause to a dibs_sql::Expr with validation.
///
/// Validates all filter arguments using FunctionSpec and returns rich errors.
pub fn where_to_expr_validated(
    ctx: &SqlGenContext,
    where_clause: &Where,
) -> Result<Option<Expr>, QError> {
    let mut exprs: Vec<Expr> = vec![];

    for (col_meta, filter_value) in &where_clause.filters {
        let col_name = &col_meta.value;
        if let Some(expr) =
            filter_value_to_expr_validated(ctx, col_name, filter_value, col_meta.span)?
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

/// Convert a ValueExpr to a dibs_sql::Expr.
///
/// Handles:
/// - `@default` -> DEFAULT
/// - `@funcname` -> FUNCNAME()
/// - `@funcname(args...)` -> FUNCNAME(args...)
/// - `$param` -> parameter reference
/// - literals -> string/int/bool literals
pub fn value_expr_to_expr(column: &ColumnName, expr: &Option<ValueExpr>) -> Expr {
    match expr {
        None => {
            // Shorthand: {col} means {col $col}
            Expr::param(column.as_str().into())
        }
        Some(ValueExpr::Default) => Expr::Default,
        Some(ValueExpr::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name or literal)
            (None, Some(Payload::Scalar(s))) => meta_string_to_expr(s),
            // Nullary function like @now
            (Some(name), None) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![],
            },
            // Function with args like @coalesce($a, $b)
            (Some(name), Some(Payload::Seq(args))) => {
                let sql_args: Vec<Expr> = args
                    .iter()
                    .map(|a| value_expr_to_expr(column, &Some(a.clone())))
                    .collect();
                Expr::FnCall {
                    name: name.to_uppercase(),
                    args: sql_args,
                }
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![meta_string_to_expr(s)],
            },
            // Shouldn't happen but handle gracefully
            (None, None) => Expr::Null,
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => Expr::Null,
        },
    }
}

/// Convert an UpdateValue to a dibs_sql::Expr.
///
/// Similar to value_expr_to_expr but for the update clause in upserts.
pub fn update_value_to_expr(column: &ColumnName, expr: &Option<UpdateValue>) -> Expr {
    match expr {
        None => {
            // Shorthand: {col} means use EXCLUDED.col
            Expr::excluded(column.clone())
        }
        Some(UpdateValue::Default) => Expr::Default,
        Some(UpdateValue::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name or literal)
            (None, Some(Payload::Scalar(s))) => meta_string_to_expr(s),
            // Nullary function like @now
            (Some(name), None) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![],
            },
            // Function with args
            (Some(name), Some(Payload::Seq(args))) => {
                // Convert each arg - but we need ValueExpr not UpdateValue
                // For now, handle the common cases
                let sql_args: Vec<Expr> = args
                    .iter()
                    .map(|a| value_expr_to_expr(column, &Some(a.clone())))
                    .collect();
                Expr::FnCall {
                    name: name.to_uppercase(),
                    args: sql_args,
                }
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![meta_string_to_expr(s)],
            },
            // Shouldn't happen but handle gracefully
            (None, None) => Expr::Null,
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => Expr::Null,
        },
    }
}
