//! Filter argument specification and parsing.

use crate::{QError, QErrorKind, QSource};
use dibs_query_schema::{Meta, Span};
use std::sync::Arc;

/// Specification of what kind of argument is valid at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgSpec {
    /// Accepts a variable reference (starting with $) or a literal value.
    VariableOrLiteral,

    /// Accepts only a variable reference.
    Variable,

    /// Accepts only a literal value.
    Literal,
}

/// Parsed filter argument.
#[derive(Debug, Clone)]
pub enum FilterArg {
    /// A variable reference like $handle (stored without the $).
    Variable(String),

    /// A literal value.
    Literal(String),
}

impl FilterArg {
    /// Parse a string into a FilterArg.
    ///
    /// If the string starts with `$`, it's a variable reference (the `$` is stripped).
    /// Otherwise, it's a literal value.
    pub fn parse(s: &str) -> Self {
        if let Some(var_name) = s.strip_prefix('$') {
            FilterArg::Variable(var_name.to_string())
        } else {
            FilterArg::Literal(s.to_string())
        }
    }

    /// Returns true if this is a variable reference.
    pub fn is_variable(&self) -> bool {
        matches!(self, FilterArg::Variable(_))
    }

    /// Returns true if this is a literal value.
    pub fn is_literal(&self) -> bool {
        matches!(self, FilterArg::Literal(_))
    }

    /// Get the inner value (variable name without $ or literal value).
    pub fn as_str(&self) -> &str {
        match self {
            FilterArg::Variable(s) | FilterArg::Literal(s) => s,
        }
    }
}

/// Specification for a filter function.
pub struct FunctionSpec {
    pub name: &'static str,
    pub args: &'static [ArgSpec],
}

impl FunctionSpec {
    /// Validate and parse arguments according to this spec.
    ///
    /// Returns rich errors if argument count doesn't match or argument types don't match spec.
    pub fn parse_args(
        &self,
        source: Arc<QSource>,
        span: Span,
        args: &[Meta<String>],
    ) -> Result<Vec<FilterArg>, QError> {
        // Check argument count
        if args.len() != self.args.len() {
            return Err(QError {
                source,
                span,
                kind: QErrorKind::InvalidFilterArgCount {
                    filter: self.name.to_string(),
                    expected: self.args.len(),
                    actual: args.len(),
                },
            });
        }

        // Parse each argument according to its spec
        let mut parsed = Vec::with_capacity(args.len());
        for (i, (arg_meta, spec)) in args.iter().zip(self.args.iter()).enumerate() {
            let filter_arg = FilterArg::parse(arg_meta.as_str());

            // Validate against spec
            match spec {
                ArgSpec::VariableOrLiteral => {
                    // Accepts both, no validation needed
                }
                ArgSpec::Variable => {
                    if !filter_arg.is_variable() {
                        return Err(QError {
                            source: source.clone(),
                            span,
                            kind: QErrorKind::InvalidFilterArgType {
                                filter: self.name.to_string(),
                                reason: format!(
                                    "argument {i} must be a variable reference (starting with $), got literal",
                                ),
                            },
                        });
                    }
                }
                ArgSpec::Literal => {
                    if !filter_arg.is_literal() {
                        return Err(QError {
                            source: source.clone(),
                            span,
                            kind: QErrorKind::InvalidFilterArgType {
                                filter: self.name.to_string(),
                                reason: format!(
                                    "argument {i} must be a literal value, got variable reference",
                                ),
                            },
                        });
                    }
                }
            }

            parsed.push(filter_arg);
        }

        Ok(parsed)
    }
}

// Define function specs for all filter operations
pub const EQ_SPEC: FunctionSpec = FunctionSpec {
    name: "eq",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const NE_SPEC: FunctionSpec = FunctionSpec {
    name: "ne",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LT_SPEC: FunctionSpec = FunctionSpec {
    name: "lt",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LTE_SPEC: FunctionSpec = FunctionSpec {
    name: "lte",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const GT_SPEC: FunctionSpec = FunctionSpec {
    name: "gt",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const GTE_SPEC: FunctionSpec = FunctionSpec {
    name: "gte",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LIKE_SPEC: FunctionSpec = FunctionSpec {
    name: "like",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const ILIKE_SPEC: FunctionSpec = FunctionSpec {
    name: "ilike",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const IN_SPEC: FunctionSpec = FunctionSpec {
    name: "in",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const JSON_GET_SPEC: FunctionSpec = FunctionSpec {
    name: "json-get",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const JSON_GET_TEXT_SPEC: FunctionSpec = FunctionSpec {
    name: "json-get-text",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const CONTAINS_SPEC: FunctionSpec = FunctionSpec {
    name: "contains",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const KEY_EXISTS_SPEC: FunctionSpec = FunctionSpec {
    name: "key-exists",
    args: &[ArgSpec::VariableOrLiteral],
};

/// EqBare is a special case - it's bare equality with a single optional argument.
pub const EQ_BARE_SPEC: FunctionSpec = FunctionSpec {
    name: "eq-bare",
    args: &[ArgSpec::VariableOrLiteral],
};

use dibs_query_schema::FilterValue;

/// Get the function spec for a filter value, along with its arguments.
///
/// Returns None for filters that take no arguments (Null, NotNull) or for EqBare.
fn get_spec_and_args(
    filter_value: &FilterValue,
) -> Option<(&'static FunctionSpec, &[Meta<String>])> {
    match filter_value {
        FilterValue::Null | FilterValue::NotNull => None,
        FilterValue::Eq(args) => Some((&EQ_SPEC, args)),
        FilterValue::Ne(args) => Some((&NE_SPEC, args)),
        FilterValue::Lt(args) => Some((&LT_SPEC, args)),
        FilterValue::Lte(args) => Some((&LTE_SPEC, args)),
        FilterValue::Gt(args) => Some((&GT_SPEC, args)),
        FilterValue::Gte(args) => Some((&GTE_SPEC, args)),
        FilterValue::Like(args) => Some((&LIKE_SPEC, args)),
        FilterValue::Ilike(args) => Some((&ILIKE_SPEC, args)),
        FilterValue::In(args) => Some((&IN_SPEC, args)),
        FilterValue::JsonGet(args) => Some((&JSON_GET_SPEC, args)),
        FilterValue::JsonGetText(args) => Some((&JSON_GET_TEXT_SPEC, args)),
        FilterValue::Contains(args) => Some((&CONTAINS_SPEC, args)),
        FilterValue::KeyExists(args) => Some((&KEY_EXISTS_SPEC, args)),
        FilterValue::EqBare(_) => None, // EqBare is handled specially (optional single arg)
    }
}

/// Validate a filter value's arguments according to its spec.
///
/// Returns the parsed FilterArgs on success, or an error with proper span information.
pub fn validate_filter(
    source: Arc<QSource>,
    filter_span: Span,
    filter_value: &FilterValue,
) -> Result<Option<Vec<FilterArg>>, QError> {
    match get_spec_and_args(filter_value) {
        Some((spec, args)) => {
            let parsed = spec.parse_args(source, filter_span, args)?;
            Ok(Some(parsed))
        }
        None => {
            // Null, NotNull, and EqBare don't need validation through specs
            // EqBare has special handling - validate the single optional argument
            if let FilterValue::EqBare(Some(meta)) = filter_value {
                // Just parse it to check it's valid (variable or literal)
                let _ = FilterArg::parse(&meta.value);
            }
            Ok(None)
        }
    }
}

use dibs_query_schema::Where;
use dibs_sql::ColumnName;

/// Validate all filters in a Where clause.
///
/// This should be called during query parsing/validation phase to catch
/// filter argument errors early with proper span information.
pub fn validate_where(source: Arc<QSource>, where_clause: &Where) -> Result<(), QError> {
    for (column_meta, filter_value) in &where_clause.filters {
        // Use the column's span for error reporting since that's where the filter is written
        validate_filter(source.clone(), column_meta.span, filter_value)?;
    }
    Ok(())
}

/// Validate filters in a relation's where clause.
///
/// Takes the relation where clause (which uses a different type) and validates it.
pub fn validate_relation_where(
    source: Arc<QSource>,
    where_clause: &indexmap::IndexMap<Meta<ColumnName>, FilterValue>,
) -> Result<(), QError> {
    for (column_meta, filter_value) in where_clause {
        validate_filter(source.clone(), column_meta.span, filter_value)?;
    }
    Ok(())
}

use dibs_query_schema::{Decl, FieldDef, QueryFile, SelectFields};

/// Validate all filters in a QueryFile.
///
/// Recursively validates filters in:
/// - SELECT queries (top-level WHERE and relation WHERE clauses)
/// - UPDATE queries (WHERE clause)
/// - DELETE queries (WHERE clause)
///
/// This should be called after parsing to catch filter argument errors early.
pub fn validate_query_file(source: Arc<QSource>, query_file: &QueryFile) -> Result<(), QError> {
    for (_name_meta, decl) in &query_file.0 {
        match decl {
            Decl::Select(select) => {
                // Validate top-level WHERE clause
                if let Some(where_clause) = &select.where_clause {
                    validate_where(source.clone(), where_clause)?;
                }
                // Validate relation WHERE clauses recursively
                if let Some(fields) = &select.fields {
                    validate_select_fields(source.clone(), fields)?;
                }
            }
            Decl::Update(update) => {
                if let Some(where_clause) = &update.where_clause {
                    validate_where(source.clone(), where_clause)?;
                }
            }
            Decl::Delete(delete) => {
                if let Some(where_clause) = &delete.where_clause {
                    validate_where(source.clone(), where_clause)?;
                }
            }
            // Insert, InsertMany, Upsert, UpsertMany don't have WHERE clauses
            Decl::Insert(_) | Decl::InsertMany(_) | Decl::Upsert(_) | Decl::UpsertMany(_) => {}
        }
    }
    Ok(())
}

/// Recursively validate filters in SelectFields (handles nested relations).
fn validate_select_fields(source: Arc<QSource>, fields: &SelectFields) -> Result<(), QError> {
    for (_field_name, field_def) in &fields.fields {
        if let Some(FieldDef::Rel(relation)) = field_def {
            // Validate relation-level WHERE clause
            if let Some(where_clause) = &relation.where_clause {
                validate_where(source.clone(), where_clause)?;
            }
            // Recurse into nested relations
            if let Some(nested_fields) = &relation.fields {
                validate_select_fields(source.clone(), nested_fields)?;
            }
        }
    }
    Ok(())
}
