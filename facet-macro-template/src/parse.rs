//! Template parser

use crate::ast::{ForLoop, IfBlock, Template, TemplateItem};
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};

impl Template {
    /// Parse a token stream into a template
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
                return Err(format!("@for: expected binding identifier, got {other:?}"));
            }
        };

        // 'in'
        match iter.next() {
            Some(TokenTree::Ident(id)) if id == "in" => {}
            other => return Err(format!("@for: expected 'in', got {other:?}")),
        }

        // collection
        let collection = match iter.next() {
            Some(TokenTree::Ident(id)) => id,
            other => {
                return Err(format!(
                    "@for: expected collection identifier, got {other:?}"
                ));
            }
        };

        // body { ... }
        let body = match iter.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                Template::parse(g.stream())?
            }
            other => return Err(format!("@for: expected braced body, got {other:?}")),
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
            other => return Err(format!("@if: expected braced body, got {other:?}")),
        };

        // Optional: @else { ... }
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
                return Err("@: expected 'else' or other keyword".to_string());
            };
            if id != "else" {
                return Err(format!("@: expected 'else', got '{id}'"));
            }
            iter.next(); // consume 'else'

            // else body { ... }
            match iter.next() {
                Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                    Some(Template::parse(g.stream())?)
                }
                other => return Err(format!("@else: expected braced body, got {other:?}")),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
