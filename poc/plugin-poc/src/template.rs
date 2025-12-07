//! # Facet Template Language
//!
//! A token-based templating language for plugin code generation.
//!
//! ## Syntax
//!
//! - `#ident` — interpolate a simple variable
//! - `#(expr)` — interpolate a complex expression (e.g., `#(variant.fields[0].ty)`)
//! - `@for ident in collection { ... }` — loop
//! - `@if condition { ... }` — conditional
//! - `@if condition { ... } @else { ... }` — conditional with else
//! - Everything else — literal Rust tokens to emit

use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use unsynn::{Ident, keyword};

keyword! {
    KFor = "for";
    KIn = "in";
    KIf = "if";
    KElse = "else";
}

// We don't actually use these keywords yet - parsing is manual for now

// =============================================================================
// AST
// =============================================================================

/// A complete template
#[derive(Debug, Clone)]
pub struct Template {
    pub items: Vec<TemplateItem>,
}

/// An item in a template
#[derive(Debug, Clone)]
pub enum TemplateItem {
    /// Literal tokens to emit as-is
    Literal(TokenStream2),
    /// `#ident` — simple variable interpolation
    VarSimple(Ident),
    /// `#(expr)` — complex expression interpolation
    VarExpr(TokenStream2),
    /// `@for ident in collection { ... }`
    For(ForLoop),
    /// `@if condition { ... }` with optional else
    If(IfBlock),
}

#[derive(Debug, Clone)]
pub struct ForLoop {
    pub binding: Ident,
    pub collection: Ident,
    pub body: Template,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfBlock {
    pub condition: TokenStream2,
    pub then_body: Template,
    pub else_body: Option<Template>,
    pub span: Span,
}

// =============================================================================
// Parser
// =============================================================================

impl Template {
    pub fn parse(tokens: TokenStream2) -> Result<Self, String> {
        let mut items = Vec::new();
        let mut literal_acc = TokenStream2::new();
        let mut iter = tokens.into_iter().peekable();

        while let Some(tt) = iter.next() {
            match &tt {
                // Check for `#` — variable interpolation
                TokenTree::Punct(p) if p.as_char() == '#' => {
                    // Flush accumulated literals
                    if !literal_acc.is_empty() {
                        items.push(TemplateItem::Literal(std::mem::take(&mut literal_acc)));
                    }

                    // Peek at next token
                    match iter.peek() {
                        Some(TokenTree::Ident(ident)) => {
                            let ident = ident.clone();
                            iter.next(); // consume it
                            items.push(TemplateItem::VarSimple(ident));
                        }
                        Some(TokenTree::Group(g))
                            if g.delimiter() == proc_macro2::Delimiter::Parenthesis =>
                        {
                            let g = g.clone();
                            iter.next(); // consume it
                            items.push(TemplateItem::VarExpr(g.stream()));
                        }
                        _ => {
                            // Just a lone `#`, keep it as literal
                            literal_acc.extend(std::iter::once(tt));
                        }
                    }
                }

                // Check for `@` — control flow
                TokenTree::Punct(p) if p.as_char() == '@' => {
                    // Flush accumulated literals
                    if !literal_acc.is_empty() {
                        items.push(TemplateItem::Literal(std::mem::take(&mut literal_acc)));
                    }

                    // Peek at next token for keyword
                    match iter.peek() {
                        Some(TokenTree::Ident(kw)) if kw == "for" => {
                            let span = kw.span();
                            iter.next(); // consume 'for'
                            let for_loop = Self::parse_for(&mut iter, span)?;
                            items.push(TemplateItem::For(for_loop));
                        }
                        Some(TokenTree::Ident(kw)) if kw == "if" => {
                            let span = kw.span();
                            iter.next(); // consume 'if'
                            let if_block = Self::parse_if(&mut iter, span)?;
                            items.push(TemplateItem::If(if_block));
                        }
                        _ => {
                            // Just a lone `@`, keep it as literal
                            literal_acc.extend(std::iter::once(tt));
                        }
                    }
                }

                // Anything else is a literal
                _ => {
                    literal_acc.extend(std::iter::once(tt));
                }
            }
        }

        // Flush remaining literals
        if !literal_acc.is_empty() {
            items.push(TemplateItem::Literal(literal_acc));
        }

        Ok(Template { items })
    }

    fn parse_for(
        iter: &mut std::iter::Peekable<impl Iterator<Item = TokenTree>>,
        span: Span,
    ) -> Result<ForLoop, String> {
        // Expect: <binding> in <collection> { ... }

        // binding
        let binding = match iter.next() {
            Some(TokenTree::Ident(id)) => id,
            other => {
                return Err(format!(
                    "@for: expected binding identifier, got {:?}",
                    other
                ));
            }
        };

        // 'in'
        match iter.next() {
            Some(TokenTree::Ident(id)) if id == "in" => {}
            other => return Err(format!("@for: expected 'in', got {:?}", other)),
        }

        // collection
        let collection = match iter.next() {
            Some(TokenTree::Ident(id)) => id,
            other => {
                return Err(format!(
                    "@for: expected collection identifier, got {:?}",
                    other
                ));
            }
        };

        // body { ... }
        let body = match iter.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                Template::parse(g.stream())?
            }
            other => return Err(format!("@for: expected braced body, got {:?}", other)),
        };

        Ok(ForLoop {
            binding,
            collection,
            body,
            span,
        })
    }

    fn parse_if(
        iter: &mut std::iter::Peekable<impl Iterator<Item = TokenTree>>,
        span: Span,
    ) -> Result<IfBlock, String> {
        // Expect: <condition tokens...> { ... } [@else { ... }]

        // Collect condition tokens until we hit a brace
        let mut condition = TokenStream2::new();
        loop {
            match iter.peek() {
                Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                    break;
                }
                Some(_) => {
                    condition.extend(iter.next());
                }
                None => return Err("@if: expected braced body".to_string()),
            }
        }

        // then body { ... }
        let then_body = match iter.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                Template::parse(g.stream())?
            }
            other => return Err(format!("@if: expected braced body, got {:?}", other)),
        };

        // Optional: @else { ... }
        // We need to check for @ followed by else
        let else_body = 'else_block: {
            // Check for @
            let Some(TokenTree::Punct(p)) = iter.peek() else {
                break 'else_block None;
            };
            if p.as_char() != '@' {
                break 'else_block None;
            }
            iter.next(); // consume @

            // Check for 'else'
            let Some(TokenTree::Ident(id)) = iter.peek() else {
                // Not else, but we consumed @, treat as literal (would need backtracking)
                // For now, just error
                return Err("@: expected 'else' or other keyword".to_string());
            };
            if id != "else" {
                return Err(format!("@: expected 'else', got '{}'", id));
            }
            iter.next(); // consume 'else'

            // else body { ... }
            match iter.next() {
                Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                    Some(Template::parse(g.stream())?)
                }
                other => return Err(format!("@else: expected braced body, got {:?}", other)),
            }
        };

        Ok(IfBlock {
            condition,
            then_body,
            else_body,
            span,
        })
    }
}

// =============================================================================
// Evaluation
// =============================================================================

use quote::quote;
use std::collections::HashMap;

/// A value in the template evaluation context
#[derive(Debug, Clone)]
pub enum Value {
    /// A token stream (type name, field type, etc.)
    Tokens(TokenStream2),
    /// A string (doc comment, attribute value, etc.)
    String(String),
    /// A boolean
    Bool(bool),
    /// An integer (field index, etc.)
    Int(usize),
    /// A list of values (variants, fields, etc.)
    List(Vec<Value>),
    /// An object with named fields
    Object(HashMap<String, Value>),
}

impl Value {
    /// Convert to tokens for interpolation
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            Value::Tokens(ts) => ts.clone(),
            Value::String(s) => {
                let lit = proc_macro2::Literal::string(s);
                quote! { #lit }
            }
            Value::Bool(b) => {
                if *b {
                    quote! { true }
                } else {
                    quote! { false }
                }
            }
            Value::Int(n) => {
                let lit = proc_macro2::Literal::usize_unsuffixed(*n);
                quote! { #lit }
            }
            Value::List(_) => {
                panic!("Cannot convert list to tokens directly")
            }
            Value::Object(_) => {
                panic!("Cannot convert object to tokens directly")
            }
        }
    }

    /// Get a field from an object value
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.get(key),
            _ => None,
        }
    }

    /// Get as a list for iteration
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    /// Get as a bool for conditionals
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::List(v) => !v.is_empty(),
            Value::Int(n) => *n != 0,
            _ => true,
        }
    }
}

/// Evaluation context with variable bindings
#[derive(Debug, Clone)]
pub struct EvalContext {
    bindings: HashMap<String, Value>,
}

impl EvalContext {
    pub fn new() -> Self {
        EvalContext {
            bindings: HashMap::new(),
        }
    }

    /// Set a binding
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
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

impl Default for EvalContext {
    fn default() -> Self {
        Self::new()
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
                    .ok_or_else(|| format!("undefined variable: {}", name))?;
                Ok(value.to_tokens())
            }

            TemplateItem::VarExpr(expr) => {
                // Parse and evaluate the expression
                // Expression syntax: ident.field.field or ident.method("arg")
                eval_expr(expr.clone(), ctx)
            }

            TemplateItem::For(for_loop) => {
                let collection_name = for_loop.collection.to_string();
                let collection = ctx
                    .get(&collection_name)
                    .ok_or_else(|| format!("undefined collection: {}", collection_name))?;

                let items = collection
                    .as_list()
                    .ok_or_else(|| format!("{} is not a list", collection_name))?;

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
                // Evaluate the condition
                let _condition_result = eval_expr(if_block.condition.clone(), ctx)?;

                // For now, we check if the condition evaluates to a truthy value
                // This is a simplification - in reality we'd need to evaluate the expression
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
        other => return Err(format!("expected identifier, got {:?}", other)),
    };

    let mut current = ctx
        .get(&base_ident)
        .ok_or_else(|| format!("undefined variable: {}", base_ident))?
        .clone();

    // Process field accesses and method calls
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '.' => {
                // Field access or method call
                let field_or_method = match iter.next() {
                    Some(TokenTree::Ident(id)) => id.to_string(),
                    other => return Err(format!("expected field name after '.', got {:?}", other)),
                };

                // Check if it's a method call (followed by parentheses)
                if let Some(TokenTree::Group(g)) = iter.peek() {
                    if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
                        let g = iter.next().unwrap(); // consume the group
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
                    .ok_or_else(|| format!("no field '{}' on value", field_or_method))?
                    .clone();
            }
            TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Bracket => {
                // Index access: [0], [1], etc.
                let index_str = g.stream().to_string().trim().to_string();
                let index: usize = index_str
                    .parse()
                    .map_err(|_| format!("invalid index: {}", index_str))?;

                let list = current
                    .as_list()
                    .ok_or_else(|| "cannot index non-list".to_string())?;
                current = list
                    .get(index)
                    .ok_or_else(|| format!("index {} out of bounds", index))?
                    .clone();
            }
            _ => {
                return Err(format!("unexpected token in expression: {:?}", tt));
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
                _ => return Err(format!("cannot get len of {:?}", value)),
            };
            Ok(Value::Int(len))
        }
        _ => Err(format!("unknown method: {}", method)),
    }
}

/// Parse a string literal argument from a token stream
fn parse_string_arg(args: TokenStream2) -> Result<String, String> {
    let mut iter = args.into_iter();
    match iter.next() {
        Some(TokenTree::Literal(lit)) => {
            let s = lit.to_string();
            // Remove quotes
            Ok(s.trim_matches('"').to_string())
        }
        other => Err(format!("expected string literal, got {:?}", other)),
    }
}

/// Evaluate a condition for @if
fn eval_condition(condition: &TokenStream2, ctx: &EvalContext) -> Result<bool, String> {
    // Try to evaluate as an expression and check truthiness
    let value = eval_expr(condition.clone(), ctx);
    match value {
        Ok(tokens) => {
            // If it evaluated to tokens, check if they represent true/false
            let s = tokens.to_string();
            Ok(s != "false" && !s.is_empty())
        }
        Err(_) => {
            // If evaluation failed, try parsing as a simple truthy check
            // For now, just return false on error
            Ok(false)
        }
    }
}

// =============================================================================
// Debug display
// =============================================================================

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.items {
            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for TemplateItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateItem::Literal(ts) => write!(f, "{}", ts),
            TemplateItem::VarSimple(id) => write!(f, "#{}", id),
            TemplateItem::VarExpr(ts) => write!(f, "#({})", ts),
            TemplateItem::For(for_loop) => {
                write!(
                    f,
                    "@for {} in {} {{ {} }}",
                    for_loop.binding, for_loop.collection, for_loop.body
                )
            }
            TemplateItem::If(if_block) => {
                write!(f, "@if {} {{ {} }}", if_block.condition, if_block.then_body)?;
                if let Some(else_body) = &if_block.else_body {
                    write!(f, " @else {{ {} }}", else_body)?;
                }
                Ok(())
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to parse a string as tokens (avoids quote! interpolation issues)
    fn parse_str(s: &str) -> TokenStream2 {
        s.parse().unwrap()
    }

    #[test]
    fn test_parse_literal() {
        let tokens = parse_str("fn foo() {}");
        let template = Template::parse(tokens).unwrap();
        assert_eq!(template.items.len(), 1);
        assert!(matches!(template.items[0], TemplateItem::Literal(_)));
    }

    #[test]
    fn test_parse_var_simple() {
        let tokens = parse_str("impl Display for #Self {}");
        let template = Template::parse(tokens).unwrap();
        // Should have: Literal("impl Display for"), VarSimple("Self"), Literal("{}")
        assert_eq!(template.items.len(), 3);
        assert!(matches!(&template.items[1], TemplateItem::VarSimple(id) if id == "Self"));
    }

    #[test]
    fn test_parse_var_expr() {
        let tokens = parse_str("type T = #(variant.fields[0].ty);");
        let template = Template::parse(tokens).unwrap();
        assert!(
            template
                .items
                .iter()
                .any(|i| matches!(i, TemplateItem::VarExpr(_)))
        );
    }

    #[test]
    fn test_parse_for() {
        let tokens = parse_str(
            r#"
            @for v in variants {
                Self::Foo => {}
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();
        assert_eq!(template.items.len(), 1);
        if let TemplateItem::For(for_loop) = &template.items[0] {
            assert_eq!(for_loop.binding.to_string(), "v");
            assert_eq!(for_loop.collection.to_string(), "variants");
        } else {
            panic!("expected For, got {:?}", template.items[0]);
        }
    }

    #[test]
    fn test_parse_if() {
        let tokens = parse_str(
            r#"
            @if v.has_attr("from") {
                Some(e)
            } @else {
                None
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();
        assert_eq!(template.items.len(), 1);
        if let TemplateItem::If(if_block) = &template.items[0] {
            assert!(if_block.else_body.is_some());
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_parse_if_no_else() {
        let tokens = parse_str(
            r#"
            @if v.has_attr("from") {
                Some(e)
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();
        assert_eq!(template.items.len(), 1);
        if let TemplateItem::If(if_block) = &template.items[0] {
            assert!(if_block.else_body.is_none());
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn test_parse_nested() {
        let tokens = parse_str(
            r#"
            impl Display for #Self {
                fn fmt(&self, f: &mut Formatter) -> Result {
                    match self {
                        @for v in variants {
                            @if v.is_unit {
                                Self::Foo => write!(f, #(v.doc)),
                            }
                        }
                    }
                }
            }
        "#,
        );
        let template = Template::parse(tokens).unwrap();
        println!("Parsed: {}", template);

        // Verify structure: Literal, VarSimple(Self), Literal containing For
        assert!(
            template
                .items
                .iter()
                .any(|i| matches!(i, TemplateItem::VarSimple(id) if id == "Self"))
        );
    }

    #[test]
    fn test_display() {
        let tokens = parse_str("@for v in variants { #v }");
        let template = Template::parse(tokens).unwrap();
        let display = format!("{}", template);
        assert!(display.contains("@for"));
        assert!(display.contains("#v"));
    }

    // =========================================================================
    // Evaluation tests
    // =========================================================================

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
