//! Lint: Relation (@rel) issues.

use super::{DiagnosticBuilder, LintContext};
use dibs_proto::SchemaInfo;
use dibs_query_schema::*;

/// Check if there's a FK relationship between two tables (in either direction).
fn has_fk_relationship(table_a: &str, table_b: &str, schema: &SchemaInfo) -> bool {
    // Check if table_a has FK pointing to table_b
    if let Some(table_info) = schema.tables.iter().find(|t| t.name == table_a) {
        for fk in &table_info.foreign_keys {
            if fk.references_table == table_b {
                return true;
            }
        }
    }

    // Check if table_b has FK pointing to table_a
    if let Some(table_info) = schema.tables.iter().find(|t| t.name == table_b) {
        for fk in &table_info.foreign_keys {
            if fk.references_table == table_a {
                return true;
            }
        }
    }

    false
}

pub fn lint_relation(rel: &Relation, parent_table: Option<&str>, ctx: &mut LintContext<'_>) {
    // first without order-by
    if let Some(first) = &rel.first
        && first.value
        && rel.order_by.is_none()
    {
        DiagnosticBuilder::warning("rel-first-without-order-by")
            .at(first.span)
            .msg("'first' in @rel without 'order-by' returns arbitrary row")
            .emit(ctx.diagnostics);
    }

    // FK relationship check
    if let (Some(parent), Some(from)) = (parent_table, rel.from.as_ref())
        && !has_fk_relationship(parent, from.as_str(), ctx.schema)
    {
        DiagnosticBuilder::error("no-fk-relationship")
            .at(from.span)
            .msg(format!(
                "no FK relationship between '{}' and '{}'",
                parent,
                from.as_str()
            ))
            .emit(ctx.diagnostics);
    }
}

/// Recursively lint relations in a select block.
pub fn lint_relations_in_select(
    select: &SelectFields,
    parent_table: Option<&str>,
    ctx: &mut LintContext<'_>,
) {
    for (_col_name, field_def) in &select.fields {
        if let Some(FieldDef::Rel(rel)) = field_def {
            lint_relation(rel, parent_table, ctx);

            // Recurse into nested selects
            if let Some(nested_select) = &rel.fields {
                let rel_table = rel.from.as_ref().map(|m| m.value.as_str());
                lint_relations_in_select(nested_select, rel_table, ctx);
            }
        }
    }
}
