//! Template AST types

use proc_macro2::{Ident, Span, TokenStream as TokenStream2};

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

/// A for loop in a template
#[derive(Debug, Clone)]
pub struct ForLoop {
    /// The loop variable name
    pub binding: Ident,
    /// The collection to iterate over
    pub collection: Ident,
    /// The loop body
    pub body: Template,
    /// Source span for error reporting
    pub span: Span,
}

/// An if block in a template
#[derive(Debug, Clone)]
pub struct IfBlock {
    /// The condition expression
    pub condition: TokenStream2,
    /// The then branch
    pub then_body: Template,
    /// Optional else branch
    pub else_body: Option<Template>,
    /// Source span for error reporting
    pub span: Span,
}

// =============================================================================
// Display implementations
// =============================================================================

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.items {
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for TemplateItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateItem::Literal(ts) => write!(f, "{ts}"),
            TemplateItem::VarSimple(id) => write!(f, "#{id}"),
            TemplateItem::VarExpr(ts) => write!(f, "#({ts})"),
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
                    write!(f, " @else {{ {else_body} }}")?;
                }
                Ok(())
            }
        }
    }
}
