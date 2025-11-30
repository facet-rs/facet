//! Expression evaluator
//!
//! Evaluates template expressions against a Facet context.
//! Expressions → Values (not strings yet, that's the render phase).

use super::ast::*;
use super::error::{
    TemplateSource, TypeError, UndefinedError, UnknownFieldError, UnknownFilterError,
    UnknownTestError,
};
use miette::Result;
use std::collections::HashMap;

/// A runtime value in the template
#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Dict(HashMap<String, Value>),
    /// A Facet value (opaque, accessed via reflection)
    Facet(FacetValue),
    /// A "safe" value that should not be HTML-escaped when rendered
    Safe(Box<Value>),
}

/// A wrapped Facet value for template evaluation
#[derive(Debug, Clone)]
pub struct FacetValue {
    // TODO: This will hold a reference to the actual Facet data
    // For now, placeholder
    _placeholder: (),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Dict(d) => !d.is_empty(),
            Value::Facet(_) => true,
            Value::Safe(inner) => inner.is_truthy(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "none",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
            Value::Facet(_) => "object",
            Value::Safe(inner) => inner.type_name(),
        }
    }

    pub fn render_to_string(&self) -> String {
        match self {
            Value::None => "".to_string(),
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => s.clone(),
            Value::List(l) => {
                let items: Vec<_> = l.iter().map(|v| v.render_to_string()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Dict(_) => "[object]".to_string(),
            Value::Facet(_) => "[object]".to_string(),
            Value::Safe(inner) => inner.render_to_string(),
        }
    }

    /// Check if this value is marked as safe (should not be HTML-escaped)
    pub fn is_safe(&self) -> bool {
        matches!(self, Value::Safe(_))
    }
}

/// A global function that can be called from templates
pub type GlobalFn = Box<dyn Fn(&[Value], &[(String, Value)]) -> Result<Value> + Send + Sync>;

/// Evaluation context (variables in scope)
#[derive(Clone)]
pub struct Context {
    /// Variable scopes (innermost last)
    scopes: Vec<HashMap<String, Value>>,
    /// Global functions available in this context (shared via Arc)
    global_fns: std::sync::Arc<HashMap<String, std::sync::Arc<GlobalFn>>>,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("scopes", &self.scopes)
            .field(
                "global_fns",
                &format!("<{} functions>", self.global_fns.len()),
            )
            .finish()
    }
}

impl Context {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            global_fns: std::sync::Arc::new(HashMap::new()),
        }
    }

    /// Register a global function
    pub fn register_fn(&mut self, name: impl Into<String>, f: GlobalFn) {
        let fns = std::sync::Arc::make_mut(&mut self.global_fns);
        fns.insert(name.into(), std::sync::Arc::new(f));
    }

    /// Call a global function by name
    pub fn call_fn(
        &self,
        name: &str,
        args: &[Value],
        kwargs: &[(String, Value)],
    ) -> Option<Result<Value>> {
        self.global_fns.get(name).map(|f| f(args, kwargs))
    }

    /// Set a variable in the current scope
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), value);
        }
    }

    /// Get a variable (searches all scopes)
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value);
            }
        }
        None
    }

    /// Get all variable names (for error messages)
    pub fn available_vars(&self) -> Vec<String> {
        let mut vars: Vec<_> = self.scopes.iter().flat_map(|s| s.keys().cloned()).collect();
        vars.sort();
        vars.dedup();
        vars
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the innermost scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Expression evaluator
pub struct Evaluator<'a> {
    ctx: &'a Context,
    source: &'a TemplateSource,
}

impl<'a> Evaluator<'a> {
    pub fn new(ctx: &'a Context, source: &'a TemplateSource) -> Self {
        Self { ctx, source }
    }

    /// Evaluate an expression to a value
    pub fn eval(&self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Literal(lit) => self.eval_literal(lit),
            Expr::Var(ident) => self.eval_var(ident),
            Expr::Field(field) => self.eval_field(field),
            Expr::Index(index) => self.eval_index(index),
            Expr::Filter(filter) => self.eval_filter(filter),
            Expr::Binary(binary) => self.eval_binary(binary),
            Expr::Unary(unary) => self.eval_unary(unary),
            Expr::Call(call) => self.eval_call(call),
            Expr::Ternary(ternary) => self.eval_ternary(ternary),
            Expr::Test(test) => self.eval_test(test),
            Expr::MacroCall(_macro_call) => {
                // Macro calls are evaluated during rendering, not expression evaluation
                // Return None for now - the renderer will handle macro expansion
                Ok(Value::None)
            }
        }
    }

    fn eval_literal(&self, lit: &Literal) -> Result<Value> {
        Ok(match lit {
            Literal::None(_) => Value::None,
            Literal::Bool(b) => Value::Bool(b.value),
            Literal::Int(i) => Value::Int(i.value),
            Literal::Float(f) => Value::Float(f.value),
            Literal::String(s) => Value::String(s.value.clone()),
            Literal::List(l) => {
                let elements: Result<Vec<_>> = l.elements.iter().map(|e| self.eval(e)).collect();
                Value::List(elements?)
            }
            Literal::Dict(d) => {
                let mut map = HashMap::new();
                for (k, v) in &d.entries {
                    let key = self.eval(k)?.render_to_string();
                    let value = self.eval(v)?;
                    map.insert(key, value);
                }
                Value::Dict(map)
            }
        })
    }

    fn eval_var(&self, ident: &Ident) -> Result<Value> {
        self.ctx.get(&ident.name).cloned().ok_or_else(|| {
            UndefinedError {
                name: ident.name.clone(),
                available: self.ctx.available_vars(),
                span: ident.span,
                src: self.source.named_source(),
            }
            .into()
        })
    }

    fn eval_field(&self, field: &FieldExpr) -> Result<Value> {
        let base = self.eval(&field.base)?;

        match &base {
            Value::Dict(map) => map.get(&field.field.name).cloned().ok_or_else(|| {
                UnknownFieldError {
                    base_type: "dict".to_string(),
                    field: field.field.name.clone(),
                    known_fields: map.keys().cloned().collect(),
                    span: field.field.span,
                    src: self.source.named_source(),
                }
                .into()
            }),
            // TODO: Handle Facet values
            _ => Err(TypeError {
                expected: "object or dict".to_string(),
                found: base.type_name().to_string(),
                context: "field access".to_string(),
                span: field.base.span(),
                src: self.source.named_source(),
            })?,
        }
    }

    fn eval_index(&self, index: &IndexExpr) -> Result<Value> {
        let base = self.eval(&index.base)?;
        let idx = self.eval(&index.index)?;

        match (&base, &idx) {
            (Value::List(list), Value::Int(i)) => {
                let i = if *i < 0 {
                    (list.len() as i64 + *i) as usize
                } else {
                    *i as usize
                };
                list.get(i).cloned().ok_or_else(|| {
                    TypeError {
                        expected: format!("index < {}", list.len()),
                        found: format!("index {i}"),
                        context: "list index".to_string(),
                        span: index.index.span(),
                        src: self.source.named_source(),
                    }
                    .into()
                })
            }
            (Value::Dict(map), Value::String(key)) => map.get(key).cloned().ok_or_else(|| {
                UnknownFieldError {
                    base_type: "dict".to_string(),
                    field: key.clone(),
                    known_fields: map.keys().cloned().collect(),
                    span: index.index.span(),
                    src: self.source.named_source(),
                }
                .into()
            }),
            (Value::String(s), Value::Int(i)) => {
                let i = if *i < 0 {
                    (s.len() as i64 + *i) as usize
                } else {
                    *i as usize
                };
                s.chars()
                    .nth(i)
                    .map(|c| Value::String(c.to_string()))
                    .ok_or_else(|| {
                        TypeError {
                            expected: format!("index < {}", s.len()),
                            found: format!("index {i}"),
                            context: "string index".to_string(),
                            span: index.index.span(),
                            src: self.source.named_source(),
                        }
                        .into()
                    })
            }
            _ => Err(TypeError {
                expected: "list, dict, or string".to_string(),
                found: base.type_name().to_string(),
                context: "index access".to_string(),
                span: index.base.span(),
                src: self.source.named_source(),
            })?,
        }
    }

    fn eval_filter(&self, filter: &FilterExpr) -> Result<Value> {
        let value = self.eval(&filter.expr)?;
        let args: Result<Vec<_>> = filter.args.iter().map(|a| self.eval(a)).collect();
        let args = args?;

        let kwargs: Result<Vec<(String, Value)>> = filter
            .kwargs
            .iter()
            .map(|(ident, expr)| Ok((ident.name.clone(), self.eval(expr)?)))
            .collect();
        let kwargs = kwargs?;

        apply_filter(
            &filter.filter.name,
            value,
            &args,
            &kwargs,
            filter.filter.span,
            self.source,
        )
    }

    fn eval_binary(&self, binary: &BinaryExpr) -> Result<Value> {
        // Short-circuit for and/or
        match binary.op {
            BinaryOp::And => {
                let left = self.eval(&binary.left)?;
                if !left.is_truthy() {
                    return Ok(left);
                }
                return self.eval(&binary.right);
            }
            BinaryOp::Or => {
                let left = self.eval(&binary.left)?;
                if left.is_truthy() {
                    return Ok(left);
                }
                return self.eval(&binary.right);
            }
            _ => {}
        }

        let left = self.eval(&binary.left)?;
        let right = self.eval(&binary.right)?;

        Ok(match binary.op {
            BinaryOp::Add => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                (Value::String(a), Value::String(b)) => Value::String(format!("{a}{b}")),
                (Value::List(a), Value::List(b)) => {
                    let mut result = a.clone();
                    result.extend(b.clone());
                    Value::List(result)
                }
                _ => Value::None,
            },
            BinaryOp::Sub => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
                (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 - b),
                (Value::Float(a), Value::Int(b)) => Value::Float(a - *b as f64),
                _ => Value::None,
            },
            BinaryOp::Mul => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
                (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 * b),
                (Value::Float(a), Value::Int(b)) => Value::Float(a * *b as f64),
                (Value::String(s), Value::Int(n)) | (Value::Int(n), Value::String(s)) => {
                    Value::String(s.repeat(*n as usize))
                }
                _ => Value::None,
            },
            BinaryOp::Div => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Float(*a as f64 / *b as f64),
                (Value::Float(a), Value::Float(b)) if *b != 0.0 => Value::Float(a / b),
                (Value::Int(a), Value::Float(b)) if *b != 0.0 => Value::Float(*a as f64 / b),
                (Value::Float(a), Value::Int(b)) if *b != 0 => Value::Float(a / *b as f64),
                _ => Value::None,
            },
            BinaryOp::FloorDiv => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a / b),
                _ => Value::None,
            },
            BinaryOp::Mod => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a % b),
                _ => Value::None,
            },
            BinaryOp::Pow => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) if *b >= 0 => Value::Int(a.pow(*b as u32)),
                (Value::Float(a), Value::Float(b)) => Value::Float(a.powf(*b)),
                (Value::Int(a), Value::Float(b)) => Value::Float((*a as f64).powf(*b)),
                (Value::Float(a), Value::Int(b)) => Value::Float(a.powi(*b as i32)),
                _ => Value::None,
            },
            BinaryOp::Eq => Value::Bool(values_equal(&left, &right)),
            BinaryOp::Ne => Value::Bool(!values_equal(&left, &right)),
            BinaryOp::Lt => Value::Bool(
                compare_values(&left, &right)
                    .map(|o| o.is_lt())
                    .unwrap_or(false),
            ),
            BinaryOp::Le => Value::Bool(
                compare_values(&left, &right)
                    .map(|o| o.is_le())
                    .unwrap_or(false),
            ),
            BinaryOp::Gt => Value::Bool(
                compare_values(&left, &right)
                    .map(|o| o.is_gt())
                    .unwrap_or(false),
            ),
            BinaryOp::Ge => Value::Bool(
                compare_values(&left, &right)
                    .map(|o| o.is_ge())
                    .unwrap_or(false),
            ),
            BinaryOp::In => Value::Bool(value_in(&left, &right)),
            BinaryOp::NotIn => Value::Bool(!value_in(&left, &right)),
            BinaryOp::Concat => Value::String(format!(
                "{}{}",
                left.render_to_string(),
                right.render_to_string()
            )),
            BinaryOp::And | BinaryOp::Or => unreachable!(), // Handled above
        })
    }

    fn eval_unary(&self, unary: &UnaryExpr) -> Result<Value> {
        let value = self.eval(&unary.expr)?;

        Ok(match unary.op {
            UnaryOp::Not => Value::Bool(!value.is_truthy()),
            UnaryOp::Neg => match value {
                Value::Int(i) => Value::Int(-i),
                Value::Float(f) => Value::Float(-f),
                _ => Value::None,
            },
            UnaryOp::Pos => match value {
                Value::Int(i) => Value::Int(i),
                Value::Float(f) => Value::Float(f),
                _ => Value::None,
            },
        })
    }

    fn eval_call(&self, call: &CallExpr) -> Result<Value> {
        // Evaluate arguments
        let args: Vec<Value> = call
            .args
            .iter()
            .map(|a| self.eval(a))
            .collect::<Result<Vec<_>>>()?;

        let kwargs: Vec<(String, Value)> = call
            .kwargs
            .iter()
            .map(|(ident, expr)| Ok((ident.name.clone(), self.eval(expr)?)))
            .collect::<Result<Vec<_>>>()?;

        // Check if this is a global function call
        if let Expr::Var(ident) = &*call.func {
            if let Some(result) = self.ctx.call_fn(&ident.name, &args, &kwargs) {
                return result;
            }
        }

        // Method calls on values (like .items(), etc.) - not implemented yet
        Ok(Value::None)
    }

    fn eval_ternary(&self, ternary: &TernaryExpr) -> Result<Value> {
        let condition = self.eval(&ternary.condition)?;
        if condition.is_truthy() {
            self.eval(&ternary.value)
        } else {
            self.eval(&ternary.otherwise)
        }
    }

    fn eval_test(&self, test: &TestExpr) -> Result<Value> {
        let value = self.eval(&test.expr)?;
        let args: Vec<Value> = test
            .args
            .iter()
            .map(|a| self.eval(a))
            .collect::<Result<Vec<_>>>()?;

        let result = match test.test_name.name.as_str() {
            // String tests
            "starting_with" | "startswith" => {
                if let (Value::String(s), Some(Value::String(prefix))) = (&value, args.first()) {
                    s.starts_with(prefix)
                } else {
                    false
                }
            }
            "ending_with" | "endswith" => {
                if let (Value::String(s), Some(Value::String(suffix))) = (&value, args.first()) {
                    s.ends_with(suffix)
                } else {
                    false
                }
            }
            "containing" | "contains" => {
                if let (Value::String(s), Some(Value::String(needle))) = (&value, args.first()) {
                    s.contains(needle)
                } else if let Value::List(list) = &value {
                    args.first()
                        .map(|needle| list.iter().any(|item| values_equal(item, needle)))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            // Type tests
            "defined" => !matches!(value, Value::None),
            "undefined" => matches!(value, Value::None),
            "none" => matches!(value, Value::None),
            "string" => matches!(value, Value::String(_)),
            "number" => matches!(value, Value::Int(_) | Value::Float(_)),
            "integer" => matches!(value, Value::Int(_)),
            "float" => matches!(value, Value::Float(_)),
            "mapping" | "dict" => matches!(value, Value::Dict(_)),
            "iterable" | "sequence" => {
                matches!(value, Value::List(_) | Value::String(_) | Value::Dict(_))
            }
            // Value tests
            "odd" => {
                if let Value::Int(n) = value {
                    n % 2 != 0
                } else {
                    false
                }
            }
            "even" => {
                if let Value::Int(n) = value {
                    n % 2 == 0
                } else {
                    false
                }
            }
            "truthy" => value.is_truthy(),
            "falsy" => !value.is_truthy(),
            "empty" => match &value {
                Value::String(s) => s.is_empty(),
                Value::List(l) => l.is_empty(),
                Value::Dict(d) => d.is_empty(),
                _ => false,
            },
            // Comparison tests
            "eq" | "equalto" | "sameas" => args
                .first()
                .map(|other| values_equal(&value, other))
                .unwrap_or(false),
            "ne" => args
                .first()
                .map(|other| !values_equal(&value, other))
                .unwrap_or(false),
            "lt" | "lessthan" => {
                if let (Value::Int(a), Some(Value::Int(b))) = (&value, args.first()) {
                    a < b
                } else if let (Value::Float(a), Some(Value::Float(b))) = (&value, args.first()) {
                    a < b
                } else {
                    false
                }
            }
            "gt" | "greaterthan" => {
                if let (Value::Int(a), Some(Value::Int(b))) = (&value, args.first()) {
                    a > b
                } else if let (Value::Float(a), Some(Value::Float(b))) = (&value, args.first()) {
                    a > b
                } else {
                    false
                }
            }
            other => {
                return Err(UnknownTestError {
                    name: other.to_string(),
                    span: test.test_name.span,
                    src: self.source.named_source(),
                })?;
            }
        };

        Ok(Value::Bool(if test.negated { !result } else { result }))
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
        (Value::String(a), Value::String(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        _ => false,
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
        (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn value_in(needle: &Value, haystack: &Value) -> bool {
    match haystack {
        Value::List(list) => list.iter().any(|v| values_equal(needle, v)),
        Value::Dict(map) => {
            if let Value::String(key) = needle {
                map.contains_key(key)
            } else {
                false
            }
        }
        Value::String(s) => {
            if let Value::String(sub) = needle {
                s.contains(sub.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Apply a built-in filter
fn apply_filter(
    name: &str,
    value: Value,
    args: &[Value],
    kwargs: &[(String, Value)],
    span: Span,
    source: &TemplateSource,
) -> Result<Value> {
    let known_filters = vec![
        "upper",
        "lower",
        "capitalize",
        "title",
        "trim",
        "length",
        "first",
        "last",
        "reverse",
        "sort",
        "join",
        "default",
        "escape",
        "safe",
    ];

    // Helper to get kwarg value
    let get_kwarg =
        |key: &str| -> Option<&Value> { kwargs.iter().find(|(k, _)| k == key).map(|(_, v)| v) };

    Ok(match name {
        "upper" => Value::String(value.render_to_string().to_uppercase()),
        "lower" => Value::String(value.render_to_string().to_lowercase()),
        "capitalize" => {
            let s = value.render_to_string();
            let mut chars = s.chars();
            match chars.next() {
                None => Value::String(String::new()),
                Some(first) => Value::String(first.to_uppercase().chain(chars).collect()),
            }
        }
        "title" => {
            let s = value.render_to_string();
            Value::String(
                s.split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => first.to_uppercase().chain(chars).collect(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        }
        "trim" => Value::String(value.render_to_string().trim().to_string()),
        "length" => match &value {
            Value::String(s) => Value::Int(s.len() as i64),
            Value::List(l) => Value::Int(l.len() as i64),
            Value::Dict(d) => Value::Int(d.len() as i64),
            _ => Value::Int(0),
        },
        "first" => match value {
            Value::List(mut l) if !l.is_empty() => l.remove(0),
            Value::String(s) => s
                .chars()
                .next()
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::None),
            _ => Value::None,
        },
        "last" => match value {
            Value::List(mut l) if !l.is_empty() => l.pop().unwrap_or(Value::None),
            Value::String(s) => s
                .chars()
                .last()
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::None),
            _ => Value::None,
        },
        "reverse" => match value {
            Value::List(mut l) => {
                l.reverse();
                Value::List(l)
            }
            Value::String(s) => Value::String(s.chars().rev().collect()),
            _ => value,
        },
        "sort" => match value {
            Value::List(mut l) => {
                // Check for attribute= kwarg for sorting dicts by field
                if let Some(Value::String(attr)) = get_kwarg("attribute") {
                    l.sort_by(|a, b| {
                        let a_val = if let Value::Dict(d) = a {
                            d.get(attr)
                        } else {
                            None
                        };
                        let b_val = if let Value::Dict(d) = b {
                            d.get(attr)
                        } else {
                            None
                        };
                        match (a_val, b_val) {
                            (Some(a), Some(b)) => {
                                compare_values(a, b).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                    });
                } else {
                    l.sort_by(|a, b| compare_values(a, b).unwrap_or(std::cmp::Ordering::Equal));
                }
                Value::List(l)
            }
            _ => value,
        },
        "join" => {
            let sep = args
                .first()
                .map(|v| v.render_to_string())
                .unwrap_or_default();
            match value {
                Value::List(l) => {
                    let strings: Vec<_> = l.iter().map(|v| v.render_to_string()).collect();
                    Value::String(strings.join(&sep))
                }
                _ => value,
            }
        }
        "default" => {
            // Support both positional: default("fallback") and kwarg: default(value="fallback")
            let default_val = get_kwarg("value")
                .cloned()
                .or_else(|| args.first().cloned())
                .unwrap_or(Value::None);

            if matches!(value, Value::None) || (matches!(&value, Value::String(s) if s.is_empty()))
            {
                default_val
            } else {
                value
            }
        }
        "escape" => {
            let s = value.render_to_string();
            Value::String(
                s.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;")
                    .replace('\'', "&#x27;"),
            )
        }
        "safe" => Value::Safe(Box::new(value)),
        _ => {
            return Err(UnknownFilterError {
                name: name.to_string(),
                known_filters: known_filters.into_iter().map(String::from).collect(),
                span,
                src: source.named_source(),
            }
            .into());
        }
    })
}
