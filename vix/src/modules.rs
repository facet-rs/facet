//! Module-set compilation front: import resolution, visibility, and the
//! source-level merge that feeds a module set through the single-file
//! compiler front (ratchet rungs 106–110).
//!
//! Modules compile away before lowering. Every library item is renamed to its
//! fully-qualified `module::item` spelling — the book's nominal-identity rule
//! ("Nominal identity includes the fully-qualified type name and shape") —
//! and every reference is rewritten to the spelling of the *declaring*
//! module. The item lists then merge into one surface AST and lower through
//! the ordinary front. Canonical recipes therefore carry declaring-module
//! identity only: an island's recipe never depends on which module spelled
//! the call (FOUNDATION: memo/lowering identity across module boundaries).
//!
//! Span caveat: merged items keep their per-file byte spans, so spans from
//! different files may collide numerically. Recipe identity is span-
//! insensitive (certified at rung 001), so this degrades only attribution of
//! post-merge diagnostics in library code, never identity. Import-resolution
//! diagnostics (the reject rungs) are produced before the merge and always
//! carry the importing file's own spans.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::support::{Span, Spanned};
use crate::surface::ast;

/// One named library module presented alongside a root compilation.
#[derive(Clone, Copy, Debug)]
pub struct ModuleSource<'a> {
    /// The module name `import` statements refer to (`geometry`).
    pub name: &'a str,
    /// The module's Vix source text.
    pub source: &'a str,
}

struct DeclaredItem {
    public: bool,
}

/// The declared (non-import) items of one file: name → visibility.
fn declared_items(file: &ast::SourceFile) -> BTreeMap<String, DeclaredItem> {
    let mut items = BTreeMap::new();
    for item in &file.items {
        let (name, public) = match item {
            ast::Item::Fn(function) => (&function.name.value, function.vis.is_some()),
            ast::Item::Struct(record) => (&record.name.value, record.vis.is_some()),
            ast::Item::Enum(enumeration) => (&enumeration.name.value, enumeration.vis.is_some()),
            ast::Item::Import(_) => continue,
        };
        items
            .entry(name.clone())
            .or_insert(DeclaredItem { public });
    }
    items
}

fn duplicate_name(span: Span, name: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::DuplicateDefinition,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Name {
            name: name.to_owned(),
        },
    })
}

fn unknown_name(span: Span, name: impl Into<String>) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::UnknownName,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Name { name: name.into() },
    })
}

/// The imported leaves of one `import` item, each with the span of its own
/// name occurrence.
fn import_leaves(import: &ast::ImportItem) -> Vec<Spanned<String>> {
    match (&import.name, &import.names) {
        (Some(name), _) => vec![name.clone()],
        (None, Some(names)) => names.names.clone(),
        (None, None) => Vec::new(),
    }
}

/// Resolve one file's imports against the module set, in source order and
/// interleaved with the file's own declarations so a name bound twice —
/// import-then-declaration or declaration-then-import — is diagnosed at its
/// *second* binding site (rung 109 anchors at the local declaration that
/// follows the import).
///
/// Returns the file's import aliases: local spelling → fully-qualified
/// declaring-module spelling.
fn resolve_imports(
    own_module: Option<&str>,
    file: &ast::SourceFile,
    set: &BTreeMap<String, BTreeMap<String, DeclaredItem>>,
) -> Result<BTreeMap<String, String>, Diagnostics> {
    let mut bound: BTreeSet<String> = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    for item in &file.items {
        match item {
            ast::Item::Import(import) => {
                let module = &import.module;
                let Some(exports) = set.get(&module.value) else {
                    return Err(unknown_name(module.span, module.value.clone()));
                };
                for leaf in import_leaves(import) {
                    let Some(declared) = exports.get(&leaf.value) else {
                        return Err(unknown_name(
                            leaf.span,
                            format!("{}::{}", module.value, leaf.value),
                        ));
                    };
                    // r[impl lang.module.use] — public items use `pub`; a
                    // non-pub item is not importable from another module.
                    if own_module != Some(module.value.as_str()) && !declared.public {
                        return Err(Diagnostics::one(Diagnostic {
                            code: DiagnosticCode::PrivateImport,
                            primary: leaf.span,
                            labels: Vec::new(),
                            payload: DiagnosticPayload::Name {
                                name: format!("{}::{}", module.value, leaf.value),
                            },
                        }));
                    }
                    if !bound.insert(leaf.value.clone()) {
                        return Err(duplicate_name(leaf.span, &leaf.value));
                    }
                    aliases.insert(
                        leaf.value.clone(),
                        format!("{}::{}", module.value, leaf.value),
                    );
                }
            }
            ast::Item::Fn(function) => {
                // r[impl lang.module.use] — unqualified collisions are
                // compile errors: a local declaration may not rebind an
                // imported name.
                if !bound.insert(function.name.value.clone()) {
                    return Err(duplicate_name(function.name.span, &function.name.value));
                }
            }
            ast::Item::Struct(record) => {
                if !bound.insert(record.name.value.clone()) {
                    return Err(duplicate_name(record.name.span, &record.name.value));
                }
            }
            ast::Item::Enum(enumeration) => {
                if !bound.insert(enumeration.name.value.clone()) {
                    return Err(duplicate_name(
                        enumeration.name.span,
                        &enumeration.name.value,
                    ));
                }
            }
        }
    }
    Ok(aliases)
}

/// Merge a parsed module set into one surface AST for the single-file front.
///
/// Library items are renamed to `module::item`; references in every file are
/// rewritten to the declaring module's spelling; import items are consumed.
/// Item order is deterministic: library modules in presentation order, root
/// items last, so `FunctionId` assignment is stable for a given set.
pub(crate) fn merge_module_set(
    root: ast::SourceFile,
    modules: &[(String, ast::SourceFile)],
) -> Result<ast::SourceFile, Diagnostics> {
    let set: BTreeMap<String, BTreeMap<String, DeclaredItem>> = modules
        .iter()
        .map(|(name, file)| (name.clone(), declared_items(file)))
        .collect();
    let module_names: BTreeSet<String> = set.keys().cloned().collect();

    let mut merged_items = Vec::new();
    for (name, file) in modules {
        let aliases = resolve_imports(Some(name), file, &set)?;
        // A library module's own items are spelled fully qualified; its
        // imports alias to their declaring modules.
        let mut renames: BTreeMap<String, String> = declared_items(file)
            .into_keys()
            .map(|item| (item.clone(), format!("{name}::{item}")))
            .collect();
        renames.extend(aliases);
        let mut rewriter = Rewriter {
            renames: &renames,
            modules: &module_names,
            scopes: Vec::new(),
        };
        for item in &file.items {
            if matches!(item, ast::Item::Import(_)) {
                continue;
            }
            let mut item = item.clone();
            rewriter.item(&mut item)?;
            merged_items.push(item);
        }
    }

    let aliases = resolve_imports(None, &root, &set)?;
    let mut rewriter = Rewriter {
        renames: &aliases,
        modules: &module_names,
        scopes: Vec::new(),
    };
    let span = root.span;
    for item in root.items {
        if matches!(item, ast::Item::Import(_)) {
            continue;
        }
        let mut item = item;
        rewriter.item(&mut item)?;
        merged_items.push(item);
    }

    Ok(ast::SourceFile {
        span,
        items: merged_items,
    })
}

/// Scope-aware reference rewriter: rewrites every free occurrence of a name
/// in `renames` to its fully-qualified spelling, leaving lexically bound
/// occurrences (params, lets, closure params, pattern bindings) untouched.
/// Qualified `module::…` spellings are rejected outright — `import` is the
/// only way to reach another module's items, so a private item cannot be
/// reached by spelling its qualified name either.
struct Rewriter<'a> {
    renames: &'a BTreeMap<String, String>,
    modules: &'a BTreeSet<String>,
    scopes: Vec<BTreeSet<String>>,
}

impl Rewriter<'_> {
    fn in_scope(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn bind(&mut self, name: &Spanned<String>) {
        self.scopes
            .last_mut()
            .expect("rewriter binds inside a scope")
            .insert(name.value.clone());
    }

    /// Rewrite one free name occurrence in place.
    fn reference(&mut self, name: &mut Spanned<String>) {
        if self.in_scope(&name.value) {
            return;
        }
        if let Some(qualified) = self.renames.get(&name.value) {
            name.value = qualified.clone();
        }
    }

    fn item(&mut self, item: &mut ast::Item) -> Result<(), Diagnostics> {
        match item {
            ast::Item::Import(_) => unreachable!("imports are consumed before rewriting"),
            ast::Item::Fn(function) => {
                self.reference(&mut function.name);
                self.scopes.push(BTreeSet::new());
                for param in &mut function.params.params {
                    self.ty(&mut param.ty)?;
                }
                if let Some(where_params) = &mut function.where_params {
                    if let Some(inline) = &mut where_params.inline {
                        for param in &mut inline.params {
                            self.ty(&mut param.ty)?;
                            if let Some(default) = &mut param.default {
                                self.expr(default)?;
                            }
                        }
                    }
                    if let Some(named) = &mut where_params.named {
                        self.type_path(named)?;
                    }
                }
                if let Some(return_type) = &mut function.return_type {
                    self.ty(return_type)?;
                }
                for param in &function.params.params {
                    self.bind(&param.name);
                }
                if let Some(where_params) = &function.where_params
                    && let Some(inline) = &where_params.inline
                {
                    for param in &inline.params {
                        self.bind(&param.name);
                    }
                }
                self.block(&mut function.body)?;
                self.scopes.pop();
            }
            ast::Item::Struct(record) => {
                self.reference(&mut record.name);
                for field in &mut record.fields.fields {
                    self.ty(&mut field.ty)?;
                }
            }
            ast::Item::Enum(enumeration) => {
                self.reference(&mut enumeration.name);
                for variant in &mut enumeration.variants.variants {
                    match &mut variant.payload {
                        None => {}
                        Some(ast::VariantTypePayload::Tuple(tuple)) => {
                            for element in &mut tuple.elems {
                                self.ty(element)?;
                            }
                        }
                        Some(ast::VariantTypePayload::Record(record)) => {
                            for field in &mut record.fields {
                                self.ty(&mut field.ty)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn type_path(&mut self, path: &mut ast::TypePath) -> Result<(), Diagnostics> {
        match path.segments.as_mut_slice() {
            [single] => {
                if !self.in_scope(&single.value)
                    && let Some(qualified) = self.renames.get(&single.value)
                {
                    single.value = qualified.clone();
                }
                Ok(())
            }
            // A `module::…` path spelled in source: modules have no qualified
            // access surface, only imports — so a private item cannot be
            // reached by spelling its qualified name either.
            [head, ..] if self.modules.contains(&head.value) => {
                let full = path
                    .segments
                    .iter()
                    .map(|segment| segment.value.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                Err(unknown_name(path.span, full))
            }
            _ => Ok(()),
        }
    }

    fn ty(&mut self, ty: &mut ast::Type) -> Result<(), Diagnostics> {
        match ty {
            ast::Type::Path(path) => self.type_path(path),
            ast::Type::Generic(generic) => {
                self.type_path(&mut generic.base)?;
                for argument in &mut generic.args {
                    self.ty(argument)?;
                }
                Ok(())
            }
            ast::Type::Array(array) => self.ty(&mut array.elem),
            ast::Type::Tuple(tuple) => {
                for element in &mut tuple.elems {
                    self.ty(element)?;
                }
                Ok(())
            }
            ast::Type::Function(function) => {
                self.ty(&mut function.parameter)?;
                self.ty(&mut function.result)
            }
        }
    }

    fn block(&mut self, block: &mut ast::Block) -> Result<(), Diagnostics> {
        self.scopes.push(BTreeSet::new());
        for stmt in &mut block.stmts {
            match stmt {
                ast::Stmt::Let(binding) => {
                    // Initializer first: `let x = f(x)` sees the outer `x`.
                    self.expr(&mut binding.value)?;
                    if let Some(ty) = &mut binding.ty {
                        self.ty(ty)?;
                    }
                    self.pattern(&mut binding.pattern)?;
                }
                ast::Stmt::Yield(yielded) => self.expr(&mut yielded.value)?,
                ast::Stmt::Expression(expression) => self.expr(&mut expression.value)?,
            }
        }
        if let Some(tail) = &mut block.tail {
            self.expr(tail)?;
        }
        self.scopes.pop();
        Ok(())
    }

    /// Walk a pattern: rewrite its type references, bind its value names.
    fn pattern(&mut self, pattern: &mut ast::Pattern) -> Result<(), Diagnostics> {
        match pattern {
            ast::Pattern::Wildcard(_)
            | ast::Pattern::None(_)
            | ast::Pattern::Str(_)
            | ast::Pattern::Number(_) => Ok(()),
            ast::Pattern::Binding(binding) => {
                let name = binding.binding.clone();
                self.bind(&name);
                Ok(())
            }
            ast::Pattern::Some(some) => self.pattern(&mut some.payload),
            ast::Pattern::Tuple(tuple) => {
                for element in &mut tuple.elems {
                    self.pattern(element)?;
                }
                Ok(())
            }
            ast::Pattern::Variant(variant) => {
                self.reference(&mut variant.path.type_name);
                if let Some(payload) = &mut variant.tuple_payload {
                    for element in &mut payload.elems {
                        self.pattern(element)?;
                    }
                }
                Ok(())
            }
            ast::Pattern::Record(record) => {
                self.type_path(&mut record.ty)?;
                for field in &mut record.fields.fields {
                    match &mut field.pattern {
                        Some(inner) => self.pattern(inner)?,
                        // Shorthand: the field name IS the binding.
                        None => {
                            let name = field.name.clone();
                            self.bind(&name);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Record-expression and where-args fields: the field NAME names the
    /// callee's field, never a scope value — but a shorthand field references
    /// the value by that name, so a renamed shorthand expands to
    /// `name: qualified`.
    fn named_values(&mut self, fields: &mut [ast::NamedValue]) -> Result<(), Diagnostics> {
        for field in fields {
            match &mut field.value {
                Some(value) => self.expr(value)?,
                None => {
                    if !self.in_scope(&field.name.value)
                        && let Some(qualified) = self.renames.get(&field.name.value)
                    {
                        field.value = Some(ast::Expr::Identifier(Spanned {
                            span: field.name.span,
                            value: qualified.clone(),
                        }));
                    }
                }
            }
        }
        Ok(())
    }

    fn expr(&mut self, expr: &mut ast::Expr) -> Result<(), Diagnostics> {
        match expr {
            ast::Expr::Identifier(name) => {
                self.reference(name);
                Ok(())
            }
            ast::Expr::Path(_)
            | ast::Expr::Str(_)
            | ast::Expr::Quantity(_)
            | ast::Expr::Number(_)
            | ast::Expr::Bool(_) => Ok(()),
            ast::Expr::Binary(binary) => {
                self.expr(&mut binary.left)?;
                self.expr(&mut binary.right)
            }
            ast::Expr::Unary(unary) => self.expr(&mut unary.value),
            ast::Expr::Paren(paren) => self.expr(&mut paren.inner),
            ast::Expr::Call(call) => {
                self.reference(&mut call.callee);
                for argument in &mut call.args.args {
                    self.expr(argument)?;
                }
                if let Some(named) = &mut call.named_args {
                    self.named_values(&mut named.fields)?;
                }
                Ok(())
            }
            ast::Expr::WhereCall(call) => {
                self.reference(&mut call.callee);
                self.named_values(&mut call.named_args.fields)
            }
            ast::Expr::MethodCall(call) => {
                // The method name resolves against the receiver, not scope.
                self.expr(&mut call.receiver)?;
                if let Some(args) = &mut call.args {
                    for argument in &mut args.args {
                        self.expr(argument)?;
                    }
                }
                if let Some(named) = &mut call.named_args {
                    self.named_values(&mut named.fields)?;
                }
                Ok(())
            }
            ast::Expr::Field(field) => self.expr(&mut field.receiver),
            ast::Expr::Index(index) => {
                self.expr(&mut index.receiver)?;
                self.expr(&mut index.index)
            }
            ast::Expr::Array(array) => {
                for element in &mut array.elems {
                    self.expr(element)?;
                }
                Ok(())
            }
            ast::Expr::Map(map) => {
                for row in &mut map.rows {
                    self.expr(&mut row.key)?;
                    self.expr(&mut row.value)?;
                }
                Ok(())
            }
            ast::Expr::Set(set) => {
                for element in &mut set.elems {
                    self.expr(element)?;
                }
                Ok(())
            }
            ast::Expr::Tuple(tuple) => {
                for element in &mut tuple.elems {
                    self.expr(element)?;
                }
                Ok(())
            }
            ast::Expr::Variant(variant) => {
                self.reference(&mut variant.path.type_name);
                if let Some(payload) = &mut variant.tuple_payload {
                    for argument in &mut payload.args {
                        self.expr(argument)?;
                    }
                }
                Ok(())
            }
            ast::Expr::Record(record) => {
                self.type_path(&mut record.ty)?;
                if let Some(spread) = &mut record.fields.spread {
                    self.expr(&mut spread.base)?;
                }
                self.named_values(&mut record.fields.fields)
            }
            ast::Expr::If(if_expr) => self.if_expr(if_expr),
            ast::Expr::Match(match_expr) => {
                self.expr(&mut match_expr.scrutinee)?;
                for arm in &mut match_expr.arms.arms {
                    self.scopes.push(BTreeSet::new());
                    self.pattern(&mut arm.pattern)?;
                    if let Some(guard) = &mut arm.guard {
                        self.expr(guard)?;
                    }
                    match &mut arm.body {
                        ast::MatchArmBody::Block(block) => self.block(block)?,
                        ast::MatchArmBody::Expr(expression) => self.expr(expression)?,
                    }
                    self.scopes.pop();
                }
                Ok(())
            }
            ast::Expr::Closure(closure) => {
                self.scopes.push(BTreeSet::new());
                if let Some(ty) = &mut closure.ty {
                    self.ty(ty)?;
                }
                for pattern in &mut closure.patterns {
                    self.pattern(pattern)?;
                }
                match &mut closure.body {
                    ast::ClosureBody::Block(block) => self.block(block)?,
                    ast::ClosureBody::Expr(expression) => self.expr(expression)?,
                }
                self.scopes.pop();
                Ok(())
            }
        }
    }

    fn if_expr(&mut self, if_expr: &mut ast::IfExpr) -> Result<(), Diagnostics> {
        self.expr(&mut if_expr.condition)?;
        self.block(&mut if_expr.consequent)?;
        match &mut if_expr.alternative {
            ast::IfBranch::Block(block) => self.block(block),
            ast::IfBranch::If(nested) => self.if_expr(nested),
        }
    }
}
