//! Semantic information for Gingembre templates.
//!
//! This module builds a scoped symbol/reference index from the parsed AST. It is
//! intentionally editor-agnostic: LSP frontends can map spans to protocol ranges,
//! while Gingembre owns the meaning of bindings and references.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinaryExpr, CallExpr, DictLit, Expr, FilterExpr, Ident, IfNode, ListLit, Literal,
    MacroCallExpr, MacroNode, Node, SetValue, Span, Target, Template, TernaryExpr, TestExpr,
    UnaryExpr,
};
use facet::Facet;

#[derive(Debug, Clone, Facet)]
pub struct TemplateSemanticIndex {
    pub symbols: Vec<TemplateSymbol>,
    pub references: Vec<TemplateReference>,
    pub tokens: Vec<TemplateSemanticToken>,
    scopes: Vec<TemplateScope>,
}

#[derive(Debug, Clone, Facet)]
pub struct TemplateSymbol {
    pub id: usize,
    pub name: String,
    pub kind: TemplateSymbolKind,
    pub span: Option<Span>,
    pub origin: Option<TemplateSymbolOrigin>,
    scope: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TemplateSymbolOrigin {
    ContextRoot,
    Function,
    ExpressionPath(Vec<String>),
    IterationItem(Vec<String>),
    MacroParam,
    ImportAlias,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TemplateSymbolKind {
    ContextRoot,
    Function,
    SetBinding,
    LoopBinding,
    MacroParam,
    ImportAlias,
    Macro,
}

#[derive(Debug, Clone, Facet)]
pub struct TemplateReference {
    pub name: String,
    pub span: Span,
    pub kind: TemplateReferenceKind,
    pub access: TemplateReferenceAccess,
    pub symbol_id: Option<usize>,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TemplateReferenceAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TemplateReferenceKind {
    Variable,
    Function,
    Field,
    Filter,
    Test,
    MacroNamespace,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct TemplateSemanticToken {
    pub span: Span,
    pub kind: TemplateSemanticTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TemplateSemanticTokenKind {
    Variable,
    Parameter,
    Property,
    Function,
    Macro,
    String,
    Number,
    Keyword,
}

#[derive(Debug, Clone, Facet)]
struct TemplateScope {
    parent: Option<usize>,
    span: Span,
    symbols: HashMap<String, usize>,
}

impl TemplateSemanticIndex {
    pub fn build(
        template: &Template,
        context_roots: &[&str],
        functions: &[&str],
    ) -> TemplateSemanticIndex {
        let mut builder = SemanticBuilder {
            index: TemplateSemanticIndex {
                symbols: Vec::new(),
                references: Vec::new(),
                tokens: Vec::new(),
                scopes: Vec::new(),
            },
        };
        let root_scope = builder.push_scope(None, template.span);
        for name in context_roots {
            builder.define(
                root_scope,
                *name,
                TemplateSymbolKind::ContextRoot,
                None,
                Some(TemplateSymbolOrigin::ContextRoot),
            );
        }
        for name in functions {
            builder.define(
                root_scope,
                *name,
                TemplateSymbolKind::Function,
                None,
                Some(TemplateSymbolOrigin::Function),
            );
        }
        builder.collect_nodes(&template.body, root_scope);
        builder.index
    }

    pub fn symbol_at_offset(&self, offset: usize) -> Option<&TemplateSymbol> {
        self.symbols.iter().find(|symbol| {
            symbol
                .span
                .is_some_and(|span| span_contains_offset(span, offset))
        })
    }

    pub fn reference_at_offset(&self, offset: usize) -> Option<&TemplateReference> {
        self.references
            .iter()
            .find(|reference| span_contains_offset(reference.span, offset))
    }

    pub fn symbol_for_offset(&self, offset: usize) -> Option<&TemplateSymbol> {
        if let Some(reference) = self.reference_at_offset(offset) {
            return reference
                .symbol_id
                .and_then(|symbol_id| self.symbols.get(symbol_id));
        }
        self.symbol_at_offset(offset)
    }

    pub fn references_to_symbol(&self, symbol_id: usize) -> Vec<&TemplateReference> {
        self.references
            .iter()
            .filter(|reference| reference.symbol_id == Some(symbol_id))
            .collect()
    }

    pub fn references_to_symbol_with_access(
        &self,
        symbol_id: usize,
        access: TemplateReferenceAccess,
    ) -> Vec<&TemplateReference> {
        self.references
            .iter()
            .filter(|reference| {
                reference.symbol_id == Some(symbol_id) && reference.access == access
            })
            .collect()
    }

    pub fn read_references_to_symbol(&self, symbol_id: usize) -> Vec<&TemplateReference> {
        self.references_to_symbol_with_access(symbol_id, TemplateReferenceAccess::Read)
    }

    pub fn write_references_to_symbol(&self, symbol_id: usize) -> Vec<&TemplateReference> {
        self.references_to_symbol_with_access(symbol_id, TemplateReferenceAccess::Write)
    }

    pub fn visible_symbols_at_offset(&self, offset: usize) -> Vec<&TemplateSymbol> {
        let mut scope_ids = self
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| span_contains_offset(scope.span, offset))
            .map(|(scope_id, _)| scope_id)
            .collect::<Vec<_>>();
        scope_ids.sort_by_key(|scope_id| std::cmp::Reverse(self.scope_depth(*scope_id)));

        let mut visible = Vec::new();
        let mut seen = HashSet::new();
        for scope_id in scope_ids {
            let mut current = Some(scope_id);
            while let Some(id) = current {
                for (name, symbol_id) in &self.scopes[id].symbols {
                    if let Some(symbol) = self.symbols.get(*symbol_id)
                        && symbol_is_visible_at_offset(symbol, offset)
                        && seen.insert(name.clone())
                    {
                        visible.push(symbol);
                    }
                }
                current = self.scopes[id].parent;
            }
        }
        visible
    }

    pub fn visible_symbol_named_at_offset(
        &self,
        name: &str,
        offset: usize,
    ) -> Option<&TemplateSymbol> {
        self.visible_symbols_at_offset(offset)
            .into_iter()
            .find(|symbol| symbol.name == name)
    }

    fn scope_depth(&self, mut scope_id: usize) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.scopes[scope_id].parent {
            depth += 1;
            scope_id = parent;
        }
        depth
    }
}

struct SemanticBuilder {
    index: TemplateSemanticIndex,
}

impl SemanticBuilder {
    fn push_scope(&mut self, parent: Option<usize>, span: Span) -> usize {
        let scope_id = self.index.scopes.len();
        self.index.scopes.push(TemplateScope {
            parent,
            span,
            symbols: HashMap::new(),
        });
        scope_id
    }

    fn define(
        &mut self,
        scope: usize,
        name: impl Into<String>,
        kind: TemplateSymbolKind,
        span: Option<Span>,
        origin: Option<TemplateSymbolOrigin>,
    ) -> usize {
        let name = name.into();
        let id = self.index.symbols.len();
        self.index.symbols.push(TemplateSymbol {
            id,
            name: name.clone(),
            kind,
            span,
            origin,
            scope,
        });
        self.index.scopes[scope].symbols.insert(name, id);
        if let Some(span) = span {
            self.index.tokens.push(TemplateSemanticToken {
                span,
                kind: semantic_token_kind_for_symbol(kind),
            });
        }
        id
    }

    fn resolve(&self, scope: usize, name: &str) -> Option<usize> {
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            if let Some(symbol_id) = self.index.scopes[scope_id].symbols.get(name) {
                return Some(*symbol_id);
            }
            current = self.index.scopes[scope_id].parent;
        }
        None
    }

    fn reference(&mut self, scope: usize, ident: &Ident, kind: TemplateReferenceKind) {
        self.reference_with_access(scope, ident, kind, TemplateReferenceAccess::Read);
    }

    fn reference_with_access(
        &mut self,
        scope: usize,
        ident: &Ident,
        kind: TemplateReferenceKind,
        access: TemplateReferenceAccess,
    ) {
        let symbol_id = self.resolve(scope, &ident.name);
        self.index.references.push(TemplateReference {
            name: ident.name.clone(),
            span: ident.span,
            kind,
            access,
            symbol_id,
            path: vec![ident.name.clone()],
        });
        self.index.tokens.push(TemplateSemanticToken {
            span: ident.span,
            kind: semantic_token_kind_for_reference(
                kind,
                symbol_id.and_then(|id| self.index.symbols.get(id)),
            ),
        });
    }

    fn collect_nodes(&mut self, nodes: &[Node], scope: usize) {
        for node in nodes {
            match node {
                Node::Text(_) | Node::Comment(_) | Node::Continue(_) | Node::Break(_) => {}
                Node::Print(node) => self.collect_expr(&node.expr, scope),
                Node::If(node) => self.collect_if(node, scope),
                Node::For(node) => {
                    self.collect_expr(&node.iter, scope);
                    let body_scope = self.push_scope(Some(scope), node.span);
                    self.define_target(body_scope, &node.target, expr_path(&node.iter));
                    self.collect_nodes(&node.body, body_scope);
                    if let Some(body) = &node.else_body {
                        self.collect_nodes(body, scope);
                    }
                }
                Node::Include(node) => {
                    self.index.tokens.push(TemplateSemanticToken {
                        span: node.path.span,
                        kind: TemplateSemanticTokenKind::String,
                    });
                    if let Some(context) = &node.context {
                        self.collect_expr(context, scope);
                    }
                }
                Node::Block(node) => self.collect_nodes(&node.body, scope),
                Node::Extends(node) => self.index.tokens.push(TemplateSemanticToken {
                    span: node.path.span,
                    kind: TemplateSemanticTokenKind::String,
                }),
                Node::Set(node) => {
                    let origin = match &node.value {
                        SetValue::Expr(expr) => {
                            self.collect_expr(expr, scope);
                            expr_path(expr).map(TemplateSymbolOrigin::ExpressionPath)
                        }
                        SetValue::Body(body) => {
                            self.collect_nodes(body, scope);
                            None
                        }
                    };
                    self.define_set_binding(scope, &node.name, origin);
                }
                Node::Import(node) => {
                    self.index.tokens.push(TemplateSemanticToken {
                        span: node.path.span,
                        kind: TemplateSemanticTokenKind::String,
                    });
                    self.define(
                        scope,
                        node.alias.name.clone(),
                        TemplateSymbolKind::ImportAlias,
                        Some(node.alias.span),
                        Some(TemplateSymbolOrigin::ImportAlias),
                    );
                }
                Node::Macro(node) => self.collect_macro(node, scope),
                Node::CallBlock(node) => {
                    self.reference(scope, &node.func_name, TemplateReferenceKind::Function);
                    for (name, expr) in &node.kwargs {
                        self.index.tokens.push(TemplateSemanticToken {
                            span: name.span,
                            kind: TemplateSemanticTokenKind::Property,
                        });
                        self.collect_expr(expr, scope);
                    }
                }
            }
        }
    }

    fn collect_if(&mut self, node: &IfNode, scope: usize) {
        self.collect_expr(&node.condition, scope);
        self.collect_nodes(&node.then_body, scope);
        for branch in &node.elif_branches {
            self.collect_expr(&branch.condition, scope);
            self.collect_nodes(&branch.body, scope);
        }
        if let Some(body) = &node.else_body {
            self.collect_nodes(body, scope);
        }
    }

    fn define_set_binding(
        &mut self,
        scope: usize,
        ident: &Ident,
        origin: Option<TemplateSymbolOrigin>,
    ) {
        if let Some(symbol_id) = self.index.scopes[scope]
            .symbols
            .get(&ident.name)
            .copied()
            .filter(|symbol_id| {
                self.index
                    .symbols
                    .get(*symbol_id)
                    .is_some_and(|symbol| symbol.kind == TemplateSymbolKind::SetBinding)
            })
        {
            self.index.references.push(TemplateReference {
                name: ident.name.clone(),
                span: ident.span,
                kind: TemplateReferenceKind::Variable,
                access: TemplateReferenceAccess::Write,
                symbol_id: Some(symbol_id),
                path: vec![ident.name.clone()],
            });
            self.index.tokens.push(TemplateSemanticToken {
                span: ident.span,
                kind: TemplateSemanticTokenKind::Variable,
            });
            return;
        }

        self.define(
            scope,
            ident.name.clone(),
            TemplateSymbolKind::SetBinding,
            Some(ident.span),
            origin,
        );
    }

    fn collect_macro(&mut self, node: &MacroNode, scope: usize) {
        self.define(
            scope,
            node.name.name.clone(),
            TemplateSymbolKind::Macro,
            Some(node.name.span),
            Some(TemplateSymbolOrigin::Macro),
        );
        let macro_scope = self.push_scope(Some(scope), node.span);
        for param in &node.params {
            self.define(
                macro_scope,
                param.name.name.clone(),
                TemplateSymbolKind::MacroParam,
                Some(param.name.span),
                Some(TemplateSymbolOrigin::MacroParam),
            );
            if let Some(default) = &param.default {
                self.collect_expr(default, macro_scope);
            }
        }
        self.collect_nodes(&node.body, macro_scope);
    }

    fn define_target(&mut self, scope: usize, target: &Target, iter_path: Option<Vec<String>>) {
        match target {
            Target::Single { name, span } => {
                self.define(
                    scope,
                    name.clone(),
                    TemplateSymbolKind::LoopBinding,
                    Some(*span),
                    iter_path.map(TemplateSymbolOrigin::IterationItem),
                );
            }
            Target::Tuple { names, .. } => {
                for (name, span) in names {
                    self.define(
                        scope,
                        name.clone(),
                        TemplateSymbolKind::LoopBinding,
                        Some(*span),
                        iter_path.clone().map(TemplateSymbolOrigin::IterationItem),
                    );
                }
            }
        }
    }

    fn collect_expr(&mut self, expr: &Expr, scope: usize) {
        match expr {
            Expr::Literal(literal) => self.collect_literal(literal, scope),
            Expr::Var(ident) => self.reference(scope, ident, TemplateReferenceKind::Variable),
            Expr::Field(expr) => {
                self.collect_expr(&expr.base, scope);
                self.index.references.push(TemplateReference {
                    name: expr.field.name.clone(),
                    span: expr.field.span,
                    kind: TemplateReferenceKind::Field,
                    access: TemplateReferenceAccess::Read,
                    symbol_id: None,
                    path: field_expr_path(expr).unwrap_or_else(|| vec![expr.field.name.clone()]),
                });
                self.index.tokens.push(TemplateSemanticToken {
                    span: expr.field.span,
                    kind: TemplateSemanticTokenKind::Property,
                });
            }
            Expr::Index(expr) => {
                self.collect_expr(&expr.base, scope);
                self.collect_expr(&expr.index, scope);
            }
            Expr::Filter(expr) => self.collect_filter(expr, scope),
            Expr::Binary(expr) => self.collect_binary(expr, scope),
            Expr::Unary(expr) => self.collect_unary(expr, scope),
            Expr::Call(expr) => self.collect_call(expr, scope),
            Expr::Ternary(expr) => self.collect_ternary(expr, scope),
            Expr::Test(expr) => self.collect_test(expr, scope),
            Expr::MacroCall(expr) => self.collect_macro_call(expr, scope),
            Expr::Optional(expr) => self.collect_expr(&expr.expr, scope),
        }
    }

    fn collect_filter(&mut self, expr: &FilterExpr, scope: usize) {
        self.collect_expr(&expr.expr, scope);
        self.index.references.push(TemplateReference {
            name: expr.filter.name.clone(),
            span: expr.filter.span,
            kind: TemplateReferenceKind::Filter,
            access: TemplateReferenceAccess::Read,
            symbol_id: None,
            path: vec![expr.filter.name.clone()],
        });
        self.index.tokens.push(TemplateSemanticToken {
            span: expr.filter.span,
            kind: TemplateSemanticTokenKind::Function,
        });
        for arg in &expr.args {
            self.collect_expr(arg, scope);
        }
        for (name, expr) in &expr.kwargs {
            self.index.tokens.push(TemplateSemanticToken {
                span: name.span,
                kind: TemplateSemanticTokenKind::Property,
            });
            self.collect_expr(expr, scope);
        }
    }

    fn collect_binary(&mut self, expr: &BinaryExpr, scope: usize) {
        self.collect_expr(&expr.left, scope);
        self.collect_expr(&expr.right, scope);
    }

    fn collect_unary(&mut self, expr: &UnaryExpr, scope: usize) {
        self.collect_expr(&expr.expr, scope);
    }

    fn collect_call(&mut self, expr: &CallExpr, scope: usize) {
        self.collect_expr(&expr.func, scope);
        for arg in &expr.args {
            self.collect_expr(arg, scope);
        }
        for (name, expr) in &expr.kwargs {
            self.index.tokens.push(TemplateSemanticToken {
                span: name.span,
                kind: TemplateSemanticTokenKind::Property,
            });
            self.collect_expr(expr, scope);
        }
    }

    fn collect_ternary(&mut self, expr: &TernaryExpr, scope: usize) {
        self.collect_expr(&expr.value, scope);
        self.collect_expr(&expr.condition, scope);
        self.collect_expr(&expr.otherwise, scope);
    }

    fn collect_test(&mut self, expr: &TestExpr, scope: usize) {
        self.collect_expr(&expr.expr, scope);
        self.index.references.push(TemplateReference {
            name: expr.test_name.name.clone(),
            span: expr.test_name.span,
            kind: TemplateReferenceKind::Test,
            access: TemplateReferenceAccess::Read,
            symbol_id: None,
            path: vec![expr.test_name.name.clone()],
        });
        self.index.tokens.push(TemplateSemanticToken {
            span: expr.test_name.span,
            kind: TemplateSemanticTokenKind::Function,
        });
        for arg in &expr.args {
            self.collect_expr(arg, scope);
        }
    }

    fn collect_macro_call(&mut self, expr: &MacroCallExpr, scope: usize) {
        self.reference(
            scope,
            &expr.namespace,
            TemplateReferenceKind::MacroNamespace,
        );
        self.index.references.push(TemplateReference {
            name: expr.macro_name.name.clone(),
            span: expr.macro_name.span,
            kind: TemplateReferenceKind::Macro,
            access: TemplateReferenceAccess::Read,
            symbol_id: None,
            path: vec![expr.namespace.name.clone(), expr.macro_name.name.clone()],
        });
        self.index.tokens.push(TemplateSemanticToken {
            span: expr.macro_name.span,
            kind: TemplateSemanticTokenKind::Macro,
        });
        for arg in &expr.args {
            self.collect_expr(arg, scope);
        }
        for (name, expr) in &expr.kwargs {
            self.index.tokens.push(TemplateSemanticToken {
                span: name.span,
                kind: TemplateSemanticTokenKind::Property,
            });
            self.collect_expr(expr, scope);
        }
    }

    fn collect_literal(&mut self, literal: &Literal, scope: usize) {
        self.token_literal(literal);
        match literal {
            Literal::List(ListLit { elements, .. }) => {
                for expr in elements {
                    self.collect_expr(expr, scope);
                }
            }
            Literal::Dict(DictLit { entries, .. }) => {
                for (key, value) in entries {
                    self.collect_expr(key, scope);
                    self.collect_expr(value, scope);
                }
            }
            Literal::String(_)
            | Literal::Int(_)
            | Literal::Float(_)
            | Literal::Bool(_)
            | Literal::None(_) => {}
        }
    }

    fn token_literal(&mut self, literal: &Literal) {
        let kind = match literal {
            Literal::String(_) => TemplateSemanticTokenKind::String,
            Literal::Int(_) | Literal::Float(_) => TemplateSemanticTokenKind::Number,
            Literal::Bool(_) | Literal::None(_) => TemplateSemanticTokenKind::Keyword,
            Literal::List(_) | Literal::Dict(_) => return,
        };
        self.index.tokens.push(TemplateSemanticToken {
            span: literal.span(),
            kind,
        });
    }
}

fn semantic_token_kind_for_symbol(kind: TemplateSymbolKind) -> TemplateSemanticTokenKind {
    match kind {
        TemplateSymbolKind::ContextRoot | TemplateSymbolKind::SetBinding => {
            TemplateSemanticTokenKind::Variable
        }
        TemplateSymbolKind::Function => TemplateSemanticTokenKind::Function,
        TemplateSymbolKind::LoopBinding | TemplateSymbolKind::MacroParam => {
            TemplateSemanticTokenKind::Parameter
        }
        TemplateSymbolKind::ImportAlias | TemplateSymbolKind::Macro => {
            TemplateSemanticTokenKind::Macro
        }
    }
}

fn semantic_token_kind_for_reference(
    kind: TemplateReferenceKind,
    symbol: Option<&TemplateSymbol>,
) -> TemplateSemanticTokenKind {
    match kind {
        TemplateReferenceKind::Field => TemplateSemanticTokenKind::Property,
        TemplateReferenceKind::Filter
        | TemplateReferenceKind::Test
        | TemplateReferenceKind::Function => TemplateSemanticTokenKind::Function,
        TemplateReferenceKind::MacroNamespace | TemplateReferenceKind::Macro => {
            TemplateSemanticTokenKind::Macro
        }
        TemplateReferenceKind::Variable => symbol
            .map(|symbol| semantic_token_kind_for_symbol(symbol.kind))
            .unwrap_or(TemplateSemanticTokenKind::Variable),
    }
}

fn span_contains_offset(span: Span, offset: usize) -> bool {
    span.offset() <= offset && offset <= span.offset().saturating_add(span.len())
}

fn symbol_is_visible_at_offset(symbol: &TemplateSymbol, offset: usize) -> bool {
    symbol.span.is_none_or(|span| span.offset() <= offset)
}

fn expr_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Var(ident) => Some(vec![ident.name.clone()]),
        Expr::Field(field) => field_expr_path(field),
        _ => None,
    }
}

fn field_expr_path(expr: &crate::ast::FieldExpr) -> Option<Vec<String>> {
    let mut path = expr_path(&expr.base)?;
    path.push(expr.field.name.clone());
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_index(source: &str) -> TemplateSemanticIndex {
        let template = crate::parse_template("test.html", source).expect("template");
        TemplateSemanticIndex::build(&template, &["page", "section", "root"], &[])
    }

    #[test]
    fn records_set_and_loop_binding_origins() {
        let source =
            "{% set current = page %}\n{% for item in section.pages %}{{ item.path }}{% endfor %}";
        let index = semantic_index(source);

        let current = index
            .symbols
            .iter()
            .find(|symbol| symbol.name == "current")
            .expect("current symbol");
        assert_eq!(
            current.origin,
            Some(TemplateSymbolOrigin::ExpressionPath(vec![
                "page".to_string()
            ]))
        );

        let item = index
            .symbols
            .iter()
            .find(|symbol| symbol.name == "item")
            .expect("item symbol");
        assert_eq!(
            item.origin,
            Some(TemplateSymbolOrigin::IterationItem(vec![
                "section".to_string(),
                "pages".to_string()
            ]))
        );
    }

    #[test]
    fn hides_later_bindings_from_visible_symbol_lookup() {
        let source = "{{ alpha }}\n{% set alpha = page %}\n{{ alpha.path }}";
        let index = semantic_index(source);
        let first_alpha = source.find("alpha").expect("first alpha");
        assert!(
            index
                .visible_symbol_named_at_offset("alpha", first_alpha)
                .is_none(),
            "set bindings are not visible before their declaration"
        );
        let last_alpha = source.rfind("alpha").expect("last alpha");
        assert!(
            index
                .visible_symbol_named_at_offset("alpha", last_alpha)
                .is_some(),
            "set bindings are visible after their declaration"
        );
    }

    #[test]
    fn repeated_set_in_same_scope_references_original_binding() {
        let source = "{% set current_path = \"/\" %}\n{% set current_path = section.path %}\n{% set current_path = page.path %}\n{{ current_path }}";
        let index = semantic_index(source);
        let offsets = source
            .match_indices("current_path")
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 4);

        let symbol = index
            .symbol_for_offset(offsets[0])
            .expect("first set binding");
        assert_eq!(symbol.name, "current_path");
        assert_eq!(symbol.kind, TemplateSymbolKind::SetBinding);
        assert_eq!(symbol.span.expect("symbol span").offset(), offsets[0]);

        for offset in offsets.iter().copied().skip(1) {
            let resolved = index
                .symbol_for_offset(offset)
                .expect("later assignment or use resolves");
            assert_eq!(resolved.id, symbol.id);
        }

        assert_eq!(index.references_to_symbol(symbol.id).len(), 3);
    }

    #[test]
    fn classifies_symbol_references_as_reads_or_writes() {
        let source = "{% set current_path = \"/\" %}\n{% set current_path = section.path %}\n{{ current_path }}";
        let index = semantic_index(source);
        let offsets = source
            .match_indices("current_path")
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 3);

        let symbol = index
            .symbol_for_offset(offsets[0])
            .expect("first set binding");
        let references = index.references_to_symbol(symbol.id);
        assert_eq!(index.write_references_to_symbol(symbol.id).len(), 1);
        assert_eq!(index.read_references_to_symbol(symbol.id).len(), 1);
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.access)
                .collect::<Vec<_>>(),
            vec![
                TemplateReferenceAccess::Write,
                TemplateReferenceAccess::Read
            ]
        );
        assert_eq!(references[0].span.offset(), offsets[1]);
        assert_eq!(references[1].span.offset(), offsets[2]);
    }
}
