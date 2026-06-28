//! Expression evaluator
//!
//! Evaluates template expressions against a context using facet_value::Value.

use super::ast::*;
use super::error::{
    SourceLocation, TemplateError, TemplateSource, TypeError, UndefinedError, UnknownFieldError,
    UnknownFilterError, UnknownTestError,
};
use facet_value::{DestructuredRef, VArray, VObject, VString};
use futures::future::BoxFuture;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinItemInfo {
    pub name: &'static str,
    pub detail: &'static str,
    pub documentation: &'static str,
}

pub const BUILTIN_FILTERS: &[BuiltinItemInfo] = &[
    BuiltinItemInfo {
        name: "upper",
        detail: "Gingembre filter",
        documentation: "Converts text to uppercase.",
    },
    BuiltinItemInfo {
        name: "lower",
        detail: "Gingembre filter",
        documentation: "Converts text to lowercase.",
    },
    BuiltinItemInfo {
        name: "capitalize",
        detail: "Gingembre filter",
        documentation: "Uppercases the first character and leaves the remaining characters unchanged.",
    },
    BuiltinItemInfo {
        name: "title",
        detail: "Gingembre filter",
        documentation: "Uppercases the first character of each whitespace-separated word.",
    },
    BuiltinItemInfo {
        name: "trim",
        detail: "Gingembre filter",
        documentation: "Removes leading and trailing whitespace.",
    },
    BuiltinItemInfo {
        name: "length",
        detail: "Gingembre filter",
        documentation: "Returns the length of a string, array, or mapping.",
    },
    BuiltinItemInfo {
        name: "first",
        detail: "Gingembre filter",
        documentation: "Returns the first item from an array or the first character from a string.",
    },
    BuiltinItemInfo {
        name: "last",
        detail: "Gingembre filter",
        documentation: "Returns the last item from an array or the last character from a string.",
    },
    BuiltinItemInfo {
        name: "reverse",
        detail: "Gingembre filter",
        documentation: "Reverses an array or string.",
    },
    BuiltinItemInfo {
        name: "sort",
        detail: "Gingembre filter",
        documentation: "Sorts an array, optionally by an object attribute with `attribute=`.",
    },
    BuiltinItemInfo {
        name: "join",
        detail: "Gingembre filter",
        documentation: "Joins array items into a string, using the first argument as the separator.",
    },
    BuiltinItemInfo {
        name: "split",
        detail: "Gingembre filter",
        documentation: "Splits a string into a list. Pass the separator as the first argument or `pat=`.",
    },
    BuiltinItemInfo {
        name: "default",
        detail: "Gingembre filter",
        documentation: "Returns a fallback when the value is null or an empty string.",
    },
    BuiltinItemInfo {
        name: "escape",
        detail: "Gingembre filter",
        documentation: "HTML-escapes `&`, `<`, `>`, double quotes, and single quotes.",
    },
    BuiltinItemInfo {
        name: "safe",
        detail: "Gingembre filter",
        documentation: "Marks a string as safe HTML so it is not escaped during rendering.",
    },
    BuiltinItemInfo {
        name: "typeof",
        detail: "Gingembre filter",
        documentation: "Returns Gingembre's runtime type name for the value.",
    },
    BuiltinItemInfo {
        name: "slice",
        detail: "Gingembre filter",
        documentation: "Slices an array with `start` and `end` positional arguments or keyword arguments.",
    },
    BuiltinItemInfo {
        name: "map",
        detail: "Gingembre filter",
        documentation: "Extracts `attribute=` from each object in an array.",
    },
    BuiltinItemInfo {
        name: "selectattr",
        detail: "Gingembre filter",
        documentation: "Keeps array items whose named attribute passes a test.",
    },
    BuiltinItemInfo {
        name: "rejectattr",
        detail: "Gingembre filter",
        documentation: "Drops array items whose named attribute passes a test.",
    },
    BuiltinItemInfo {
        name: "groupby",
        detail: "Gingembre filter",
        documentation: "Groups array items by an object attribute and returns `[key, items]` pairs.",
    },
    BuiltinItemInfo {
        name: "path_segments",
        detail: "Gingembre filter",
        documentation: "Splits a path into non-empty slash-separated segments.",
    },
    BuiltinItemInfo {
        name: "path_first",
        detail: "Gingembre filter",
        documentation: "Returns the first non-empty segment of a path.",
    },
    BuiltinItemInfo {
        name: "path_parent",
        detail: "Gingembre filter",
        documentation: "Returns the parent path: `/foo/bar` becomes `/foo`, `/foo` becomes `/`, and `/` stays `/`.",
    },
    BuiltinItemInfo {
        name: "path_basename",
        detail: "Gingembre filter",
        documentation: "Returns the final non-empty segment of a path.",
    },
    BuiltinItemInfo {
        name: "escape_for_attribute",
        detail: "Gingembre filter",
        documentation: "HTML-escapes a string for safe insertion into a quoted HTML attribute value.",
    },
    BuiltinItemInfo {
        name: "basic_markdown",
        detail: "Gingembre filter",
        documentation: "Converts basic inline markdown (bold, italic, code, links) to HTML. Returns safe HTML.",
    },
];

pub const BUILTIN_TESTS: &[BuiltinItemInfo] = &[
    BuiltinItemInfo {
        name: "starting_with",
        detail: "Gingembre test",
        documentation: "True when text starts with the argument.",
    },
    BuiltinItemInfo {
        name: "startswith",
        detail: "Gingembre test",
        documentation: "Alias for `starting_with`.",
    },
    BuiltinItemInfo {
        name: "ending_with",
        detail: "Gingembre test",
        documentation: "True when text ends with the argument.",
    },
    BuiltinItemInfo {
        name: "endswith",
        detail: "Gingembre test",
        documentation: "Alias for `ending_with`.",
    },
    BuiltinItemInfo {
        name: "containing",
        detail: "Gingembre test",
        documentation: "True when a string or array contains the argument.",
    },
    BuiltinItemInfo {
        name: "contains",
        detail: "Gingembre test",
        documentation: "Alias for `containing`.",
    },
    BuiltinItemInfo {
        name: "defined",
        detail: "Gingembre test",
        documentation: "True when the value is not null.",
    },
    BuiltinItemInfo {
        name: "undefined",
        detail: "Gingembre test",
        documentation: "True when the value is null.",
    },
    BuiltinItemInfo {
        name: "none",
        detail: "Gingembre test",
        documentation: "True when the value is null.",
    },
    BuiltinItemInfo {
        name: "string",
        detail: "Gingembre test",
        documentation: "True when the value is a string.",
    },
    BuiltinItemInfo {
        name: "number",
        detail: "Gingembre test",
        documentation: "True when the value is numeric.",
    },
    BuiltinItemInfo {
        name: "integer",
        detail: "Gingembre test",
        documentation: "True when the value is an integer.",
    },
    BuiltinItemInfo {
        name: "float",
        detail: "Gingembre test",
        documentation: "True when the value is a float.",
    },
    BuiltinItemInfo {
        name: "mapping",
        detail: "Gingembre test",
        documentation: "True when the value is an object or mapping.",
    },
    BuiltinItemInfo {
        name: "dict",
        detail: "Gingembre test",
        documentation: "Alias for `mapping`.",
    },
    BuiltinItemInfo {
        name: "iterable",
        detail: "Gingembre test",
        documentation: "True when the value can be iterated.",
    },
    BuiltinItemInfo {
        name: "sequence",
        detail: "Gingembre test",
        documentation: "Alias for `iterable`.",
    },
    BuiltinItemInfo {
        name: "odd",
        detail: "Gingembre test",
        documentation: "True when an integer is odd.",
    },
    BuiltinItemInfo {
        name: "even",
        detail: "Gingembre test",
        documentation: "True when an integer is even.",
    },
    BuiltinItemInfo {
        name: "truthy",
        detail: "Gingembre test",
        documentation: "True when Gingembre treats the value as truthy.",
    },
    BuiltinItemInfo {
        name: "falsy",
        detail: "Gingembre test",
        documentation: "True when Gingembre treats the value as false.",
    },
    BuiltinItemInfo {
        name: "empty",
        detail: "Gingembre test",
        documentation: "True when the value has no items or text.",
    },
    BuiltinItemInfo {
        name: "eq",
        detail: "Gingembre test",
        documentation: "Compares values for equality.",
    },
    BuiltinItemInfo {
        name: "equalto",
        detail: "Gingembre test",
        documentation: "Alias for `eq`.",
    },
    BuiltinItemInfo {
        name: "sameas",
        detail: "Gingembre test",
        documentation: "Alias for `eq`.",
    },
    BuiltinItemInfo {
        name: "ne",
        detail: "Gingembre test",
        documentation: "Compares values for inequality.",
    },
    BuiltinItemInfo {
        name: "lt",
        detail: "Gingembre test",
        documentation: "True when the value is less than the argument.",
    },
    BuiltinItemInfo {
        name: "lessthan",
        detail: "Gingembre test",
        documentation: "Alias for `lt`.",
    },
    BuiltinItemInfo {
        name: "gt",
        detail: "Gingembre test",
        documentation: "True when the value is greater than the argument.",
    },
    BuiltinItemInfo {
        name: "greaterthan",
        detail: "Gingembre test",
        documentation: "Alias for `gt`.",
    },
];

pub fn builtin_filter(name: &str) -> Option<&'static BuiltinItemInfo> {
    BUILTIN_FILTERS.iter().find(|info| info.name == name)
}

pub fn builtin_test(name: &str) -> Option<&'static BuiltinItemInfo> {
    BUILTIN_TESTS.iter().find(|info| info.name == name)
}

pub fn builtin_filter_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_FILTERS.iter().map(|info| info.name)
}

pub fn builtin_test_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_TESTS.iter().map(|info| info.name)
}

/// Re-export facet_value::Value as the template Value type
pub use facet_value::Value;

/// Helper trait to extend Value with template-specific operations
pub trait ValueExt {
    /// Check if the value is truthy (for conditionals)
    fn is_truthy(&self) -> bool;

    /// Get a human-readable type name
    fn type_name(&self) -> &'static str;

    /// Render the value to a string for output
    fn render_to_string(&self) -> String;

    /// Check if this value is marked as "safe" (should not be HTML-escaped)
    fn is_safe(&self) -> bool;
}

impl ValueExt for Value {
    fn is_truthy(&self) -> bool {
        match self.destructure_ref() {
            DestructuredRef::Null => false,
            DestructuredRef::Bool(b) => b,
            DestructuredRef::Number(n) => {
                if let Some(i) = n.to_i64() {
                    i != 0
                } else if let Some(f) = n.to_f64() {
                    f != 0.0
                } else {
                    true
                }
            }
            DestructuredRef::String(s) => !s.is_empty(),
            DestructuredRef::Bytes(b) => !b.is_empty(),
            DestructuredRef::Array(arr) => !arr.is_empty(),
            DestructuredRef::Object(obj) => !obj.is_empty(),
            DestructuredRef::DateTime(_) => true,
            DestructuredRef::QName(_) => true,
            DestructuredRef::Uuid(_) => true,
            DestructuredRef::Char(c) => c != '\0',
            _ => true,
        }
    }

    fn type_name(&self) -> &'static str {
        match self.destructure_ref() {
            DestructuredRef::Null => "none",
            DestructuredRef::Bool(_) => "bool",
            DestructuredRef::Number(_) => "number",
            DestructuredRef::String(_) => "string",
            DestructuredRef::Bytes(_) => "bytes",
            DestructuredRef::Array(_) => "list",
            DestructuredRef::Object(_) => "dict",
            DestructuredRef::DateTime(_) => "datetime",
            DestructuredRef::QName(_) => "qname",
            DestructuredRef::Uuid(_) => "uuid",
            DestructuredRef::Char(_) => "char",
            _ => "unknown",
        }
    }

    fn render_to_string(&self) -> String {
        match self.destructure_ref() {
            DestructuredRef::Null => String::new(),
            DestructuredRef::Bool(b) => if b { "true" } else { "false" }.to_string(),
            DestructuredRef::Number(n) => {
                if let Some(i) = n.to_i64() {
                    i.to_string()
                } else if let Some(f) = n.to_f64() {
                    f.to_string()
                } else {
                    // Fallback for numbers that don't fit i64 or f64
                    "0".to_string()
                }
            }
            DestructuredRef::String(s) => s.to_string(),
            DestructuredRef::Bytes(b) => {
                // Render bytes as hex or base64
                format!("<bytes: {} bytes>", b.len())
            }
            DestructuredRef::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.render_to_string()).collect();
                format!("[{}]", items.join(", "))
            }
            DestructuredRef::Object(_) => "[object]".to_string(),
            DestructuredRef::DateTime(dt) => format!("{:?}", dt),
            DestructuredRef::QName(qn) => format!("{:?}", qn),
            DestructuredRef::Uuid(uuid) => format!("{:?}", uuid),
            DestructuredRef::Char(c) => c.to_string(),
            other => format!("{other:?}"),
        }
    }

    fn is_safe(&self) -> bool {
        // Check if this is a safe string using VSafeString's flag
        self.as_string().is_some_and(|s| s.is_safe())
    }
}

use crate::lazy::{DataPath, DataResolver, LazyValue};

/// Error type for global function calls
pub type GlobalFnError = Box<dyn std::error::Error + Send + Sync>;

/// A global function that can be called from templates.
/// Functions receive resolved (concrete) values and return a future that resolves to a value.
/// This allows functions to make async calls (like RPC to the host).
pub type GlobalFn = Box<
    dyn Fn(&[Value], &[(String, Value)]) -> BoxFuture<'static, Result<Value, GlobalFnError>>
        + Send
        + Sync,
>;

/// Evaluation context (variables in scope)
///
/// The context stores [`LazyValue`]s, which can be either concrete values or
/// lazy references that resolve on demand. This enables fine-grained dependency
/// tracking for incremental computation.
#[derive(Clone)]
pub struct Context {
    /// Variable scopes (innermost last)
    scopes: Vec<HashMap<String, LazyValue>>,
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
    ) -> Option<BoxFuture<'static, Result<Value, GlobalFnError>>> {
        self.global_fns.get(name).map(|f| f(args, kwargs))
    }

    /// Set a variable in the current scope (concrete value)
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<LazyValue>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), value.into());
        }
    }

    /// Get all variable names across all scopes
    pub fn variable_names(&self) -> Vec<&str> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.keys().map(String::as_str))
            .collect()
    }

    /// Set a variable as "safe" (won't be HTML-escaped when rendered)
    /// If the value is a string, it will be converted to a VSafeString
    pub fn set_safe(&mut self, name: impl Into<String>, value: Value) {
        // Convert string values to safe strings
        let safe_value = if let Some(s) = value.as_string() {
            s.clone().into_safe().into_value()
        } else {
            value
        };
        self.set(name, safe_value);
    }

    /// Set a lazy data resolver as the "data" variable.
    ///
    /// This creates a lazy value at the root path that will resolve fields
    /// on demand, enabling fine-grained dependency tracking.
    pub fn set_data_resolver(&mut self, resolver: std::sync::Arc<dyn DataResolver>) {
        self.set("data", LazyValue::lazy(resolver, DataPath::root()));
    }

    /// Get a variable (searches all scopes)
    // r[impl scope.lexical]
    pub fn get(&self, name: &str) -> Option<&LazyValue> {
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
    // r[impl scope.block]
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
///
/// The evaluator returns [`LazyValue`]s, which may be either concrete or lazy.
/// Field and index access on lazy values extends the path without resolving.
/// Operations that need concrete values (arithmetic, comparison) force resolution.
pub struct Evaluator<'a> {
    ctx: &'a Context,
    source: &'a TemplateSource,
}

impl<'a> Evaluator<'a> {
    pub fn new(ctx: &'a Context, source: &'a TemplateSource) -> Self {
        Self { ctx, source }
    }

    /// Evaluate an expression to a (possibly lazy) value
    pub fn eval<'b>(&'b self, expr: &'b Expr) -> BoxFuture<'b, Result<LazyValue, TemplateError>> {
        Box::pin(async move {
            match expr {
                Expr::Literal(lit) => self.eval_literal(lit).await,
                Expr::Var(ident) => self.eval_var(ident),
                Expr::Field(field) => self.eval_field(field).await,
                Expr::Index(index) => self.eval_index(index).await,
                Expr::Filter(filter) => self.eval_filter(filter).await,
                Expr::Binary(binary) => self.eval_binary(binary).await,
                Expr::Unary(unary) => self.eval_unary(unary).await,
                Expr::Call(call) => self.eval_call(call).await,
                Expr::Ternary(ternary) => self.eval_ternary(ternary).await,
                Expr::Test(test) => self.eval_test(test).await,
                Expr::MacroCall(_macro_call) => {
                    // Macro calls are evaluated during rendering, not expression evaluation
                    Ok(LazyValue::concrete(Value::NULL))
                }
                // r[impl expr.optional]
                // `expr?` resolves the inner expression, yielding null instead of raising
                // when it (or a field/index within it) is undefined.
                Expr::Optional(opt) => match self.eval_concrete(&opt.expr).await {
                    Ok(v) => Ok(LazyValue::concrete(v)),
                    Err(TemplateError::Undefined(_)) => Ok(LazyValue::concrete(Value::NULL)),
                    Err(e) => Err(e),
                },
            }
        })
    }

    /// Evaluate and resolve to a concrete value (forces lazy resolution)
    pub async fn eval_concrete(&self, expr: &Expr) -> Result<Value, TemplateError> {
        self.eval(expr).await?.resolve().await
    }

    async fn eval_literal(&self, lit: &Literal) -> Result<LazyValue, TemplateError> {
        Ok(LazyValue::concrete(match lit {
            Literal::None(_) => Value::NULL,
            Literal::Bool(b) => Value::from(b.value),
            Literal::Int(i) => Value::from(i.value),
            Literal::Float(f) => Value::from(f.value),
            Literal::String(s) => Value::from(s.value.as_str()),
            // r[impl literal.list]
            Literal::List(l) => {
                // List elements are resolved to concrete values
                let mut elements = Vec::with_capacity(l.elements.len());
                for e in &l.elements {
                    elements.push(self.eval_concrete(e).await?);
                }
                VArray::from_iter(elements).into()
            }
            // r[impl literal.dict]
            Literal::Dict(d) => {
                let mut obj = VObject::new();
                for (k, v) in &d.entries {
                    let key = self.eval(k).await?.render_to_string().await;
                    let value = self.eval_concrete(v).await?;
                    obj.insert(VString::from(key.as_str()), value);
                }
                obj.into()
            }
        }))
    }

    // r[impl expr.var.lookup]
    fn eval_var(&self, ident: &Ident) -> Result<LazyValue, TemplateError> {
        // r[impl expr.var.undefined]
        self.ctx.get(&ident.name).cloned().ok_or_else(|| {
            UndefinedError {
                name: ident.name.clone(),
                available: self.ctx.available_vars(),
                loc: SourceLocation::new(ident.span, self.source.named_source()),
            }
            .into()
        })
    }

    // r[impl expr.field.missing]
    async fn eval_field(&self, field: &FieldExpr) -> Result<LazyValue, TemplateError> {
        let base = self.eval(&field.base).await?;
        // Use LazyValue's field method - extends path for lazy, normal access for concrete
        base.field(&field.field.name, field.field.span, self.source)
    }

    async fn eval_index(&self, index: &IndexExpr) -> Result<LazyValue, TemplateError> {
        let base = self.eval(&index.base).await?;
        let idx = self.eval(&index.index).await?;

        // For lazy base, we need to resolve the index to get a concrete key/index
        match &base {
            LazyValue::Lazy { .. } => {
                // Resolve the index to get the key
                let idx_resolved = idx.resolve().await?;
                match idx_resolved.destructure_ref() {
                    DestructuredRef::Number(n) => {
                        let i = n.to_i64().unwrap_or(0);
                        base.index(i, index.index.span(), self.source)
                    }
                    DestructuredRef::String(s) => {
                        base.index_str(s.as_str(), index.index.span(), self.source)
                    }
                    _ => Err(TypeError {
                        expected: "number or string".to_string(),
                        found: idx.type_name().to_string(),
                        context: "index".to_string(),
                        loc: SourceLocation::new(index.index.span(), self.source.named_source()),
                    }
                    .into()),
                }
            }
            LazyValue::Concrete(base_val) => {
                // Original concrete logic
                let idx_resolved = idx.resolve().await?;
                match (base_val.destructure_ref(), idx_resolved.destructure_ref()) {
                    (DestructuredRef::Array(arr), DestructuredRef::Number(n)) => {
                        let i = n.to_i64().unwrap_or(0);
                        let i = if i < 0 {
                            (arr.len() as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        // r[impl expr.index.out-of-bounds]
                        arr.get(i).cloned().map(LazyValue::concrete).ok_or_else(|| {
                            TypeError {
                                expected: format!("index < {}", arr.len()),
                                found: format!("index {i}"),
                                context: "list index".to_string(),
                                loc: SourceLocation::new(
                                    index.index.span(),
                                    self.source.named_source(),
                                ),
                            }
                            .into()
                        })
                    }
                    // r[impl expr.index.missing-key]
                    (DestructuredRef::Object(obj), DestructuredRef::String(key)) => obj
                        .get(key.as_str())
                        .cloned()
                        .map(LazyValue::concrete)
                        .ok_or_else(|| {
                            UnknownFieldError {
                                base_type: "dict".to_string(),
                                field: key.to_string(),
                                known_fields: obj.keys().map(|k| k.to_string()).collect(),
                                loc: SourceLocation::new(
                                    index.index.span(),
                                    self.source.named_source(),
                                ),
                            }
                            .into()
                        }),
                    (DestructuredRef::String(s), DestructuredRef::Number(n)) => {
                        let i = n.to_i64().unwrap_or(0);
                        let len = s.len();
                        let i = if i < 0 {
                            (len as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        s.as_str()
                            .chars()
                            .nth(i)
                            .map(|c| LazyValue::concrete(Value::from(c)))
                            .ok_or_else(|| {
                                TypeError {
                                    expected: format!("index < {}", len),
                                    found: format!("index {i}"),
                                    context: "string index".to_string(),
                                    loc: SourceLocation::new(
                                        index.index.span(),
                                        self.source.named_source(),
                                    ),
                                }
                                .into()
                            })
                    }
                    _ => Err(TypeError {
                        expected: "list, dict, or string".to_string(),
                        found: base.type_name().to_string(),
                        context: "index access".to_string(),
                        loc: SourceLocation::new(index.base.span(), self.source.named_source()),
                    })?,
                }
            }
        }
    }

    async fn eval_filter(&self, filter: &FilterExpr) -> Result<LazyValue, TemplateError> {
        // Filters always work on concrete values. The `default` filter is special:
        // if the expression is undefined, treat it as NULL so `default(value=...)` works.
        let value = match self.eval_concrete(&filter.expr).await {
            Ok(v) => v,
            Err(TemplateError::Undefined(_)) if filter.filter.name == "default" => Value::NULL,
            Err(e) => return Err(e),
        };
        let mut args = Vec::with_capacity(filter.args.len());
        for a in &filter.args {
            args.push(self.eval_concrete(a).await?);
        }

        let mut kwargs = Vec::with_capacity(filter.kwargs.len());
        for (ident, expr) in &filter.kwargs {
            kwargs.push((ident.name.clone(), self.eval_concrete(expr).await?));
        }

        apply_filter(
            &filter.filter.name,
            value,
            &args,
            &kwargs,
            filter.filter.span,
            self.source,
        )
        .map(LazyValue::concrete)
    }

    async fn eval_binary(&self, binary: &BinaryExpr) -> Result<LazyValue, TemplateError> {
        // Short-circuit for and/or - these can stay lazy
        match binary.op {
            BinaryOp::And => {
                let left = self.eval(&binary.left).await?;
                if !left.is_truthy().await {
                    return Ok(left);
                }
                return self.eval(&binary.right).await;
            }
            BinaryOp::Or => {
                let left = self.eval(&binary.left).await?;
                if left.is_truthy().await {
                    return Ok(left);
                }
                return self.eval(&binary.right).await;
            }
            _ => {}
        }

        // All other binary ops need concrete values
        let left = self.eval_concrete(&binary.left).await?;
        let right = self.eval_concrete(&binary.right).await?;

        Ok(LazyValue::concrete(match binary.op {
            BinaryOp::Add => binary_add(&left, &right),
            BinaryOp::Sub => binary_sub(&left, &right),
            BinaryOp::Mul => binary_mul(&left, &right),
            BinaryOp::Div => binary_div(&left, &right),
            BinaryOp::FloorDiv => binary_floor_div(&left, &right),
            BinaryOp::Mod => binary_mod(&left, &right),
            BinaryOp::Pow => binary_pow(&left, &right),
            BinaryOp::Eq => Value::from(values_equal(&left, &right)),
            BinaryOp::Ne => Value::from(!values_equal(&left, &right)),
            BinaryOp::Lt => Value::from(
                compare_values(&left, &right)
                    .map(|o| o.is_lt())
                    .unwrap_or(false),
            ),
            BinaryOp::Le => Value::from(
                compare_values(&left, &right)
                    .map(|o| o.is_le())
                    .unwrap_or(false),
            ),
            BinaryOp::Gt => Value::from(
                compare_values(&left, &right)
                    .map(|o| o.is_gt())
                    .unwrap_or(false),
            ),
            BinaryOp::Ge => Value::from(
                compare_values(&left, &right)
                    .map(|o| o.is_ge())
                    .unwrap_or(false),
            ),
            BinaryOp::In => Value::from(value_in(&left, &right)),
            BinaryOp::NotIn => Value::from(!value_in(&left, &right)),
            BinaryOp::Concat => Value::from(
                format!("{}{}", left.render_to_string(), right.render_to_string()).as_str(),
            ),
            BinaryOp::And | BinaryOp::Or => unreachable!(), // Handled above
        }))
    }

    async fn eval_unary(&self, unary: &UnaryExpr) -> Result<LazyValue, TemplateError> {
        // Unary ops need concrete values
        let value = self.eval_concrete(&unary.expr).await?;

        Ok(LazyValue::concrete(match unary.op {
            UnaryOp::Not => Value::from(!value.is_truthy()),
            UnaryOp::Neg => match value.destructure_ref() {
                DestructuredRef::Number(n) => {
                    if let Some(i) = n.to_i64() {
                        Value::from(-i)
                    } else if let Some(f) = n.to_f64() {
                        Value::from(-f)
                    } else {
                        Value::NULL
                    }
                }
                _ => Value::NULL,
            },
            UnaryOp::Pos => match value.destructure_ref() {
                DestructuredRef::Number(_) => value,
                _ => Value::NULL,
            },
        }))
    }

    async fn eval_call(&self, call: &CallExpr) -> Result<LazyValue, TemplateError> {
        // Function calls require concrete argument values
        let mut args = Vec::with_capacity(call.args.len());
        for a in &call.args {
            args.push(self.eval_concrete(a).await?);
        }

        let mut kwargs = Vec::with_capacity(call.kwargs.len());
        for (ident, expr) in &call.kwargs {
            kwargs.push((ident.name.clone(), self.eval_concrete(expr).await?));
        }

        // Check if this is a global function call
        if let Expr::Var(ident) = &*call.func
            && let Some(result_fut) = self.ctx.call_fn(&ident.name, &args, &kwargs)
        {
            return result_fut
                .await
                .map(LazyValue::concrete)
                .map_err(|e| TemplateError::GlobalFn(e.to_string()));
        }

        // Check for method calls on special objects: obj.method()
        // e.g., build.git_hash() -> calls "build" function with step name "git_hash"
        if let Expr::Field(field) = &*call.func
            && let Expr::Var(base_ident) = &*field.base
        {
            // Convert obj.method() to obj("method", ...kwargs)
            let func_name = &base_ident.name;
            let method_name = &field.field.name;

            // Prepend method name as first positional arg
            let mut full_args = vec![Value::from(method_name.as_str())];
            full_args.extend(args.iter().cloned());

            if let Some(result_fut) = self.ctx.call_fn(func_name, &full_args, &kwargs) {
                return result_fut
                    .await
                    .map(LazyValue::concrete)
                    .map_err(|e| TemplateError::GlobalFn(e.to_string()));
            }
        }

        // Method call on an arbitrary value: `<expr>.method(args)` — e.g.
        // `get_media(src).markup(...)`, where the receiver is itself a call result.
        // Evaluate the receiver and dispatch to a host function named after the method,
        // passing the receiver as the first positional argument.
        if let Expr::Field(field) = &*call.func {
            let receiver = self.eval_concrete(&field.base).await?;
            let method_name = &field.field.name;
            let mut full_args = Vec::with_capacity(args.len() + 1);
            full_args.push(receiver);
            full_args.extend(args);
            if let Some(result_fut) = self.ctx.call_fn(method_name, &full_args, &kwargs) {
                return result_fut
                    .await
                    .map(LazyValue::concrete)
                    .map_err(|e| TemplateError::GlobalFn(e.to_string()));
            }
        }

        // Unresolved method call.
        Ok(LazyValue::concrete(Value::NULL))
    }

    async fn eval_ternary(&self, ternary: &TernaryExpr) -> Result<LazyValue, TemplateError> {
        let condition = self.eval(&ternary.condition).await?;
        if condition.is_truthy().await {
            self.eval(&ternary.value).await
        } else {
            self.eval(&ternary.otherwise).await
        }
    }

    async fn eval_test(&self, test: &TestExpr) -> Result<LazyValue, TemplateError> {
        // Tests require concrete values.
        //
        // `defined`/`undefined` must tolerate an undefined operand — checking whether a
        // variable exists is the whole point of the test, so an undefined variable reads
        // as `null` here rather than raising. (Jinja2 semantics.)
        let lenient_undefined = matches!(test.test_name.name.as_str(), "defined" | "undefined");
        let value = match self.eval_concrete(&test.expr).await {
            Ok(v) => v,
            Err(TemplateError::Undefined(_)) if lenient_undefined => Value::NULL,
            Err(e) => return Err(e),
        };
        let mut args = Vec::with_capacity(test.args.len());
        for a in &test.args {
            args.push(self.eval_concrete(a).await?);
        }

        let result = match test.test_name.name.as_str() {
            // String tests
            // r[impl test.starting-with]
            "starting_with" | "startswith" => {
                if let (DestructuredRef::String(s), Some(prefix)) =
                    (value.destructure_ref(), args.first())
                {
                    if let DestructuredRef::String(p) = prefix.destructure_ref() {
                        s.as_str().starts_with(p.as_str())
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            // r[impl test.ending-with]
            "ending_with" | "endswith" => {
                if let (DestructuredRef::String(s), Some(suffix)) =
                    (value.destructure_ref(), args.first())
                {
                    if let DestructuredRef::String(p) = suffix.destructure_ref() {
                        s.as_str().ends_with(p.as_str())
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            // r[impl test.containing]
            "containing" | "contains" => match value.destructure_ref() {
                DestructuredRef::String(s) => {
                    if let Some(needle) = args.first() {
                        if let DestructuredRef::String(n) = needle.destructure_ref() {
                            s.as_str().contains(n.as_str())
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                DestructuredRef::Array(arr) => args
                    .first()
                    .map(|needle| arr.iter().any(|item| values_equal(item, needle)))
                    .unwrap_or(false),
                _ => false,
            },
            // Type tests
            // r[impl test.defined]
            "defined" => !value.is_null(),
            // r[impl test.undefined]
            "undefined" => value.is_null(),
            // r[impl test.none]
            "none" => value.is_null(),
            // r[impl test.string]
            "string" => value.is_string(),
            // r[impl test.number]
            "number" => value.is_number(),
            // r[impl test.integer]
            "integer" => {
                if let DestructuredRef::Number(n) = value.destructure_ref() {
                    n.to_i64().is_some() && n.to_f64().map(|f| f.fract() == 0.0).unwrap_or(false)
                } else {
                    false
                }
            }
            // r[impl test.float]
            "float" => {
                if let DestructuredRef::Number(n) = value.destructure_ref() {
                    n.to_f64().map(|f| f.fract() != 0.0).unwrap_or(false)
                } else {
                    false
                }
            }
            // r[impl test.mapping]
            "mapping" | "dict" => value.is_object(),
            // r[impl test.iterable]
            "iterable" | "sequence" => {
                matches!(
                    value.destructure_ref(),
                    DestructuredRef::Array(_)
                        | DestructuredRef::String(_)
                        | DestructuredRef::Object(_)
                )
            }
            // Value tests
            // r[impl test.odd]
            "odd" => {
                if let DestructuredRef::Number(n) = value.destructure_ref() {
                    n.to_i64().map(|i| i % 2 != 0).unwrap_or(false)
                } else {
                    false
                }
            }
            // r[impl test.even]
            "even" => {
                if let DestructuredRef::Number(n) = value.destructure_ref() {
                    n.to_i64().map(|i| i % 2 == 0).unwrap_or(false)
                } else {
                    false
                }
            }
            // r[impl test.truthy]
            "truthy" => value.is_truthy(),
            // r[impl test.falsy]
            "falsy" => !value.is_truthy(),
            // r[impl test.empty]
            "empty" => match value.destructure_ref() {
                DestructuredRef::String(s) => s.is_empty(),
                DestructuredRef::Array(arr) => arr.is_empty(),
                DestructuredRef::Object(obj) => obj.is_empty(),
                _ => false,
            },
            // Comparison tests
            // r[impl test.eq]
            "eq" | "equalto" | "sameas" => args
                .first()
                .map(|other| values_equal(&value, other))
                .unwrap_or(false),
            // r[impl test.ne]
            "ne" => args
                .first()
                .map(|other| !values_equal(&value, other))
                .unwrap_or(false),
            // r[impl test.lt]
            "lt" | "lessthan" => {
                if let Some(other) = args.first() {
                    compare_values(&value, other)
                        .map(|o| o.is_lt())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            // r[impl test.gt]
            "gt" | "greaterthan" => {
                if let Some(other) = args.first() {
                    compare_values(&value, other)
                        .map(|o| o.is_gt())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            other => {
                return Err(UnknownTestError {
                    name: other.to_string(),
                    loc: SourceLocation::new(test.test_name.span, self.source.named_source()),
                })?;
            }
        };

        Ok(LazyValue::concrete(Value::from(if test.negated {
            !result
        } else {
            result
        })))
    }
}

// === Binary operation helpers ===

fn binary_add(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                Value::from(ai + bi)
            } else if let (Some(af), Some(bf)) = (a.to_f64(), b.to_f64()) {
                Value::from(af + bf)
            } else {
                Value::NULL
            }
        }
        (DestructuredRef::String(a), DestructuredRef::String(b)) => {
            Value::from(format!("{}{}", a.as_str(), b.as_str()).as_str())
        }
        (DestructuredRef::Array(a), DestructuredRef::Array(b)) => {
            let mut result: Vec<Value> = a.iter().cloned().collect();
            result.extend(b.iter().cloned());
            VArray::from_iter(result).into()
        }
        _ => Value::NULL,
    }
}

fn binary_sub(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                Value::from(ai - bi)
            } else if let (Some(af), Some(bf)) = (a.to_f64(), b.to_f64()) {
                Value::from(af - bf)
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn binary_mul(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                Value::from(ai * bi)
            } else if let (Some(af), Some(bf)) = (a.to_f64(), b.to_f64()) {
                Value::from(af * bf)
            } else {
                Value::NULL
            }
        }
        (DestructuredRef::String(s), DestructuredRef::Number(n))
        | (DestructuredRef::Number(n), DestructuredRef::String(s)) => {
            if let Some(count) = n.to_i64() {
                Value::from(s.as_str().repeat(count as usize).as_str())
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn binary_div(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(af), Some(bf)) = (a.to_f64(), b.to_f64()) {
                if bf != 0.0 {
                    Value::from(af / bf)
                } else {
                    Value::NULL
                }
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn binary_floor_div(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                if bi != 0 {
                    Value::from(ai / bi)
                } else {
                    Value::NULL
                }
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn binary_mod(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                if bi != 0 {
                    Value::from(ai % bi)
                } else {
                    Value::NULL
                }
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn binary_pow(left: &Value, right: &Value) -> Value {
    match (left.destructure_ref(), right.destructure_ref()) {
        (DestructuredRef::Number(a), DestructuredRef::Number(b)) => {
            if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
                if bi >= 0 {
                    Value::from(ai.pow(bi as u32))
                } else {
                    Value::NULL
                }
            } else if let (Some(af), Some(bf)) = (a.to_f64(), b.to_f64()) {
                Value::from(af.powf(bf))
            } else {
                Value::NULL
            }
        }
        _ => Value::NULL,
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    a == b
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    a.partial_cmp(b)
}

fn value_in(needle: &Value, haystack: &Value) -> bool {
    match haystack.destructure_ref() {
        DestructuredRef::Array(arr) => arr.iter().any(|v| values_equal(needle, v)),
        DestructuredRef::Object(obj) => {
            if let DestructuredRef::String(key) = needle.destructure_ref() {
                obj.contains_key(key.as_str())
            } else {
                false
            }
        }
        DestructuredRef::String(s) => {
            if let DestructuredRef::String(sub) = needle.destructure_ref() {
                s.as_str().contains(sub.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Look up a (possibly dotted) attribute path on a value. Each segment must
/// resolve to an object until the last; the last segment's value is returned.
/// Returns `None` if any segment is missing or non-traversable.
fn lookup_attr_path(root: &Value, path: &str) -> Option<Value> {
    let mut current = root.clone();
    for segment in path.split('.') {
        let next = match current.destructure_ref() {
            DestructuredRef::Object(obj) => obj.get(segment).cloned(),
            _ => return None,
        };
        current = next?;
    }
    Some(current)
}

/// Helper for selectattr/rejectattr filters.
///
/// Accepts both positional and kwarg invocations:
/// - `selectattr("field")` — keep items where field is truthy
/// - `selectattr("field", "eq", value)` — keep items where field equals value
/// - `selectattr(attribute="field", value=value)` — kwarg form, defaults test to "eq"
/// - `selectattr("a.b.c", ...)` — dotted path traverses nested objects
fn filter_by_attr<'a>(
    value: &Value,
    args: &[Value],
    get_kwarg: impl Fn(&str) -> Option<&'a Value>,
    reject: bool,
) -> Value {
    match value.destructure_ref() {
        DestructuredRef::Array(arr) => {
            // attribute: args[0] or kwarg "attribute" (required)
            let attr_name = match args
                .first()
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
                .or_else(|| {
                    get_kwarg("attribute")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string())
                }) {
                Some(s) => s,
                None => return value.clone(),
            };

            // value: args[2] or kwarg "value" (optional)
            let test_value = args.get(2).cloned().or_else(|| get_kwarg("value").cloned());

            // test: args[1] (optional); defaults to "eq" if a value is present,
            // otherwise to "truthy". Matches Jinja2/Tera convention so that
            // `selectattr("x", value=y)` and `selectattr("x", "eq", y)` agree.
            let test_name = args
                .get(1)
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if test_value.is_some() {
                        "eq".to_string()
                    } else {
                        "truthy".to_string()
                    }
                });

            let filtered: Vec<Value> = arr
                .iter()
                .filter(|item| {
                    let attr_val = lookup_attr_path(item, attr_name.as_str());

                    let passes = match test_name.as_str() {
                        "truthy" => attr_val.as_ref().map(|v| v.is_truthy()).unwrap_or(false),
                        "falsy" => attr_val.as_ref().map(|v| !v.is_truthy()).unwrap_or(true),
                        "defined" => attr_val.is_some(),
                        "undefined" => attr_val.is_none(),
                        "none" => attr_val.as_ref().map(|v| v.is_null()).unwrap_or(true),
                        "eq" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => values_equal(a, b),
                            _ => false,
                        },
                        "ne" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => !values_equal(a, b),
                            _ => true,
                        },
                        "gt" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                compare_values(a, b) == Some(std::cmp::Ordering::Greater)
                            }
                            _ => false,
                        },
                        "ge" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                matches!(
                                    compare_values(a, b),
                                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                                )
                            }
                            _ => false,
                        },
                        "lt" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                compare_values(a, b) == Some(std::cmp::Ordering::Less)
                            }
                            _ => false,
                        },
                        "le" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                matches!(
                                    compare_values(a, b),
                                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                                )
                            }
                            _ => false,
                        },
                        "starting_with" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                a.render_to_string().starts_with(&b.render_to_string())
                            }
                            _ => false,
                        },
                        "ending_with" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                a.render_to_string().ends_with(&b.render_to_string())
                            }
                            _ => false,
                        },
                        "containing" => match (&attr_val, &test_value) {
                            (Some(a), Some(b)) => {
                                a.render_to_string().contains(&b.render_to_string())
                            }
                            _ => false,
                        },
                        _ => attr_val.as_ref().map(|v| v.is_truthy()).unwrap_or(false),
                    };

                    if reject { !passes } else { passes }
                })
                .cloned()
                .collect();

            VArray::from_iter(filtered).into()
        }
        _ => value.clone(),
    }
}

/// Convert basic inline markdown to HTML.
///
/// Handles: **bold**, *italic*, `code`, and [text](url) links.
/// All other characters are HTML-escaped. Returns a Value string with
/// HTML-safe content (caller should mark it safe with `| safe` if needed,
/// or use `basic_markdown | safe`).
fn basic_markdown_to_html(s: &str) -> Value {
    let mut out = String::with_capacity(s.len() + 16);
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '*' if s[i..].starts_with("**") => {
                // **bold**
                chars.next(); // skip second *
                let rest = &s[i + 2..];
                if let Some(end) = rest.find("**") {
                    let inner = html_escape_str(&rest[..end]);
                    out.push_str("<strong>");
                    out.push_str(&inner);
                    out.push_str("</strong>");
                    // advance past the bold span
                    for _ in 0..(end + 2) {
                        chars.next();
                    }
                } else {
                    out.push_str("**");
                }
            }
            '*' => {
                // *italic*
                let rest = &s[i + 1..];
                if let Some(end) = rest.find('*') {
                    let inner = html_escape_str(&rest[..end]);
                    out.push_str("<em>");
                    out.push_str(&inner);
                    out.push_str("</em>");
                    for _ in 0..end {
                        chars.next();
                    }
                    chars.next(); // closing *
                } else {
                    out.push('*');
                }
            }
            '`' => {
                // `code`
                let rest = &s[i + 1..];
                if let Some(end) = rest.find('`') {
                    let inner = html_escape_str(&rest[..end]);
                    out.push_str("<code>");
                    out.push_str(&inner);
                    out.push_str("</code>");
                    for _ in 0..end {
                        chars.next();
                    }
                    chars.next(); // closing `
                } else {
                    out.push('`');
                }
            }
            '[' => {
                // [text](url)
                let rest = &s[i + 1..];
                if let Some(text_end) = rest.find("](") {
                    let text = &rest[..text_end];
                    let after = &rest[text_end + 2..];
                    if let Some(url_end) = after.find(')') {
                        let url = &after[..url_end];
                        out.push_str("<a href=\"");
                        out.push_str(&html_escape_str(url));
                        out.push_str("\">");
                        out.push_str(&html_escape_str(text));
                        out.push_str("</a>");
                        // advance past the link
                        let skip = text_end + 2 + url_end + 1;
                        for _ in 0..skip {
                            chars.next();
                        }
                        continue;
                    }
                }
                out.push('[');
            }
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    Value::from(out.as_str())
}

fn html_escape_str(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Apply a built-in filter
fn apply_filter(
    name: &str,
    value: Value,
    args: &[Value],
    kwargs: &[(String, Value)],
    span: Span,
    source: &TemplateSource,
) -> Result<Value, TemplateError> {
    // Helper to get kwarg value
    let get_kwarg =
        |key: &str| -> Option<&Value> { kwargs.iter().find(|(k, _)| k == key).map(|(_, v)| v) };

    Ok(match name {
        // r[impl filter.upper]
        "upper" => Value::from(value.render_to_string().to_uppercase().as_str()),
        // r[impl filter.lower]
        "lower" => Value::from(value.render_to_string().to_lowercase().as_str()),
        // r[impl filter.capitalize]
        "capitalize" => {
            let s = value.render_to_string();
            let mut chars = s.chars();
            match chars.next() {
                None => Value::from(""),
                Some(first) => {
                    let result: String = first.to_uppercase().chain(chars).collect();
                    Value::from(result.as_str())
                }
            }
        }
        // r[impl filter.title]
        "title" => {
            let s = value.render_to_string();
            let result = s
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().chain(chars).collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Value::from(result.as_str())
        }
        // r[impl filter.trim]
        "trim" => Value::from(value.render_to_string().trim()),
        // r[impl filter.length]
        "length" => match value.destructure_ref() {
            DestructuredRef::String(s) => Value::from(s.len() as i64),
            DestructuredRef::Array(arr) => Value::from(arr.len() as i64),
            DestructuredRef::Object(obj) => Value::from(obj.len() as i64),
            _ => Value::from(0i64),
        },
        // r[impl filter.first]
        "first" => match value.destructure_ref() {
            DestructuredRef::Array(arr) if !arr.is_empty() => {
                arr.get(0).cloned().unwrap_or(Value::NULL)
            }
            DestructuredRef::String(s) => s
                .as_str()
                .chars()
                .next()
                .map(|c| Value::from(c.to_string().as_str()))
                .unwrap_or(Value::NULL),
            _ => Value::NULL,
        },
        // r[impl filter.last]
        "last" => match value.destructure_ref() {
            DestructuredRef::Array(arr) if !arr.is_empty() => {
                arr.get(arr.len() - 1).cloned().unwrap_or(Value::NULL)
            }
            DestructuredRef::String(s) => s
                .as_str()
                .chars()
                .last()
                .map(|c| Value::from(c.to_string().as_str()))
                .unwrap_or(Value::NULL),
            _ => Value::NULL,
        },
        // r[impl filter.reverse]
        "reverse" => match value.destructure_ref() {
            DestructuredRef::Array(arr) => {
                let reversed: Vec<Value> = arr.iter().rev().cloned().collect();
                VArray::from_iter(reversed).into()
            }
            DestructuredRef::String(s) => {
                let reversed: String = s.as_str().chars().rev().collect();
                Value::from(reversed.as_str())
            }
            _ => value,
        },
        // r[impl filter.sort]
        "sort" => match value.destructure_ref() {
            DestructuredRef::Array(arr) => {
                let mut items: Vec<Value> = arr.iter().cloned().collect();
                // Check for attribute= kwarg for sorting objects by field
                if let Some(attr_val) = get_kwarg("attribute") {
                    if let DestructuredRef::String(attr) = attr_val.destructure_ref() {
                        items.sort_by(|a, b| {
                            let a_val = if let DestructuredRef::Object(obj) = a.destructure_ref() {
                                obj.get(attr.as_str())
                            } else {
                                None
                            };
                            let b_val = if let DestructuredRef::Object(obj) = b.destructure_ref() {
                                obj.get(attr.as_str())
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
                    }
                } else {
                    items.sort_by(|a, b| compare_values(a, b).unwrap_or(std::cmp::Ordering::Equal));
                }
                VArray::from_iter(items).into()
            }
            _ => value,
        },
        // r[impl filter.join]
        "join" => {
            let sep = args
                .first()
                .map(|v| v.render_to_string())
                .unwrap_or_default();
            match value.destructure_ref() {
                DestructuredRef::Array(arr) => {
                    let strings: Vec<String> = arr.iter().map(|v| v.render_to_string()).collect();
                    Value::from(strings.join(&sep).as_str())
                }
                _ => value,
            }
        }
        // r[impl filter.split]
        "split" => {
            // Support both positional: split("/") and kwarg: split(pat="/")
            let pat = get_kwarg("pat")
                .map(|v| v.render_to_string())
                .or_else(|| args.first().map(|v| v.render_to_string()))
                .unwrap_or_else(|| " ".to_string());
            let s = value.render_to_string();
            let parts: Vec<Value> = s.split(&pat).map(Value::from).collect();
            VArray::from_iter(parts).into()
        }
        // r[impl filter.default]
        "default" => {
            // Support both positional: default("fallback") and kwarg: default(value="fallback")
            let default_val = get_kwarg("value")
                .cloned()
                .or_else(|| args.first().cloned())
                .unwrap_or(Value::NULL);

            if value.is_null() {
                default_val
            } else if let DestructuredRef::String(s) = value.destructure_ref() {
                if s.is_empty() { default_val } else { value }
            } else {
                value
            }
        }
        // r[impl filter.escape]
        "escape" => {
            let s = value.render_to_string();
            let escaped = s
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#x27;");
            Value::from(escaped.as_str())
        }
        // r[impl filter.safe]
        "safe" => {
            // Convert string to safe string using VSafeString
            if let Some(s) = value.as_string() {
                s.clone().into_safe().into_value()
            } else {
                // Non-strings can't be marked safe, return as-is
                value
            }
        }
        // r[impl filter.typeof]
        "typeof" => {
            // Return the type name of the value
            Value::from(value.type_name())
        }
        // r[impl filter.slice]
        "slice" => {
            // Slice a list: slice(start=0, end=N) or slice(0, N)
            match value.destructure_ref() {
                DestructuredRef::Array(arr) => {
                    let start = get_kwarg("start")
                        .and_then(|v| v.as_number().and_then(|n| n.to_i64()))
                        .or_else(|| {
                            args.first()
                                .and_then(|v| v.as_number().and_then(|n| n.to_i64()))
                        })
                        .unwrap_or(0)
                        .max(0) as usize;
                    let end = get_kwarg("end")
                        .and_then(|v| v.as_number().and_then(|n| n.to_i64()))
                        .or_else(|| {
                            args.get(1)
                                .and_then(|v| v.as_number().and_then(|n| n.to_i64()))
                        })
                        .map(|e| e.max(0) as usize)
                        .unwrap_or(arr.len());
                    let end = end.min(arr.len());
                    let start = start.min(end);
                    VArray::from_iter(arr.iter().skip(start).take(end - start).cloned()).into()
                }
                _ => value,
            }
        }
        // r[impl filter.map]
        "map" => {
            // Extract an attribute from each item: map(attribute="field")
            // Dotted paths traverse nested objects: map(attribute="user.name").
            match value.destructure_ref() {
                DestructuredRef::Array(arr) => {
                    if let Some(attr) = get_kwarg("attribute").and_then(|v| v.as_string()) {
                        let mapped: Vec<Value> = arr
                            .iter()
                            .filter_map(|item| lookup_attr_path(item, attr.as_str()))
                            .collect();
                        VArray::from_iter(mapped).into()
                    } else {
                        value
                    }
                }
                _ => value,
            }
        }
        // r[impl filter.selectattr]
        "selectattr" => {
            // Filter items where attribute passes a test: selectattr("field", "eq", value)
            filter_by_attr(&value, args, get_kwarg, false)
        }
        // r[impl filter.rejectattr]
        "rejectattr" => {
            // Filter items where attribute fails a test: rejectattr("field", "eq", value)
            filter_by_attr(&value, args, get_kwarg, true)
        }
        // r[impl filter.groupby]
        "groupby" => {
            // Group items by attribute: groupby(attribute="field")
            match value.destructure_ref() {
                DestructuredRef::Array(arr) => {
                    if let Some(attr) = get_kwarg("attribute").and_then(|v| v.as_string()) {
                        // Use Vec to maintain insertion order
                        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
                        for item in arr.iter() {
                            let key = lookup_attr_path(item, attr.as_str())
                                .map(|v| v.render_to_string())
                                .unwrap_or_default();
                            // Find or create group
                            if let Some((_, items)) = groups.iter_mut().find(|(k, _)| k == &key) {
                                items.push(item.clone());
                            } else {
                                groups.push((key, vec![item.clone()]));
                            }
                        }
                        // Return as array of [key, items] pairs for tuple unpacking
                        let pairs: Vec<Value> = groups
                            .into_iter()
                            .map(|(k, v)| {
                                let items_arr: Value = VArray::from_iter(v).into();
                                let pair: Value =
                                    VArray::from_iter([Value::from(k.as_str()), items_arr]).into();
                                pair
                            })
                            .collect();
                        VArray::from_iter(pairs).into()
                    } else {
                        value
                    }
                }
                _ => value,
            }
        }
        // Path manipulation filters
        // r[impl filter.path-segments]
        "path_segments" => {
            // Split path into segments, removing empty strings from leading/trailing slashes
            // "/foo/bar/" -> ["foo", "bar"]
            let s = value.render_to_string();
            let segments: Vec<Value> = s
                .split('/')
                .filter(|seg| !seg.is_empty())
                .map(Value::from)
                .collect();
            VArray::from_iter(segments).into()
        }
        // r[impl filter.path-first]
        "path_first" => {
            // Get the first segment of a path
            // "/foo/bar" -> "foo"
            let s = value.render_to_string();
            s.split('/')
                .find(|seg| !seg.is_empty())
                .map(Value::from)
                .unwrap_or(Value::NULL)
        }
        // r[impl filter.path-parent]
        "path_parent" => {
            // Get the parent path
            // "/foo/bar" -> "/foo", "/foo" -> "/", "/" -> "/"
            let s = value.render_to_string();
            let trimmed = s.trim_end_matches('/');
            if trimmed.is_empty() {
                return Ok(Value::from("/"));
            }
            match trimmed.rfind('/') {
                Some(0) => Value::from("/"),
                Some(idx) => Value::from(&trimmed[..idx]),
                None => Value::from("/"),
            }
        }
        // r[impl filter.path-basename]
        "path_basename" => {
            // Get the last segment of a path (basename)
            // "/foo/bar" -> "bar", "/foo/" -> "foo"
            let s = value.render_to_string();
            s.trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|seg| !seg.is_empty())
                .map(Value::from)
                .unwrap_or(Value::NULL)
        }
        // r[impl filter.escape_for_attribute]
        "escape_for_attribute" => {
            let s = value.render_to_string();
            let escaped = s
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#x27;");
            Value::from(escaped.as_str())
        }
        // r[impl filter.basic_markdown]
        "basic_markdown" => {
            // Minimal inline-only markdown pass: bold, italic, inline code, auto-links.
            // Returns safe HTML so callers don't need `| safe`.
            let s = value.render_to_string();
            let html = basic_markdown_to_html(&s);
            if let Some(sv) = html.as_string() {
                sv.clone().into_safe().into_value()
            } else {
                html
            }
        }
        _ => {
            return Err(UnknownFilterError {
                name: name.to_string(),
                known_filters: builtin_filter_names().map(str::to_string).collect(),
                loc: SourceLocation::new(span, source.named_source()),
            }
            .into());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_filter_metadata_documents_runtime_filters() {
        let names = builtin_filter_names().collect::<Vec<_>>();
        assert!(names.contains(&"path_parent"));

        let path_parent = builtin_filter("path_parent").expect("path_parent metadata");
        assert_eq!(path_parent.detail, "Gingembre filter");
        assert!(
            path_parent
                .documentation
                .contains("Returns the parent path")
        );

        let unique = names.iter().copied().collect::<HashSet<_>>();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn builtin_test_metadata_documents_runtime_tests() {
        let names = builtin_test_names().collect::<Vec<_>>();
        assert!(names.contains(&"string"));
        assert!(names.contains(&"startswith"));

        let string = builtin_test("string").expect("string test metadata");
        assert_eq!(string.detail, "Gingembre test");
        assert!(string.documentation.contains("value is a string"));

        let unique = names.iter().copied().collect::<HashSet<_>>();
        assert_eq!(unique.len(), names.len());
    }
}
