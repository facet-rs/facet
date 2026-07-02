//! The binder: scopes, symbols, reference resolution over the generated AST.
//!
//! Ring 2 of the LSP story and the demand-engine oracle's front half — one resolver,
//! two consumers. Written against the GENERATED AST on purpose: when the grammar
//! changes, the AST regenerates and this file breaks loudly at compile time instead
//! of drifting like a parallel-universe front end.
//!
//! v0 scope model:
//!   - file scope: `fn` items (order-independent) and `use` leaves (imports);
//!   - fn scope: params;
//!   - block scope: `let`s, sequential (a let is visible AFTER its own initializer,
//!     so `let x = f(x)` sees the outer `x`); shadowing allowed;
//!   - closure scope: params.
//!
//! Command names resolve as VALUE references: `cc! { … }` invokes the capability
//! `cc` in scope, so renaming the binding renames the invocation.
//!
//! Identifier patterns (`Linux => …`) are constructor-like and there is no enum
//! declaration surface yet — they land in `unresolved`, as do calls to primitives
//! that need a prelude (`fetch`, `extract`) and unimported types (`Flag`).
//! Unresolved references are values, not errors: the future type/prelude layers
//! consume this list.

use crate::ast::{
    Arg, ArrayElem, Block, Callee, CommandPart, Expr, Item, Pattern, SourceFile, Span, Spanned,
    Stmt, Type,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Fn,
    Param,
    Let,
    ClosureParam,
    Import,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// Span of the defining occurrence (the name itself, not the whole item).
    pub def: Span,
}

/// The result of binding one source file.
#[derive(Debug, Default)]
pub struct Bindings {
    symbols: Vec<Symbol>,
    /// Resolved reference occurrences (span of the referencing identifier).
    refs: Vec<(Span, SymbolId)>,
    /// References nothing in scope resolves: primitives awaiting a prelude,
    /// constructor-like patterns, unimported types.
    unresolved: Vec<Spanned<String>>,
}

impl Bindings {
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0]
    }

    pub fn symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        self.symbols.iter().enumerate().map(|(i, s)| (SymbolId(i), s))
    }

    pub fn unresolved(&self) -> &[Spanned<String>] {
        &self.unresolved
    }

    /// The symbol whose definition or reference covers this byte offset.
    pub fn symbol_at(&self, offset: u32) -> Option<SymbolId> {
        if let Some((_, id)) = self.refs.iter().find(|(span, _)| span.contains(offset)) {
            return Some(*id);
        }
        self.symbols
            .iter()
            .position(|s| s.def.contains(offset))
            .map(SymbolId)
    }

    /// Reference sites only (excluding the definition).
    pub fn references(&self, id: SymbolId) -> Vec<Span> {
        self.refs
            .iter()
            .filter(|(_, rid)| *rid == id)
            .map(|(span, _)| *span)
            .collect()
    }

    /// Definition + references, in source order — the rename set.
    pub fn occurrences(&self, id: SymbolId) -> Vec<Span> {
        let mut out = self.references(id);
        out.push(self.symbols[id.0].def);
        out.sort_by_key(|s| s.start);
        out
    }

    /// Text edits renaming every occurrence of the symbol.
    pub fn rename_edits(&self, id: SymbolId, new_name: &str) -> Vec<(Span, String)> {
        self.occurrences(id)
            .into_iter()
            .map(|span| (span, new_name.to_string()))
            .collect()
    }
}

/// Bind a source file: build the symbol table and resolve every reference.
pub fn bind(file: &SourceFile) -> Bindings {
    let mut b = Binder {
        out: Bindings::default(),
        scopes: vec![Vec::new()],
    };

    // Pass 1: file scope. Items are visible file-wide regardless of order.
    for item in &file.items {
        match item {
            Item::Use(u) => {
                for leaf in &u.tree.leaves {
                    b.define(SymbolKind::Import, leaf);
                }
                // Single-name imports (`use foo::bar;`) import their last segment.
                if u.tree.leaves.is_empty()
                    && let Some(last) = u.tree.segments.last()
                {
                    b.define(SymbolKind::Import, last);
                }
            }
            Item::Fn(f) => {
                b.define(SymbolKind::Fn, &f.name);
            }
        }
    }

    // Pass 2: bodies.
    for item in &file.items {
        if let Item::Fn(f) = item {
            b.push();
            for p in &f.params.params {
                b.define(SymbolKind::Param, &p.name);
                b.ty(&p.ty);
            }
            if let Some(rt) = &f.return_type {
                b.ty(rt);
            }
            b.block(&f.body);
            b.pop();
        }
    }

    b.out
}

struct Binder {
    out: Bindings,
    /// Innermost scope last; within a scope, later definitions shadow earlier ones.
    scopes: Vec<Vec<(String, SymbolId)>>,
}

impl Binder {
    fn push(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, kind: SymbolKind, name: &Spanned<String>) -> SymbolId {
        let id = SymbolId(self.out.symbols.len());
        self.out.symbols.push(Symbol {
            name: name.value.clone(),
            kind,
            def: name.span,
        });
        self.scopes
            .last_mut()
            .expect("binder always has a scope")
            .push((name.value.clone(), id));
        id
    }

    fn resolve(&mut self, name: &Spanned<String>) {
        for scope in self.scopes.iter().rev() {
            if let Some((_, id)) = scope.iter().rev().find(|(n, _)| *n == name.value) {
                self.out.refs.push((name.span, *id));
                return;
            }
        }
        self.out.unresolved.push(name.clone());
    }

    fn ty(&mut self, t: &Type) {
        match t {
            Type::Array(a) => self.ty(&a.elem),
            // Multi-segment type paths resolve their head; the rest waits for modules.
            Type::Path(p) => {
                if let Some(head) = p.segments.first() {
                    self.resolve(head);
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        self.push();
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let(l) => {
                    // Initializer first: `let x = f(x)` sees the OUTER x.
                    self.expr(&l.value);
                    if let Some(ty) = &l.ty {
                        self.ty(ty);
                    }
                    self.define(SymbolKind::Let, &l.name);
                }
                Stmt::Expr(e) => self.expr(&e.expr),
            }
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
        self.pop();
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Binary(x) => {
                self.expr(&x.left);
                self.expr(&x.right);
            }
            Expr::Unary(x) => self.expr(&x.operand),
            Expr::Paren(x) => self.expr(&x.inner),
            Expr::Call(x) => {
                match &x.callee {
                    Callee::Identifier(name) => self.resolve(name),
                    Callee::Scoped(s) => {
                        if let Some(head) = s.segments.first() {
                            self.resolve(head);
                        }
                    }
                }
                self.args(&x.args);
            }
            Expr::MethodCall(x) => {
                // The method name is not a value reference (no method decls yet).
                self.expr(&x.receiver);
                self.args(&x.args);
            }
            Expr::Field(x) => self.expr(&x.receiver),
            Expr::Match(x) => {
                self.expr(&x.scrutinee);
                for arm in &x.arms {
                    if let Pattern::Identifier(name) = &arm.pattern {
                        // Constructor-like; no enum surface yet. Recorded, not bound.
                        self.out.unresolved.push(name.clone());
                    }
                    self.expr(&arm.value);
                }
            }
            Expr::Closure(x) => {
                self.push();
                for p in &x.params {
                    self.define(SymbolKind::ClosureParam, p);
                }
                self.expr(&x.body);
                self.pop();
            }
            Expr::Command(x) => {
                // `cc! { … }` invokes the capability value `cc` in scope.
                self.resolve(&x.command);
                for part in &x.parts {
                    if let CommandPart::Splice(s) = part {
                        self.expr(&s.expr);
                    }
                }
            }
            Expr::Array(x) => {
                for elem in &x.elems {
                    if let ArrayElem::Expr(e) = elem {
                        self.expr(e);
                    }
                }
            }
            Expr::Scoped(s) => {
                if let Some(head) = s.segments.first() {
                    self.resolve(head);
                }
            }
            Expr::Identifier(name) => self.resolve(name),
            Expr::Str(_) | Expr::Path(_) | Expr::Number(_) | Expr::Bool(_) => {}
        }
    }

    fn args(&mut self, args: &crate::ast::ArgList) {
        for arg in &args.args {
            match arg {
                // A kwarg's NAME names the callee's parameter, not a value in scope.
                Arg::Kwarg(k) => self.expr(&k.value),
                Arg::Expr(e) => self.expr(e),
            }
        }
    }
}
