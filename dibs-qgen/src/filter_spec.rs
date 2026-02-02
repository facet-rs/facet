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
