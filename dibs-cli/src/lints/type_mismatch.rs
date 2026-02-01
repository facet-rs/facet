//! Lint: Type mismatches between params and columns.

use super::{DiagnosticBuilder, LintContext};
use dibs_proto::TableInfo;
use dibs_query_schema::*;

/// Check if a param type is compatible with a SQL column type.
fn types_compatible(param_type: &str, sql_type: &str) -> bool {
    match param_type {
        "string" => matches!(
            sql_type.to_uppercase().as_str(),
            "TEXT" | "VARCHAR" | "CHAR" | "CHARACTER VARYING"
        ),
        "int" => matches!(
            sql_type.to_uppercase().as_str(),
            "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "INT4" | "INT8" | "INT2"
        ),
        "bool" | "boolean" => matches!(sql_type.to_uppercase().as_str(), "BOOLEAN" | "BOOL"),
        "float" => matches!(
            sql_type.to_uppercase().as_str(),
            "FLOAT" | "DOUBLE" | "REAL" | "NUMERIC" | "DECIMAL" | "FLOAT4" | "FLOAT8"
        ),
        _ => true, // Unknown types are assumed compatible
    }
}

/// Infer the type of a literal value.
/// Returns None if it's a param reference (starts with $) or unknown.
fn infer_literal_type(value: &str) -> Option<&'static str> {
    if value.starts_with('$') {
        return None; // Param reference, not a literal
    }
    if value == "true" || value == "false" {
        return Some("boolean");
    }
    if value.parse::<i64>().is_ok() {
        return Some("int");
    }
    if value.parse::<f64>().is_ok() {
        return Some("float");
    }
    // Everything else is a string (quoted strings arrive without quotes in EqBare)
    Some("string")
}

fn param_type_name(param_type: &ParamType) -> String {
    match param_type {
        ParamType::String => "string".to_string(),
        ParamType::Int => "int".to_string(),
        ParamType::Bool => "bool".to_string(),
        ParamType::Uuid => "uuid".to_string(),
        ParamType::Decimal => "decimal".to_string(),
        ParamType::Timestamp => "timestamp".to_string(),
        ParamType::Bytes => "bytes".to_string(),
        ParamType::Optional(inner) => {
            if let Some(first) = inner.first() {
                format!("optional({})", param_type_name(first))
            } else {
                "optional".to_string()
            }
        }
    }
}

/// Check for literal type mismatches in where clause (no params needed).
pub fn lint_literal_types_in_where(
    where_clause: &Where,
    table: &TableInfo,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, filter) in &where_clause.filters {
        let Some(column) = table.columns.iter().find(|c| c.name == col_name.as_str()) else {
            continue;
        };

        // Check for literal type mismatch (EqBare with non-param value)
        if let FilterValue::EqBare(Some(meta)) = filter {
            let value = meta.as_str();
            if !value.starts_with('$') {
                // It's a literal, check type compatibility
                if let Some(literal_type) = infer_literal_type(value)
                    && !types_compatible(literal_type, &column.sql_type)
                {
                    DiagnosticBuilder::error("literal-type-mismatch")
                        .at(meta.span)
                        .msg(format!(
                            "type mismatch: literal '{}' is {} but column '{}' is {}",
                            value, literal_type, column.name, column.sql_type
                        ))
                        .emit(ctx.diagnostics);
                }
            }
        }
    }
}

/// Check for param type mismatches in where clause.
pub fn lint_param_types_in_where(
    where_clause: &Where,
    table: &TableInfo,
    params: &Params,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, filter) in &where_clause.filters {
        let Some(column) = table.columns.iter().find(|c| c.name == col_name.as_str()) else {
            continue;
        };

        // Extract param name from filter
        let param_name = match filter {
            FilterValue::EqBare(Some(meta)) => meta.as_str().strip_prefix('$'),
            FilterValue::EqBare(None) => Some(col_name.as_str()),
            FilterValue::Eq(args)
            | FilterValue::Ilike(args)
            | FilterValue::Like(args)
            | FilterValue::Gt(args)
            | FilterValue::Lt(args)
            | FilterValue::Gte(args)
            | FilterValue::Lte(args)
            | FilterValue::Ne(args) => args.first().and_then(|a| a.as_str().strip_prefix('$')),
            _ => None,
        };

        if let Some(param_name) = param_name
            && let Some((param_meta, param_type)) =
                params.params.iter().find(|(k, _)| k.as_str() == param_name)
        {
            let type_name = param_type_name(param_type);
            if !types_compatible(&type_name, &column.sql_type) {
                DiagnosticBuilder::error("param-type-mismatch")
                    .at(param_meta.span)
                    .msg(format!(
                        "type mismatch: param '{}' is @{} but column '{}' is {}",
                        param_name, type_name, column.name, column.sql_type
                    ))
                    .emit(ctx.diagnostics);
            }
        }
    }
}
