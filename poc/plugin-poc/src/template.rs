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
}
