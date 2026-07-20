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

use std::collections::BTreeMap;

use crate::ast::{
    Arg, ArrayElem, Block, CommandAtom, CommandPart, CommandPattern, EnumItem, Expr, FieldList,
    GenericParams, Item, PathRef, Pattern, SourceFile, Span, Spanned, Stmt, StructItem,
    TupleFields, Type,
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
    /// A struct or enum declaration.
    Type,
    /// A generic type parameter (`<A, B>`).
    TypeParam,
    /// A name bound by a match pattern (payload/shorthand positions).
    Binding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Fn,
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub module: String,
    pub name: String,
    pub kind: ImportKind,
}

#[derive(Debug)]
pub struct ModuleBindings {
    modules: BTreeMap<String, Bindings>,
    imports: BTreeMap<(String, String), ResolvedImport>,
}

impl ModuleBindings {
    pub fn module(&self, path: &str) -> Option<&Bindings> {
        self.modules.get(path)
    }

    pub fn modules(&self) -> impl Iterator<Item = (&str, &Bindings)> {
        self.modules
            .iter()
            .map(|(path, bindings)| (path.as_str(), bindings))
    }

    pub fn import(&self, module: &str, name: &str) -> Option<&ResolvedImport> {
        self.imports.get(&(module.to_string(), name.to_string()))
    }

    pub fn imports(&self) -> impl Iterator<Item = (&(String, String), &ResolvedImport)> {
        self.imports.iter()
    }
}

/// Primitive scalar types need no declaration or import; they resolve silently
/// (no ref recorded — there is no def site to jump to).
const BUILTIN_TYPES: &[&str] = &[
    "Int",
    "Float",
    "String",
    "Bool",
    "Blob",
    "Doc",
    "Tree",
    "Version",
    "VersionSet",
    "Sealed",
];

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
        self.symbols
            .iter()
            .enumerate()
            .map(|(i, s)| (SymbolId(i), s))
    }

    /// Every resolved reference occurrence.
    pub fn refs(&self) -> impl Iterator<Item = (Span, SymbolId)> + '_ {
        self.refs.iter().copied()
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
            Item::Struct(s) => {
                b.define(SymbolKind::Type, &s.name);
            }
            Item::Enum(e) => {
                b.define(SymbolKind::Type, &e.name);
            }
            Item::Command(c) => {
                b.define(SymbolKind::Type, &c.name);
            }
        }
    }

    // Pass 2: bodies and type declarations.
    for item in &file.items {
        match item {
            Item::Use(_) => {}
            Item::Fn(f) => {
                b.push();
                b.generics(&f.generics);
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
            Item::Struct(s) => b.struct_item(s),
            Item::Enum(e) => b.enum_item(e),
            Item::Command(c) => {
                if let Some(return_type) = &c.return_type {
                    b.ty(return_type);
                }
                b.command_pattern(&c.grammar.pattern);
            }
        }
    }

    b.out
}

pub fn bind_module_set(
    root: &str,
    files: &BTreeMap<String, SourceFile>,
) -> Result<ModuleBindings, String> {
    if !files.contains_key(root) {
        return Err(format!("root module `{root}` is not in the module set"));
    }

    let modules = files
        .iter()
        .map(|(path, file)| (path.clone(), bind(file)))
        .collect();
    let mut imports = BTreeMap::new();
    for (from, file) in files {
        for item in &file.items {
            let Item::Use(use_item) = item else {
                continue;
            };
            let import_specs = import_specs(use_item)?;
            for (target_module, name) in import_specs {
                let item = if let Some(item) = builtin_module_item(&target_module, &name) {
                    item
                } else {
                    let target = files.get(&target_module).ok_or_else(|| {
                        format!(
                            "module `{from}` imports `{name}` from missing module `{target_module}`"
                        )
                    })?;
                    module_item(target, &name).ok_or_else(|| {
                        format!("module `{target_module}` has no item named `{name}`")
                    })?
                };
                if from != &target_module && !item.public {
                    return Err(format!(
                        "module `{from}` cannot import private item `{target_module}::{name}`"
                    ));
                }
                imports.insert(
                    (from.clone(), name.clone()),
                    ResolvedImport {
                        module: target_module,
                        name,
                        kind: item.kind,
                    },
                );
            }
        }
    }

    Ok(ModuleBindings { modules, imports })
}

struct ModuleItem {
    kind: ImportKind,
    public: bool,
}

fn import_specs(use_item: &crate::ast::UseItem) -> Result<Vec<(String, String)>, String> {
    let segments = &use_item.tree.segments;
    if segments.is_empty() {
        return Err("empty use path".into());
    }
    if use_item.tree.leaves.is_empty() {
        let Some((name, module_segments)) = segments.split_last() else {
            return Err("empty use path".into());
        };
        if module_segments.is_empty() {
            return Err(format!("use `{}` has no module path", name.value));
        }
        return Ok(vec![(
            module_segments
                .iter()
                .map(|segment| segment.value.as_str())
                .collect::<Vec<_>>()
                .join("::"),
            name.value.clone(),
        )]);
    }

    let module = segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join("::");
    Ok(use_item
        .tree
        .leaves
        .iter()
        .map(|leaf| (module.clone(), leaf.value.clone()))
        .collect())
}

fn module_item(file: &SourceFile, name: &str) -> Option<ModuleItem> {
    file.items.iter().find_map(|item| match item {
        Item::Fn(f) if f.name.value == name => Some(ModuleItem {
            kind: ImportKind::Fn,
            public: f.vis.is_some(),
        }),
        Item::Struct(s) if s.name.value == name => Some(ModuleItem {
            kind: ImportKind::Type,
            public: s.vis.is_some(),
        }),
        Item::Enum(e) if e.name.value == name => Some(ModuleItem {
            kind: ImportKind::Type,
            public: e.vis.is_some(),
        }),
        _ => None,
    })
}

fn builtin_module_item(module: &str, name: &str) -> Option<ModuleItem> {
    let kind = match (module, name) {
        (
            "vix",
            "Int" | "Float" | "String" | "Bool" | "Blob" | "Doc" | "Tree" | "Path" | "Target"
            | "Map" | "Array" | "Arg" | "Flag" | "Run" | "Os" | "Arch" | "Version" | "VersionSet"
            | "Sealed",
        ) => ImportKind::Type,
        ("caps", "Cc" | "Ar" | "Rustc") => ImportKind::Type,
        _ => return None,
    };
    Some(ModuleItem { kind, public: true })
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
        if BUILTIN_TYPES.contains(&name.value.as_str()) {
            return;
        }
        for scope in self.scopes.iter().rev() {
            if let Some((_, id)) = scope.iter().rev().find(|(n, _)| *n == name.value) {
                self.out.refs.push((name.span, *id));
                return;
            }
        }
        // Prelude bindings (`fetch`, `observe`, …) resolve silently, like
        // BUILTIN_TYPES above: there is no in-file def site to record a ref to.
        // Checked after scopes so a local binding still shadows the prelude.
        if crate::binding::is_prelude_name(&name.value) {
            return;
        }
        self.out.unresolved.push(name.clone());
    }

    fn generics(&mut self, generics: &Option<GenericParams>) {
        if let Some(g) = generics {
            for p in &g.params {
                self.define(SymbolKind::TypeParam, p);
            }
        }
    }

    fn struct_item(&mut self, s: &StructItem) {
        self.push();
        self.generics(&s.generics);
        if let Some(fields) = &s.fields {
            self.field_list(fields);
        }
        if let Some(tuple) = &s.tuple {
            self.tuple_fields(tuple);
        }
        self.pop();
    }

    fn enum_item(&mut self, e: &EnumItem) {
        self.push();
        self.generics(&e.generics);
        for v in &e.variants {
            if let Some(tuple) = &v.tuple {
                self.tuple_fields(tuple);
            }
            if let Some(fields) = &v.fields {
                self.field_list(fields);
            }
        }
        self.pop();
    }

    fn field_list(&mut self, fields: &FieldList) {
        for f in &fields.fields {
            self.ty(&f.ty);
            if let Some(default) = &f.default {
                self.expr(default);
            }
        }
    }

    fn tuple_fields(&mut self, tuple: &TupleFields) {
        for t in &tuple.types {
            self.ty(t);
        }
    }

    fn command_pattern(&mut self, pattern: &CommandPattern) {
        for alternative in &pattern.alternatives {
            for term in &alternative.terms {
                match &term.atom {
                    CommandAtom::Literal(_) => {}
                    CommandAtom::Slot(slot) => self.command_slot_ty(&slot.ty),
                    CommandAtom::Optional(optional) => {
                        self.command_pattern(&optional.pattern);
                    }
                    CommandAtom::Group(group) => self.command_pattern(&group.pattern),
                }
            }
        }
    }

    fn command_slot_ty(&mut self, ty: &Type) {
        const ROLES: &[&str] = &[
            "Executable",
            "Input",
            "InputFlag",
            "Output",
            "OutputFlag",
            "OutputDir",
            "Stdout",
            "Env",
            "SearchDir",
            "SearchDirFlag",
        ];
        match ty {
            Type::Generic(g)
                if g.base
                    .segments
                    .last()
                    .is_some_and(|name| ROLES.contains(&name.value.as_str())) =>
            {
                for arg in &g.args {
                    self.ty(arg);
                }
            }
            Type::Path(path)
                if path
                    .segments
                    .last()
                    .is_some_and(|name| ROLES.contains(&name.value.as_str())) => {}
            _ => self.ty(ty),
        }
    }

    fn path_ref(&mut self, p: &PathRef) {
        match p {
            PathRef::Identifier(name) => self.resolve(name),
            PathRef::Scoped(s) => {
                if let Some(head) = s.segments.first() {
                    self.resolve(head);
                }
            }
        }
    }

    fn ty(&mut self, t: &Type) {
        match t {
            Type::Array(a) => self.ty(&a.elem),
            Type::Generic(g) => {
                if let Some(head) = g.base.segments.first() {
                    self.resolve(head);
                }
                for arg in &g.args {
                    self.ty(arg);
                }
            }
            Type::Tuple(t) => {
                for elem in &t.elems {
                    self.ty(elem);
                }
            }
            Type::Fn(f) => {
                for param in &f.params {
                    self.ty(param);
                }
                if let Some(rt) = &f.return_type {
                    self.ty(rt);
                }
            }
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
                self.path_ref(&x.callee);
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
                    // Pattern bindings scope over the guard and the arm value.
                    self.push();
                    self.pattern(&arm.pattern, true);
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&arm.value);
                    self.pop();
                }
            }
            Expr::StructLit(x) => {
                self.path_ref(&x.path);
                for f in &x.fields {
                    // Field NAMES name the type's fields, not scope values.
                    self.expr(&f.value);
                }
                for s in &x.spreads {
                    // `..base` = record update; bare `..` = partial construction.
                    if let Some(base) = &s.base {
                        self.expr(base);
                    }
                }
            }
            Expr::Map(x) => {
                for entry in &x.entries {
                    self.expr(&entry.key);
                    self.expr(&entry.value);
                }
            }
            Expr::Tuple(x) => {
                for elem in &x.elems {
                    self.expr(elem);
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
            Expr::Template(_) | Expr::Str(_) | Expr::Path(_) | Expr::Number(_) | Expr::Bool(_) => {}
        }
    }

    fn args(&mut self, args: &crate::ast::ArgList) {
        for arg in &args.args {
            match arg {
                // A kwarg's NAME names the callee's parameter, not a value in scope.
                Arg::Kwarg(k) => self.expr(&k.value),
                // A trailing `..` marks the call partial — nothing to resolve.
                Arg::Partial(_) => {}
                Arg::Expr(e) => self.expr(e),
            }
        }
    }

    /// Bind a match pattern. The rule (from the types design sketch): inside
    /// payload positions a bare identifier BINDS; at the TOP of an arm it is
    /// constructor-like and stays unresolved until type-directed resolution
    /// lands (this preserves `Linux => …` behaving as a variant reference, and
    /// keeps the "typo'd variant silently becomes a catch-all" footgun shut).
    ///
    /// Known rename limitation: a shorthand field pattern (`{ name, .. }`)
    /// binds `name`, but renaming that binding must expand the shorthand to
    /// `name: new_name` — rename_edits doesn't know that yet.
    fn pattern(&mut self, p: &Pattern, top: bool) {
        match p {
            Pattern::Wildcard(_) | Pattern::Str(_) | Pattern::Number(_) | Pattern::Bool(_) => {}
            Pattern::Identifier(name) => {
                if top {
                    self.out.unresolved.push(name.clone());
                } else {
                    self.define(SymbolKind::Binding, name);
                }
            }
            Pattern::Scoped(s) => {
                if let Some(head) = s.segments.first() {
                    self.resolve(head);
                }
            }
            Pattern::Variant(v) => {
                self.path_ref(&v.path);
                for arg in &v.args {
                    self.pattern(arg, false);
                }
            }
            Pattern::Struct(sp) => {
                self.path_ref(&sp.path);
                for f in &sp.fields {
                    match &f.pattern {
                        Some(inner) => self.pattern(inner, false),
                        // Shorthand: the field name IS the binding.
                        None => {
                            self.define(SymbolKind::Binding, &f.name);
                        }
                    }
                }
            }
            Pattern::Tuple(t) => {
                for elem in &t.elems {
                    self.pattern(elem, false);
                }
            }
        }
    }
}
