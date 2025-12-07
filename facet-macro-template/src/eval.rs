//! Template evaluation

use crate::ast::{Template, TemplateItem};
use crate::value::Value;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use std::collections::HashMap;

/// Evaluation context with variable bindings
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    bindings: HashMap<String, Value>,
}

impl EvalContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a binding
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.bindings.insert(name.into(), value.into());
    }

    /// Get a binding
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }

    /// Create a child context with additional bindings
    pub fn child(&self) -> Self {
        EvalContext {
            bindings: self.bindings.clone(),
        }
    }
}

impl Template {
    /// Evaluate the template with the given context
    pub fn eval(&self, ctx: &EvalContext) -> Result<TokenStream2, String> {
        let mut output = TokenStream2::new();
        for item in &self.items {
            output.extend(item.eval(ctx)?);
        }
        Ok(output)
    }
}

impl TemplateItem {
    fn eval(&self, ctx: &EvalContext) -> Result<TokenStream2, String> {
        match self {
            TemplateItem::Literal(ts) => Ok(ts.clone()),

            TemplateItem::VarSimple(ident) => {
                let name = ident.to_string();
                let value = ctx
                    .get(&name)
                    .ok_or_else(|| format!("undefined variable: {name}"))?;
                Ok(value.to_tokens())
            }

            TemplateItem::VarExpr(expr) => {
                // Parse and evaluate the expression
                eval_expr(expr.clone(), ctx)
            }

            TemplateItem::For(for_loop) => {
                let collection_name = for_loop.collection.to_string();
                let collection = ctx
                    .get(&collection_name)
                    .ok_or_else(|| format!("undefined collection: {collection_name}"))?;

                let items = collection
                    .as_list()
                    .ok_or_else(|| format!("{collection_name} is not a list"))?;

                let binding_name = for_loop.binding.to_string();
                let mut output = TokenStream2::new();

                for item in items {
                    let mut child_ctx = ctx.child();
                    child_ctx.set(&binding_name, item.clone());
                    output.extend(for_loop.body.eval(&child_ctx)?);
                }

                Ok(output)
            }

            TemplateItem::If(if_block) => {
                let is_true = eval_condition(&if_block.condition, ctx)?;

                if is_true {
                    if_block.then_body.eval(ctx)
                } else if let Some(else_body) = &if_block.else_body {
                    else_body.eval(ctx)
                } else {
                    Ok(TokenStream2::new())
                }
            }
        }
    }
}

/// Evaluate an expression like `v.name` or `v.has_attr("from")`
fn eval_expr(expr: TokenStream2, ctx: &EvalContext) -> Result<TokenStream2, String> {
    let mut iter = expr.into_iter().peekable();

    // First token should be an identifier
    let base_ident = match iter.next() {
        Some(TokenTree::Ident(id)) => id.to_string(),
        other => return Err(format!("expected identifier, got {other:?}")),
    };

    let mut current = ctx
        .get(&base_ident)
        .ok_or_else(|| format!("undefined variable: {base_ident}"))?
        .clone();

    // Process field accesses and method calls
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '.' => {
                // Field access or method call
                let field_or_method = match iter.next() {
                    Some(TokenTree::Ident(id)) => id.to_string(),
                    other => return Err(format!("expected field name after '.', got {other:?}")),
                };

                // Check if it's a method call (followed by parentheses)
                if let Some(TokenTree::Group(g)) = iter.peek() {
                    if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
                        let g = iter.next().unwrap();
                        if let TokenTree::Group(g) = g {
                            // Method call
                            current = eval_method(&current, &field_or_method, g.stream())?;
                        }
                        continue;
                    }
                }

                // Field access
                current = current
                    .get(&field_or_method)
                    .ok_or_else(|| format!("no field '{field_or_method}' on value"))?;
            }
            TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Bracket => {
                // Index access: [0], [1], etc.
                let index_str = g.stream().to_string().trim().to_string();
                let index: usize = index_str
                    .parse()
                    .map_err(|_| format!("invalid index: {index_str}"))?;

                let list = current
                    .as_list()
                    .ok_or_else(|| "cannot index non-list".to_string())?;
                current = list
                    .get(index)
                    .ok_or_else(|| format!("index {index} out of bounds"))?
                    .clone();
            }
            _ => {
                return Err(format!("unexpected token in expression: {tt:?}"));
            }
        }
    }

    Ok(current.to_tokens())
}

/// Evaluate a method call on a value
fn eval_method(value: &Value, method: &str, args: TokenStream2) -> Result<Value, String> {
    match method {
        "has_attr" => {
            // has_attr("name") -> check if variant/field has an attribute
            let attr_name = parse_string_arg(args)?;
            let has_it = match value {
                Value::Object(map) => {
                    if let Some(Value::List(attrs)) = map.get("attrs") {
                        attrs.iter().any(|a| {
                            if let Value::String(s) = a {
                                s == &attr_name
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    }
                }
                Value::Variant(v) => v.attrs.has_builtin(&attr_name),
                Value::Field(f) => f.attrs.has_builtin(&attr_name),
                _ => false,
            };
            Ok(Value::Bool(has_it))
        }
        "is_empty" => {
            let empty = match value {
                Value::List(v) => v.is_empty(),
                Value::String(s) => s.is_empty(),
                _ => false,
            };
            Ok(Value::Bool(empty))
        }
        "len" => {
            let len = match value {
                Value::List(v) => v.len(),
                Value::String(s) => s.len(),
                _ => return Err(format!("cannot get len of {value:?}")),
            };
            Ok(Value::Int(len))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Parse a string literal argument from a token stream
fn parse_string_arg(args: TokenStream2) -> Result<String, String> {
    let mut iter = args.into_iter();
    match iter.next() {
        Some(TokenTree::Literal(lit)) => {
            let s = lit.to_string();
            Ok(s.trim_matches('"').to_string())
        }
        other => Err(format!("expected string literal, got {other:?}")),
    }
}

/// Evaluate a condition for @if
fn eval_condition(condition: &TokenStream2, ctx: &EvalContext) -> Result<bool, String> {
    let value = eval_expr(condition.clone(), ctx);
    match value {
        Ok(tokens) => {
            let s = tokens.to_string();
            Ok(s != "false" && !s.is_empty())
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Template;

    fn parse_str(s: &str) -> TokenStream2 {
        s.parse().unwrap()
    }

    #[test]
    fn test_eval_literal() {
        let tokens = parse_str("fn foo() {}");
        let template = Template::parse(tokens).unwrap();
        let ctx = EvalContext::new();
        let result = template.eval(&ctx).unwrap();
        assert_eq!(result.to_string(), "fn foo () { }");
    }

    #[test]
    fn test_eval_var_simple() {
        let tokens = parse_str("impl Display for #Self {}");
        let template = Template::parse(tokens).unwrap();

        let mut ctx = EvalContext::new();
        ctx.set("Self", Value::Tokens(parse_str("MyType")));

        let result = template.eval(&ctx).unwrap();
        assert!(result.to_string().contains("MyType"));
    }

    #[test]
    fn test_eval_var_expr_field() {
        let tokens = parse_str("let name = #(v.name);");
        let template = Template::parse(tokens).unwrap();

        let mut variant = HashMap::new();
        variant.insert("name".to_string(), Value::Tokens(parse_str("Foo")));

        let mut ctx = EvalContext::new();
        ctx.set("v", Value::Object(variant));

        let result = template.eval(&ctx).unwrap();
        assert!(result.to_string().contains("Foo"));
    }

    #[test]
    fn test_eval_for_loop() {
        let tokens = parse_str(
            r#"
            @for v in variants {
                #(v.name),
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();

        let mut v1 = HashMap::new();
        v1.insert("name".to_string(), Value::Tokens(parse_str("Foo")));

        let mut v2 = HashMap::new();
        v2.insert("name".to_string(), Value::Tokens(parse_str("Bar")));

        let mut ctx = EvalContext::new();
        ctx.set(
            "variants",
            Value::List(vec![Value::Object(v1), Value::Object(v2)]),
        );

        let result = template.eval(&ctx).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Foo"));
        assert!(result_str.contains("Bar"));
    }

    #[test]
    fn test_eval_if_true() {
        let tokens = parse_str(
            r#"
            @if v.has_attr("from") {
                has_from
            } @else {
                no_from
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();

        let mut variant = HashMap::new();
        variant.insert(
            "attrs".to_string(),
            Value::List(vec![Value::String("from".to_string())]),
        );

        let mut ctx = EvalContext::new();
        ctx.set("v", Value::Object(variant));

        let result = template.eval(&ctx).unwrap();
        assert!(result.to_string().contains("has_from"));
        assert!(!result.to_string().contains("no_from"));
    }

    #[test]
    fn test_eval_if_false() {
        let tokens = parse_str(
            r#"
            @if v.has_attr("from") {
                has_from
            } @else {
                no_from
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();

        let mut variant = HashMap::new();
        variant.insert("attrs".to_string(), Value::List(vec![]));

        let mut ctx = EvalContext::new();
        ctx.set("v", Value::Object(variant));

        let result = template.eval(&ctx).unwrap();
        assert!(!result.to_string().contains("has_from"));
        assert!(result.to_string().contains("no_from"));
    }

    #[test]
    fn test_eval_index() {
        let tokens = parse_str("type T = #(v.fields[0].ty);");
        let template = Template::parse(tokens).unwrap();

        let mut field0 = HashMap::new();
        field0.insert("ty".to_string(), Value::Tokens(parse_str("String")));

        let mut variant = HashMap::new();
        variant.insert(
            "fields".to_string(),
            Value::List(vec![Value::Object(field0)]),
        );

        let mut ctx = EvalContext::new();
        ctx.set("v", Value::Object(variant));

        let result = template.eval(&ctx).unwrap();
        assert!(result.to_string().contains("String"));
    }
}
