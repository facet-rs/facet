//! Module-set compilation front: import resolution, visibility, and the
//! source-level merge that feeds a module set through the single-file
//! compiler front (ratchet rungs 106–110). A file may either bring an item
//! into local scope with `import geometry::Point`, refer to a supplied module
//! through `geometry::Point`, or define that namespace inline with
//! `mod geometry { … }`.
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
    /// The name used by imports and qualified paths (`geometry`).
    pub name: &'a str,
    /// The module's Vix source text.
    pub source: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclaredItemKind {
    Function,
    Other,
}

struct DeclaredItem {
    public: bool,
    kind: DeclaredItemKind,
}

/// The declared (non-import) items of one file: name → visibility.
fn declared_items(file: &ast::SourceFile) -> BTreeMap<String, DeclaredItem> {
    let mut items = BTreeMap::new();
    for item in &file.items {
        let (name, public, kind) = match item {
            ast::Item::Fn(function) => (
                &function.name.value,
                function.vis.is_some(),
                DeclaredItemKind::Function,
            ),
            ast::Item::Struct(record) => (
                &record.name.value,
                record.vis.is_some(),
                DeclaredItemKind::Other,
            ),
            ast::Item::Enum(enumeration) => (
                &enumeration.name.value,
                enumeration.vis.is_some(),
                DeclaredItemKind::Other,
            ),
            ast::Item::Command(command) => (
                &command.name.value,
                command.vis.is_some(),
                DeclaredItemKind::Other,
            ),
            ast::Item::Mod(_) | ast::Item::Import(_) => continue,
        };
        items
            .entry(name.clone())
            .or_insert(DeclaredItem { public, kind });
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

fn first_inline_module(items: &[ast::Item]) -> Option<&ast::ModItem> {
    items.iter().find_map(|item| match item {
        ast::Item::Mod(module) => Some(module.as_ref()),
        ast::Item::Fn(_)
        | ast::Item::Struct(_)
        | ast::Item::Enum(_)
        | ast::Item::Import(_)
        | ast::Item::Command(_) => None,
    })
}

fn reject_nested_module(items: &[ast::Item]) -> Result<(), Diagnostics> {
    let Some(nested) = first_inline_module(items) else {
        return Ok(());
    };
    Err(Diagnostics::one(Diagnostic::unsupported(
        nested.span,
        "nested inline modules are not supported",
    )))
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
/// interleaved with the file's own declarations so a name bound twice is
/// diagnosed at its second binding site (rung 109 anchors at the local
/// declaration that follows the import).
fn resolve_imports(
    own_module: Option<&str>,
    file: &ast::SourceFile,
    set: &BTreeMap<String, BTreeMap<String, DeclaredItem>>,
) -> Result<BTreeMap<String, String>, Diagnostics> {
    let mut bound: BTreeSet<String> = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    for item in &file.items {
        match item {
            ast::Item::Mod(_) => unreachable!("inline modules are extracted before resolution"),
            ast::Item::Import(import) => {
                let module = &import.module;
                let Some(exports) = set.get(&module.value) else {
                    return Err(unknown_name(module.span, module.value.clone()));
                };
                for leaf in import_leaves(import) {
                    let declared = exports.get(&leaf.value);
                    if declared.is_none()
                        && !crate::binding::is_qualified_binding(&module.value, &leaf.value)
                    {
                        return Err(unknown_name(
                            leaf.span,
                            format!("{}::{}", module.value, leaf.value),
                        ));
                    }
                    // r[impl lang.module.use] — public items use `pub`; a
                    // non-pub item is not importable from another module.
                    if own_module != Some(module.value.as_str())
                        && declared.is_some_and(|item| !item.public)
                    {
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
            ast::Item::Command(command) => {
                if !bound.insert(command.name.value.clone()) {
                    return Err(duplicate_name(command.name.span, &command.name.value));
                }
            }
        }
    }
    Ok(aliases)
}

/// Remove root-inline module items and return them as named files for the
/// ordinary module-set merger. Nested modules are not part of the current
/// one-segment module surface, so reject them instead of silently hoisting
/// them into the root namespace.
fn extract_inline_modules(
    mut file: ast::SourceFile,
    module_names: &mut BTreeSet<String>,
) -> Result<(ast::SourceFile, Vec<(String, ast::SourceFile)>), Diagnostics> {
    let mut retained = Vec::new();
    let mut extracted = Vec::new();
    for item in std::mem::take(&mut file.items) {
        let ast::Item::Mod(module) = item else {
            retained.push(item);
            continue;
        };
        if !module_names.insert(module.name.value.clone()) {
            return Err(duplicate_name(module.name.span, &module.name.value));
        }
        reject_nested_module(&module.items)?;
        let name = module.name.value;
        let nested_file = ast::SourceFile {
            span: module.span,
            items: module.items,
        };
        extracted.push((name, nested_file));
    }
    file.items = retained;
    Ok((file, extracted))
}

fn rewrite_module_items(
    name: &str,
    file: &ast::SourceFile,
    set: &BTreeMap<String, BTreeMap<String, DeclaredItem>>,
    module_names: &BTreeSet<String>,
) -> Result<Vec<ast::Item>, Diagnostics> {
    let aliases = resolve_imports(Some(name), file, set)?;
    let mut renames: BTreeMap<String, String> = declared_items(file)
        .into_keys()
        .map(|item| (item.clone(), format!("{name}::{item}")))
        .collect();
    renames.extend(aliases);
    let mut rewriter = Rewriter {
        renames: &renames,
        available_modules: module_names,
        module_set: set,
        own_module: Some(name),
        scopes: Vec::new(),
    };
    let mut rewritten = Vec::new();
    for item in &file.items {
        if matches!(item, ast::Item::Mod(_) | ast::Item::Import(_)) {
            continue;
        }
        let mut item = item.clone();
        rewriter.item(&mut item)?;
        rewritten.push(item);
    }
    Ok(rewritten)
}

/// Merge a parsed module set into one surface AST for the single-file front.
///
/// Library items are renamed to `module::item`; references in every file are
/// rewritten to the declaring module's spelling; import items are consumed.
/// Item order is deterministic: supplied library modules in presentation
/// order, root items next, and root-inline modules last. Keeping root-inline
/// modules after the root preserves existing root `FunctionId`s when an
/// embedder injects a module such as `std`.
pub(crate) fn merge_module_set(
    root: ast::SourceFile,
    modules: &[(String, ast::SourceFile)],
) -> Result<ast::SourceFile, Diagnostics> {
    let mut module_names = BTreeSet::new();
    for (name, file) in modules {
        if !module_names.insert(name.clone()) {
            return Err(duplicate_name(file.span, name));
        }
    }

    let mut expanded_modules = Vec::new();
    for (name, file) in modules {
        reject_nested_module(&file.items)?;
        expanded_modules.push((name.clone(), file.clone()));
    }
    let (root, root_inline) = extract_inline_modules(root, &mut module_names)?;
    let root_inline_start = expanded_modules.len();
    expanded_modules.extend(root_inline);
    let modules = &expanded_modules;

    let set: BTreeMap<String, BTreeMap<String, DeclaredItem>> = modules
        .iter()
        .map(|(name, file)| (name.clone(), declared_items(file)))
        .collect();
    let module_names: BTreeSet<String> = set.keys().cloned().collect();

    let mut merged_items = Vec::new();
    for (name, file) in &modules[..root_inline_start] {
        // A library module's own items are spelled fully qualified; its
        // imports alias to their declaring modules.
        merged_items.extend(rewrite_module_items(name, file, &set, &module_names)?);
    }

    let aliases = resolve_imports(None, &root, &set)?;
    let mut rewriter = Rewriter {
        renames: &aliases,
        available_modules: &module_names,
        module_set: &set,
        own_module: None,
        scopes: Vec::new(),
    };
    let span = root.span;
    for item in root.items {
        if matches!(item, ast::Item::Mod(_) | ast::Item::Import(_)) {
            continue;
        }
        let mut item = item;
        rewriter.item(&mut item)?;
        merged_items.push(item);
    }

    for (name, file) in &modules[root_inline_start..] {
        merged_items.extend(rewrite_module_items(name, file, &set, &module_names)?);
    }

    Ok(ast::SourceFile {
        span,
        items: merged_items,
    })
}

/// Scope-aware reference rewriter: rewrites every free occurrence of a name
/// in `renames` to its fully-qualified spelling, leaving lexically bound
/// occurrences (params, lets, closure params, pattern bindings) untouched.
/// Qualified `module::…` spellings resolve against supplied and inline
/// modules. Private items remain inaccessible through both qualified paths
/// and imports.
struct Rewriter<'a> {
    renames: &'a BTreeMap<String, String>,
    available_modules: &'a BTreeSet<String>,
    module_set: &'a BTreeMap<String, BTreeMap<String, DeclaredItem>>,
    own_module: Option<&'a str>,
    scopes: Vec<BTreeSet<String>>,
}

impl Rewriter<'_> {
    fn qualified_name(
        &self,
        module: &Spanned<String>,
        item: &Spanned<String>,
    ) -> Result<String, Diagnostics> {
        let full = format!("{}::{}", module.value, item.value);
        let declared = self
            .module_set
            .get(&module.value)
            .and_then(|items| items.get(&item.value));
        if declared.is_none() && crate::binding::is_qualified_binding(&module.value, &item.value) {
            return Ok(full);
        }
        let Some(declared) = declared else {
            return Err(unknown_name(item.span, full));
        };
        if !declared.public && self.own_module != Some(module.value.as_str()) {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::PrivateImport,
                primary: item.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name { name: full },
            }));
        }
        Ok(full)
    }

    fn qualified_function_name(
        &self,
        module: &Spanned<String>,
        item: &Spanned<String>,
    ) -> Result<Option<String>, Diagnostics> {
        let is_function = self
            .module_set
            .get(&module.value)
            .and_then(|items| items.get(&item.value))
            .is_some_and(|declared| declared.kind == DeclaredItemKind::Function);
        if !is_function {
            return Ok(None);
        }
        self.qualified_name(module, item).map(Some)
    }

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
            ast::Item::Mod(_) | ast::Item::Import(_) => {
                unreachable!("module declarations and imports are consumed before rewriting")
            }
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
            ast::Item::Command(command) => {
                self.reference(&mut command.name);
                if let Some(return_type) = &mut command.return_type {
                    self.ty(return_type)?;
                }
                self.command_pattern(&mut command.grammar.pattern)?;
            }
        }
        Ok(())
    }

    fn command_pattern(&mut self, pattern: &mut ast::CommandPattern) -> Result<(), Diagnostics> {
        for alternative in &mut pattern.alternatives {
            for term in &mut alternative.terms {
                match &mut term.atom {
                    ast::CommandAtom::Literal(_) => {}
                    ast::CommandAtom::Slot(slot) => self.ty(&mut slot.ty)?,
                    ast::CommandAtom::Optional(optional) => {
                        self.command_pattern(&mut optional.pattern)?;
                    }
                    ast::CommandAtom::Group(group) => {
                        self.command_pattern(&mut group.pattern)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn type_path(&mut self, path: &mut ast::TypePath) -> Result<(), Diagnostics> {
        if path.segments.len() >= 2 && self.available_modules.contains(&path.segments[0].value) {
            if path.segments.len() != 2 {
                let full = path
                    .segments
                    .iter()
                    .map(|segment| segment.value.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                return Err(unknown_name(path.span, full));
            }
            let qualified = self.qualified_name(&path.segments[0], &path.segments[1])?;
            path.segments = vec![Spanned {
                span: path.span,
                value: qualified,
            }];
            return Ok(());
        }

        match path.segments.as_mut_slice() {
            [single] => {
                if !self.in_scope(&single.value)
                    && let Some(qualified) = self.renames.get(&single.value)
                {
                    single.value = qualified.clone();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn variant_path(&mut self, path: &mut ast::VariantPath) -> Result<(), Diagnostics> {
        if let Some(module) = path.module.take() {
            path.type_name.value = self.qualified_name(&module, &path.type_name)?;
        } else {
            self.reference(&mut path.type_name);
        }
        Ok(())
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
            ast::Pattern::Ok(ok) => self.pattern(&mut ok.payload),
            ast::Pattern::Err(err) => self.pattern(&mut err.payload),
            ast::Pattern::Tuple(tuple) => {
                for element in &mut tuple.elems {
                    self.pattern(element)?;
                }
                Ok(())
            }
            ast::Pattern::Variant(variant) => {
                self.variant_path(&mut variant.path)?;
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
        // Qualified function calls/references and enum variants share the
        // existing `Variant` syntax shape. A matching supplied or inline
        // module lets resolution turn the former into an ordinary call or
        // identifier while leaving actual enum variants intact.
        let qualified = match expr {
            ast::Expr::Variant(variant)
                if self
                    .available_modules
                    .contains(&variant.path.type_name.value) =>
            {
                Some((
                    variant.span,
                    variant.path.type_name.clone(),
                    variant.path.variant.clone(),
                    variant.tuple_payload.clone(),
                    variant.named_args.clone(),
                ))
            }
            _ => None,
        };
        if let Some((span, module, item, args, named_args)) = qualified {
            if let Some(mut args) = args {
                let callee = self.qualified_name(&module, &item)?;
                for argument in &mut args.args {
                    self.expr(argument)?;
                }
                let mut named_args = named_args;
                if let Some(named) = &mut named_args {
                    self.named_values(&mut named.fields)?;
                }
                *expr = ast::Expr::Call(Box::new(ast::Call {
                    span,
                    callee: Spanned {
                        span: item.span,
                        value: callee,
                    },
                    type_args: None,
                    args,
                    named_args,
                }));
                return Ok(());
            }
            if let Some(function) = self.qualified_function_name(&module, &item)? {
                *expr = ast::Expr::Identifier(Spanned {
                    span,
                    value: function,
                });
                return Ok(());
            }
        }

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
            ast::Expr::Command(command) => {
                self.reference(&mut command.tag);
                Ok(())
            }
            ast::Expr::Exec(exec) => {
                self.reference(&mut exec.command.tag);
                Ok(())
            }
            ast::Expr::Try(try_expr) => self.expr(&mut try_expr.value),
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
                self.variant_path(&mut variant.path)?;
                if let Some(payload) = &mut variant.tuple_payload {
                    for argument in &mut payload.args {
                        self.expr(argument)?;
                    }
                }
                if let Some(named) = &mut variant.named_args {
                    self.named_values(&mut named.fields)?;
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
