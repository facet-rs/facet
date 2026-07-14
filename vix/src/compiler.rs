//! Surface AST checking and lowering to Vix IR.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::decode::{self, DecodeFormat, DecodedValue};
use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics, Label};
use crate::support::{Span, Spanned};
use crate::surface::{SurfaceParser, ast};
use crate::vir::DescribedWire;
use crate::vir::{
    ArrayMapGrain, ArrayMapGrainKey, Budget, CheckRecipe, ControlRegion, EffectFacts, EffectKind,
    EnumType, EnumVariant, ExternKind, Function, FunctionId, GeneratorArm, GeneratorBody,
    GeneratorStep, MatchArm as VirMatchArm, Module, Node, NodeId, OPTION_NONE_VARIANT,
    OPTION_SOME_VARIANT, ORDERING_GREATER_VARIANT, ORDERING_LESS_VARIANT, Op, OrderedMatchArm,
    Parameter, ParameterId, ParameterKind, RESULT_ERR_VARIANT, RESULT_OK_VARIANT, RecordField,
    RecordType, Test, TestMetadata, TraceCheck, Type, VariantPayload, WireArg, WireSelector,
    YieldSite, YieldSiteId,
};

pub struct Compiler {
    parser: SurfaceParser,
    config: CompilerConfig,
}

/// Compile-time knobs that select between observationally identical execution
/// shapes. The molten one-item-append fold is admitted under the as-if law; the
/// forced-copy differential compiles the same source with the molten shape
/// disabled so the two value sets can be proven identical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompilerConfig {
    /// When set, every `Array.fold` keeps the semantic copy path even where the
    /// strict one-item-append shape would otherwise be admitted molten.
    pub force_molten_copy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compilation {
    pub module: Module,
    pub warnings: Diagnostics,
}

impl core::ops::Deref for Compilation {
    type Target = Module;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}

impl Compiler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: SurfaceParser::new(),
            config: CompilerConfig::default(),
        }
    }

    /// A compiler that carries explicit shape-selection configuration.
    #[must_use]
    pub fn with_config(config: CompilerConfig) -> Self {
        Self {
            parser: SurfaceParser::new(),
            config,
        }
    }

    /// Parse, check, and lower to architecture-neutral VIR.
    pub fn compile(&self, source: &str) -> Result<Compilation, Diagnostics> {
        self.compile_with_modules(source, &[])
    }

    /// Parse a root source together with named library modules, resolve
    /// imports and visibility, and lower the merged module set to VIR.
    ///
    /// Import-resolution diagnostics carry spans in the importing file's own
    /// coordinates. Post-merge diagnostics in library code keep their own
    /// file's byte spans, which may collide with root offsets — attribution
    /// only; recipe identity is span-insensitive.
    pub fn compile_with_modules(
        &self,
        source: &str,
        modules: &[crate::modules::ModuleSource<'_>],
    ) -> Result<Compilation, Diagnostics> {
        let root = self.parser.parse(source)?;
        let mut parsed = Vec::with_capacity(modules.len());
        for module in modules {
            parsed.push((module.name.to_owned(), self.parser.parse(module.source)?));
        }
        let merged = crate::modules::merge_module_set(root, &parsed)?;
        let module = lower_module(&merged, self.config)?;
        let warnings = lint_module(&module);
        Ok(Compilation { module, warnings })
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

// r[impl lang.diagnostic.must-use]
fn lint_module(module: &Module) -> Diagnostics {
    let mut entries = Vec::new();
    for function in &module.functions {
        let mut consumed = BTreeSet::new();
        for node in &function.nodes {
            consumed.extend(node.inputs.iter().copied());
        }
        consumed.extend(function.output);
        consumed.extend(function.yielded_checks.iter().copied());
        for node in &function.nodes {
            let operation = match node.op {
                Op::ArrayAppend => "+",
                Op::ArrayConcat => "++",
                Op::MapAdd | Op::SetAdd => "+",
                Op::MapConcat | Op::SetConcat => "++",
                Op::MapWith => "with",
                _ if node.ty == Type::Check => "Check",
                _ => continue,
            };
            if consumed.contains(&node.id) {
                continue;
            }
            entries.push(Diagnostic {
                code: DiagnosticCode::UnusedMustUse,
                primary: node.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::UnusedResult {
                    operation: operation.to_owned(),
                },
            });
        }
    }
    Diagnostics { entries }
}

#[derive(Clone)]
struct FunctionSignature {
    id: FunctionId,
    is_test: bool,
    parameters: Vec<ParameterSignature>,
    return_type: Type,
    /// Typed `#[test]` metadata, default for non-test functions.
    metadata: TestMetadata,
}

#[derive(Clone)]
struct ParameterSignature {
    id: ParameterId,
    name: String,
    span: Span,
    ty: Type,
    kind: ParameterKind,
}

struct ModuleContext<'a> {
    signatures: &'a BTreeMap<String, FunctionSignature>,
    types: &'a BTreeMap<String, Type>,
    closures: RefCell<ClosureState>,
    config: CompilerConfig,
}

struct ClosureState {
    next_function: u32,
    functions: BTreeMap<FunctionId, Function>,
    scopes: Vec<ClosureScope>,
}

struct ClosureScope {
    path: String,
    next_closure: u32,
}

impl ModuleContext<'_> {
    fn enter_function(&self, path: String) {
        self.closures.borrow_mut().scopes.push(ClosureScope {
            path,
            next_closure: 0,
        });
    }

    fn leave_function(&self) {
        self.closures
            .borrow_mut()
            .scopes
            .pop()
            .expect("function lowering has an active closure scope");
    }

    fn allocate_closure(&self) -> (FunctionId, String) {
        let mut state = self.closures.borrow_mut();
        let scope = state
            .scopes
            .last_mut()
            .expect("closure lowering occurs inside a function");
        let ordinal = scope.next_closure;
        scope.next_closure = scope
            .next_closure
            .checked_add(1)
            .expect("closure count fits u32");
        let name = format!("{}::closure#{ordinal}", scope.path);
        let id = FunctionId(state.next_function);
        state.next_function = state
            .next_function
            .checked_add(1)
            .expect("function count fits u32");
        (id, name)
    }

    fn insert_closure(&self, function: Function) {
        assert!(
            self.closures
                .borrow_mut()
                .functions
                .insert(function.id, function)
                .is_none(),
            "closure function ids are unique"
        );
    }
}

#[derive(Clone, Copy)]
enum TypeDeclaration<'a> {
    Record(&'a ast::StructItem),
    Enum(&'a ast::EnumItem),
}

impl<'a> TypeDeclaration<'a> {
    fn span(self) -> Span {
        match self {
            Self::Record(record) => record.span,
            Self::Enum(enumeration) => enumeration.span,
        }
    }

    fn generic_params(self) -> Option<&'a ast::GenericParams> {
        match self {
            Self::Record(_) => None,
            Self::Enum(enumeration) => enumeration.generics.as_ref(),
        }
    }
}

struct TypeResolver<'a> {
    declarations: BTreeMap<String, TypeDeclaration<'a>>,
    resolving: BTreeSet<String>,
    resolved: BTreeMap<String, Type>,
}

impl<'a> TypeResolver<'a> {
    fn new(source: &'a ast::SourceFile) -> Result<Self, Diagnostics> {
        let mut declarations = BTreeMap::new();
        for item in &source.items {
            let (name, span, declaration) = match item {
                ast::Item::Struct(record) => (
                    &record.name.value,
                    record.name.span,
                    TypeDeclaration::Record(record),
                ),
                ast::Item::Enum(enumeration) => (
                    &enumeration.name.value,
                    enumeration.name.span,
                    TypeDeclaration::Enum(enumeration),
                ),
                // Imports are consumed by the module front before lowering;
                // a stray one contributes no type declaration.
                ast::Item::Fn(_) | ast::Item::Import(_) => continue,
            };
            if name == "Ordering" {
                return Err(Diagnostics::one(Diagnostic {
                    code: DiagnosticCode::DuplicateDefinition,
                    primary: span,
                    labels: Vec::new(),
                    payload: DiagnosticPayload::Name { name: name.clone() },
                }));
            }
            if declarations.insert(name.clone(), declaration).is_some() {
                return Err(Diagnostics::one(Diagnostic {
                    code: DiagnosticCode::DuplicateDefinition,
                    primary: span,
                    labels: Vec::new(),
                    payload: DiagnosticPayload::Name { name: name.clone() },
                }));
            }
        }
        Ok(Self {
            declarations,
            resolving: BTreeSet::new(),
            resolved: BTreeMap::from([("Ordering".to_owned(), Type::ordering())]),
        })
    }

    fn resolve_all(
        mut self,
        source: &'a ast::SourceFile,
    ) -> Result<BTreeMap<String, Type>, Diagnostics> {
        let names = self.declarations.keys().cloned().collect::<Vec<_>>();
        for name in names {
            if self
                .declarations
                .get(&name)
                .is_some_and(|declaration| declaration.generic_params().is_none())
            {
                self.resolve_nominal(&name)?;
            }
        }
        for item in &source.items {
            let ast::Item::Fn(function) = item else {
                continue;
            };
            if function.generics.is_some() {
                continue;
            }
            self.resolve_function_types(function)?;
        }
        Ok(self.resolved)
    }

    fn resolve_function_types(&mut self, function: &ast::FnItem) -> Result<(), Diagnostics> {
        for parameter in &function.params.params {
            self.resolve_type(&parameter.ty)?;
        }
        if let Some(where_params) = &function.where_params
            && let Some(inline) = &where_params.inline
        {
            for parameter in &inline.params {
                self.resolve_type(&parameter.ty)?;
                if let Some(default) = &parameter.default {
                    self.resolve_expr_types(default)?;
                }
            }
        }
        if let Some(return_type) = &function.return_type {
            self.resolve_type(return_type)?;
        }
        self.resolve_block_types(&function.body)
    }

    fn resolve_block_types(&mut self, block: &ast::Block) -> Result<(), Diagnostics> {
        for statement in &block.stmts {
            match statement {
                ast::Stmt::Let(statement) => {
                    if let Some(ty) = &statement.ty {
                        self.resolve_type(ty)?;
                    }
                    self.resolve_expr_types(&statement.value)?;
                }
                ast::Stmt::Yield(statement) => self.resolve_expr_types(&statement.value)?,
                ast::Stmt::Expression(statement) => {
                    self.resolve_expr_types(&statement.value)?;
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.resolve_expr_types(tail)?;
        }
        Ok(())
    }

    fn resolve_expr_types(&mut self, expression: &ast::Expr) -> Result<(), Diagnostics> {
        match expression {
            ast::Expr::Closure(closure) => {
                if let Some(ty) = &closure.ty {
                    self.resolve_type(ty)?;
                }
                match &closure.body {
                    ast::ClosureBody::Block(block) => self.resolve_block_types(block)?,
                    ast::ClosureBody::Expr(expression) => self.resolve_expr_types(expression)?,
                }
            }
            ast::Expr::If(expression) => self.resolve_if_types(expression)?,
            ast::Expr::Match(expression) => {
                self.resolve_expr_types(&expression.scrutinee)?;
                for arm in &expression.arms.arms {
                    if let Some(guard) = &arm.guard {
                        self.resolve_expr_types(guard)?;
                    }
                    match &arm.body {
                        ast::MatchArmBody::Block(block) => self.resolve_block_types(block)?,
                        ast::MatchArmBody::Expr(expression) => {
                            self.resolve_expr_types(expression)?;
                        }
                    }
                }
            }
            ast::Expr::Binary(expression) => {
                self.resolve_expr_types(&expression.left)?;
                self.resolve_expr_types(&expression.right)?;
            }
            ast::Expr::Unary(expression) => self.resolve_expr_types(&expression.value)?,
            ast::Expr::Call(expression) => {
                for argument in &expression.args.args {
                    self.resolve_expr_types(argument)?;
                }
                if let Some(named) = &expression.named_args {
                    self.resolve_named_value_types(&named.fields)?;
                }
            }
            ast::Expr::MethodCall(expression) => {
                self.resolve_expr_types(&expression.receiver)?;
                for argument in method_positional_args(expression) {
                    self.resolve_expr_types(argument)?;
                }
                if let Some(named) = &expression.named_args {
                    self.resolve_named_value_types(&named.fields)?;
                }
            }
            ast::Expr::WhereCall(expression) => {
                self.resolve_named_value_types(&expression.named_args.fields)?;
            }
            ast::Expr::Index(expression) => {
                self.resolve_expr_types(&expression.receiver)?;
                self.resolve_expr_types(&expression.index)?;
            }
            ast::Expr::Array(expression) => {
                for element in &expression.elems {
                    self.resolve_expr_types(element)?;
                }
            }
            ast::Expr::Map(expression) => {
                for row in &expression.rows {
                    self.resolve_expr_types(&row.key)?;
                    self.resolve_expr_types(&row.value)?;
                }
            }
            ast::Expr::Set(expression) => {
                for element in &expression.elems {
                    self.resolve_expr_types(element)?;
                }
            }
            ast::Expr::Field(expression) => self.resolve_expr_types(&expression.receiver)?,
            ast::Expr::Variant(expression) => {
                if let Some(payload) = &expression.tuple_payload {
                    for argument in &payload.args {
                        self.resolve_expr_types(argument)?;
                    }
                }
            }
            ast::Expr::Record(expression) => {
                if let Some(spread) = &expression.fields.spread {
                    self.resolve_expr_types(&spread.base)?;
                }
                self.resolve_named_value_types(&expression.fields.fields)?;
            }
            ast::Expr::Tuple(expression) => {
                for element in &expression.elems {
                    self.resolve_expr_types(element)?;
                }
            }
            ast::Expr::Paren(expression) => self.resolve_expr_types(&expression.inner)?,
            ast::Expr::Try(expression) => self.resolve_expr_types(&expression.value)?,
            // A command's tag is a value reference and its template a scalar
            // token; neither mentions a type.
            ast::Expr::Exec(_)
            | ast::Expr::Command(_)
            | ast::Expr::Identifier(_)
            | ast::Expr::Path(_)
            | ast::Expr::Str(_)
            | ast::Expr::Number(_)
            | ast::Expr::Quantity(_)
            | ast::Expr::Bool(_) => {}
        }
        Ok(())
    }

    fn resolve_if_types(&mut self, expression: &ast::IfExpr) -> Result<(), Diagnostics> {
        self.resolve_expr_types(&expression.condition)?;
        self.resolve_block_types(&expression.consequent)?;
        match &expression.alternative {
            ast::IfBranch::Block(block) => self.resolve_block_types(block),
            ast::IfBranch::If(expression) => self.resolve_if_types(expression),
        }
    }

    fn resolve_named_value_types(&mut self, fields: &[ast::NamedValue]) -> Result<(), Diagnostics> {
        for field in fields {
            if let Some(value) = &field.value {
                self.resolve_expr_types(value)?;
            }
        }
        Ok(())
    }

    fn resolve_nominal(&mut self, name: &str) -> Result<Type, Diagnostics> {
        if let Some(ty) = self.resolved.get(name) {
            return Ok(ty.clone());
        }
        let declaration = *self
            .declarations
            .get(name)
            .ok_or_else(|| unknown_name(Span { start: 0, end: 0 }, name))?;
        if let Some(generics) = declaration.generic_params() {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                generics.span,
                format!("generic type `{name}` requires arguments"),
            )));
        }
        if !self.resolving.insert(name.to_owned()) {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                declaration.span(),
                format!("recursive inline nominal type `{name}`"),
            )));
        }

        let ty = match declaration {
            TypeDeclaration::Record(record) => Type::Record(RecordType {
                name: name.to_owned(),
                fields: self.resolve_record_fields(name, &record.fields.fields)?,
            }),
            TypeDeclaration::Enum(enumeration) => {
                let mut variant_names = BTreeSet::new();
                let mut variants = Vec::with_capacity(enumeration.variants.variants.len());
                for variant in &enumeration.variants.variants {
                    if !variant_names.insert(variant.name.value.clone()) {
                        return Err(variant_diagnostic(
                            DiagnosticCode::DuplicateVariant,
                            variant.name.span,
                            name,
                            &variant.name.value,
                        ));
                    }
                    let payload = match &variant.payload {
                        None => VariantPayload::Unit,
                        Some(ast::VariantTypePayload::Tuple(tuple)) => VariantPayload::Tuple(
                            tuple
                                .elems
                                .iter()
                                .map(|element| self.resolve_type(element))
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                        Some(ast::VariantTypePayload::Record(record)) => {
                            VariantPayload::Record(self.resolve_record_fields(
                                &format!("{name}::{}", variant.name.value),
                                &record.fields,
                            )?)
                        }
                    };
                    variants.push(EnumVariant {
                        name: variant.name.value.clone(),
                        payload,
                    });
                }
                Type::Enum(EnumType {
                    name: name.to_owned(),
                    variants,
                })
            }
        };
        self.resolving.remove(name);
        self.resolved.insert(name.to_owned(), ty.clone());
        Ok(ty)
    }

    // r[impl lang.types.generic-enum-monomorphized]
    fn resolve_generic_nominal(
        &mut self,
        base: &str,
        arguments: Vec<Type>,
        span: Span,
    ) -> Result<Type, Diagnostics> {
        let declaration = *self
            .declarations
            .get(base)
            .ok_or_else(|| unknown_name(span, base))?;
        let generics = declaration
            .generic_params()
            .ok_or_else(|| invalid_arity(span, 0, arguments.len()))?;
        if generics.params.len() != arguments.len() {
            return Err(invalid_arity(
                generics.span,
                generics.params.len(),
                arguments.len(),
            ));
        }
        let name = applied_type_name(base, &arguments);
        if let Some(ty) = self.resolved.get(&name) {
            return Ok(ty.clone());
        }
        if !self.resolving.insert(name.clone()) {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                span,
                format!("recursive inline nominal type `{name}`"),
            )));
        }

        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in generics.params.iter().zip(arguments) {
            if substitutions
                .insert(parameter.value.clone(), argument)
                .is_some()
            {
                return Err(Diagnostics::one(Diagnostic {
                    code: DiagnosticCode::DuplicateBinding,
                    primary: parameter.span,
                    labels: Vec::new(),
                    payload: DiagnosticPayload::Name {
                        name: parameter.value.clone(),
                    },
                }));
            }
        }

        let TypeDeclaration::Enum(enumeration) = declaration else {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                span,
                "generic record declaration",
            )));
        };
        let mut variant_names = BTreeSet::new();
        let mut variants = Vec::with_capacity(enumeration.variants.variants.len());
        for variant in &enumeration.variants.variants {
            if !variant_names.insert(variant.name.value.clone()) {
                return Err(variant_diagnostic(
                    DiagnosticCode::DuplicateVariant,
                    variant.name.span,
                    &name,
                    &variant.name.value,
                ));
            }
            let payload = match &variant.payload {
                None => VariantPayload::Unit,
                Some(ast::VariantTypePayload::Tuple(tuple)) => VariantPayload::Tuple(
                    tuple
                        .elems
                        .iter()
                        .map(|element| self.resolve_type_with(element, &substitutions))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Some(ast::VariantTypePayload::Record(record)) => {
                    VariantPayload::Record(self.resolve_record_fields_with(
                        &format!("{name}::{}", variant.name.value),
                        &record.fields,
                        &substitutions,
                    )?)
                }
            };
            variants.push(EnumVariant {
                name: variant.name.value.clone(),
                payload,
            });
        }
        let ty = Type::Enum(EnumType {
            name: name.clone(),
            variants,
        });
        self.resolving.remove(&name);
        self.resolved.insert(name, ty.clone());
        Ok(ty)
    }

    fn resolve_record_fields(
        &mut self,
        owner: &str,
        declared_fields: &[ast::RecordField],
    ) -> Result<Vec<RecordField>, Diagnostics> {
        self.resolve_record_fields_with(owner, declared_fields, &BTreeMap::new())
    }

    fn resolve_record_fields_with(
        &mut self,
        owner: &str,
        declared_fields: &[ast::RecordField],
        substitutions: &BTreeMap<String, Type>,
    ) -> Result<Vec<RecordField>, Diagnostics> {
        let mut field_names = BTreeSet::new();
        let mut fields = Vec::with_capacity(declared_fields.len());
        for field in declared_fields {
            if !field_names.insert(field.name.value.clone()) {
                return Err(field_diagnostic(
                    DiagnosticCode::DuplicateField,
                    field.name.span,
                    owner,
                    &field.name.value,
                ));
            }
            fields.push(RecordField {
                name: field.name.value.clone(),
                ty: self.resolve_type_with(&field.ty, substitutions)?,
            });
        }
        Ok(fields)
    }

    fn resolve_type(&mut self, ty: &ast::Type) -> Result<Type, Diagnostics> {
        self.resolve_type_with(ty, &BTreeMap::new())
    }

    fn resolve_type_with(
        &mut self,
        ty: &ast::Type,
        substitutions: &BTreeMap<String, Type>,
    ) -> Result<Type, Diagnostics> {
        match ty {
            ast::Type::Path(path) if path.segments.len() == 1 => {
                let name = &path.segments[0].value;
                if let Some(ty) = substitutions.get(name) {
                    return Ok(ty.clone());
                }
                self.resolve_non_parameter_type(ty, substitutions)
            }
            _ => self.resolve_non_parameter_type(ty, substitutions),
        }
    }

    fn resolve_non_parameter_type(
        &mut self,
        ty: &ast::Type,
        substitutions: &BTreeMap<String, Type>,
    ) -> Result<Type, Diagnostics> {
        match ty {
            ast::Type::Path(path) if path_is(path, "Bool") => Ok(Type::Bool),
            ast::Type::Path(path) if path_is(path, "Int") => Ok(Type::Int),
            ast::Type::Path(path) if path_is(path, "String") => Ok(Type::String),
            ast::Type::Path(path) if path_is(path, "Path") => Ok(Type::Path),
            ast::Type::Path(path) if path_is(path, "Check") => Ok(Type::Check),
            ast::Type::Path(path)
                if CAPABILITY_TYPE_NAMES.iter().any(|name| path_is(path, name)) =>
            {
                Ok(capability_type(&path_name(path)))
            }
            ast::Type::Generic(_) if is_stream_check_type(ty) => Ok(Type::StreamCheck),
            ast::Type::Generic(generic) if path_is(&generic.base, "Option") => {
                if generic.args.len() != 1 {
                    return Err(invalid_arity(generic.span, 1, generic.args.len()));
                }
                Ok(Type::option(
                    self.resolve_type_with(&generic.args[0], substitutions)?,
                ))
            }
            ast::Type::Generic(generic) if path_is(&generic.base, "Map") => {
                if generic.args.len() != 2 {
                    return Err(invalid_arity(generic.span, 2, generic.args.len()));
                }
                let key = self.resolve_type_with(&generic.args[0], substitutions)?;
                if !key.structural_order_is_defined() {
                    return Err(type_mismatch(
                        type_span(&generic.args[0]),
                        "structurally ordered map key",
                        key.name(),
                    ));
                }
                Ok(Type::map(
                    key,
                    self.resolve_type_with(&generic.args[1], substitutions)?,
                ))
            }
            ast::Type::Generic(generic) if path_is(&generic.base, "Set") => {
                if generic.args.len() != 1 {
                    return Err(invalid_arity(generic.span, 1, generic.args.len()));
                }
                let element = self.resolve_type_with(&generic.args[0], substitutions)?;
                if !element.structural_order_is_defined() {
                    return Err(type_mismatch(
                        type_span(&generic.args[0]),
                        "structurally ordered set element",
                        element.name(),
                    ));
                }
                Ok(Type::set(element))
            }
            ast::Type::Generic(generic) if path_is(&generic.base, "Result") => {
                if generic.args.len() != 2 {
                    return Err(invalid_arity(generic.span, 2, generic.args.len()));
                }
                let ok = self.resolve_type_with(&generic.args[0], substitutions)?;
                let err = self.resolve_type_with(&generic.args[1], substitutions)?;
                Ok(Type::result(ok, err))
            }
            ast::Type::Path(path) if path_is(path, "DecodeError") => Ok(decode_error_type()),
            ast::Type::Path(path) => {
                let name = path_name(path);
                if let Some(ty) = self.resolved.get(&name) {
                    return Ok(ty.clone());
                }
                if !self.declarations.contains_key(&name) {
                    return Err(unknown_name(path.span, name));
                }
                self.resolve_nominal(&name)
            }
            ast::Type::Generic(generic) => {
                let arguments = generic
                    .args
                    .iter()
                    .map(|argument| self.resolve_type_with(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?;
                self.resolve_generic_nominal(&path_name(&generic.base), arguments, generic.span)
            }
            ast::Type::Function(function) => Ok(Type::Function {
                parameter: Box::new(self.resolve_type_with(&function.parameter, substitutions)?),
                result: Box::new(self.resolve_type_with(&function.result, substitutions)?),
            }),
            ast::Type::Array(array) => Ok(Type::array(
                self.resolve_type_with(&array.elem, substitutions)?,
            )),
            ast::Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .map(|element| self.resolve_type_with(element, substitutions))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple),
        }
    }
}

/// The capability types the ratchet harness can supply to a `#[test]` as
/// parameters. A capability value is opaque: its single `$program` field is
/// not a legal surface identifier, so a program can neither construct nor
/// project one — it can only receive it and tag a command template with it.
///
/// r[impl machine.primitive.capabilities-by-identity]
pub const CAPABILITY_TYPE_NAMES: &[&str] = &["Echo", "Sh"];

/// The single opaque field carrying a capability's executable identity.
pub const CAPABILITY_PROGRAM_FIELD: &str = "$program";

#[must_use]
pub fn capability_type(name: &str) -> Type {
    Type::Record(RecordType {
        name: name.to_owned(),
        fields: vec![RecordField {
            name: CAPABILITY_PROGRAM_FIELD.to_owned(),
            ty: Type::String,
        }],
    })
}

#[must_use]
pub fn is_capability_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Record(record)
            if record.fields.len() == 1 && record.fields[0].name == CAPABILITY_PROGRAM_FIELD
    )
}

/// The completed view of one exec output stream under the ratchet capability
/// packages' output protocol: line-framed text codata, keyed by line number.
/// The payload field is not a legal surface identifier; `.collect()` is the
/// only projection.
#[must_use]
pub fn byte_stream_type() -> Type {
    Type::Record(RecordType {
        name: "ByteStream".to_owned(),
        fields: vec![RecordField {
            name: "$lines".to_owned(),
            ty: Type::map(Type::Int, Type::String),
        }],
    })
}

/// `exec`'s result type. There is no exit-status field: termination becomes
/// the typed answer or a typed failure. (`answer`/`tree` await their rungs:
/// unit values and `Tree` are not yet representable surface types here.)
///
/// r[impl machine.primitive.exec-outcome]
/// r[impl machine.primitive.exit-status-is-not-a-value]
#[must_use]
pub fn exec_outcome_type() -> Type {
    Type::Record(RecordType {
        name: "ExecOutcome".to_owned(),
        fields: vec![
            RecordField {
                name: "stdout".to_owned(),
                ty: byte_stream_type(),
            },
            RecordField {
                name: "stderr".to_owned(),
                ty: byte_stream_type(),
            },
        ],
    })
}

fn lower_module(source: &ast::SourceFile, config: CompilerConfig) -> Result<Module, Diagnostics> {
    let mut types = TypeResolver::new(source)?.resolve_all(source)?;
    for name in CAPABILITY_TYPE_NAMES {
        types
            .entry((*name).to_owned())
            .or_insert_with(|| capability_type(name));
    }
    let declared_type_names = source
        .items
        .iter()
        .filter_map(|item| match item {
            ast::Item::Struct(record) => Some(record.name.value.as_str()),
            ast::Item::Enum(enumeration) => Some(enumeration.name.value.as_str()),
            ast::Item::Fn(_) | ast::Item::Import(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let mut signatures = BTreeMap::new();
    let mut ordered_signatures = Vec::new();

    for item in &source.items {
        let ast::Item::Fn(function) = item else {
            continue;
        };
        if declared_type_names.contains(function.name.value.as_str())
            || signatures.contains_key(&function.name.value)
        {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::DuplicateDefinition,
                primary: function.name.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name {
                    name: function.name.value.clone(),
                },
            }));
        }
        let id = FunctionId(
            u32::try_from(ordered_signatures.len()).expect("module function count fits u32"),
        );
        let signature = declare_function(id, function, &types)?;
        signatures.insert(function.name.value.clone(), signature.clone());
        ordered_signatures.push(signature);
    }

    let context = ModuleContext {
        signatures: &signatures,
        types: &types,
        closures: RefCell::new(ClosureState {
            next_function: u32::try_from(ordered_signatures.len())
                .expect("module function count fits u32"),
            functions: BTreeMap::new(),
            scopes: Vec::new(),
        }),
        config,
    };
    let mut module = Module {
        force_molten_copy: config.force_molten_copy,
        records: source
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Struct(record) => match types.get(&record.name.value) {
                    Some(Type::Record(record)) => Some(record.clone()),
                    _ => None,
                },
                ast::Item::Enum(_) | ast::Item::Fn(_) | ast::Item::Import(_) => None,
            })
            .collect(),
        enums: resolved_enum_declarations(source, &types),
        ..Module::default()
    };
    let mut ordered_signatures = ordered_signatures.iter();
    for item in &source.items {
        let ast::Item::Fn(function) = item else {
            continue;
        };
        let signature = ordered_signatures
            .next()
            .expect("every function has a declared signature");
        context.enter_function(function.name.value.clone());
        let lowered = lower_function(signature, function, &context)
            .map_err(|diagnostics| anchor_function_diagnostics(function, diagnostics));
        context.leave_function();
        let lowered = lowered?;
        if signature.is_test {
            module.tests.push(Test {
                name: function.name.value.clone(),
                function: signature.id,
                generator: lowered
                    .generator
                    .expect("test function lowering produces a generator body"),
                metadata: signature.metadata,
            });
        }
        module.functions.push(lowered.function);
    }
    let closures = context.closures.into_inner().functions;
    for (id, function) in closures {
        assert_eq!(
            usize::try_from(id.0).expect("function id fits usize"),
            module.functions.len(),
            "closure functions append in FunctionId order"
        );
        module.functions.push(function);
    }

    Ok(module)
}

fn resolved_enum_declarations(
    source: &ast::SourceFile,
    types: &BTreeMap<String, Type>,
) -> Vec<EnumType> {
    let mut resolved = Vec::new();
    for item in &source.items {
        let ast::Item::Enum(declaration) = item else {
            continue;
        };
        if declaration.generics.is_none() {
            if let Some(Type::Enum(enumeration)) = types.get(&declaration.name.value) {
                resolved.push(enumeration.clone());
            }
            continue;
        }
        let prefix = format!("{}<", declaration.name.value);
        resolved.extend(types.iter().filter_map(|(name, ty)| match ty {
            Type::Enum(enumeration) if name.starts_with(&prefix) => Some(enumeration.clone()),
            _ => None,
        }));
    }
    resolved
}

/// r[impl lang.diagnostics.non-exhaustive-match]
fn anchor_function_diagnostics(
    function: &ast::FnItem,
    mut diagnostics: Diagnostics,
) -> Diagnostics {
    for diagnostic in &mut diagnostics.entries {
        match diagnostic.code {
            DiagnosticCode::NonExhaustiveMatch => {
                let match_span = diagnostic.primary;
                diagnostic.primary = function.name.span;
                diagnostic.labels.push(Label {
                    span: match_span,
                    text: "non-exhaustive match occurs here".to_owned(),
                });
            }
            // An unbound capability tag is the declaration's incompleteness:
            // there is no way to name the tool HERE, so the primary span names
            // the declaration (from its `#[test]` attribute — where capability
            // parameters would have to be supplied) and the use site becomes a
            // label. The CST item span absorbs leading trivia, so the anchor is
            // the first attribute token, not the item span.
            DiagnosticCode::UnboundIdentifier => {
                let use_span = diagnostic.primary;
                diagnostic.primary = function
                    .attributes
                    .first()
                    .map(|attribute| attribute.span)
                    .unwrap_or(function.name.span);
                diagnostic.labels.push(Label {
                    span: use_span,
                    text: "used here without a declaring parameter".to_owned(),
                });
            }
            _ => {}
        }
    }
    diagnostics
}

fn declare_function(
    id: FunctionId,
    function: &ast::FnItem,
    types: &BTreeMap<String, Type>,
) -> Result<FunctionSignature, Diagnostics> {
    let is_test = function
        .attributes
        .iter()
        .any(|attribute| attribute.name.value == "test");
    if is_test {
        check_test_signature(function, types)?;
        let mut names = BTreeSet::new();
        let mut parameters = Vec::new();
        for parameter in &function.params.params {
            declare_parameter(
                &mut parameters,
                &mut names,
                &parameter.name.value,
                parameter.name.span,
                &parameter.ty,
                ParameterKind::Positional,
                types,
            )?;
        }
        return Ok(FunctionSignature {
            id,
            is_test,
            parameters,
            return_type: Type::StreamCheck,
            metadata: parse_test_metadata(function)?,
        });
    }
    if let Some(generics) = &function.generics {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            generics.span,
            "generic function",
        )));
    }
    if function.params.params.len() > 1 {
        return Err(invalid_arity(
            function.params.span,
            1,
            function.params.params.len(),
        ));
    }

    let mut names = BTreeSet::new();
    let mut parameters = Vec::new();
    for parameter in &function.params.params {
        declare_parameter(
            &mut parameters,
            &mut names,
            &parameter.name.value,
            parameter.name.span,
            &parameter.ty,
            ParameterKind::Positional,
            types,
        )?;
    }
    if let Some(where_params) = &function.where_params {
        if where_params.named.is_some() {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                where_params.span,
                "named where-parameter record",
            )));
        }
        if let Some(inline) = &where_params.inline {
            for parameter in &inline.params {
                if parameter.default.is_some() {
                    return Err(Diagnostics::one(Diagnostic::unsupported(
                        parameter.span,
                        "defaulted named parameter",
                    )));
                }
                declare_parameter(
                    &mut parameters,
                    &mut names,
                    &parameter.name.value,
                    parameter.name.span,
                    &parameter.ty,
                    ParameterKind::Named,
                    types,
                )?;
            }
        }
    }
    let return_type = function
        .return_type
        .as_ref()
        .ok_or_else(|| {
            Diagnostics::one(Diagnostic::unsupported(
                function.span,
                "function without a return type",
            ))
        })
        .and_then(|ty| lower_declared_type(ty, types))?;
    Ok(FunctionSignature {
        id,
        is_test,
        parameters,
        return_type,
        metadata: TestMetadata::default(),
    })
}

fn declare_parameter(
    parameters: &mut Vec<ParameterSignature>,
    names: &mut BTreeSet<String>,
    name: &str,
    span: Span,
    ty: &ast::Type,
    kind: ParameterKind,
    types: &BTreeMap<String, Type>,
) -> Result<(), Diagnostics> {
    if !names.insert(name.to_owned()) {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::DuplicateBinding,
            primary: span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: name.to_owned(),
            },
        }));
    }
    parameters.push(ParameterSignature {
        id: ParameterId(u32::try_from(parameters.len()).expect("parameter count fits u32")),
        name: name.to_owned(),
        span,
        ty: lower_declared_type(ty, types)?,
        kind,
    });
    Ok(())
}

fn check_test_signature(
    function: &ast::FnItem,
    types: &BTreeMap<String, Type>,
) -> Result<(), Diagnostics> {
    let valid_return = function
        .return_type
        .as_ref()
        .is_some_and(is_stream_check_type);
    // A test's positional parameters are exactly its capability inputs —
    // typed values the harness (the demand root) supplies by identity.
    // Anything else in the signature stays invalid.
    let capability_params_only = function.params.params.iter().all(|parameter| {
        lower_declared_type(&parameter.ty, types).is_ok_and(|ty| is_capability_type(&ty))
    });
    if !capability_params_only
        || function.where_params.is_some()
        || !valid_return
        || function.generics.is_some()
    {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::InvalidTestSignature,
            primary: function.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Type {
                expected: "fn(capabilities…) -> Stream<Check>".to_owned(),
                found: function.name.value.clone(),
            },
        }));
    }
    Ok(())
}

/// Parse the `#[test { … }]` attribute arguments into typed [`TestMetadata`].
/// Only unit-bearing budget fields are accepted; unknown fields, duplicate
/// fields, missing/non-unit values, and unit mismatches are typed diagnostics.
/// Unit-bearing literals are accepted only here, never in ordinary values.
fn parse_test_metadata(function: &ast::FnItem) -> Result<TestMetadata, Diagnostics> {
    let mut budget = Budget::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for attribute in &function.attributes {
        if attribute.name.value != "test" {
            continue;
        }
        let Some(args) = &attribute.args else {
            continue;
        };
        for field in &args.fields {
            let name = field.name.value.as_str();
            if !seen.insert(name.to_owned()) {
                return Err(field_diagnostic(
                    DiagnosticCode::DuplicateField,
                    field.name.span,
                    "test",
                    name,
                ));
            }
            match name {
                "budget_wall" => {
                    let (text, span) = attribute_quantity(field)?;
                    budget.wall_ns = Some(duration_nanos(text, span)?);
                }
                "budget_rss" => {
                    let (text, span) = attribute_quantity(field)?;
                    budget.rss_bytes = Some(byte_count(text, span)?);
                }
                _ => {
                    return Err(field_diagnostic(
                        DiagnosticCode::UnknownField,
                        field.name.span,
                        "test",
                        name,
                    ));
                }
            }
        }
    }
    Ok(TestMetadata { budget })
}

/// A budget field value must be a unit-bearing literal (`5s`, `256MB`); a bare
/// field, a plain number, or any other expression is a typed error.
fn attribute_quantity(field: &ast::NamedValue) -> Result<(&str, Span), Diagnostics> {
    match &field.value {
        Some(ast::Expr::Quantity(quantity)) => Ok((quantity.value.as_str(), quantity.span)),
        Some(other) => Err(type_mismatch(
            expr_span(other),
            "a unit-bearing literal",
            "expression",
        )),
        None => Err(type_mismatch(
            field.name.span,
            "a unit-bearing literal",
            "bare field",
        )),
    }
}

/// Split a unit-bearing literal into its decimal magnitude and unit suffix. The
/// `quantity` token guarantees at least one digit followed by at least one
/// letter, so both halves are non-empty.
fn split_quantity(text: &str, span: Span) -> Result<(u64, &str), Diagnostics> {
    let boundary = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(boundary);
    let magnitude = digits
        .parse::<u64>()
        .map_err(|_| type_mismatch(span, "a representable magnitude", format!("`{text}`")))?;
    Ok((magnitude, unit))
}

/// Convert a duration literal to nanoseconds with checked arithmetic. A byte
/// unit here is a typed unit mismatch, not a silent reinterpretation.
fn duration_nanos(text: &str, span: Span) -> Result<u64, Diagnostics> {
    let (magnitude, unit) = split_quantity(text, span)?;
    let per_unit = match unit {
        "ns" => 1_u64,
        "us" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60_000_000_000,
        "h" => 3_600_000_000_000,
        _ => {
            return Err(type_mismatch(
                span,
                "a duration unit (ns/us/ms/s/m/h)",
                format!("`{unit}`"),
            ));
        }
    };
    magnitude
        .checked_mul(per_unit)
        .ok_or_else(|| type_mismatch(span, "a representable duration", format!("`{text}`")))
}

/// Convert a byte-size literal to a byte count with checked arithmetic. Binary
/// units: `KB`/`MB`/`GB` are 1024-based, matching resident-set budgeting; the
/// `KiB`/`MiB`/`GiB` spellings are accepted as explicit aliases. A duration
/// unit here is a typed unit mismatch.
fn byte_count(text: &str, span: Span) -> Result<u64, Diagnostics> {
    let (magnitude, unit) = split_quantity(text, span)?;
    let per_unit = match unit {
        "B" => 1_u64,
        "KB" | "KiB" => 1 << 10,
        "MB" | "MiB" => 1 << 20,
        "GB" | "GiB" => 1 << 30,
        _ => {
            return Err(type_mismatch(
                span,
                "a byte unit (B/KB/MB/GB)",
                format!("`{unit}`"),
            ));
        }
    };
    magnitude
        .checked_mul(per_unit)
        .ok_or_else(|| type_mismatch(span, "a representable byte count", format!("`{text}`")))
}

fn is_stream_check_type(ty: &ast::Type) -> bool {
    let ast::Type::Generic(generic) = ty else {
        return false;
    };
    path_is(&generic.base, "Stream")
        && generic.args.len() == 1
        && matches!(&generic.args[0], ast::Type::Path(path) if path_is(path, "Check"))
}

fn path_is(path: &ast::TypePath, expected: &str) -> bool {
    path.segments.len() == 1 && path.segments[0].value == expected
}

fn path_name(path: &ast::TypePath) -> String {
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn applied_type_name(base: &str, arguments: &[Type]) -> String {
    format!(
        "{base}<{}>",
        arguments
            .iter()
            .map(Type::name)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn nominal_base_name(name: &str) -> &str {
    name.split_once('<').map_or(name, |(base, _)| base)
}

fn unknown_name(span: Span, name: impl Into<String>) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::UnknownName,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Name { name: name.into() },
    })
}

fn field_diagnostic(code: DiagnosticCode, span: Span, record: &str, field: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Field {
            record: record.to_owned(),
            field: field.to_owned(),
        },
    })
}

fn variant_diagnostic(
    code: DiagnosticCode,
    span: Span,
    enumeration: &str,
    variant: &str,
) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Variant {
            enumeration: enumeration.to_owned(),
            variant: variant.to_owned(),
        },
    })
}

fn lower_declared_type(
    ty: &ast::Type,
    types: &BTreeMap<String, Type>,
) -> Result<Type, Diagnostics> {
    match ty {
        ast::Type::Path(path) if path_is(path, "Bool") => Ok(Type::Bool),
        ast::Type::Path(path) if path_is(path, "Int") => Ok(Type::Int),
        ast::Type::Path(path) if path_is(path, "String") => Ok(Type::String),
        ast::Type::Path(path) if path_is(path, "Path") => Ok(Type::Path),
        ast::Type::Path(path) if path_is(path, "Check") => Ok(Type::Check),
        ast::Type::Generic(_) if is_stream_check_type(ty) => Ok(Type::StreamCheck),
        ast::Type::Generic(generic) if path_is(&generic.base, "Option") => {
            if generic.args.len() != 1 {
                return Err(invalid_arity(generic.span, 1, generic.args.len()));
            }
            Ok(Type::option(lower_declared_type(&generic.args[0], types)?))
        }
        ast::Type::Generic(generic) if path_is(&generic.base, "Map") => {
            if generic.args.len() != 2 {
                return Err(invalid_arity(generic.span, 2, generic.args.len()));
            }
            let key = lower_declared_type(&generic.args[0], types)?;
            if !key.structural_order_is_defined() {
                return Err(type_mismatch(
                    type_span(&generic.args[0]),
                    "structurally ordered map key",
                    key.name(),
                ));
            }
            Ok(Type::map(
                key,
                lower_declared_type(&generic.args[1], types)?,
            ))
        }
        ast::Type::Generic(generic) if path_is(&generic.base, "Set") => {
            if generic.args.len() != 1 {
                return Err(invalid_arity(generic.span, 1, generic.args.len()));
            }
            let element = lower_declared_type(&generic.args[0], types)?;
            if !element.structural_order_is_defined() {
                return Err(type_mismatch(
                    type_span(&generic.args[0]),
                    "structurally ordered set element",
                    element.name(),
                ));
            }
            Ok(Type::set(element))
        }
        ast::Type::Generic(generic) if path_is(&generic.base, "Result") => {
            if generic.args.len() != 2 {
                return Err(invalid_arity(generic.span, 2, generic.args.len()));
            }
            let ok = lower_declared_type(&generic.args[0], types)?;
            let err = lower_declared_type(&generic.args[1], types)?;
            Ok(Type::result(ok, err))
        }
        ast::Type::Path(path) if path_is(path, "DecodeError") => Ok(decode_error_type()),
        ast::Type::Path(path) => types
            .get(&path_name(path))
            .cloned()
            .ok_or_else(|| unknown_name(path.span, path_name(path))),
        ast::Type::Generic(generic) => {
            let arguments = generic
                .args
                .iter()
                .map(|argument| lower_declared_type(argument, types))
                .collect::<Result<Vec<_>, _>>()?;
            let name = applied_type_name(&path_name(&generic.base), &arguments);
            types
                .get(&name)
                .cloned()
                .ok_or_else(|| unknown_name(generic.span, name))
        }
        ast::Type::Function(function) => Ok(Type::Function {
            parameter: Box::new(lower_declared_type(&function.parameter, types)?),
            result: Box::new(lower_declared_type(&function.result, types)?),
        }),
        ast::Type::Array(array) => Ok(Type::array(lower_declared_type(&array.elem, types)?)),
        ast::Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .map(|element| lower_declared_type(element, types))
            .collect::<Result<Vec<_>, _>>()
            .map(Type::Tuple),
    }
}

struct LoweredFunction {
    function: Function,
    /// Present for `#[test]` functions: the lowered generator/codata body.
    generator: Option<GeneratorBody>,
}

fn lower_function(
    signature: &FunctionSignature,
    function: &ast::FnItem,
    context: &ModuleContext<'_>,
) -> Result<LoweredFunction, Diagnostics> {
    let mut nodes = Vec::new();
    let mut yielded_checks = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut parameters = Vec::with_capacity(signature.parameters.len());

    for parameter in &signature.parameters {
        let node = push_node(
            &mut nodes,
            parameter.span,
            parameter.ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Parameter(parameter.id),
        );
        bindings.insert(
            parameter.name.clone(),
            LoweredValue {
                node,
                ty: parameter.ty.clone(),
            },
        );
        parameters.push(Parameter {
            id: parameter.id,
            node,
            name: parameter.name.clone(),
            ty: parameter.ty.clone(),
            kind: parameter.kind,
        });
    }

    let (output, generator) = if signature.is_test {
        if let Some(tail) = &function.body.tail {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                expr_span(tail),
                "test tail expression",
            )));
        }
        let mut site_counter = 0u32;
        let generator = lower_generator_body(
            &mut nodes,
            &mut yielded_checks,
            &mut site_counter,
            &bindings,
            context,
            GeneratorBodySource {
                statements: &function.body.stmts,
                tail: None,
                top_level: true,
            },
        )?;
        (None, Some(generator))
    } else {
        for statement in &function.body.stmts {
            match statement {
                ast::Stmt::Expression(statement) => {
                    return Err(expression_statement_diagnostic(statement.span));
                }
                ast::Stmt::Yield(statement) => {
                    return Err(Diagnostics::one(Diagnostic::unsupported(
                        statement.span,
                        "yield outside a Stream<Check> test",
                    )));
                }
                ast::Stmt::Let(statement) => {
                    lower_let_statement(&mut nodes, &mut bindings, context, statement)?;
                }
            }
        }
        let output = match &function.body.tail {
            Some(tail) => {
                let value = lower_value_expected(
                    &mut nodes,
                    &bindings,
                    context,
                    tail,
                    Some(&signature.return_type),
                )?;
                require_type(&value, &signature.return_type, expr_span(tail))?;
                Some(value.node)
            }
            None => {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    function.body.span,
                    "function without a tail value",
                )));
            }
        };
        (output, None)
    };

    Ok(LoweredFunction {
        function: Function {
            id: signature.id,
            name: function.name.value.clone(),
            span: function.span,
            parameters,
            return_type: signature.return_type.clone(),
            nodes,
            output,
            yielded_checks,
        },
        generator,
    })
}

/// Lower an ordered sequence of generator statements (a test body or the body
/// of a taken control arm) into a [`GeneratorBody`]. Value nodes are appended
/// to `nodes`; every published check node is recorded in `yielded_checks` for
/// the `must_use` lint and the flat static runner. `top_level` marks the test
/// body itself, whose unconditional leaf yields retain the historical
/// `Op::Yield` codata marker node so flat tests keep their exact VIR shape.
struct GeneratorBodySource<'a> {
    statements: &'a [ast::Stmt],
    tail: Option<&'a ast::Expr>,
    top_level: bool,
}

fn lower_generator_body(
    nodes: &mut Vec<Node>,
    yielded_checks: &mut Vec<NodeId>,
    site_counter: &mut u32,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    source: GeneratorBodySource<'_>,
) -> Result<GeneratorBody, Diagnostics> {
    let GeneratorBodySource {
        statements,
        tail,
        top_level,
    } = source;
    let mut bindings = bindings.clone();
    let mut steps = Vec::new();
    for statement in statements {
        match statement {
            ast::Stmt::Expression(statement) => {
                return Err(expression_statement_diagnostic(statement.span));
            }
            ast::Stmt::Let(statement) => {
                lower_let_statement(nodes, &mut bindings, context, statement)?;
            }
            ast::Stmt::Yield(statement) => match &statement.value {
                ast::Expr::Match(expression) => {
                    steps.push(lower_generator_match(
                        nodes,
                        yielded_checks,
                        site_counter,
                        &bindings,
                        context,
                        expression,
                    )?);
                }
                ast::Expr::If(expression) => {
                    steps.push(lower_generator_if(
                        nodes,
                        yielded_checks,
                        site_counter,
                        &bindings,
                        context,
                        expression,
                    )?);
                }
                value => {
                    let site = lower_yield_check_site(
                        nodes,
                        yielded_checks,
                        site_counter,
                        &bindings,
                        context,
                        value,
                        statement.span,
                    )?;
                    // An unconditional top-level value check retains the
                    // historical `Op::Yield` codata marker so flat tests keep
                    // their exact VIR shape. A snapshot publishes its value node
                    // through the same codata edge. A trace site publishes no
                    // codata island, so it emits no marker node.
                    let published = match &site.recipe {
                        CheckRecipe::Value { check } => Some(*check),
                        CheckRecipe::Snapshot { value, .. } => Some(*value),
                        CheckRecipe::Trace(_) => None,
                    };
                    if top_level && let Some(published) = published {
                        push_node(
                            nodes,
                            statement.span,
                            Type::StreamCheck,
                            EffectFacts::CODATA,
                            vec![published],
                            Op::Yield,
                        );
                    }
                    steps.push(GeneratorStep::Yield(site));
                }
            },
        }
    }
    if let Some(tail) = tail {
        let site = lower_yield_check_site(
            nodes,
            yielded_checks,
            site_counter,
            &bindings,
            context,
            tail,
            expr_span(tail),
        )?;
        steps.push(GeneratorStep::Yield(site));
    }
    Ok(GeneratorBody { steps })
}

/// Lower one leaf `yield <check>` into a static [`YieldSite`]. A value check is
/// a pure parameterized recipe (`Op::Expect` over captured values), never an
/// evaluated boolean or a host call. A trace check is a self-contained
/// descriptor and contributes no demandable node, so it is not recorded in
/// `yielded_checks` (there is nothing to demand or `must_use`-consume).
fn lower_yield_check_site(
    nodes: &mut Vec<Node>,
    yielded_checks: &mut Vec<NodeId>,
    site_counter: &mut u32,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    value: &ast::Expr,
    span: Span,
) -> Result<YieldSite, Diagnostics> {
    let recipe = lower_check(nodes, bindings, context, value)?;
    if let CheckRecipe::Value { check } = &recipe {
        yielded_checks.push(*check);
    }
    let id = YieldSiteId(*site_counter);
    *site_counter = site_counter
        .checked_add(1)
        .expect("yield site count fits u32");
    Ok(YieldSite { id, recipe, span })
}

/// Lower a yielded `match` into a generator [`GeneratorStep::Match`]: real
/// variant dispatch on a scrutinee value whose taken arm is a nested generator
/// body. Untaken arms contribute no yield sites, so there are no phantom checks.
fn lower_generator_match(
    nodes: &mut Vec<Node>,
    yielded_checks: &mut Vec<NodeId>,
    site_counter: &mut u32,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::MatchExpr,
) -> Result<GeneratorStep, Diagnostics> {
    let scrutinee = lower_value(nodes, bindings, context, &expression.scrutinee)?;
    let Type::Enum(enumeration) = scrutinee.ty.clone() else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(&expression.scrutinee),
            "generator match on a non-enum scrutinee",
        )));
    };
    if !expression
        .arms
        .arms
        .iter()
        .all(|arm| enum_pattern(&arm.pattern).is_some() && arm.guard.is_none())
    {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expression.arms.span,
            "generator match requires exhaustive enum arms without guards",
        )));
    }

    let mut seen = BTreeSet::new();
    let mut arms = Vec::with_capacity(expression.arms.arms.len());
    for arm in &expression.arms.arms {
        let pattern = enum_pattern(&arm.pattern).expect("generator match arm shape checked above");
        let (variant_index, variant, variant_span) =
            find_enum_pattern_variant(&enumeration, pattern)?;
        if !seen.insert(variant_index) {
            return Err(variant_diagnostic(
                DiagnosticCode::DuplicateVariant,
                variant_span,
                &enumeration.name,
                &variant.name,
            ));
        }
        let first_binding_node = nodes.len();
        let mut arm_bindings = bindings.clone();
        bind_enum_pattern(
            nodes,
            &mut arm_bindings,
            &scrutinee,
            &enumeration,
            variant_index,
            variant,
            pattern,
        )?;
        let binding_nodes = (first_binding_node..nodes.len())
            .map(|index| NodeId(u32::try_from(index).expect("VIR node index fits u32")))
            .collect();
        let body = match &arm.body {
            ast::MatchArmBody::Block(block) => lower_generator_body(
                nodes,
                yielded_checks,
                site_counter,
                &arm_bindings,
                context,
                GeneratorBodySource {
                    statements: &block.stmts,
                    tail: block.tail.as_ref(),
                    top_level: false,
                },
            )?,
            ast::MatchArmBody::Expr(expression) => {
                let site = lower_yield_check_site(
                    nodes,
                    yielded_checks,
                    site_counter,
                    &arm_bindings,
                    context,
                    expression,
                    expr_span(expression),
                )?;
                GeneratorBody {
                    steps: vec![GeneratorStep::Yield(site)],
                }
            }
        };
        arms.push(GeneratorArm {
            variant: u32::try_from(variant_index).map_err(|_| {
                variant_diagnostic(
                    DiagnosticCode::VariantPayloadMismatch,
                    enum_pattern_span(pattern),
                    &enumeration.name,
                    &variant.name,
                )
            })?,
            condition: NodeId(u32::MAX),
            bindings: binding_nodes,
            body,
        });
    }

    let missing = enumeration
        .variants
        .iter()
        .enumerate()
        .filter(|(index, _)| !seen.contains(index))
        .map(|(_, variant)| variant.name.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::NonExhaustiveMatch,
            primary: expression.arms.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Match { missing },
        }));
    }

    for arm in &mut arms {
        let mut condition_arms = Vec::with_capacity(enumeration.variants.len());
        for variant in 0..enumeration.variants.len() {
            let literal = push_node(
                nodes,
                expression.span,
                Type::Bool,
                EffectFacts::PURE,
                Vec::new(),
                Op::Bool(variant == arm.variant as usize),
            );
            condition_arms.push(VirMatchArm {
                variant: u32::try_from(variant).expect("enum variant index fits u32"),
                nodes: vec![literal],
                output: literal,
            });
        }
        arm.condition = push_node(
            nodes,
            expression.span,
            Type::Bool,
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::Match {
                arms: condition_arms,
            },
        );
    }

    Ok(GeneratorStep::Match {
        scrutinee: scrutinee.node,
        arms,
    })
}

/// Lower a yielded `if` into a generator [`GeneratorStep::If`]: real two-way
/// dispatch on a Bool condition whose taken branch is a nested generator body.
///
/// r[impl machine.test.generator-step]
fn lower_generator_if(
    nodes: &mut Vec<Node>,
    yielded_checks: &mut Vec<NodeId>,
    site_counter: &mut u32,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::IfExpr,
) -> Result<GeneratorStep, Diagnostics> {
    let condition = lower_value(nodes, bindings, context, &expression.condition)?;
    require_type(&condition, &Type::Bool, expr_span(&expression.condition))?;
    let consequent = lower_generator_body(
        nodes,
        yielded_checks,
        site_counter,
        bindings,
        context,
        GeneratorBodySource {
            statements: &expression.consequent.stmts,
            tail: expression.consequent.tail.as_ref(),
            top_level: false,
        },
    )?;
    let alternative = match &expression.alternative {
        ast::IfBranch::Block(block) => lower_generator_body(
            nodes,
            yielded_checks,
            site_counter,
            bindings,
            context,
            GeneratorBodySource {
                statements: &block.stmts,
                tail: block.tail.as_ref(),
                top_level: false,
            },
        )?,
        ast::IfBranch::If(inner) => GeneratorBody {
            steps: vec![lower_generator_if(
                nodes,
                yielded_checks,
                site_counter,
                bindings,
                context,
                inner,
            )?],
        },
    };
    Ok(GeneratorStep::If {
        condition: condition.node,
        consequent,
        alternative,
    })
}

fn lower_let_statement(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    statement: &ast::LetStmt,
) -> Result<(), Diagnostics> {
    let expected = statement
        .ty
        .as_ref()
        .map(|annotation| lower_declared_type(annotation, context.types))
        .transpose()?;
    let value = lower_value_expected(
        nodes,
        bindings,
        context,
        &statement.value,
        expected.as_ref(),
    )?;
    if let (Some(annotation), Some(expected)) = (&statement.ty, &expected) {
        require_type(&value, expected, type_span(annotation))?;
    }
    bind_irrefutable_pattern(nodes, bindings, &statement.pattern, &value)
}

fn bind_irrefutable_pattern(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    pattern: &ast::Pattern,
    value: &LoweredValue,
) -> Result<(), Diagnostics> {
    match pattern {
        ast::Pattern::Binding(pattern) => bind_name(bindings, value, &pattern.binding),
        ast::Pattern::Record(pattern) => {
            let Type::Record(record) = &value.ty else {
                return Err(type_mismatch(
                    pattern.span,
                    path_name(&pattern.ty),
                    value.ty.name(),
                ));
            };
            require_record_pattern_owner(pattern, record)?;
            for (index, declared, field) in
                select_record_pattern_fields(&pattern.fields, &record.fields, &record.name)?
            {
                let projected = project_record_field(
                    nodes,
                    value,
                    index,
                    &declared.ty,
                    pattern_field_span(field),
                )?;
                if let Some(field_pattern) = &field.pattern {
                    bind_irrefutable_pattern(nodes, bindings, field_pattern, &projected)?;
                } else {
                    bind_name(bindings, &projected, &field.name)?;
                }
            }
            Ok(())
        }
        ast::Pattern::Tuple(pattern) => {
            let Type::Tuple(elements) = &value.ty else {
                return Err(type_mismatch(pattern.span, "tuple", value.ty.name()));
            };
            if pattern.elems.len() != elements.len() {
                return Err(type_mismatch(
                    pattern.span,
                    format!("tuple pattern with {} elements", elements.len()),
                    format!("tuple pattern with {} elements", pattern.elems.len()),
                ));
            }
            let elements = elements.clone();
            for (index, (element, ty)) in pattern.elems.iter().zip(elements).enumerate() {
                let index = u32::try_from(index).map_err(|_| {
                    type_mismatch(pattern.span, "tuple field index", index.to_string())
                })?;
                let projected = LoweredValue {
                    node: push_node(
                        nodes,
                        pattern_span(element),
                        ty.clone(),
                        EffectFacts::PURE,
                        vec![value.node],
                        Op::Project { index },
                    ),
                    ty,
                };
                bind_irrefutable_pattern(nodes, bindings, element, &projected)?;
            }
            Ok(())
        }
        ast::Pattern::Wildcard(_) => Ok(()),
        ast::Pattern::Some(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
        ast::Pattern::Ok(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
        ast::Pattern::Err(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
        ast::Pattern::None(span) => Err(Diagnostics::one(Diagnostic::unsupported(
            *span,
            "refutable pattern",
        ))),
        ast::Pattern::Variant(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
        ast::Pattern::Str(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
        ast::Pattern::Number(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable pattern",
        ))),
    }
}

fn expression_statement_diagnostic(span: Span) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::ExpressionStatement,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::ExpressionStatement,
    })
}

fn lower_check(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::Expr,
) -> Result<CheckRecipe, Diagnostics> {
    let ast::Expr::Call(call) = expression else {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::TypeMismatch,
            primary: expr_span(expression),
            labels: Vec::new(),
            payload: DiagnosticPayload::Type {
                expected: "Check".to_owned(),
                found: "expression".to_owned(),
            },
        }));
    };
    if call.callee.value == "expect_snapshot" {
        return lower_snapshot_check(nodes, bindings, context, call);
    }
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a check constructor",
        )));
    }
    // Trace-check constructors are descriptors over the completed run. They
    // build no node: their scalar bound is captured directly, so nothing is
    // lowered to a Weavy island, no scheduler request is issued, and no operand
    // is demanded. Value-check constructors fall through and root an
    // `Op::Expect` recipe below.
    match call.callee.value.as_str() {
        "scheduler_requests_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::SchedulerRequestsAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "memo_entries_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::MemoEntriesAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "store_interns_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::StoreInternsAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "value_island_spawns_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::ValueIslandSpawnsAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "successful_aggregate_freezes_at_most" => {
            return Ok(CheckRecipe::Trace(
                TraceCheck::SuccessfulAggregateFreezesAtMost {
                    bound: trace_bound(call)?,
                },
            ));
        }
        "active_molten_selections_at_most" => {
            return Ok(CheckRecipe::Trace(
                TraceCheck::ActiveMoltenSelectionsAtMost {
                    bound: trace_bound(call)?,
                },
            ));
        }
        "forced_copy_selections_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::ForcedCopySelectionsAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "framed_bytes_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::FramedBytesAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "peak_molten_bytes_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::PeakMoltenBytesAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "peak_molten_nodes_at_most" => {
            return Ok(CheckRecipe::Trace(TraceCheck::PeakMoltenNodesAtMost {
                bound: trace_bound(call)?,
            }));
        }
        "demanded_times" => {
            return Ok(CheckRecipe::Trace(trace_function_calls(
                nodes, bindings, context, call,
            )?));
        }
        "demanded" => {
            return Ok(CheckRecipe::Trace(TraceCheck::Demanded {
                wire: described_wire(nodes, bindings, context, call)?,
            }));
        }
        "never_demanded" => {
            return Ok(CheckRecipe::Trace(TraceCheck::NeverDemanded {
                wire: described_wire(nodes, bindings, context, call)?,
            }));
        }
        "demanded_once" => {
            return Ok(CheckRecipe::Trace(TraceCheck::DemandedOnce {
                wire: described_wire(nodes, bindings, context, call)?,
            }));
        }
        "ran_processes" => {
            return Ok(CheckRecipe::Trace(TraceCheck::RanProcesses {
                count: trace_bound(call)?,
            }));
        }
        "read" => {
            return Ok(CheckRecipe::Trace(TraceCheck::Read {
                path: trace_path(call)?,
            }));
        }
        "never_read" => {
            return Ok(CheckRecipe::Trace(TraceCheck::NeverRead {
                path: trace_path(call)?,
            }));
        }
        "fetched" => {
            return Ok(CheckRecipe::Trace(TraceCheck::Fetched {
                times: trace_bound(call)?,
            }));
        }
        _ => {}
    }
    let condition = match call.callee.value.as_str() {
        "expect" => {
            check_arity(call, 1)?;
            let condition = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&condition, &Type::Bool, expr_span(&call.args.args[0]))?;
            condition.node
        }
        "expect_eq" | "expect_ne" => {
            check_arity(call, 2)?;
            let left = lower_value(nodes, bindings, context, &call.args.args[0])?;
            let right =
                lower_value_expected(nodes, bindings, context, &call.args.args[1], Some(&left.ty))?;
            require_same_type(&left, &right, call.span)?;
            push_equality_condition(
                nodes,
                call.span,
                &left,
                &right,
                call.callee.value == "expect_ne",
            )
        }
        "expect_some" | "expect_none" => {
            check_arity(call, 1)?;
            let option = lower_value(nodes, bindings, context, &call.args.args[0])?;
            if option.ty.option_inner().is_none() {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    "Option<_>",
                    option.ty.name(),
                ));
            }
            push_node(
                nodes,
                call.span,
                Type::Bool,
                EffectFacts::PURE,
                vec![option.node],
                Op::IsVariant {
                    variant: if call.callee.value == "expect_some" {
                        OPTION_SOME_VARIANT
                    } else {
                        OPTION_NONE_VARIANT
                    },
                },
            )
        }
        _ => {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::UnknownName,
                primary: call.callee.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name {
                    name: call.callee.value.clone(),
                },
            }));
        }
    };
    Ok(CheckRecipe::Value {
        check: push_node(
            nodes,
            call.span,
            Type::Check,
            EffectFacts::PURE,
            vec![condition],
            Op::Expect,
        ),
    })
}

/// `expect_snapshot (value, "name")` — snapshot any value structurally.
///
/// Per the Calling chapter, application is juxtaposition and the comma builds a
/// tuple, so this is `expect_snapshot` applied to one pair `(T, String)` — the
/// subject value and its stable name — exactly parallel to `expect_eq (a, b)`.
/// The pair's first component is lowered without a type constraint (every value
/// renders) and becomes the recipe root of a value-publishing island. The second
/// component is a compile-time string literal: a stable harness artifact, never a
/// runtime value, so it is knowable without demanding anything.
fn lower_snapshot_check(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<CheckRecipe, Diagnostics> {
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a check constructor",
        )));
    }
    check_arity(call, 2)?;
    let value = lower_value(nodes, bindings, context, &call.args.args[0])?;
    let ast::Expr::Str(literal) = &call.args.args[1] else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(&call.args.args[1]),
            "snapshot name must be a string literal",
        )));
    };
    Ok(CheckRecipe::Snapshot {
        value: value.node,
        name: literal.value.clone(),
    })
}

/// The single integer bound of a trace-check constructor. The argument is a
/// non-negative integer literal read directly from the surface — never demanded
/// or lowered — so a trace check pins a bound without any evaluation.
fn trace_bound(call: &ast::Call) -> Result<i64, Diagnostics> {
    check_arity(call, 1)?;
    let argument = &call.args.args[0];
    let ast::Expr::Number(number) = argument else {
        return Err(type_mismatch(
            expr_span(argument),
            "an integer bound literal",
            "expression",
        ));
    };
    number.value.parse::<i64>().map_err(|_| {
        type_mismatch(
            number.span,
            "Int",
            format!("number literal `{}`", number.value),
        )
    })
}

/// The path literal of a `never_read` trace check. It is read directly from
/// the surface (`p"..."`) and validated as a relative path — never lowered or
/// demanded, so pinning a never-read path costs no evaluation.
fn trace_path(call: &ast::Call) -> Result<String, Diagnostics> {
    check_arity(call, 1)?;
    let argument = &call.args.args[0];
    let ast::Expr::Path(path) = argument else {
        return Err(type_mismatch(
            expr_span(argument),
            "a Path literal",
            "expression",
        ));
    };
    validate_path_literal(path.value.as_str(), path.span)?;
    Ok(path.value.clone())
}

fn trace_function_calls(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<TraceCheck, Diagnostics> {
    check_arity(call, 2)?;
    let callable = lower_value(nodes, bindings, context, &call.args.args[0])?;
    let function = nodes
        .iter()
        .find(|node| node.id == callable.node)
        .and_then(|node| match node.op {
            Op::Closure(function) => Some(function),
            _ => None,
        })
        .ok_or_else(|| {
            type_mismatch(
                expr_span(&call.args.args[0]),
                "a direct one-argument function reference",
                callable.ty.name(),
            )
        })?;
    let argument = &call.args.args[1];
    let ast::Expr::Number(number) = argument else {
        return Err(type_mismatch(
            expr_span(argument),
            "an integer demand count literal",
            "expression",
        ));
    };
    let times = number.value.parse::<i64>().map_err(|_| {
        type_mismatch(
            number.span,
            "Int",
            format!("number literal `{}`", number.value),
        )
    })?;
    Ok(TraceCheck::FunctionCallsExactly { function, times })
}

/// Describe the operand of a `demanded` / `never_demanded` / `demanded_once`
/// intrinsic as a held [`DescribedWire`], WITHOUT lowering or demanding it.
///
/// Because function arguments are wires, the operand `f(3)` in argument position
/// is a description of an invocation, not a computed value. This resolves that
/// description to the callee's function identity and its exact scalar argument
/// literals. A zero-argument selector `f()` on a function that declares
/// parameters is a name-level selector (every argument demand of `f`); a
/// call-site selector carries the literal arguments so equal-recipe/different-
/// argument demands stay distinct. An identifier operand names a let-bound
/// invocation — including composite and where-clause arguments, which have no
/// literal spelling — selected by its already-lowered call node's canonical
/// preimage. No node is built and no operand is evaluated.
fn described_wire(
    nodes: &[Node],
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<DescribedWire, Diagnostics> {
    check_arity(call, 1)?;
    if let ast::Expr::Identifier(identifier) = &call.args.args[0] {
        let bound = lookup_binding(bindings, &identifier.value, identifier.span)?;
        let Op::Call(function) = nodes[bound.node.0 as usize].op else {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                identifier.span,
                "a described-wire binding names a let-bound function invocation",
            )));
        };
        return Ok(DescribedWire {
            function,
            selector: WireSelector::Binding(bound.node),
        });
    }
    let ast::Expr::Call(operand) = &call.args.args[0] else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(&call.args.args[0]),
            "a described-wire trace check takes a direct function invocation",
        )));
    };
    if operand.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            operand.span,
            "a described-wire selector does not carry where-clause arguments",
        )));
    }
    let signature = context
        .signatures
        .get(&operand.callee.value)
        .ok_or_else(|| {
            Diagnostics::one(Diagnostic {
                code: DiagnosticCode::UnknownName,
                primary: operand.callee.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name {
                    name: operand.callee.value.clone(),
                },
            })
        })?;
    let arguments = operand
        .args
        .args
        .iter()
        .map(wire_argument_literal)
        .collect::<Result<Vec<_>, _>>()?;
    let selector = if arguments.is_empty() && !signature.parameters.is_empty() {
        WireSelector::Name
    } else {
        WireSelector::CallSite(arguments)
    };
    Ok(DescribedWire {
        function: signature.id,
        selector,
    })
}

/// One closed scalar literal in a described-wire selector. A described selector
/// only names literal arguments; it never evaluates a sub-expression to obtain
/// an argument identity.
fn wire_argument_literal(argument: &ast::Expr) -> Result<WireArg, Diagnostics> {
    match argument {
        ast::Expr::Number(number) => number.value.parse::<i64>().map(WireArg::Int).map_err(|_| {
            type_mismatch(
                number.span,
                "Int",
                format!("number literal `{}`", number.value),
            )
        }),
        ast::Expr::Bool(boolean) => Ok(WireArg::Bool(boolean.value)),
        other => Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(other),
            "a described-wire argument must be a closed scalar literal",
        ))),
    }
}

#[derive(Clone)]
struct LoweredValue {
    node: NodeId,
    ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreludeReceiverType {
    Array,
    String,
    Map,
    Set,
    Stream,
    Int,
    Path,
    ByteStream,
    Tree,
    TreeEntry,
    Blob,
    Registry,
}

impl PreludeReceiverType {
    fn from_vir_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(_) => Some(Self::Array),
            Type::String => Some(Self::String),
            Type::Map { .. } => Some(Self::Map),
            Type::Set(_) => Some(Self::Set),
            Type::Stream { .. } => Some(Self::Stream),
            Type::Int => Some(Self::Int),
            Type::Path => Some(Self::Path),
            Type::Record(record) if record.name == "ByteStream" => Some(Self::ByteStream),
            Type::Extern(ExternKind::Tree) => Some(Self::Tree),
            Type::Extern(ExternKind::TreeEntry) => Some(Self::TreeEntry),
            Type::Extern(ExternKind::Blob) => Some(Self::Blob),
            Type::Extern(ExternKind::Registry) => Some(Self::Registry),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreludeMethod {
    ArrayLen,
    ArrayMap,
    ArrayFold,
    ArraySplitLast,
    ArrayAll,
    ArrayAny,
    ArrayContains,
    StringContains,
    StringSplitOnce,
    StringParseInt,
    StringIsNumeric,
    ArraySorted,
    ArrayStream,
    IntRem,
    MapGet,
    MapHas,
    MapLen,
    MapKeys,
    MapValues,
    MapWith,
    SetHas,
    SetLen,
    SetValues,
    StreamFilter,
    StreamFilterMap,
    StreamFlatMap,
    StreamCollect,
    StreamFindMin,
    StreamFindMax,
    StreamSplitMin,
    PathToString,
    IntToString,
    ByteStreamCollect,
    TreeGlob,
    TreeEntryText,
    BlobLen,
    RegistryUrl,
}

#[derive(Clone, Copy)]
struct PreludeMethodEntry {
    receiver: PreludeReceiverType,
    name: &'static str,
    arity: usize,
    method: PreludeMethod,
}

struct PreludeMethodRegistry {
    entries: &'static [PreludeMethodEntry],
}

impl PreludeMethodRegistry {
    const STANDARD: Self = Self {
        entries: &[
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "len",
                arity: 0,
                method: PreludeMethod::ArrayLen,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "map",
                arity: 1,
                method: PreludeMethod::ArrayMap,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "fold",
                arity: 2,
                method: PreludeMethod::ArrayFold,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "split_last",
                arity: 0,
                method: PreludeMethod::ArraySplitLast,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "all",
                arity: 1,
                method: PreludeMethod::ArrayAll,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "any",
                arity: 1,
                method: PreludeMethod::ArrayAny,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "contains",
                arity: 1,
                method: PreludeMethod::ArrayContains,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::String,
                name: "contains",
                arity: 1,
                method: PreludeMethod::StringContains,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::String,
                name: "split_once",
                arity: 1,
                method: PreludeMethod::StringSplitOnce,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::String,
                name: "parse_int",
                arity: 0,
                method: PreludeMethod::StringParseInt,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::String,
                name: "is_numeric",
                arity: 0,
                method: PreludeMethod::StringIsNumeric,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "sorted",
                arity: 0,
                method: PreludeMethod::ArraySorted,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Array,
                name: "stream",
                arity: 0,
                method: PreludeMethod::ArrayStream,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Int,
                name: "rem",
                arity: 1,
                method: PreludeMethod::IntRem,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "get",
                arity: 1,
                method: PreludeMethod::MapGet,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "has",
                arity: 1,
                method: PreludeMethod::MapHas,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "len",
                arity: 0,
                method: PreludeMethod::MapLen,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "keys",
                arity: 0,
                method: PreludeMethod::MapKeys,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "values",
                arity: 0,
                method: PreludeMethod::MapValues,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Map,
                name: "with",
                arity: 2,
                method: PreludeMethod::MapWith,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Set,
                name: "has",
                arity: 1,
                method: PreludeMethod::SetHas,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Set,
                name: "len",
                arity: 0,
                method: PreludeMethod::SetLen,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Set,
                name: "values",
                arity: 0,
                method: PreludeMethod::SetValues,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "filter",
                arity: 1,
                method: PreludeMethod::StreamFilter,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "filter_map",
                arity: 1,
                method: PreludeMethod::StreamFilterMap,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "flat_map",
                arity: 1,
                method: PreludeMethod::StreamFlatMap,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "collect",
                arity: 0,
                method: PreludeMethod::StreamCollect,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::ByteStream,
                name: "collect",
                arity: 0,
                method: PreludeMethod::ByteStreamCollect,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "find_min",
                arity: 1,
                method: PreludeMethod::StreamFindMin,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "find_max",
                arity: 1,
                method: PreludeMethod::StreamFindMax,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Stream,
                name: "split_min",
                arity: 0,
                method: PreludeMethod::StreamSplitMin,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Path,
                name: "to_string",
                arity: 0,
                method: PreludeMethod::PathToString,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Int,
                name: "to_string",
                arity: 0,
                method: PreludeMethod::IntToString,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Tree,
                name: "glob",
                arity: 1,
                method: PreludeMethod::TreeGlob,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::TreeEntry,
                name: "text",
                arity: 0,
                method: PreludeMethod::TreeEntryText,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Blob,
                name: "len",
                arity: 0,
                method: PreludeMethod::BlobLen,
            },
            PreludeMethodEntry {
                receiver: PreludeReceiverType::Registry,
                name: "url",
                arity: 1,
                method: PreludeMethod::RegistryUrl,
            },
        ],
    };

    fn resolve(&self, receiver: &Type, name: &str) -> Option<PreludeMethodEntry> {
        let receiver = PreludeReceiverType::from_vir_type(receiver)?;
        self.entries
            .iter()
            .copied()
            .find(|entry| entry.receiver == receiver && entry.name == name)
    }
}

fn lower_value(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::Expr,
) -> Result<LoweredValue, Diagnostics> {
    lower_value_expected(nodes, bindings, context, expression, None)
}

fn validate_path_literal(value: &str, span: Span) -> Result<(), Diagnostics> {
    if value.is_empty() {
        return Ok(());
    }
    if value.starts_with('/') || value.contains('\\') || value.contains('\0') {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            span,
            "Path literal must be a relative path",
        )));
    }
    for segment in value.split('/') {
        validate_path_segment(segment, span)?;
    }
    Ok(())
}

fn validate_path_segment(value: &str, span: Span) -> Result<(), Diagnostics> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            span,
            "Path join requires one nonempty segment without separators, dot segments, or NUL",
        )));
    }
    Ok(())
}

fn lower_value_expected(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::Expr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    match expression {
        ast::Expr::Closure(closure) => lower_closure(nodes, bindings, context, closure, expected),
        ast::Expr::If(expression) => lower_if(nodes, bindings, context, expression, expected),
        ast::Expr::Bool(value) => Ok(lower_bool_constant(nodes, value.span, value.value)),
        ast::Expr::Number(value) => lower_integer_literal(nodes, value.span, &value.value),
        // A unit-bearing literal only has meaning inside an attribute argument
        // value (e.g. a `#[test]` budget). In an ordinary value position it is a
        // typed error, never a silently-parsed number.
        ast::Expr::Quantity(value) => Err(Diagnostics::one(Diagnostic::unsupported(
            value.span,
            "unit-bearing literal outside an attribute",
        ))),
        ast::Expr::Str(value) => Ok(LoweredValue {
            node: push_node(
                nodes,
                value.span,
                Type::String,
                EffectFacts::PURE,
                Vec::new(),
                Op::String(value.value.clone()),
            ),
            ty: Type::String,
        }),
        ast::Expr::Path(value) => {
            validate_path_literal(value.value.as_str(), value.span)?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    value.span,
                    Type::Path,
                    EffectFacts::PURE,
                    Vec::new(),
                    Op::Path(value.value.clone()),
                ),
                ty: Type::Path,
            })
        }
        ast::Expr::Identifier(identifier) if identifier.value == "None" => {
            lower_none(nodes, identifier.span, expected)
        }
        ast::Expr::Identifier(identifier) => {
            if let Some(value) = bindings.get(&identifier.value) {
                Ok(value.clone())
            } else {
                lower_function_reference(nodes, context, identifier)
            }
        }
        ast::Expr::Call(call) if call.callee.value == "by_key" => {
            lower_by_key(nodes, bindings, context, call, expected)
        }
        ast::Expr::Call(call) if call.callee.value == "Some" => {
            lower_some(nodes, bindings, context, call, expected)
        }
        ast::Expr::Call(call) if try_decode_format(&call.callee.value).is_some() => {
            lower_try_decode(nodes, bindings, context, call)
        }
        ast::Expr::Call(call) if decode_format(&call.callee.value).is_some() => {
            lower_decode(nodes, call, expected)
        }
        ast::Expr::Call(call) if effect_intrinsic(&call.callee.value) => {
            lower_effect_intrinsic(nodes, bindings, context, call)
        }
        ast::Expr::Call(call) => lower_call(nodes, bindings, context, call),
        ast::Expr::WhereCall(call) => lower_where_call(nodes, bindings, context, call),
        ast::Expr::Command(command) => lower_command(nodes, bindings, context, command),
        ast::Expr::Exec(exec) => lower_exec(nodes, bindings, context, exec),
        ast::Expr::Try(try_expr) => lower_try(nodes, bindings, context, try_expr),
        ast::Expr::Binary(binary) => lower_binary(nodes, bindings, context, binary),
        ast::Expr::Variant(variant) => lower_variant(nodes, bindings, context, variant, expected),
        ast::Expr::Match(match_expr) => lower_match(nodes, bindings, context, match_expr, expected),
        ast::Expr::Tuple(tuple) => {
            let expected_elements = match expected {
                Some(Type::Tuple(elements)) if elements.len() == tuple.elems.len() => {
                    Some(elements.as_slice())
                }
                Some(Type::Tuple(elements)) => {
                    return Err(type_mismatch(
                        tuple.span,
                        format!("tuple with {} elements", elements.len()),
                        format!("tuple with {} elements", tuple.elems.len()),
                    ));
                }
                Some(expected) => {
                    return Err(type_mismatch(tuple.span, expected.name(), "tuple"));
                }
                None => None,
            };
            let values = tuple
                .elems
                .iter()
                .enumerate()
                .map(|(index, element)| {
                    lower_value_expected(
                        nodes,
                        bindings,
                        context,
                        element,
                        expected_elements.map(|elements| &elements[index]),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let ty = Type::Tuple(values.iter().map(|value| value.ty.clone()).collect());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    tuple.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    values.iter().map(|value| value.node).collect(),
                    Op::Tuple,
                ),
                ty,
            })
        }
        ast::Expr::Record(record) => lower_named_constructor(nodes, bindings, context, record),
        ast::Expr::Field(field) => {
            let receiver = lower_value(nodes, bindings, context, &field.receiver)?;
            // `DecodeError.message` is a rendered projection, not a stored field.
            if let ast::Member::Identifier(name) = &field.name
                && name.value == "message"
                && is_decode_error(&receiver.ty)
            {
                return Ok(lower_decode_error_message(nodes, &receiver, field.span));
            }
            let (index_value, ty) = match &field.name {
                ast::Member::Index(index) => {
                    let index_value = index.value.parse::<usize>().map_err(|_| {
                        type_mismatch(
                            index.span,
                            "tuple index",
                            format!("index `{}`", index.value),
                        )
                    })?;
                    let Type::Tuple(elements) = &receiver.ty else {
                        return Err(type_mismatch(
                            expr_span(&field.receiver),
                            "tuple",
                            receiver.ty.name(),
                        ));
                    };
                    let ty = elements.get(index_value).cloned().ok_or_else(|| {
                        type_mismatch(
                            index.span,
                            format!("tuple index below {}", elements.len()),
                            index.value.clone(),
                        )
                    })?;
                    (index_value, ty)
                }
                ast::Member::Identifier(name) => {
                    let Type::Record(record) = &receiver.ty else {
                        return Err(type_mismatch(
                            expr_span(&field.receiver),
                            "record",
                            receiver.ty.name(),
                        ));
                    };
                    let (index, declared) = record
                        .fields
                        .iter()
                        .enumerate()
                        .find(|(_, declared)| declared.name == name.value)
                        .ok_or_else(|| {
                            field_diagnostic(
                                DiagnosticCode::UnknownField,
                                name.span,
                                &record.name,
                                &name.value,
                            )
                        })?;
                    (index, declared.ty.clone())
                }
            };
            let index_value = u32::try_from(index_value).map_err(|_| {
                type_mismatch(field.span, "aggregate field index", index_value.to_string())
            })?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    field.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::Project { index: index_value },
                ),
                ty,
            })
        }
        ast::Expr::Array(array) => lower_array(nodes, bindings, context, array, expected),
        ast::Expr::Map(map) => lower_map(nodes, bindings, context, map, expected),
        ast::Expr::Set(set) => lower_set(nodes, bindings, context, set, expected),
        ast::Expr::Index(index) => lower_array_index(nodes, bindings, context, index),
        ast::Expr::MethodCall(call) => lower_method_call(nodes, bindings, context, call, expected),
        ast::Expr::Paren(paren) => {
            lower_value_expected(nodes, bindings, context, &paren.inner, expected)
        }
        ast::Expr::Unary(unary) => lower_unary_value(nodes, bindings, context, unary),
    }
}

fn lower_array(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    array: &ast::ArrayExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let expected_element = match expected {
        Some(Type::Array(element)) => Some(element.as_ref()),
        Some(expected) => return Err(type_mismatch(array.span, expected.name(), "array")),
        None => None,
    };
    let values = array
        .elems
        .iter()
        .map(|element| lower_value_expected(nodes, bindings, context, element, expected_element))
        .collect::<Result<Vec<_>, _>>()?;
    let element = match (values.first(), expected_element) {
        (Some(first), _) => first.ty.clone(),
        (None, Some(expected)) => expected.clone(),
        (None, None) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                array.span,
                "an empty array literal needs an expected element type",
            )));
        }
    };
    for (value, expression) in values.iter().zip(&array.elems) {
        require_type(value, &element, expr_span(expression))?;
    }
    let ty = Type::array(element);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            array.span,
            ty.clone(),
            EffectFacts::PURE,
            values.iter().map(|value| value.node).collect(),
            Op::Array,
        ),
        ty,
    })
}

fn lower_map(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    map: &ast::MapExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let expected_types = match expected {
        Some(Type::Map { key, value }) => Some((key.as_ref(), value.as_ref())),
        Some(expected) => return Err(type_mismatch(map.span, expected.name(), "map")),
        None => None,
    };
    let rows = map
        .rows
        .iter()
        .map(|row| {
            let key = lower_value_expected(
                nodes,
                bindings,
                context,
                &row.key,
                expected_types.map(|(key, _)| key),
            )?;
            let value = lower_value_expected(
                nodes,
                bindings,
                context,
                &row.value,
                expected_types.map(|(_, value)| value),
            )?;
            Ok((key, value))
        })
        .collect::<Result<Vec<_>, Diagnostics>>()?;
    let (key, value) = match (rows.first(), expected_types) {
        (Some((key, value)), _) => (key.ty.clone(), value.ty.clone()),
        (None, Some((key, value))) => (key.clone(), value.clone()),
        (None, None) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                map.span,
                "an empty map literal needs an expected key and value type",
            )));
        }
    };
    if !key.structural_order_is_defined() {
        return Err(type_mismatch(
            map.span,
            "structurally ordered map key",
            key.name(),
        ));
    }
    for (row, (lowered_key, lowered_value)) in map.rows.iter().zip(&rows) {
        require_type(lowered_key, &key, expr_span(&row.key))?;
        require_type(lowered_value, &value, expr_span(&row.value))?;
    }
    let ty = Type::map(key, value);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            map.span,
            ty.clone(),
            EffectFacts {
                fallible: true,
                ..EffectFacts::PURE
            },
            rows.iter()
                .flat_map(|(key, value)| [key.node, value.node])
                .collect(),
            Op::Map,
        ),
        ty,
    })
}

fn lower_set(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    set: &ast::SetExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let expected_element = match expected {
        Some(Type::Set(element)) => Some(element.as_ref()),
        Some(expected) => return Err(type_mismatch(set.span, expected.name(), "set")),
        None => None,
    };
    let values = set
        .elems
        .iter()
        .map(|element| lower_value_expected(nodes, bindings, context, element, expected_element))
        .collect::<Result<Vec<_>, _>>()?;
    let element = match (values.first(), expected_element) {
        (Some(first), _) => first.ty.clone(),
        (None, Some(expected)) => expected.clone(),
        (None, None) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                set.span,
                "an empty set literal needs an expected element type",
            )));
        }
    };
    if !element.structural_order_is_defined() {
        return Err(type_mismatch(
            set.span,
            "structurally ordered set element",
            element.name(),
        ));
    }
    for (value, expression) in values.iter().zip(&set.elems) {
        require_type(value, &element, expr_span(expression))?;
    }
    let ty = Type::set(element);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            set.span,
            ty.clone(),
            EffectFacts::PURE,
            values.iter().map(|value| value.node).collect(),
            Op::Set,
        ),
        ty,
    })
}

fn lower_array_index(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    index: &ast::IndexExpr,
) -> Result<LoweredValue, Diagnostics> {
    let receiver = lower_value(nodes, bindings, context, &index.receiver)?;
    let Some(element) = receiver.ty.array_element() else {
        return Err(type_mismatch(
            expr_span(&index.receiver),
            "array",
            receiver.ty.name(),
        ));
    };
    let element = element.clone();
    let position = lower_value(nodes, bindings, context, &index.index)?;
    require_type(&position, &Type::Int, expr_span(&index.index))?;
    // Array.map is keyed lazy codata at the element boundary: projecting one
    // element of `source.map(f)` denotes `f(source[i])` — one application at a
    // canonical recipe+argument identity — never an eager whole-array map. When
    // the mapper is a capture-free function and the position is a static index
    // of a dense array literal, lower the projection directly to that single
    // application. The unprojected map materializes nothing.
    if let Some((function, argument)) = map_projection(nodes, context, receiver.node, position.node)
    {
        return Ok(LoweredValue {
            node: push_node(
                nodes,
                index.span,
                element.clone(),
                EffectFacts::PURE,
                vec![argument],
                Op::Call(function),
            ),
            ty: element,
        });
    }
    Ok(LoweredValue {
        node: push_node(
            nodes,
            index.span,
            element.clone(),
            EffectFacts {
                fallible: true,
                ..EffectFacts::PURE
            },
            vec![receiver.node, position.node],
            Op::ArrayIndex,
        ),
        ty: element,
    })
}

/// Recognize `source.map(f)[i]` where `f` is a capture-free top-level function,
/// `source` is a dense array literal, and `i` is a static in-bounds position.
/// Returns the mapper's function identity and the source element node it applies
/// to, so the projection lowers to that one application — the keyed-codata
/// element boundary at a stable recipe identity. An anonymous closure keeps the
/// fused-projection map path: it has no standalone recipe to demand.
fn map_projection(
    nodes: &[Node],
    context: &ModuleContext<'_>,
    array: NodeId,
    position: NodeId,
) -> Option<(FunctionId, NodeId)> {
    let map = nodes.get(array.0 as usize)?;
    if !matches!(map.op, Op::ArrayMap { .. }) {
        return None;
    }
    let [source, mapper] = map.inputs[..] else {
        return None;
    };
    let mapper = nodes.get(mapper.0 as usize)?;
    let Op::Closure(function) = mapper.op else {
        return None;
    };
    if !mapper.inputs.is_empty() {
        return None;
    }
    // Only a top-level named function has a standalone recipe to demand; an
    // anonymous closure is inline map machinery.
    if !context
        .signatures
        .values()
        .any(|signature| signature.id == function)
    {
        return None;
    }
    let Op::Int(index) = nodes.get(position.0 as usize)?.op else {
        return None;
    };
    let index = usize::try_from(index).ok()?;
    let source = nodes.get(source.0 as usize)?;
    if !matches!(source.op, Op::Array) {
        return None;
    }
    source
        .inputs
        .get(index)
        .copied()
        .map(|node| (function, node))
}

/// The positional arguments of a method call. The parenless named-argument
/// form (`receiver.name where { .. }`) carries no argument list, so it reads as
/// an empty positional slice.
fn method_positional_args(call: &ast::MethodCall) -> &[ast::Expr] {
    call.args.as_ref().map_or(&[][..], |args| &args.args)
}

/// Uniform function-call dispatch: `recv.method(args)` → `method(recv) where {
/// p0: args[0], .. }`. The callee must have exactly one positional parameter
/// (the receiver) whose type matches the receiver; the method's positional
/// arguments fill the callee's `where` parameters in declaration order. Builds
/// the same `Op::Call` shape as a direct call: positional input first, then the
/// named parameters in signature order.
fn lower_uniform_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::MethodCall,
    receiver: LoweredValue,
    signature: &FunctionSignature,
) -> Result<LoweredValue, Diagnostics> {
    if signature.is_test {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "calling a test function as a method",
        )));
    }
    if let Some(named) = &call.named_args {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            named.span,
            "named method arguments",
        )));
    }
    let positional: Vec<_> = signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == ParameterKind::Positional)
        .collect();
    let [receiver_parameter] = positional.as_slice() else {
        return Err(type_mismatch(
            call.name.span,
            "a method with one receiver parameter",
            &call.name.value,
        ));
    };
    require_type(&receiver, &receiver_parameter.ty, expr_span(&call.receiver))?;

    let named_parameters: Vec<_> = signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == ParameterKind::Named)
        .collect();
    let arguments = method_positional_args(call);
    if arguments.len() != named_parameters.len() {
        return Err(invalid_arity(
            call.span,
            named_parameters.len(),
            arguments.len(),
        ));
    }

    let mut inputs = Vec::with_capacity(signature.parameters.len());
    inputs.push(receiver.node);
    for (parameter, argument) in named_parameters.into_iter().zip(arguments) {
        let value = lower_value_expected(nodes, bindings, context, argument, Some(&parameter.ty))?;
        require_type(&value, &parameter.ty, expr_span(argument))?;
        inputs.push(value.node);
    }

    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            signature.return_type.clone(),
            EffectFacts::PURE,
            inputs,
            Op::Call(signature.id),
        ),
        ty: signature.return_type.clone(),
    })
}

fn lower_method_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::MethodCall,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let receiver = lower_value(nodes, bindings, context, &call.receiver)?;
    let Some(entry) = PreludeMethodRegistry::STANDARD.resolve(&receiver.ty, &call.name.value)
    else {
        // Uniform function-call syntax: `recv.method(args)` on a value with no
        // builtin method of that name resolves to the free function `method`
        // whose sole positional parameter is the receiver, with the method's
        // positional arguments bound to the function's `where` parameters in
        // declaration order. This is the general dispatch the version-set
        // methods (`contains`, `intersect`, `is_empty`) ride on.
        if let Some(signature) = context.signatures.get(&call.name.value) {
            return lower_uniform_call(nodes, bindings, context, call, receiver, signature);
        }
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownMethod,
            primary: call.name.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: call.name.value.clone(),
            },
        }));
    };
    let positional = method_positional_args(call);
    if positional.len() != entry.arity {
        return Err(invalid_arity(call.span, entry.arity, positional.len()));
    }
    // `sorted` is the sole method that accepts a named argument: an explicit
    // `Order<T>` recipe. Every other method rejects named arguments.
    if let Some(named) = &call.named_args
        && !matches!(entry.method, PreludeMethod::ArraySorted)
    {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            named.span,
            "named method arguments",
        )));
    }
    match entry.method {
        PreludeMethod::ArrayLen => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![receiver.node],
                Op::ArrayLen,
            ),
            ty: Type::Int,
        }),
        PreludeMethod::ArrayMap => {
            // r[impl lang.collection.array-map]
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array map registry entry has an array receiver")
            };
            let mapper = match &positional[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, element)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &mapper.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> _", element.name()),
                    mapper.ty.name(),
                ));
            };
            if parameter.as_ref() != element.as_ref() {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> _", element.name()),
                    mapper.ty.name(),
                ));
            }
            let ty = Type::array(result.as_ref().clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node, mapper.node],
                    Op::ArrayMap {
                        grain: ArrayMapGrain {
                            key: ArrayMapGrainKey::InputPosition,
                            origin: ArrayMapGrainKey::InputPosition,
                        },
                    },
                ),
                ty,
            })
        }
        PreludeMethod::ArrayFold => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array fold registry entry has an array receiver")
            };
            let element = element.as_ref().clone();
            // Checkpoint 4: the strict one-item-append fold over `[]` denotes a
            // per-element map under the as-if law. Selecting the molten map
            // shape builds one dense array in-frame and interns it once, exactly
            // as the copy fold's value but without the O(n²) rebuild. The
            // forced-copy differential disables this and keeps the semantic copy
            // path, so the two value sets can be proven identical.
            if !context.config.force_molten_copy
                && let Some(mapper) = try_build_molten_append_mapper(
                    nodes,
                    bindings,
                    context,
                    &positional[0],
                    &positional[1],
                    &element,
                )?
            {
                let ty = Type::array(element);
                return Ok(LoweredValue {
                    node: push_node(
                        nodes,
                        call.span,
                        ty.clone(),
                        EffectFacts {
                            fallible: true,
                            ..EffectFacts::PURE
                        },
                        vec![receiver.node, mapper.node],
                        Op::ArrayMap {
                            grain: ArrayMapGrain {
                                key: ArrayMapGrainKey::InputPosition,
                                origin: ArrayMapGrainKey::InputPosition,
                            },
                        },
                    ),
                    ty,
                });
            }
            let initial = lower_fold_accumulator(
                nodes,
                bindings,
                context,
                &positional[0],
                &positional[1],
                &element,
                expected,
            )?;
            let parameter_ty = Type::Tuple(vec![initial.ty.clone(), element.clone()]);
            let folder = match &positional[1] {
                ast::Expr::Closure(closure) => lower_closure_typed(
                    nodes,
                    bindings,
                    context,
                    closure,
                    parameter_ty.clone(),
                    Some(&initial.ty),
                )?,
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &folder.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[1]),
                    format!("fn({}) -> {}", parameter_ty.name(), initial.ty.name()),
                    folder.ty.name(),
                ));
            };
            if parameter.as_ref() != &parameter_ty || result.as_ref() != &initial.ty {
                return Err(type_mismatch(
                    expr_span(&positional[1]),
                    format!("fn({}) -> {}", parameter_ty.name(), initial.ty.name()),
                    folder.ty.name(),
                ));
            }
            let ty = initial.ty.clone();
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts {
                        fallible: true,
                        ..EffectFacts::PURE
                    },
                    vec![receiver.node, initial.node, folder.node],
                    Op::ArrayFold,
                ),
                ty,
            })
        }
        PreludeMethod::ArraySplitLast => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array split_last registry entry has an array receiver")
            };
            let payload = Type::Tuple(vec![
                element.as_ref().clone(),
                Type::array(element.as_ref().clone()),
            ]);
            let ty = Type::option(payload);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::ArraySplitLast,
                ),
                ty,
            })
        }
        PreludeMethod::ArraySorted => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array sorted registry entry has an array receiver")
            };
            let element = element.as_ref().clone();
            match &call.named_args {
                Some(named) => {
                    lower_sorted_with_order(nodes, bindings, context, &receiver, &element, named)
                }
                None => {
                    if !element.structural_order_is_defined() {
                        return Err(type_mismatch(
                            call.span,
                            "Array<T: Ord>",
                            receiver.ty.name(),
                        ));
                    }
                    let ty = receiver.ty.clone();
                    Ok(LoweredValue {
                        node: push_node(
                            nodes,
                            call.span,
                            ty.clone(),
                            EffectFacts::PURE,
                            vec![receiver.node],
                            Op::ArraySorted,
                        ),
                        ty,
                    })
                }
            }
        }
        PreludeMethod::ArrayAll | PreludeMethod::ArrayAny => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array predicate registry entry has an array receiver")
            };
            let predicate = match &positional[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, element)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &predicate.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", element.name()),
                    predicate.ty.name(),
                ));
            };
            if parameter.as_ref() != element.as_ref() || result.as_ref() != &Type::Bool {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", element.name()),
                    predicate.ty.name(),
                ));
            }
            let op = match entry.method {
                PreludeMethod::ArrayAll => Op::ArrayAll,
                PreludeMethod::ArrayAny => Op::ArrayAny,
                _ => unreachable!("array predicate dispatch is closed"),
            };
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![receiver.node, predicate.node],
                    op,
                ),
                ty: Type::Bool,
            })
        }
        PreludeMethod::ArrayContains => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array contains registry entry has an array receiver")
            };
            let element = element.as_ref().clone();
            let value = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&value, &element, expr_span(&positional[0]))?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![receiver.node, value.node],
                    Op::ArrayContains,
                ),
                ty: Type::Bool,
            })
        }
        PreludeMethod::StringContains => {
            let needle = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&needle, &Type::String, expr_span(&positional[0]))?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![receiver.node, needle.node],
                    Op::StringContains,
                ),
                ty: Type::Bool,
            })
        }
        PreludeMethod::StringSplitOnce => {
            let delimiter = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&delimiter, &Type::String, expr_span(&positional[0]))?;
            let ty = Type::Tuple(vec![Type::String, Type::String]);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts {
                        fallible: true,
                        ..EffectFacts::PURE
                    },
                    vec![receiver.node, delimiter.node],
                    Op::StringSplitOnce,
                ),
                ty,
            })
        }
        PreludeMethod::StringParseInt => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts {
                    fallible: true,
                    ..EffectFacts::PURE
                },
                vec![receiver.node],
                Op::StringParseInt,
            ),
            ty: Type::Int,
        }),
        PreludeMethod::StringIsNumeric => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::Bool,
                EffectFacts::PURE,
                vec![receiver.node],
                Op::StringIsNumeric,
            ),
            ty: Type::Bool,
        }),
        PreludeMethod::ArrayStream => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array stream registry entry has an array receiver")
            };
            let ty = Type::stream(Type::Int, element.as_ref().clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::CODATA,
                    vec![receiver.node],
                    Op::ArrayStream,
                ),
                ty,
            })
        }
        PreludeMethod::IntRem => {
            let divisor = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&divisor, &Type::Int, expr_span(&positional[0]))?;
            let quotient = push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![receiver.node, divisor.node],
                Op::Div,
            );
            let product = push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![quotient, divisor.node],
                Op::Mul,
            );
            let remainder = push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![receiver.node, product],
                Op::Sub,
            );
            Ok(LoweredValue {
                node: remainder,
                ty: Type::Int,
            })
        }
        PreludeMethod::MapGet | PreludeMethod::MapHas | PreludeMethod::MapWith => {
            let (key, value) = receiver
                .ty
                .map_types()
                .map(|(key, value)| (key.clone(), value.clone()))
                .ok_or_else(|| type_mismatch(call.span, "Map<K, V>", receiver.ty.name()))?;
            let lowered_key = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&lowered_key, &key, expr_span(&positional[0]))?;
            let (ty, effect, inputs, op) = match entry.method {
                PreludeMethod::MapGet => (
                    value,
                    EffectFacts {
                        fallible: true,
                        ..EffectFacts::PURE
                    },
                    vec![receiver.node, lowered_key.node],
                    Op::MapGet,
                ),
                PreludeMethod::MapHas => (
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![receiver.node, lowered_key.node],
                    Op::MapHas,
                ),
                PreludeMethod::MapWith => {
                    let lowered_value = lower_value(nodes, bindings, context, &positional[1])?;
                    require_type(&lowered_value, &value, expr_span(&positional[1]))?;
                    (
                        receiver.ty.clone(),
                        EffectFacts::PURE,
                        vec![receiver.node, lowered_key.node, lowered_value.node],
                        Op::MapWith,
                    )
                }
                _ => unreachable!("map method dispatch is closed"),
            };
            Ok(LoweredValue {
                node: push_node(nodes, call.span, ty.clone(), effect, inputs, op),
                ty,
            })
        }
        PreludeMethod::MapLen => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![receiver.node],
                Op::MapLen,
            ),
            ty: Type::Int,
        }),
        PreludeMethod::MapKeys => {
            let (key, _) = receiver
                .ty
                .map_types()
                .ok_or_else(|| type_mismatch(call.span, "Map<K, V>", receiver.ty.name()))?;
            let ty = Type::array(key.clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::MapKeys,
                ),
                ty,
            })
        }
        PreludeMethod::MapValues => {
            let (_, value) = receiver
                .ty
                .map_types()
                .ok_or_else(|| type_mismatch(call.span, "Map<K, V>", receiver.ty.name()))?;
            let ty = Type::array(value.clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::MapValues,
                ),
                ty,
            })
        }
        PreludeMethod::SetHas => {
            let element = receiver
                .ty
                .set_element()
                .cloned()
                .ok_or_else(|| type_mismatch(call.span, "Set<T>", receiver.ty.name()))?;
            let candidate = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&candidate, &element, expr_span(&positional[0]))?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![receiver.node, candidate.node],
                    Op::SetHas,
                ),
                ty: Type::Bool,
            })
        }
        PreludeMethod::SetLen => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::Int,
                EffectFacts::PURE,
                vec![receiver.node],
                Op::SetLen,
            ),
            ty: Type::Int,
        }),
        PreludeMethod::SetValues => {
            let element = receiver
                .ty
                .set_element()
                .cloned()
                .ok_or_else(|| type_mismatch(call.span, "Set<T>", receiver.ty.name()))?;
            let ty = Type::array(element);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::SetValues,
                ),
                ty,
            })
        }
        PreludeMethod::StreamFilter => {
            let (_, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            let predicate = match &positional[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, value)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &predicate.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", value.name()),
                    predicate.ty.name(),
                ));
            };
            if parameter.as_ref() != value || result.as_ref() != &Type::Bool {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", value.name()),
                    predicate.ty.name(),
                ));
            }
            let ty = receiver.ty.clone();
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::CODATA,
                    vec![receiver.node, predicate.node],
                    Op::StreamFilter,
                ),
                ty,
            })
        }
        PreludeMethod::StreamFilterMap => {
            let (key, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            let key = key.clone();
            let value = value.clone();
            let transform = match &positional[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, &value)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &transform.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Option<_>", value.name()),
                    transform.ty.name(),
                ));
            };
            if parameter.as_ref() != &value {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Option<_>", value.name()),
                    transform.ty.name(),
                ));
            }
            let output = result.option_inner().ok_or_else(|| {
                type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Option<_>", value.name()),
                    transform.ty.name(),
                )
            })?;
            let ty = Type::stream(key, output.clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::CODATA,
                    vec![receiver.node, transform.node],
                    Op::StreamFilterMap,
                ),
                ty,
            })
        }
        PreludeMethod::StreamFlatMap => {
            let (key, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            let key = key.clone();
            let value = value.clone();
            let ast::Expr::Closure(closure) = &positional[0] else {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    expr_span(&positional[0]),
                    "flat_map expects a closure returning an array stream",
                )));
            };
            // The closure is lowered as an array-returning frame `fn(V) -> [W]`;
            // its inner stream keys are the dense positions of that array.
            let transform = lower_array_stream_closure(nodes, bindings, context, closure, &value)?;
            let Type::Function { parameter, result } = &transform.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Stream<_, _>", value.name()),
                    transform.ty.name(),
                ));
            };
            if parameter.as_ref() != &value {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Stream<_, _>", value.name()),
                    transform.ty.name(),
                ));
            }
            let inner_value = result.array_element().ok_or_else(|| {
                type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Stream<_, _>", value.name()),
                    transform.ty.name(),
                )
            })?;
            let composed_key = Type::Tuple(vec![key, Type::Int]);
            let ty = Type::stream(composed_key, inner_value.clone());
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::CODATA,
                    vec![receiver.node, transform.node],
                    Op::StreamFlatMap,
                ),
                ty,
            })
        }
        PreludeMethod::StreamCollect => {
            let (key, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            let ty = Type::map(key.clone(), value.clone());
            // Collecting an effect codata recipe (a tree glob) is itself the
            // machine-plane demand that realizes it; collecting a pure stream
            // stays lowered vocabulary.
            let receiver_kind = nodes
                .iter()
                .find(|node| node.id == receiver.node)
                .map(|node| node.effect.kind);
            let effect = if receiver_kind == Some(EffectKind::Effect) {
                EffectFacts::EFFECT
            } else {
                EffectFacts {
                    fallible: true,
                    ..EffectFacts::PURE
                }
            };
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    effect,
                    vec![receiver.node],
                    Op::StreamCollect,
                ),
                ty,
            })
        }
        PreludeMethod::TreeGlob => {
            let pattern = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&pattern, &Type::String, expr_span(&positional[0]))?;
            let ty = Type::stream(Type::Path, Type::Path);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::EFFECT,
                    vec![receiver.node, pattern.node],
                    Op::TreeGlob,
                ),
                ty,
            })
        }
        PreludeMethod::TreeEntryText => {
            let ty = Type::String;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::EFFECT,
                    vec![receiver.node],
                    Op::TreeEntryText,
                ),
                ty,
            })
        }
        PreludeMethod::BlobLen => {
            let ty = Type::Int;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::EFFECT,
                    vec![receiver.node],
                    Op::BlobLen,
                ),
                ty,
            })
        }
        PreludeMethod::RegistryUrl => {
            let name = lower_value(nodes, bindings, context, &positional[0])?;
            require_type(&name, &Type::String, expr_span(&positional[0]))?;
            let ty = Type::Extern(ExternKind::PinnedUrl);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::EFFECT,
                    vec![receiver.node, name.node],
                    Op::RegistryUrl,
                ),
                ty,
            })
        }
        PreludeMethod::StreamFindMin | PreludeMethod::StreamFindMax => {
            let (_, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            if !value.structural_order_is_defined() {
                return Err(type_mismatch(
                    call.span,
                    "Stream<K, V: Ord>",
                    receiver.ty.name(),
                ));
            }
            let value = value.clone();
            let predicate = match &positional[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, &value)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &predicate.ty else {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", value.name()),
                    predicate.ty.name(),
                ));
            };
            if parameter.as_ref() != &value || result.as_ref() != &Type::Bool {
                return Err(type_mismatch(
                    expr_span(&positional[0]),
                    format!("fn({}) -> Bool", value.name()),
                    predicate.ty.name(),
                ));
            }
            let op = match entry.method {
                PreludeMethod::StreamFindMin => Op::StreamFindMin,
                PreludeMethod::StreamFindMax => Op::StreamFindMax,
                _ => unreachable!("stream selection dispatch is closed"),
            };
            let ty = Type::option(value);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node, predicate.node],
                    op,
                ),
                ty,
            })
        }
        PreludeMethod::StreamSplitMin => {
            let (_, value) = receiver
                .ty
                .stream_types()
                .ok_or_else(|| type_mismatch(call.span, "Stream<K, V>", receiver.ty.name()))?;
            if !value.structural_order_is_defined() {
                return Err(type_mismatch(
                    call.span,
                    "Stream<K, V: Ord>",
                    receiver.ty.name(),
                ));
            }
            // The remainder is realized as a dense array of the surviving values
            // in canonical key order: realization is explicit in the type, and
            // no stream recipe is placed inside the Option.
            let rest_ty = Type::array(value.clone());
            let payload = Type::Tuple(vec![value.clone(), rest_ty]);
            let ty = Type::option(payload);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::StreamSplitMin,
                ),
                ty,
            })
        }
        PreludeMethod::PathToString => Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                Type::String,
                EffectFacts::PURE,
                vec![receiver.node],
                Op::PathToString,
            ),
            ty: Type::String,
        }),
        // Completing an exec output stream to its semantic content: under the
        // ratchet capability packages' line-framed output protocol the
        // completed value is the line-keyed map, physically the stream
        // record's single payload field.
        PreludeMethod::ByteStreamCollect => {
            let ty = Type::map(Type::Int, Type::String);
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::Project { index: 0 },
                ),
                ty,
            })
        }
        PreludeMethod::IntToString => {
            require_type(&receiver, &Type::Int, expr_span(&call.receiver))?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    Type::String,
                    EffectFacts::PURE,
                    vec![receiver.node],
                    Op::IntToString,
                ),
                ty: Type::String,
            })
        }
    }
}

fn lower_none(
    nodes: &mut Vec<Node>,
    span: Span,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let expected = expected.ok_or_else(|| {
        Diagnostics::one(Diagnostic::unsupported(
            span,
            "None without an expected Option type",
        ))
    })?;
    if expected.option_inner().is_none() {
        return Err(type_mismatch(span, "Option<_>", expected.name()));
    }
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            expected.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Variant {
                variant: OPTION_NONE_VARIANT,
            },
        ),
        ty: expected.clone(),
    })
}

fn lower_some(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on Some",
        )));
    }
    check_arity(call, 1)?;
    let expected_inner = match expected {
        Some(expected) => Some(
            expected
                .option_inner()
                .ok_or_else(|| type_mismatch(call.span, "Option<_>", expected.name()))?,
        ),
        None => None,
    };
    let payload =
        lower_value_expected(nodes, bindings, context, &call.args.args[0], expected_inner)?;
    if let Some(expected_inner) = expected_inner {
        require_type(&payload, expected_inner, expr_span(&call.args.args[0]))?;
    }
    let ty = Type::option(payload.ty.clone());
    if let Some(expected) = expected
        && &ty != expected
    {
        return Err(type_mismatch(call.span, expected.name(), ty.name()));
    }
    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![payload.node],
            Op::Variant {
                variant: OPTION_SOME_VARIANT,
            },
        ),
        ty,
    })
}

/// The decode intrinsics: a compile-time **constant fold** of typed decode,
/// restricted to compile-time-constant document literals. The literal is decoded
/// against the compiler-known target type (the `let`/call-site expected type) in
/// a single parser pass and lowered to the exact typed-construction VIR a
/// hand-written literal of the same value produces. For a constant, pure,
/// deterministic decode this is a legitimate as-if rewrite; it is **not** the
/// runtime `r[machine.primitive.typed-deserialization]` doc-parse primitive
/// (which serves *dynamic* documents and returns a runtime `Outcome`). When that
/// primitive lands, this fold must become the constant-folded case *of* it, not
/// a replacement — nonliteral sources are rejected at a named runtime seam
/// ([`DiagnosticCode::RuntimeDecodeUnavailable`]) rather than host-evaluated.
fn decode_format(name: &str) -> Option<DecodeFormat> {
    match name {
        "json_decode" => Some(DecodeFormat::Json),
        "toml_decode" => Some(DecodeFormat::Toml),
        _ => None,
    }
}

/// The machine-plane primitive constructors of the tree/fetch band. Each
/// lowers to an [`EffectKind::Effect`] node the partitioner hoists into its
/// own effect island; nothing here is a Weavy-lowerable pure operation.
fn effect_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "fixture_tree" | "fixture_registry" | "fetch" | "untar"
    )
}

fn lower_effect_intrinsic(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<LoweredValue, Diagnostics> {
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a primitive constructor",
        )));
    }
    let (ty, op, inputs) = match call.callee.value.as_str() {
        "fixture_tree" => {
            check_arity(call, 1)?;
            let name = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&name, &Type::String, expr_span(&call.args.args[0]))?;
            (
                Type::Extern(ExternKind::Tree),
                Op::FixtureTree,
                vec![name.node],
            )
        }
        "fixture_registry" => {
            check_arity(call, 0)?;
            (
                Type::Extern(ExternKind::Registry),
                Op::FixtureRegistry,
                Vec::new(),
            )
        }
        "fetch" => {
            check_arity(call, 1)?;
            let pinned = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(
                &pinned,
                &Type::Extern(ExternKind::PinnedUrl),
                expr_span(&call.args.args[0]),
            )?;
            (Type::Extern(ExternKind::Blob), Op::Fetch, vec![pinned.node])
        }
        "untar" => {
            check_arity(call, 1)?;
            let blob = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(
                &blob,
                &Type::Extern(ExternKind::Blob),
                expr_span(&call.args.args[0]),
            )?;
            (Type::Extern(ExternKind::Tree), Op::Untar, vec![blob.node])
        }
        other => unreachable!("effect intrinsic dispatch matched `{other}`"),
    };
    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            ty.clone(),
            EffectFacts::EFFECT,
            inputs,
            op,
        ),
        ty,
    })
}

fn decode_format_label(format: DecodeFormat) -> &'static str {
    match format {
        DecodeFormat::Json => "JSON",
        DecodeFormat::Toml => "TOML",
    }
}

/// The named runtime seam a decode that cannot be constant-folded would need:
/// a nonliteral document, or a target type not known from context.
fn runtime_decode_unavailable(
    span: Span,
    format: DecodeFormat,
    target: Option<&Type>,
) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::RuntimeDecodeUnavailable,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::RuntimeDecode {
            format: decode_format_label(format).to_owned(),
            target: target.map(Type::name),
        },
    })
}

/// The fallible decode surface: `try_json_decode<T>(doc)`. Unlike the infallible
/// [`decode_format`] fold (062–065), this returns a first-class
/// `Result<T, DecodeError>` — a decode that fails is a *value*, not a compile
/// error (`r[lang.failure.typed]`). The target schema is named by the call-site
/// turbofish, not inferred from a `let` annotation.
fn try_decode_format(name: &str) -> Option<DecodeFormat> {
    match name {
        "try_json_decode" => Some(DecodeFormat::Json),
        "try_toml_decode" => Some(DecodeFormat::Toml),
        _ => None,
    }
}

/// The `DecodeError` value surfaced as the `Err` payload of a decode `Result`.
/// Its stored fields are the STRUCTURAL identity of the failure: a stable
/// machine `kind` discriminant (never rendered prose) and the dotted field
/// `path` that reached it. The human `message` is a projection over these
/// fields (see [`lower_decode_error_message`]), never a stored field — a
/// rendered message is presentation, not the error's identity
/// (`r[lang.failure.typed]`, `errors.md`).
const DECODE_ERROR_KIND_FIELD: u32 = 0;
const DECODE_ERROR_PATH_FIELD: u32 = 1;

fn decode_error_type() -> Type {
    Type::Record(RecordType {
        name: "DecodeError".to_owned(),
        fields: vec![
            RecordField {
                name: "kind".to_owned(),
                ty: Type::String,
            },
            RecordField {
                name: "path".to_owned(),
                ty: Type::String,
            },
            RecordField {
                name: "document_offset".to_owned(),
                ty: Type::Int,
            },
            RecordField {
                name: "document_len".to_owned(),
                ty: Type::Int,
            },
        ],
    })
}

/// Is `ty` the built-in `DecodeError` value shape?
fn is_decode_error(ty: &Type) -> bool {
    matches!(ty, Type::Record(record) if record.name == "DecodeError")
}

fn push_string_literal(nodes: &mut Vec<Node>, span: Span, value: String) -> NodeId {
    push_node(
        nodes,
        span,
        Type::String,
        EffectFacts::PURE,
        Vec::new(),
        Op::String(value),
    )
}

/// Construct a `DecodeError` value from a [`decode::DecodeError`]: its stable
/// kind label and its dotted field path, in declaration order. Building the same
/// structured fields a runtime decode would build keeps the constant fold
/// identity-equivalent to the runtime primitive's success/failure value.
fn lower_decode_error_value(
    nodes: &mut Vec<Node>,
    error: &decode::DecodeError,
    span: Span,
) -> LoweredValue {
    let kind = push_string_literal(nodes, span, error.kind.label().to_owned());
    let path = push_string_literal(nodes, span, error.path_names().join("."));
    let document_offset = push_node(
        nodes,
        span,
        Type::Int,
        EffectFacts::PURE,
        Vec::new(),
        Op::Int(error.span.map_or(-1, |span| i64::from(span.offset))),
    );
    let document_len = push_node(
        nodes,
        span,
        Type::Int,
        EffectFacts::PURE,
        Vec::new(),
        Op::Int(error.span.map_or(-1, |span| i64::from(span.len))),
    );
    let ty = decode_error_type();
    LoweredValue {
        node: push_node(
            nodes,
            span,
            ty.clone(),
            EffectFacts::PURE,
            vec![kind, path, document_offset, document_len],
            Op::Record,
        ),
        ty,
    }
}

/// Wrap a value in a `Result<T, DecodeError>` variant (`Ok`/`Err`).
fn lower_result_variant(
    nodes: &mut Vec<Node>,
    payload: LoweredValue,
    variant: u32,
    result_ty: &Type,
    span: Span,
) -> LoweredValue {
    LoweredValue {
        node: push_node(
            nodes,
            span,
            result_ty.clone(),
            EffectFacts::PURE,
            vec![payload.node],
            Op::Variant { variant },
        ),
        ty: result_ty.clone(),
    }
}

/// Project the rendered `message` String of a `DecodeError` value. The message
/// is a presentation projection over the structural fields — `"<kind> at
/// <path>"` — computed from the stored discriminant and field path, so it
/// carries no identity of its own.
fn lower_decode_error_message(
    nodes: &mut Vec<Node>,
    receiver: &LoweredValue,
    span: Span,
) -> LoweredValue {
    let kind = push_node(
        nodes,
        span,
        Type::String,
        EffectFacts::PURE,
        vec![receiver.node],
        Op::Project {
            index: DECODE_ERROR_KIND_FIELD,
        },
    );
    let path = push_node(
        nodes,
        span,
        Type::String,
        EffectFacts::PURE,
        vec![receiver.node],
        Op::Project {
            index: DECODE_ERROR_PATH_FIELD,
        },
    );
    let separator = push_string_literal(nodes, span, " at ".to_owned());
    let prefix = push_node(
        nodes,
        span,
        Type::String,
        EffectFacts::PURE,
        vec![kind, separator],
        Op::StringConcat,
    );
    LoweredValue {
        node: push_node(
            nodes,
            span,
            Type::String,
            EffectFacts::PURE,
            vec![prefix, path],
            Op::StringConcat,
        ),
        ty: Type::String,
    }
}

/// Lower `try_json_decode<T>(doc)` to a `Result<T, DecodeError>`. On a
/// compile-time-constant document this is the **constant fold** of the runtime
/// doc-parse primitive: the decode is run once at compile time and its success
/// or typed-failure value is emitted as ordinary typed construction, provably
/// the same value the runtime primitive produces for that document.
fn lower_try_decode(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<LoweredValue, Diagnostics> {
    let format = try_decode_format(&call.callee.value).expect("try-decode intrinsic name");
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a decode call",
        )));
    }
    check_arity(call, 1)?;
    let Some(type_args) = &call.type_args else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "try_json_decode without a target type application",
        )));
    };
    if type_args.args.len() != 1 {
        return Err(invalid_arity(type_args.span, 1, type_args.args.len()));
    }
    let target = lower_declared_type(&type_args.args[0], context.types)?;
    let result_ty = Type::result(target.clone(), decode_error_type());

    let ast::Expr::Str(document) = &call.args.args[0] else {
        let document = lower_value_expected(
            nodes,
            bindings,
            context,
            &call.args.args[0],
            Some(&Type::String),
        )?;
        require_type(&document, &Type::String, expr_span(&call.args.args[0]))?;
        return Ok(LoweredValue {
            node: push_node(
                nodes,
                call.span,
                result_ty.clone(),
                EffectFacts::PURE,
                vec![document.node],
                Op::Decode { format, target },
            ),
            ty: result_ty,
        });
    };

    // Constant fold: decode the literal once and emit the resulting Result value.
    match decode::decode(format, &document.value, &target) {
        Ok(decoded) => {
            let value = lower_decoded_value(nodes, &decoded, &target, call.span)?;
            Ok(lower_result_variant(
                nodes,
                value,
                RESULT_OK_VARIANT,
                &result_ty,
                call.span,
            ))
        }
        Err(error) => {
            let value = lower_decode_error_value(nodes, &error, call.span);
            Ok(lower_result_variant(
                nodes,
                value,
                RESULT_ERR_VARIANT,
                &result_ty,
                call.span,
            ))
        }
    }
}

fn lower_decode(
    nodes: &mut Vec<Node>,
    call: &ast::Call,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let format = decode_format(&call.callee.value).expect("decode intrinsic name");
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a decode call",
        )));
    }
    check_arity(call, 1)?;
    // The fold's precondition: a known target type and a constant document
    // literal. Either missing names the runtime doc-parse seam — never a
    // host-evaluation of a dynamic string.
    let Some(target) = expected else {
        return Err(runtime_decode_unavailable(call.span, format, None));
    };
    let ast::Expr::Str(document) = &call.args.args[0] else {
        return Err(runtime_decode_unavailable(
            expr_span(&call.args.args[0]),
            format,
            Some(target),
        ));
    };
    let decoded = decode::decode(format, &document.value, target)
        .map_err(|error| decode_failed_diagnostic(call.span, format, target, &error))?;
    lower_decoded_value(nodes, &decoded, target, call.span)
}

/// Render a typed [`decode::DecodeError`] into a structured compiler
/// diagnostic. The kind label, field path, and document byte span are preserved
/// as structured payload fields; the prose `detail` is a rendering convenience
/// and is never an identity-bearing value.
fn decode_failed_diagnostic(
    span: Span,
    format: DecodeFormat,
    target: &Type,
    error: &decode::DecodeError,
) -> Diagnostics {
    let (doc_offset, doc_len) = match error.span {
        Some(document) => (Some(document.offset), Some(document.len)),
        None => (None, None),
    };
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::DecodeFailed,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Decode {
            format: decode_format_label(format).to_owned(),
            target: target.name(),
            kind: error.kind.label().to_owned(),
            path: error.path_names(),
            doc_offset,
            doc_len,
            detail: error.render(),
        },
    })
}

/// Lower a type-directed decode result into typed-construction VIR. The
/// `DecodedValue` is aligned to `ty` by construction, so a shape disagreement
/// here is an internal decoder invariant break, not a source error.
fn lower_decoded_value(
    nodes: &mut Vec<Node>,
    decoded: &DecodedValue,
    ty: &Type,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    match (decoded, ty) {
        (DecodedValue::Int(value), Type::Int) => {
            lower_integer_literal(nodes, span, &value.to_string())
        }
        (DecodedValue::Bool(value), Type::Bool) => Ok(lower_bool_constant(nodes, span, *value)),
        (DecodedValue::Str(value), Type::String) => Ok(LoweredValue {
            node: push_node(
                nodes,
                span,
                Type::String,
                EffectFacts::PURE,
                Vec::new(),
                Op::String(value.clone()),
            ),
            ty: Type::String,
        }),
        (DecodedValue::Record(values), Type::Record(record))
            if values.len() == record.fields.len() =>
        {
            let mut inputs = Vec::with_capacity(values.len());
            for (value, field) in values.iter().zip(&record.fields) {
                inputs.push(lower_decoded_value(nodes, value, &field.ty, span)?.node);
            }
            let ty = ty.clone();
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    span,
                    ty.clone(),
                    EffectFacts::PURE,
                    inputs,
                    Op::Record,
                ),
                ty,
            })
        }
        (DecodedValue::OptionSome(inner), _) if ty.option_inner().is_some() => {
            let inner_ty = ty.option_inner().expect("option target").clone();
            let payload = lower_decoded_value(nodes, inner, &inner_ty, span)?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    span,
                    ty.clone(),
                    EffectFacts::PURE,
                    vec![payload.node],
                    Op::Variant {
                        variant: OPTION_SOME_VARIANT,
                    },
                ),
                ty: ty.clone(),
            })
        }
        (DecodedValue::OptionNone, _) if ty.option_inner().is_some() => Ok(LoweredValue {
            node: push_node(
                nodes,
                span,
                ty.clone(),
                EffectFacts::PURE,
                Vec::new(),
                Op::Variant {
                    variant: OPTION_NONE_VARIANT,
                },
            ),
            ty: ty.clone(),
        }),
        (DecodedValue::Variant { index, fields }, Type::Enum(enumeration)) => {
            let variant = enumeration.variants.get(*index as usize).ok_or_else(|| {
                Diagnostics::one(Diagnostic::unsupported(
                    span,
                    format!(
                        "a decoded variant index out of range for {}",
                        enumeration.name
                    ),
                ))
            })?;
            let field_types: Vec<Type> = match &variant.payload {
                VariantPayload::Tuple(types) => types.clone(),
                VariantPayload::Record(record_fields) => {
                    record_fields.iter().map(|field| field.ty.clone()).collect()
                }
                VariantPayload::Unit => Vec::new(),
            };
            if field_types.len() != fields.len() {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    span,
                    format!(
                        "a decoded {}::{} payload with the wrong field count",
                        enumeration.name, variant.name
                    ),
                )));
            }
            let mut inputs = Vec::with_capacity(fields.len());
            for (value, field_ty) in fields.iter().zip(&field_types) {
                inputs.push(lower_decoded_value(nodes, value, field_ty, span)?.node);
            }
            let ty = ty.clone();
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    span,
                    ty.clone(),
                    EffectFacts::PURE,
                    inputs,
                    Op::Variant { variant: *index },
                ),
                ty,
            })
        }
        _ => Err(Diagnostics::one(Diagnostic::unsupported(
            span,
            format!("a decoded value that does not align with {}", ty.name()),
        ))),
    }
}

/// Names introduced by an irrefutable (or refutable) pattern. Used to subtract
/// a closure's own parameters and inner `let`/match bindings from the set of
/// free variables it must capture.
fn collect_pattern_names(pattern: &ast::Pattern, out: &mut BTreeSet<String>) {
    match pattern {
        ast::Pattern::Binding(binding) => {
            out.insert(binding.binding.value.clone());
        }
        ast::Pattern::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_pattern_names(element, out);
            }
        }
        ast::Pattern::Record(record) => {
            for field in &record.fields.fields {
                match &field.pattern {
                    Some(sub) => collect_pattern_names(sub, out),
                    None => {
                        out.insert(field.name.value.clone());
                    }
                }
            }
        }
        ast::Pattern::Some(some) => collect_pattern_names(&some.payload, out),
        ast::Pattern::Ok(ok) => collect_pattern_names(&ok.payload, out),
        ast::Pattern::Err(err) => collect_pattern_names(&err.payload, out),
        ast::Pattern::Variant(variant) => {
            if let Some(payload) = &variant.tuple_payload {
                for element in &payload.elems {
                    collect_pattern_names(element, out);
                }
            }
        }
        ast::Pattern::None(_)
        | ast::Pattern::Wildcard(_)
        | ast::Pattern::Str(_)
        | ast::Pattern::Number(_) => {}
    }
}

/// Free value-level identifiers referenced by an expression, excluding those in
/// `bound`. A call callee is a value reference too: it either names a module
/// function (never in `bound`, so ignored by the caller's capture filter) or a
/// callable binding that must be captured.
fn collect_free_idents_expr(
    expr: &ast::Expr,
    bound: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match expr {
        ast::Expr::Identifier(identifier) => {
            if !bound.contains(&identifier.value) {
                out.insert(identifier.value.clone());
            }
        }
        ast::Expr::Call(call) => {
            if !bound.contains(&call.callee.value) {
                out.insert(call.callee.value.clone());
            }
            for argument in &call.args.args {
                collect_free_idents_expr(argument, bound, out);
            }
            if let Some(named) = &call.named_args {
                collect_free_idents_named(&named.fields, bound, out);
            }
        }
        ast::Expr::WhereCall(call) => {
            if !bound.contains(&call.callee.value) {
                out.insert(call.callee.value.clone());
            }
            collect_free_idents_named(&call.named_args.fields, bound, out);
        }
        ast::Expr::MethodCall(call) => {
            collect_free_idents_expr(&call.receiver, bound, out);
            if let Some(args) = &call.args {
                for argument in &args.args {
                    collect_free_idents_expr(argument, bound, out);
                }
            }
            if let Some(named) = &call.named_args {
                collect_free_idents_named(&named.fields, bound, out);
            }
        }
        ast::Expr::Binary(binary) => {
            collect_free_idents_expr(&binary.left, bound, out);
            collect_free_idents_expr(&binary.right, bound, out);
        }
        ast::Expr::Unary(unary) => collect_free_idents_expr(&unary.value, bound, out),
        ast::Expr::If(if_expr) => {
            collect_free_idents_expr(&if_expr.condition, bound, out);
            collect_free_idents_block(&if_expr.consequent, bound, out);
            collect_free_idents_if_branch(&if_expr.alternative, bound, out);
        }
        ast::Expr::Match(match_expr) => {
            collect_free_idents_expr(&match_expr.scrutinee, bound, out);
            for arm in &match_expr.arms.arms {
                let mut arm_bound = bound.clone();
                collect_pattern_names(&arm.pattern, &mut arm_bound);
                if let Some(guard) = &arm.guard {
                    collect_free_idents_expr(guard, &arm_bound, out);
                }
                match &arm.body {
                    ast::MatchArmBody::Block(block) => {
                        collect_free_idents_block(block, &arm_bound, out);
                    }
                    ast::MatchArmBody::Expr(expression) => {
                        collect_free_idents_expr(expression, &arm_bound, out);
                    }
                }
            }
        }
        ast::Expr::Closure(closure) => {
            let mut inner = bound.clone();
            for pattern in &closure.patterns {
                collect_pattern_names(pattern, &mut inner);
            }
            match &closure.body {
                ast::ClosureBody::Block(block) => collect_free_idents_block(block, &inner, out),
                ast::ClosureBody::Expr(expression) => {
                    collect_free_idents_expr(expression, &inner, out);
                }
            }
        }
        ast::Expr::Index(index) => {
            collect_free_idents_expr(&index.receiver, bound, out);
            collect_free_idents_expr(&index.index, bound, out);
        }
        ast::Expr::Array(array) => {
            for element in &array.elems {
                collect_free_idents_expr(element, bound, out);
            }
        }
        ast::Expr::Set(set) => {
            for element in &set.elems {
                collect_free_idents_expr(element, bound, out);
            }
        }
        ast::Expr::Map(map) => {
            for row in &map.rows {
                collect_free_idents_expr(&row.key, bound, out);
                collect_free_idents_expr(&row.value, bound, out);
            }
        }
        ast::Expr::Field(field) => collect_free_idents_expr(&field.receiver, bound, out),
        // A command tag is a value reference to a capability binding.
        ast::Expr::Command(command) => {
            if !bound.contains(&command.tag.value) {
                out.insert(command.tag.value.clone());
            }
        }
        ast::Expr::Exec(exec) => {
            if !bound.contains(&exec.command.tag.value) {
                out.insert(exec.command.tag.value.clone());
            }
        }
        ast::Expr::Try(try_expr) => collect_free_idents_expr(&try_expr.value, bound, out),
        ast::Expr::Variant(variant) => {
            if let Some(payload) = &variant.tuple_payload {
                for argument in &payload.args {
                    collect_free_idents_expr(argument, bound, out);
                }
            }
        }
        ast::Expr::Record(record) => {
            if let Some(spread) = &record.fields.spread {
                collect_free_idents_expr(&spread.base, bound, out);
            }
            collect_free_idents_named(&record.fields.fields, bound, out);
        }
        ast::Expr::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_free_idents_expr(element, bound, out);
            }
        }
        ast::Expr::Paren(paren) => collect_free_idents_expr(&paren.inner, bound, out),
        // A `Path` is a qualified type/module reference, a `Quantity` is a
        // dimensioned literal — neither can name an enclosing value binding.
        ast::Expr::Path(_)
        | ast::Expr::Quantity(_)
        | ast::Expr::Str(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_free_idents_named(
    fields: &[ast::NamedValue],
    bound: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for field in fields {
        match &field.value {
            Some(value) => collect_free_idents_expr(value, bound, out),
            None => {
                if !bound.contains(&field.name.value) {
                    out.insert(field.name.value.clone());
                }
            }
        }
    }
}

fn collect_free_idents_if_branch(
    branch: &ast::IfBranch,
    bound: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match branch {
        ast::IfBranch::Block(block) => collect_free_idents_block(block, bound, out),
        ast::IfBranch::If(if_expr) => {
            collect_free_idents_expr(&if_expr.condition, bound, out);
            collect_free_idents_block(&if_expr.consequent, bound, out);
            collect_free_idents_if_branch(&if_expr.alternative, bound, out);
        }
    }
}

fn collect_free_idents_block(
    block: &ast::Block,
    bound: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    let mut bound = bound.clone();
    for statement in &block.stmts {
        match statement {
            ast::Stmt::Let(let_stmt) => {
                collect_free_idents_expr(&let_stmt.value, &bound, out);
                collect_pattern_names(&let_stmt.pattern, &mut bound);
            }
            ast::Stmt::Yield(yield_stmt) => {
                collect_free_idents_expr(&yield_stmt.value, &bound, out);
            }
            ast::Stmt::Expression(expression) => {
                collect_free_idents_expr(&expression.value, &bound, out);
            }
        }
    }
    if let Some(tail) = &block.tail {
        collect_free_idents_expr(tail, &bound, out);
    }
}

fn lower_closure(
    nodes: &mut Vec<Node>,
    outer_bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let expected_signature = match expected {
        Some(Type::Function { parameter, result }) => Some((parameter.as_ref(), result.as_ref())),
        Some(expected) => {
            return Err(type_mismatch(closure.span, expected.name(), "closure"));
        }
        None => None,
    };
    let parameter_ty = match (&closure.ty, expected_signature) {
        (Some(declared), expected) => {
            let declared = lower_declared_type(declared, context.types)?;
            if let Some((expected, _)) = expected
                && &declared != expected
            {
                return Err(type_mismatch(
                    type_span(closure.ty.as_ref().expect("declared closure type")),
                    expected.name(),
                    declared.name(),
                ));
            }
            declared
        }
        (None, Some((expected, _))) => expected.clone(),
        // The ratchet's unannotated let-bound closures infer their parameter
        // from an arithmetic body. The language's numeric literals and
        // arithmetic operators are Int-only, so this is an exact inference,
        // rather than a defaulted host-language callback type.
        (None, None) => Type::Int,
    };

    lower_closure_typed(
        nodes,
        outer_bindings,
        context,
        closure,
        parameter_ty,
        expected_signature.map(|(_, result)| result),
    )
}

fn lower_closure_with_parameter(
    nodes: &mut Vec<Node>,
    outer_bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter: &Type,
) -> Result<LoweredValue, Diagnostics> {
    let parameter_ty = match &closure.ty {
        Some(declared) => {
            let declared = lower_declared_type(declared, context.types)?;
            if &declared != parameter {
                return Err(type_mismatch(
                    type_span(declared_type_ref(closure)),
                    parameter.name(),
                    declared.name(),
                ));
            }
            declared
        }
        None => parameter.clone(),
    };
    lower_closure_typed(nodes, outer_bindings, context, closure, parameter_ty, None)
}

/// Lower a `flat_map` closure `|v| <array>.stream()` to an array-returning
/// callable value of type `fn(V) -> [W]`. The terminal `.stream()` is a codata
/// view that `flat_map` re-derives with fresh position keys at collection time,
/// so the frame itself returns the dense array.
fn lower_array_stream_closure(
    nodes: &mut Vec<Node>,
    outer_bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter: &Type,
) -> Result<LoweredValue, Diagnostics> {
    let parameter_ty = match &closure.ty {
        Some(declared) => {
            let declared = lower_declared_type(declared, context.types)?;
            if &declared != parameter {
                return Err(type_mismatch(
                    type_span(declared_type_ref(closure)),
                    parameter.name(),
                    declared.name(),
                ));
            }
            declared
        }
        None => parameter.clone(),
    };
    lower_closure_typed_with_body_kind(
        nodes,
        outer_bindings,
        context,
        closure,
        parameter_ty,
        None,
        ClosureBodyKind::ArrayStreamSource,
    )
}

fn declared_type_ref(closure: &ast::ClosureExpr) -> &ast::Type {
    closure.ty.as_ref().expect("declared closure type")
}

/// Lower `by_key(|x| <key>)` to an `Order<T>` value.
///
/// The subject `T` comes from the expected `Order<T>` type at the call site, so
/// the key-extraction closure is typed and closed over exactly `T`. The result
/// is an ordinary Vix recipe: a closure `fn(T) -> (K, T)` that pairs the
/// extracted key with the source value. Structural comparison of that pair
/// sorts by `K` and breaks equal keys by the structural order of the source, so
/// the order is total and equality-consistent. The value is typed `Order<T>`;
/// its physical representation is the keyed closure a consuming `sorted` reads.
fn lower_by_key(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let Some(Type::Order(subject)) = expected else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "by_key is only valid where an Order<T> value is expected",
        )));
    };
    if let Some(named) = &call.named_args {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            named.span,
            "named arguments on by_key",
        )));
    }
    check_arity(call, 1)?;
    let ast::Expr::Closure(closure) = &call.args.args[0] else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(&call.args.args[0]),
            "by_key expects a key-extraction closure",
        )));
    };
    let parameter_ty = match &closure.ty {
        Some(declared) => {
            let declared = lower_declared_type(declared, context.types)?;
            if &declared != subject.as_ref() {
                return Err(type_mismatch(
                    type_span(declared_type_ref(closure)),
                    subject.name(),
                    declared.name(),
                ));
            }
            declared
        }
        None => subject.as_ref().clone(),
    };
    let keyed = lower_closure_typed_with_body_kind(
        nodes,
        bindings,
        context,
        closure,
        parameter_ty,
        None,
        ClosureBodyKind::KeyedByParameter,
    )?;
    Ok(LoweredValue {
        node: keyed.node,
        ty: Type::order(subject.as_ref().clone()),
    })
}

/// Lower `array.sorted where { order: <Order<T>> }`.
///
/// The caller's `Order<T>` carries a keyed closure `fn(T) -> (K, T)`. Sorting
/// through it desugars to three recipes that already lower through the verified
/// machine: map every element to its `(key, element)` pair, structurally sort
/// the pairs — which orders by key and breaks equal keys by the source value —
/// then project each pair back to its element. No host call, comparator
/// callback, or new sort primitive is introduced.
fn lower_sorted_with_order(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    receiver: &LoweredValue,
    element: &Type,
    named: &ast::WhereArgs,
) -> Result<LoweredValue, Diagnostics> {
    let [field] = named.fields.as_slice() else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            named.span,
            "sorted accepts exactly one named argument `order`",
        )));
    };
    if field.name.value != "order" {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            field.name.span,
            "the only named argument sorted accepts is `order`",
        )));
    }
    let Some(order_expr) = &field.value else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            field.span,
            "sorted's `order` argument needs an Order<T> value",
        )));
    };
    // Breaking equal keys by the structural order of the source requires the
    // element type itself to be structurally ordered.
    if !element.structural_order_is_defined() {
        return Err(type_mismatch(
            expr_span(order_expr),
            "Array<T: Ord>",
            Type::array(element.clone()).name(),
        ));
    }
    let order_ty = Type::order(element.clone());
    let order = lower_value_expected(nodes, bindings, context, order_expr, Some(&order_ty))?;
    require_type(&order, &order_ty, expr_span(order_expr))?;

    // The Order value's physical recipe is its keyed closure `fn(T) -> (K, T)`.
    let Type::Function { parameter, result } = &nodes[order.node.0 as usize].ty else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(order_expr),
            "sorted's order must be a by_key recipe",
        )));
    };
    if parameter.as_ref() != element {
        return Err(type_mismatch(
            expr_span(order_expr),
            format!("Order<{}>", element.name()),
            order.ty.name(),
        ));
    }
    let pair_ty = result.as_ref().clone();

    // 1. Pair every element with its extracted key: Array<(K, T)>.
    let keyed_array_ty = Type::array(pair_ty.clone());
    let keyed = push_node(
        nodes,
        named.span,
        keyed_array_ty.clone(),
        EffectFacts::PURE,
        vec![receiver.node, order.node],
        Op::ArrayMap {
            grain: ArrayMapGrain {
                key: ArrayMapGrainKey::InputPosition,
                origin: ArrayMapGrainKey::InputPosition,
            },
        },
    );

    // 2. Structurally sort the pairs: orders by key, ties by source value.
    let sorted_pairs = push_node(
        nodes,
        named.span,
        keyed_array_ty,
        EffectFacts::PURE,
        vec![keyed],
        Op::ArraySorted,
    );

    // 3. Project each sorted pair back to its element: Array<T>.
    let project = build_pair_second_closure(nodes, context, &pair_ty, element, named.span)?;
    let result_ty = Type::array(element.clone());
    Ok(LoweredValue {
        node: push_node(
            nodes,
            named.span,
            result_ty.clone(),
            EffectFacts::PURE,
            vec![sorted_pairs, project.node],
            Op::ArrayMap {
                grain: ArrayMapGrain {
                    key: ArrayMapGrainKey::InputPosition,
                    origin: ArrayMapGrainKey::InputPosition,
                },
            },
        ),
        ty: result_ty,
    })
}

/// Build the synthetic closure `|pair| pair.1` of type `fn((K, T)) -> T` that
/// projects the source value out of a `(key, source)` sort pair.
fn build_pair_second_closure(
    nodes: &mut Vec<Node>,
    context: &ModuleContext<'_>,
    pair_ty: &Type,
    element: &Type,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    let (id, name) = context.allocate_closure();
    context.enter_function(name.clone());
    let mut closure_nodes = Vec::new();
    let parameter_id = ParameterId(0);
    let parameter_node = push_node(
        &mut closure_nodes,
        span,
        pair_ty.clone(),
        EffectFacts::PURE,
        Vec::new(),
        Op::Parameter(parameter_id),
    );
    let output_node = push_node(
        &mut closure_nodes,
        span,
        element.clone(),
        EffectFacts::PURE,
        vec![parameter_node],
        Op::Project { index: 1 },
    );
    let ty = Type::Function {
        parameter: Box::new(pair_ty.clone()),
        result: Box::new(element.clone()),
    };
    context.insert_closure(Function {
        id,
        name,
        span,
        parameters: vec![Parameter {
            id: parameter_id,
            node: parameter_node,
            name: "$argument".to_owned(),
            ty: pair_ty.clone(),
            kind: ParameterKind::Positional,
        }],
        return_type: element.clone(),
        nodes: closure_nodes,
        output: Some(output_node),
        yielded_checks: Vec::new(),
    });
    context.leave_function();
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Closure(id),
        ),
        ty,
    })
}

/// Lower a fold's initial accumulator. An empty collection literal (`[]`,
/// `%{}`, `%[]`) cannot infer its element type in isolation; the fold's
/// accumulator type `A` must come from either an external expected type or
/// the closure body's `+` dispatch. A non-empty seed keeps ordinary inference
/// (scalar and string folds, prefilled maps).
fn lower_fold_accumulator(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    initial: &ast::Expr,
    folder: &ast::Expr,
    element: &Type,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    // An external expected type is the accumulator type `A`: flow it into the
    // seed directly, including empty literals.
    if let Some(expected) = expected {
        return lower_value_expected(nodes, bindings, context, initial, Some(expected));
    }
    // An empty collection literal with no external expected type needs its
    // accumulator type inferred from the closure body's `+` dispatch.
    if let Some(accumulator_ty) =
        infer_empty_seed_accumulator_type(bindings, context, initial, folder, element)?
    {
        return lower_value_expected(nodes, bindings, context, initial, Some(&accumulator_ty));
    }
    // An empty array literal that did not match the strict append grain still
    // builds an array over the fold's element type: flow it directly, as the
    // copy path owns the fold's own typing. Empty map/set literals without the
    // append grain have no element type to flow and keep their diagnostic.
    if let ast::Expr::Array(array) = initial
        && array.elems.is_empty()
    {
        return lower_value_expected(
            nodes,
            bindings,
            context,
            initial,
            Some(&Type::array(element.clone())),
        );
    }
    lower_value(nodes, bindings, context, initial)
}

/// Infer the accumulator type `A` for a fold whose seed is an empty collection
/// literal (`[]`, `%{}`, `%[]`) with no external expected type. The closure
/// body must be `acc + EXPR` where `acc` is the accumulator binding; the `+`
/// dispatch table then fixes `A` from the seed's collection kind and `EXPR`'s
/// type:
///
///   * `%{}` seed, `EXPR : (K, V)`        ⇒ `A = Map<K, V>`
///   * `%[]` seed, `EXPR : T`             ⇒ `A = Set<T>`
///
/// Returns `None` when the shape does not hold, leaving the copy path to own
/// the proper diagnostic.
fn infer_empty_seed_accumulator_type(
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    initial: &ast::Expr,
    folder: &ast::Expr,
    element: &Type,
) -> Result<Option<Type>, Diagnostics> {
    let ast::Expr::Closure(closure) = folder else {
        return Ok(None);
    };
    if closure.ty.is_some() {
        return Ok(None);
    }
    let [acc_pattern, elem_pattern] = closure.patterns.as_slice() else {
        return Ok(None);
    };
    let (Some(acc_name), Some(elem_name)) = (binding_name(acc_pattern), binding_name(elem_pattern))
    else {
        return Ok(None);
    };
    let ast::ClosureBody::Expr(body) = &closure.body else {
        return Ok(None);
    };
    let ast::Expr::Binary(binary) = unwrap_paren(body) else {
        return Ok(None);
    };
    if binary.op.value != "+" {
        return Ok(None);
    }
    let ast::Expr::Identifier(left) = unwrap_paren(&binary.left) else {
        return Ok(None);
    };
    if left.value != acc_name {
        return Ok(None);
    }
    let appended = &binary.right;
    // The accumulator is consumed once as the append base and nowhere else.
    if expr_references_name(appended, acc_name) {
        return Ok(None);
    }
    // Probe `EXPR`'s type by lowering it in a throwaway closure frame with
    // only the element parameter bound. The probe allocates a closure id and
    let probe_ty = probe_element_expression_type(
        bindings,
        context,
        appended,
        element,
        elem_name,
        closure.span,
    )?;
    let accumulator_ty = match initial {
        ast::Expr::Array(array) if array.elems.is_empty() => Type::array(probe_ty),
        ast::Expr::Map(map) if map.rows.is_empty() => {
            let Type::Tuple(fields) = &probe_ty else {
                return Ok(None);
            };
            if fields.len() != 2 {
                return Ok(None);
            }
            let key = fields[0].clone();
            if !key.structural_order_is_defined() {
                return Ok(None);
            }
            Type::map(key, fields[1].clone())
        }
        ast::Expr::Set(set) if set.elems.is_empty() => {
            if !probe_ty.structural_order_is_defined() {
                return Ok(None);
            }
            Type::set(probe_ty)
        }
        _ => return Ok(None),
    };
    Ok(Some(accumulator_ty))
}

/// Lower `body` in a throwaway single-parameter closure frame to discover its
/// result type, without committing any VIR nodes to the enclosing function.
/// The frame binds `elem_name` to `element` and lowers `body` with no expected
/// type. Used only by [`infer_empty_seed_accumulator_type`] as a type probe.
fn probe_element_expression_type(
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    body: &ast::Expr,
    element: &Type,
    elem_name: &str,
    span: Span,
) -> Result<Type, Diagnostics> {
    // Lower `body` in a throwaway node buffer with the element parameter bound
    // alongside the enclosing scope's bindings, purely to discover its result
    // type. No closure id is allocated and no function is inserted, so the
    // enclosing module's FunctionId ordering is undisturbed. The probe's VIR
    // nodes are discarded; only the inferred type survives. `EXPR` may capture
    // the element parameter and any enclosing binding, so the probe inherits
    // the outer bindings (their VIR nodes stay referenced by the real fold).
    let mut probe_nodes = Vec::new();
    let parameter_id = ParameterId(0);
    let parameter_node = push_node(
        &mut probe_nodes,
        span,
        element.clone(),
        EffectFacts::PURE,
        Vec::new(),
        Op::Parameter(parameter_id),
    );
    let mut probe_bindings = bindings.clone();
    probe_bindings.insert(
        elem_name.to_owned(),
        LoweredValue {
            node: parameter_node,
            ty: element.clone(),
        },
    );
    let output = lower_value_expected(&mut probe_nodes, &probe_bindings, context, body, None)?;
    Ok(output.ty)
}

/// Recognise the exact strict one-item-append fold shape and, when it holds,
/// synthesise the per-element mapper `|elem| EXPR : element -> element`.
///
/// The shape is: an empty `[]` initial and a closure `|acc, elem| acc + EXPR`
/// where `acc` is consumed exactly once as the append base — it does not appear
/// anywhere in `EXPR` — and `EXPR` captures nothing from the enclosing scope. A
/// fold that fails any of these keeps the semantic copy path.
fn try_build_molten_append_mapper(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    initial: &ast::Expr,
    folder: &ast::Expr,
    element: &Type,
) -> Result<Option<LoweredValue>, Diagnostics> {
    // The accumulator base must start empty for the fold to denote a pure map.
    let ast::Expr::Array(array) = initial else {
        return Ok(None);
    };
    if !array.elems.is_empty() {
        return Ok(None);
    }
    let ast::Expr::Closure(closure) = folder else {
        return Ok(None);
    };
    // Exactly two simple bindings, no type annotation to reconcile.
    if closure.ty.is_some() {
        return Ok(None);
    }
    let [acc_pattern, elem_pattern] = closure.patterns.as_slice() else {
        return Ok(None);
    };
    let (Some(acc_name), Some(elem_name)) = (binding_name(acc_pattern), binding_name(elem_pattern))
    else {
        return Ok(None);
    };
    // The body is `acc + EXPR`: a `+` whose left operand is exactly the
    // accumulator binding.
    let ast::ClosureBody::Expr(body) = &closure.body else {
        return Ok(None);
    };
    let ast::Expr::Binary(binary) = unwrap_paren(body) else {
        return Ok(None);
    };
    if binary.op.value != "+" {
        return Ok(None);
    }
    let ast::Expr::Identifier(left) = unwrap_paren(&binary.left) else {
        return Ok(None);
    };
    if left.value != acc_name {
        return Ok(None);
    }
    let appended = &binary.right;
    // The accumulator is consumed once as the append base and nowhere else.
    if expr_references_name(appended, acc_name) {
        return Ok(None);
    }
    // The appended expression captures nothing from the enclosing scope; its
    // only free binding is the element parameter.
    if bindings
        .keys()
        .any(|name| name != elem_name && expr_references_name(appended, name))
    {
        return Ok(None);
    }
    let mapper =
        build_unary_element_closure(nodes, context, element, elem_name, appended, closure.span)?;
    // The append is well typed only when the element expression has the
    // element type; otherwise this is not a same-type append fold and the copy
    // path owns the proper diagnostic.
    let Type::Function { result, .. } = &mapper.ty else {
        return Ok(None);
    };
    if result.as_ref() != element {
        return Ok(None);
    }
    Ok(Some(mapper))
}

/// The bound name of a simple binding pattern, or `None` for any richer pattern.
fn binding_name(pattern: &ast::Pattern) -> Option<&str> {
    match pattern {
        ast::Pattern::Binding(binding) => Some(binding.binding.value.as_str()),
        _ => None,
    }
}

fn unwrap_paren(expression: &ast::Expr) -> &ast::Expr {
    match expression {
        ast::Expr::Paren(paren) => unwrap_paren(&paren.inner),
        other => other,
    }
}

/// Build the synthetic closure `|elem| EXPR` of type `fn(element) -> _` by
/// lowering the real appended sub-expression with `elem` bound to the parameter.
/// It carries no captures; the caller has proven `EXPR`'s only free binding is
/// the element parameter.
fn build_unary_element_closure(
    nodes: &mut Vec<Node>,
    context: &ModuleContext<'_>,
    element: &Type,
    elem_name: &str,
    body: &ast::Expr,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    let (id, name) = context.allocate_closure();
    context.enter_function(name.clone());
    let lowered = (|| {
        let mut closure_nodes = Vec::new();
        let parameter_id = ParameterId(0);
        let parameter_node = push_node(
            &mut closure_nodes,
            span,
            element.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Parameter(parameter_id),
        );
        let mut closure_bindings = BTreeMap::new();
        closure_bindings.insert(
            elem_name.to_owned(),
            LoweredValue {
                node: parameter_node,
                ty: element.clone(),
            },
        );
        let output =
            lower_value_expected(&mut closure_nodes, &closure_bindings, context, body, None)?;
        let return_type = output.ty.clone();
        let ty = Type::Function {
            parameter: Box::new(element.clone()),
            result: Box::new(return_type.clone()),
        };
        Ok::<_, Diagnostics>((
            Function {
                id,
                name: name.clone(),
                span,
                parameters: vec![Parameter {
                    id: parameter_id,
                    node: parameter_node,
                    name: "$argument".to_owned(),
                    ty: element.clone(),
                    kind: ParameterKind::Positional,
                }],
                return_type,
                nodes: closure_nodes,
                output: Some(output.node),
                yielded_checks: Vec::new(),
            },
            ty,
        ))
    })();
    context.leave_function();
    let (function, ty) = lowered?;
    context.insert_closure(function);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Closure(id),
        ),
        ty,
    })
}

/// Whether `expression` mentions the identifier `name` anywhere in its subtree.
/// A conservative over-approximation: it descends every sub-expression, so a
/// name used as a field, method receiver, or nested closure body still counts.
fn expr_references_name(expression: &ast::Expr, name: &str) -> bool {
    match expression {
        ast::Expr::Identifier(identifier) => identifier.value == name,
        ast::Expr::Path(_)
        | ast::Expr::Str(_)
        | ast::Expr::Number(_)
        | ast::Expr::Quantity(_)
        | ast::Expr::Bool(_) => false,
        ast::Expr::Paren(paren) => expr_references_name(&paren.inner, name),
        ast::Expr::Unary(unary) => expr_references_name(&unary.value, name),
        ast::Expr::Command(command) => command.tag.value == name,
        ast::Expr::Exec(exec) => exec.command.tag.value == name,
        ast::Expr::Try(try_expr) => expr_references_name(&try_expr.value, name),
        ast::Expr::Binary(binary) => {
            expr_references_name(&binary.left, name) || expr_references_name(&binary.right, name)
        }
        ast::Expr::Index(index) => {
            expr_references_name(&index.receiver, name) || expr_references_name(&index.index, name)
        }
        ast::Expr::Field(field) => expr_references_name(&field.receiver, name),
        ast::Expr::Call(call) => {
            call.args
                .args
                .iter()
                .any(|arg| expr_references_name(arg, name))
                || named_args_reference_name(call.named_args.as_ref(), name)
        }
        ast::Expr::WhereCall(call) => named_args_reference_name(Some(&call.named_args), name),
        ast::Expr::MethodCall(call) => {
            expr_references_name(&call.receiver, name)
                || call
                    .args
                    .as_ref()
                    .is_some_and(|args| args.args.iter().any(|arg| expr_references_name(arg, name)))
                || named_args_reference_name(call.named_args.as_ref(), name)
        }
        ast::Expr::Array(array) => array.elems.iter().any(|e| expr_references_name(e, name)),
        ast::Expr::Set(set) => set.elems.iter().any(|e| expr_references_name(e, name)),
        ast::Expr::Tuple(tuple) => tuple.elems.iter().any(|e| expr_references_name(e, name)),
        ast::Expr::Map(map) => map.rows.iter().any(|row| {
            expr_references_name(&row.key, name) || expr_references_name(&row.value, name)
        }),
        ast::Expr::Variant(variant) => variant.tuple_payload.as_ref().is_some_and(|payload| {
            payload
                .args
                .iter()
                .any(|arg| expr_references_name(arg, name))
        }),
        ast::Expr::Record(record) => {
            record
                .fields
                .spread
                .as_ref()
                .is_some_and(|spread| expr_references_name(&spread.base, name))
                || record
                    .fields
                    .fields
                    .iter()
                    .any(|field| named_value_references_name(field, name))
        }
        ast::Expr::If(expression) => {
            expr_references_name(&expression.condition, name)
                || block_references_name(&expression.consequent, name)
                || if_branch_references_name(&expression.alternative, name)
        }
        ast::Expr::Match(expression) => {
            expr_references_name(&expression.scrutinee, name)
                || expression
                    .arms
                    .arms
                    .iter()
                    .any(|arm| match_arm_references_name(arm, name))
        }
        ast::Expr::Closure(closure) => match &closure.body {
            ast::ClosureBody::Expr(body) => expr_references_name(body, name),
            ast::ClosureBody::Block(block) => block_references_name(block, name),
        },
    }
}

fn named_args_reference_name(named: Option<&ast::WhereArgs>, name: &str) -> bool {
    named.is_some_and(|named| {
        named
            .fields
            .iter()
            .any(|field| named_value_references_name(field, name))
    })
}

fn named_value_references_name(field: &ast::NamedValue, name: &str) -> bool {
    match &field.value {
        Some(value) => expr_references_name(value, name),
        // Field punning `{ x }` references the in-scope binding `x`.
        None => field.name.value == name,
    }
}

fn block_references_name(block: &ast::Block, name: &str) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        ast::Stmt::Let(statement) => expr_references_name(&statement.value, name),
        ast::Stmt::Yield(statement) => expr_references_name(&statement.value, name),
        ast::Stmt::Expression(statement) => expr_references_name(&statement.value, name),
    }) || block
        .tail
        .as_ref()
        .is_some_and(|tail| expr_references_name(tail, name))
}

fn if_branch_references_name(branch: &ast::IfBranch, name: &str) -> bool {
    match branch {
        ast::IfBranch::Block(block) => block_references_name(block, name),
        ast::IfBranch::If(expression) => {
            expr_references_name(&expression.condition, name)
                || block_references_name(&expression.consequent, name)
                || if_branch_references_name(&expression.alternative, name)
        }
    }
}

fn match_arm_references_name(arm: &ast::MatchArm, name: &str) -> bool {
    let guard = arm
        .guard
        .as_ref()
        .is_some_and(|guard| expr_references_name(guard, name));
    guard
        || match &arm.body {
            ast::MatchArmBody::Block(block) => block_references_name(block, name),
            ast::MatchArmBody::Expr(body) => expr_references_name(body, name),
        }
}

fn lower_closure_typed(
    nodes: &mut Vec<Node>,
    outer_bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter_ty: Type,
    expected_result: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    lower_closure_typed_with_body_kind(
        nodes,
        outer_bindings,
        context,
        closure,
        parameter_ty,
        expected_result,
        ClosureBodyKind::Value,
    )
}

/// How a closure body's result value is finalized into the closure frame's
/// return.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClosureBodyKind {
    /// The body value is the closure's return.
    Value,
    /// The body is `<array>.stream()`; the closure returns the underlying dense
    /// array so it lowers as an ordinary array-returning frame. `flat_map`
    /// streams that array with fresh position keys at collection time.
    ArrayStreamSource,
    /// The body extracts a structurally ordered key `K` from the parameter; the
    /// frame returns the pair `(K, parameter)`. Structural comparison of that
    /// pair then sorts by the extracted key and breaks equal keys by the
    /// structural order of the whole source value. This is the recipe an
    /// `Order<T>` built by `by_key` carries.
    KeyedByParameter,
}

fn lower_closure_typed_with_body_kind(
    nodes: &mut Vec<Node>,
    outer_bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter_ty: Type,
    expected_result: Option<&Type>,
    body_kind: ClosureBodyKind,
) -> Result<LoweredValue, Diagnostics> {
    // A closure captures exactly the enclosing bindings its body reads free.
    // The free set is computed by a full traversal that descends through calls,
    // method calls, control flow, and nested closures, subtracting names bound
    // by the closure's own parameter patterns and by inner `let`/match/closure
    // scopes — so a name read only inside `f(f(n))` (a call) is captured, and a
    // name shadowed by an inner binder is not.
    let mut bound_by_parameters = BTreeSet::new();
    for pattern in &closure.patterns {
        collect_pattern_names(pattern, &mut bound_by_parameters);
    }
    let mut free = BTreeSet::new();
    match &closure.body {
        ast::ClosureBody::Block(block) => {
            collect_free_idents_block(block, &bound_by_parameters, &mut free);
        }
        ast::ClosureBody::Expr(expression) => {
            collect_free_idents_expr(expression, &bound_by_parameters, &mut free);
        }
    }
    let captures = free
        .into_iter()
        .filter_map(|name| outer_bindings.get(&name).map(|value| (name, value.clone())))
        .collect::<Vec<_>>();
    let (id, name) = context.allocate_closure();
    context.enter_function(name.clone());
    let lowered = (|| {
        let mut closure_nodes = Vec::new();
        let parameter_id = ParameterId(0);
        let parameter_node = push_node(
            &mut closure_nodes,
            closure.span,
            parameter_ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Parameter(parameter_id),
        );
        let parameter_value = LoweredValue {
            node: parameter_node,
            ty: parameter_ty.clone(),
        };
        let mut closure_bindings = BTreeMap::new();
        bind_closure_patterns(
            &mut closure_nodes,
            &mut closure_bindings,
            &closure.patterns,
            &parameter_value,
        )?;
        let mut parameters = vec![Parameter {
            id: parameter_id,
            node: parameter_node,
            name: "$argument".to_owned(),
            ty: parameter_ty.clone(),
            kind: ParameterKind::Positional,
        }];
        for (index, (name, captured)) in captures.iter().enumerate() {
            let id = ParameterId(u32::try_from(index + 1).expect("closure capture count fits u32"));
            let node = push_node(
                &mut closure_nodes,
                closure.span,
                captured.ty.clone(),
                EffectFacts::PURE,
                Vec::new(),
                Op::Parameter(id),
            );
            closure_bindings.insert(
                name.clone(),
                LoweredValue {
                    node,
                    ty: captured.ty.clone(),
                },
            );
            parameters.push(Parameter {
                id,
                node,
                name: format!("$capture_{name}"),
                ty: captured.ty.clone(),
                kind: ParameterKind::Positional,
            });
        }

        let output = match &closure.body {
            ast::ClosureBody::Block(block) => lower_value_block(
                &mut closure_nodes,
                &closure_bindings,
                context,
                block,
                expected_result,
            )?,
            ast::ClosureBody::Expr(expression) => lower_value_expected(
                &mut closure_nodes,
                &closure_bindings,
                context,
                expression,
                expected_result,
            )?,
        };
        if let Some(expected_result) = expected_result {
            require_type(&output, expected_result, closure.span)?;
        }
        // For an array-stream-producing closure, redirect the frame's return to
        // the dense array underneath the terminal `.stream()`, so the closure is
        // an ordinary array-returning frame.
        let (output_node, return_type) = match body_kind {
            ClosureBodyKind::Value => (output.node, output.ty.clone()),
            ClosureBodyKind::ArrayStreamSource => {
                let terminal = &closure_nodes[output.node.0 as usize];
                if !matches!(terminal.op, Op::ArrayStream) {
                    return Err(Diagnostics::one(Diagnostic::unsupported(
                        closure.span,
                        "flat_map closure body must be an array stream",
                    )));
                }
                let array_node = *terminal.inputs.first().ok_or_else(|| {
                    Diagnostics::one(Diagnostic::unsupported(
                        closure.span,
                        "array stream closure has no source array",
                    ))
                })?;
                let array_ty = closure_nodes[array_node.0 as usize].ty.clone();
                (array_node, array_ty)
            }
            ClosureBodyKind::KeyedByParameter => {
                if !output.ty.structural_order_is_defined() {
                    return Err(type_mismatch(
                        closure.span,
                        "structurally ordered key",
                        output.ty.name(),
                    ));
                }
                let pair_ty = Type::Tuple(vec![output.ty.clone(), parameter_ty.clone()]);
                let pair = push_node(
                    &mut closure_nodes,
                    closure.span,
                    pair_ty.clone(),
                    EffectFacts::PURE,
                    vec![output.node, parameter_node],
                    Op::Tuple,
                );
                (pair, pair_ty)
            }
        };
        let ty = Type::Function {
            parameter: Box::new(parameter_ty.clone()),
            result: Box::new(return_type.clone()),
        };
        Ok::<_, Diagnostics>((
            Function {
                id,
                name: name.clone(),
                span: closure.span,
                parameters,
                return_type,
                nodes: closure_nodes,
                output: Some(output_node),
                yielded_checks: Vec::new(),
            },
            ty,
        ))
    })();
    context.leave_function();
    let (function, ty) = lowered?;
    context.insert_closure(function);

    // The closure closes over the enclosing values by reference to their VIR
    // nodes; lowering decides per capture whether the environment word carries
    // it inline (a single word) or through a boxed environment (wider captures,
    // e.g. a captured callable).
    let capture_inputs = captures
        .iter()
        .map(|(_, captured)| {
            nodes
                .get(captured.node.0 as usize)
                .map(|_| captured.node)
                .ok_or_else(|| {
                    Diagnostics::one(Diagnostic::unsupported(
                        closure.span,
                        "captured value is absent",
                    ))
                })
        })
        .collect::<Result<Vec<_>, Diagnostics>>()?;

    Ok(LoweredValue {
        node: push_node(
            nodes,
            closure.span,
            ty.clone(),
            EffectFacts::PURE,
            capture_inputs,
            Op::Closure(id),
        ),
        ty,
    })
}

fn bind_closure_patterns(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    patterns: &[ast::Pattern],
    parameter: &LoweredValue,
) -> Result<(), Diagnostics> {
    if let [pattern] = patterns {
        return bind_irrefutable_pattern(nodes, bindings, pattern, parameter);
    }
    let Type::Tuple(fields) = &parameter.ty else {
        return Err(type_mismatch(
            patterns
                .first()
                .map(pattern_span)
                .unwrap_or(Span { start: 0, end: 0 }),
            format!("{} closure parameters", patterns.len()),
            parameter.ty.name(),
        ));
    };
    if fields.len() != patterns.len() {
        return Err(type_mismatch(
            patterns
                .first()
                .map(pattern_span)
                .unwrap_or(Span { start: 0, end: 0 }),
            format!("{} closure parameters", patterns.len()),
            parameter.ty.name(),
        ));
    }
    for (index, (pattern, ty)) in patterns.iter().zip(fields).enumerate() {
        let index = u32::try_from(index).map_err(|_| {
            type_mismatch(
                pattern_span(pattern),
                "closure field index",
                index.to_string(),
            )
        })?;
        let field = LoweredValue {
            node: push_node(
                nodes,
                pattern_span(pattern),
                ty.clone(),
                EffectFacts::PURE,
                vec![parameter.node],
                Op::Project { index },
            ),
            ty: ty.clone(),
        };
        bind_irrefutable_pattern(nodes, bindings, pattern, &field)?;
    }
    Ok(())
}

fn lower_if(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::IfExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let condition = lower_value(nodes, bindings, context, &expression.condition)?;
    require_type(&condition, &Type::Bool, expr_span(&expression.condition))?;

    let consequent_start = nodes.len();
    let consequent_value =
        lower_value_block(nodes, bindings, context, &expression.consequent, expected)?;
    let consequent = control_region(nodes, consequent_start, consequent_value.node);

    let alternative_start = nodes.len();
    let alternative_expected = expected.or(Some(&consequent_value.ty));
    let alternative_value = match &expression.alternative {
        ast::IfBranch::Block(block) => {
            lower_value_block(nodes, bindings, context, block, alternative_expected)?
        }
        ast::IfBranch::If(expression) => {
            lower_if(nodes, bindings, context, expression, alternative_expected)?
        }
    };
    require_type(
        &alternative_value,
        &consequent_value.ty,
        if_branch_span(&expression.alternative),
    )?;
    let alternative = control_region(nodes, alternative_start, alternative_value.node);

    let ty = consequent_value.ty;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            expression.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![condition.node],
            Op::If {
                consequent,
                alternative,
            },
        ),
        ty,
    })
}

fn lower_value_block(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    block: &ast::Block,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let mut bindings = bindings.clone();
    for statement in &block.stmts {
        match statement {
            ast::Stmt::Expression(statement) => {
                return Err(expression_statement_diagnostic(statement.span));
            }
            ast::Stmt::Yield(statement) => {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    statement.span,
                    "yield inside a value block",
                )));
            }
            ast::Stmt::Let(statement) => {
                lower_let_statement(nodes, &mut bindings, context, statement)?;
            }
        }
    }
    let tail = block.tail.as_ref().ok_or_else(|| {
        Diagnostics::one(Diagnostic::unsupported(
            block.span,
            "value block without a tail value",
        ))
    })?;
    lower_value_expected(nodes, &bindings, context, tail, expected)
}

fn lower_match_arm_body(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    body: &ast::MatchArmBody,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    match body {
        ast::MatchArmBody::Block(block) => {
            lower_value_block(nodes, bindings, context, block, expected)
        }
        ast::MatchArmBody::Expr(expression) => {
            lower_value_expected(nodes, bindings, context, expression, expected)
        }
    }
}

fn match_arm_body_span(body: &ast::MatchArmBody) -> Span {
    match body {
        ast::MatchArmBody::Block(block) => block.span,
        ast::MatchArmBody::Expr(expression) => expr_span(expression),
    }
}

fn if_branch_span(branch: &ast::IfBranch) -> Span {
    match branch {
        ast::IfBranch::Block(block) => block.span,
        ast::IfBranch::If(expression) => expression.span,
    }
}

fn control_region(nodes: &[Node], start: usize, output: NodeId) -> ControlRegion {
    ControlRegion {
        nodes: nodes[start..].iter().map(|node| node.id).collect(),
        output,
    }
}

fn lower_unary_value(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    unary: &ast::Unary,
) -> Result<LoweredValue, Diagnostics> {
    match unary.op.value.as_str() {
        "!" => {
            let value = lower_value(nodes, bindings, context, &unary.value)?;
            require_type(&value, &Type::Bool, expr_span(&unary.value))?;
            let false_node = push_node(
                nodes,
                unary.op.span,
                Type::Bool,
                EffectFacts::PURE,
                Vec::new(),
                Op::Bool(false),
            );
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    unary.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![value.node, false_node],
                    Op::Eq,
                ),
                ty: Type::Bool,
            })
        }
        "-" => {
            let ast::Expr::Number(number) = &unary.value else {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    unary.op.span,
                    "unary operator `-`",
                )));
            };
            lower_integer_literal(nodes, unary.span, &format!("-{}", number.value))
        }
        _ => Err(Diagnostics::one(Diagnostic::unsupported(
            unary.op.span,
            format!("unary operator `{}`", unary.op.value),
        ))),
    }
}

fn lower_integer_literal(
    nodes: &mut Vec<Node>,
    span: Span,
    literal: &str,
) -> Result<LoweredValue, Diagnostics> {
    let value = literal
        .parse::<i64>()
        .map_err(|_| type_mismatch(span, "Int", format!("number literal `{literal}`")))?;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            Type::Int,
            EffectFacts::PURE,
            Vec::new(),
            Op::Int(value),
        ),
        ty: Type::Int,
    })
}

fn lower_named_record_values(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    owner: &str,
    declared_fields: &[RecordField],
    supplied: &ast::RecordValueList,
    spread_type: Option<&Type>,
) -> Result<Vec<NodeId>, Diagnostics> {
    let mut provided = BTreeMap::new();
    for field in &supplied.fields {
        if provided.insert(field.name.value.clone(), field).is_some() {
            return Err(field_diagnostic(
                DiagnosticCode::DuplicateField,
                field.name.span,
                owner,
                &field.name.value,
            ));
        }
    }

    let spread_base = match (&supplied.spread, spread_type) {
        (None, _) => None,
        (Some(spread), Some(expected)) => {
            let base = lower_value(nodes, bindings, context, &spread.base)?;
            require_type(&base, expected, spread.span)?;
            Some((spread, base))
        }
        (Some(spread), None) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                spread.span,
                "record-variant spread",
            )));
        }
    };

    let mut inputs = Vec::with_capacity(declared_fields.len());
    for (index, declared) in declared_fields.iter().enumerate() {
        let node = if let Some(field) = provided.remove(&declared.name) {
            let value = if let Some(expression) = &field.value {
                lower_value_expected(nodes, bindings, context, expression, Some(&declared.ty))?
            } else {
                lookup_binding(bindings, &field.name.value, field.name.span)?
            };
            require_type(&value, &declared.ty, field.span)?;
            value.node
        } else if let Some((spread, base)) = &spread_base {
            let index = u32::try_from(index).map_err(|_| {
                type_mismatch(spread.span, "aggregate field index", index.to_string())
            })?;
            push_node(
                nodes,
                spread.span,
                declared.ty.clone(),
                EffectFacts::PURE,
                vec![base.node],
                Op::Project { index },
            )
        } else {
            return Err(field_diagnostic(
                DiagnosticCode::MissingField,
                supplied.span,
                owner,
                &declared.name,
            ));
        };
        inputs.push(node);
    }
    if let Some((name, field)) = provided.into_iter().next() {
        return Err(field_diagnostic(
            DiagnosticCode::UnknownField,
            field.name.span,
            owner,
            &name,
        ));
    }
    Ok(inputs)
}

fn lower_named_constructor(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::RecordExpr,
) -> Result<LoweredValue, Diagnostics> {
    let qualified_name = path_name(&expression.ty);
    if let Some(ty) = context.types.get(&qualified_name) {
        let Type::Record(record) = ty else {
            return Err(type_mismatch(expression.ty.span, "record type", ty.name()));
        };
        let inputs = lower_named_record_values(
            nodes,
            bindings,
            context,
            &qualified_name,
            &record.fields,
            &expression.fields,
            Some(ty),
        )?;
        let ty = Type::Record(record.clone());
        return Ok(LoweredValue {
            node: push_node(
                nodes,
                expression.span,
                ty.clone(),
                EffectFacts::PURE,
                inputs,
                Op::Record,
            ),
            ty,
        });
    }

    let Some((variant_name, owner_segments)) = expression.ty.segments.split_last() else {
        return Err(unknown_name(expression.ty.span, qualified_name));
    };
    if owner_segments.is_empty() {
        return Err(unknown_name(expression.ty.span, qualified_name));
    }
    let enumeration_name = owner_segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join("::");
    let enumeration = context
        .types
        .get(&enumeration_name)
        .ok_or_else(|| unknown_name(expression.ty.span, &qualified_name))?;
    let Type::Enum(enumeration) = enumeration else {
        return Err(type_mismatch(
            expression.ty.span,
            "enum type",
            enumeration.name(),
        ));
    };
    let (variant_index, variant) = enumeration
        .variants
        .iter()
        .enumerate()
        .find(|(_, variant)| variant.name == variant_name.value)
        .ok_or_else(|| {
            variant_diagnostic(
                DiagnosticCode::UnknownVariant,
                variant_name.span,
                &enumeration.name,
                &variant_name.value,
            )
        })?;
    let VariantPayload::Record(fields) = &variant.payload else {
        return Err(variant_diagnostic(
            DiagnosticCode::VariantPayloadMismatch,
            expression.span,
            &enumeration.name,
            &variant.name,
        ));
    };
    let inputs = lower_named_record_values(
        nodes,
        bindings,
        context,
        &format!("{}::{}", enumeration.name, variant.name),
        fields,
        &expression.fields,
        None,
    )?;
    let variant_index = u32::try_from(variant_index).map_err(|_| {
        variant_diagnostic(
            DiagnosticCode::VariantPayloadMismatch,
            expression.span,
            &enumeration.name,
            &variant.name,
        )
    })?;
    let ty = Type::Enum(enumeration.clone());
    Ok(LoweredValue {
        node: push_node(
            nodes,
            expression.span,
            ty.clone(),
            EffectFacts::PURE,
            inputs,
            Op::Variant {
                variant: variant_index,
            },
        ),
        ty,
    })
}

fn lower_variant(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::VariantExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let enumeration = if let Some(expected) = expected {
        let Type::Enum(enumeration) = expected else {
            return Err(type_mismatch(
                expression.span,
                expected.name(),
                format!("{} enum constructor", expression.path.type_name.value),
            ));
        };
        if nominal_base_name(&enumeration.name) != expression.path.type_name.value {
            return Err(type_mismatch(
                expression.path.type_name.span,
                nominal_base_name(&enumeration.name),
                &expression.path.type_name.value,
            ));
        }
        enumeration
    } else {
        let ty = context
            .types
            .get(&expression.path.type_name.value)
            .ok_or_else(|| {
                unknown_name(
                    expression.path.type_name.span,
                    &expression.path.type_name.value,
                )
            })?;
        let Type::Enum(enumeration) = ty else {
            return Err(type_mismatch(
                expression.path.type_name.span,
                "enum type",
                ty.name(),
            ));
        };
        enumeration
    };
    let (variant_index, variant) = find_variant(enumeration, &expression.path)?;
    let inputs = match (&variant.payload, &expression.tuple_payload) {
        (VariantPayload::Unit, None) => Vec::new(),
        (VariantPayload::Tuple(types), Some(arguments)) => {
            if types.len() != arguments.args.len() {
                return Err(invalid_arity(
                    arguments.span,
                    types.len(),
                    arguments.args.len(),
                ));
            }
            let mut inputs = Vec::with_capacity(types.len());
            for (expected, argument) in types.iter().zip(&arguments.args) {
                let value =
                    lower_value_expected(nodes, bindings, context, argument, Some(expected))?;
                require_type(&value, expected, expr_span(argument))?;
                inputs.push(value.node);
            }
            inputs
        }
        _ => {
            return Err(variant_diagnostic(
                DiagnosticCode::VariantPayloadMismatch,
                expression.span,
                &enumeration.name,
                &variant.name,
            ));
        }
    };
    let variant_index = u32::try_from(variant_index).map_err(|_| {
        variant_diagnostic(
            DiagnosticCode::VariantPayloadMismatch,
            expression.span,
            &enumeration.name,
            &variant.name,
        )
    })?;
    let ty = Type::Enum(enumeration.clone());
    Ok(LoweredValue {
        node: push_node(
            nodes,
            expression.span,
            ty.clone(),
            EffectFacts::PURE,
            inputs,
            Op::Variant {
                variant: variant_index,
            },
        ),
        ty,
    })
}

fn lower_match(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::MatchExpr,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let scrutinee = lower_value(nodes, bindings, context, &expression.scrutinee)?;
    let enumeration = match &scrutinee.ty {
        Type::Enum(enumeration)
            if expression
                .arms
                .arms
                .iter()
                .all(|arm| enum_pattern(&arm.pattern).is_some() && arm.guard.is_none()) =>
        {
            Some(enumeration.clone())
        }
        _ => None,
    };
    if let Some(enumeration) = enumeration {
        return lower_enum_match(
            nodes,
            bindings,
            context,
            expression,
            scrutinee,
            enumeration,
            expected,
        );
    }
    lower_ordered_match(nodes, bindings, context, expression, scrutinee, expected)
}

fn lower_enum_match(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::MatchExpr,
    scrutinee: LoweredValue,
    enumeration: EnumType,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let mut seen = BTreeSet::new();
    let mut arms = Vec::with_capacity(expression.arms.arms.len());
    let mut result_type = None;

    for arm in &expression.arms.arms {
        let pattern =
            enum_pattern(&arm.pattern).expect("enum match shape was checked by lower_match");
        let (variant_index, variant, variant_span) =
            find_enum_pattern_variant(&enumeration, pattern)?;
        if !seen.insert(variant_index) {
            return Err(variant_diagnostic(
                DiagnosticCode::DuplicateVariant,
                variant_span,
                &enumeration.name,
                &variant.name,
            ));
        }
        let first_arm_node = nodes.len();
        let mut arm_bindings = bindings.clone();
        bind_enum_pattern(
            nodes,
            &mut arm_bindings,
            &scrutinee,
            &enumeration,
            variant_index,
            variant,
            pattern,
        )?;
        let arm_expected = expected.or(result_type.as_ref());
        let output = lower_match_arm_body(nodes, &arm_bindings, context, &arm.body, arm_expected)?;
        if let Some(expected) = &result_type {
            require_type(&output, expected, match_arm_body_span(&arm.body))?;
        } else {
            result_type = Some(output.ty.clone());
        }
        let arm_nodes = (first_arm_node..nodes.len())
            .map(|index| {
                u32::try_from(index)
                    .map(NodeId)
                    .map_err(|_| type_mismatch(arm.span, "VIR node index", index.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        arms.push(VirMatchArm {
            variant: u32::try_from(variant_index).map_err(|_| {
                variant_diagnostic(
                    DiagnosticCode::VariantPayloadMismatch,
                    enum_pattern_span(pattern),
                    &enumeration.name,
                    &variant.name,
                )
            })?,
            nodes: arm_nodes,
            output: output.node,
        });
    }

    let missing = enumeration
        .variants
        .iter()
        .enumerate()
        .filter(|(index, _)| !seen.contains(index))
        .map(|(_, variant)| variant.name.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::NonExhaustiveMatch,
            primary: expression.arms.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Match { missing },
        }));
    }
    let ty = result_type.ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::NonExhaustiveMatch,
            primary: expression.arms.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Match {
                missing: enumeration
                    .variants
                    .iter()
                    .map(|variant| variant.name.clone())
                    .collect(),
            },
        })
    })?;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            expression.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::Match { arms },
        ),
        ty,
    })
}

fn lower_ordered_match(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::MatchExpr,
    scrutinee: LoweredValue,
    expected: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    let mut arms = Vec::new();
    let mut fallback = None;
    let mut result_type = None;

    for arm in &expression.arms.arms {
        if fallback.is_some() {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                arm.span,
                "match arm after an irrefutable pattern",
            )));
        }

        let mut arm_bindings = bindings.clone();
        let condition_start = nodes.len();
        let pattern_condition =
            lower_ordered_pattern(nodes, &mut arm_bindings, &scrutinee, &arm.pattern)?;
        let condition = match (pattern_condition, &arm.guard) {
            (Some(pattern), Some(guard)) => Some(lower_pattern_guard(
                nodes,
                &arm_bindings,
                context,
                pattern,
                guard,
            )?),
            (Some(pattern), None) => Some(pattern),
            (None, Some(guard)) => {
                let guard_span = expr_span(guard);
                let guard = lower_value(nodes, &arm_bindings, context, guard)?;
                require_type(&guard, &Type::Bool, guard_span)?;
                Some(guard)
            }
            (None, None) => None,
        }
        .map(|condition| control_region(nodes, condition_start, condition.node));

        let body_start = nodes.len();
        let arm_expected = expected.or(result_type.as_ref());
        let output = lower_match_arm_body(nodes, &arm_bindings, context, &arm.body, arm_expected)?;
        if let Some(expected) = &result_type {
            require_type(&output, expected, match_arm_body_span(&arm.body))?;
        } else {
            result_type = Some(output.ty.clone());
        }
        if let Some(condition) = condition {
            arms.push(OrderedMatchArm {
                condition,
                body: control_region(nodes, body_start, output.node),
            });
        } else {
            fallback = Some(control_region(nodes, condition_start, output.node));
        }
    }

    let fallback = fallback.ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::NonExhaustiveMatch,
            primary: expression.arms.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Match {
                missing: vec!["_".to_owned()],
            },
        })
    })?;
    let ty = result_type.ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::NonExhaustiveMatch,
            primary: expression.arms.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Match {
                missing: vec!["_".to_owned()],
            },
        })
    })?;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            expression.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::OrderedMatch { arms, fallback },
        ),
        ty,
    })
}

fn lower_ordered_pattern(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    scrutinee: &LoweredValue,
    pattern: &ast::Pattern,
) -> Result<Option<LoweredValue>, Diagnostics> {
    match pattern {
        ast::Pattern::Binding(pattern) => {
            bind_name(bindings, scrutinee, &pattern.binding)?;
            Ok(None)
        }
        ast::Pattern::Record(pattern) => {
            let Type::Record(record) = &scrutinee.ty else {
                return Err(type_mismatch(
                    pattern.span,
                    path_name(&pattern.ty),
                    scrutinee.ty.name(),
                ));
            };
            require_record_pattern_owner(pattern, record)?;
            let selected =
                select_record_pattern_fields(&pattern.fields, &record.fields, &record.name)?;
            let mut condition = None;
            for (index, declared, field) in selected {
                if matches!(field.pattern, Some(ast::Pattern::Wildcard(_))) {
                    continue;
                }
                let fragment_start = nodes.len();
                let projected = project_record_field(
                    nodes,
                    scrutinee,
                    index,
                    &declared.ty,
                    pattern_field_span(field),
                )?;
                let fragment = if let Some(field_pattern) = &field.pattern {
                    lower_ordered_pattern(nodes, bindings, &projected, field_pattern)?
                } else {
                    bind_name(bindings, &projected, &field.name)?;
                    None
                };
                condition = append_pattern_condition(
                    nodes,
                    pattern_field_span(field),
                    condition,
                    fragment_start,
                    fragment,
                );
            }
            Ok(condition)
        }
        ast::Pattern::Number(pattern) => {
            require_type(scrutinee, &Type::Int, pattern.span)?;
            let literal = lower_integer_literal(nodes, pattern.span, &pattern.value.value)?;
            Ok(Some(LoweredValue {
                node: push_node(
                    nodes,
                    pattern.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![scrutinee.node, literal.node],
                    Op::Eq,
                ),
                ty: Type::Bool,
            }))
        }
        ast::Pattern::Str(pattern) => {
            require_type(scrutinee, &Type::String, pattern.span)?;
            let literal = LoweredValue {
                node: push_node(
                    nodes,
                    pattern.span,
                    Type::String,
                    EffectFacts::PURE,
                    Vec::new(),
                    Op::String(pattern.value.value.clone()),
                ),
                ty: Type::String,
            };
            Ok(Some(LoweredValue {
                node: push_node(
                    nodes,
                    pattern.span,
                    Type::Bool,
                    EffectFacts::PURE,
                    vec![scrutinee.node, literal.node],
                    Op::Eq,
                ),
                ty: Type::Bool,
            }))
        }
        ast::Pattern::Wildcard(_) => Ok(None),
        ast::Pattern::Some(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "guarded option pattern",
        ))),
        ast::Pattern::Ok(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "guarded result pattern",
        ))),
        ast::Pattern::Err(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "guarded result pattern",
        ))),
        ast::Pattern::None(span) => Err(Diagnostics::one(Diagnostic::unsupported(
            *span,
            "guarded option pattern",
        ))),
        ast::Pattern::Variant(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "guarded enum pattern",
        ))),
        ast::Pattern::Tuple(pattern) => {
            let Type::Tuple(elements) = &scrutinee.ty else {
                return Err(type_mismatch(pattern.span, "tuple", scrutinee.ty.name()));
            };
            if pattern.elems.len() != elements.len() {
                return Err(type_mismatch(
                    pattern.span,
                    format!("tuple pattern with {} elements", elements.len()),
                    format!("tuple pattern with {} elements", pattern.elems.len()),
                ));
            }

            let elements = elements.clone();
            let mut condition = None;
            for (index, (element, ty)) in pattern.elems.iter().zip(elements).enumerate() {
                if matches!(element, ast::Pattern::Wildcard(_)) {
                    continue;
                }
                let fragment_start = nodes.len();
                let index = u32::try_from(index).map_err(|_| {
                    type_mismatch(pattern.span, "tuple field index", index.to_string())
                })?;
                let projected = LoweredValue {
                    node: push_node(
                        nodes,
                        pattern_span(element),
                        ty.clone(),
                        EffectFacts::PURE,
                        vec![scrutinee.node],
                        Op::Project { index },
                    ),
                    ty,
                };
                let fragment = lower_ordered_pattern(nodes, bindings, &projected, element)?;
                condition = append_pattern_condition(
                    nodes,
                    pattern_span(element),
                    condition,
                    fragment_start,
                    fragment,
                );
            }
            Ok(condition)
        }
    }
}

fn append_pattern_condition(
    nodes: &mut Vec<Node>,
    span: Span,
    accumulated: Option<LoweredValue>,
    fragment_start: usize,
    fragment: Option<LoweredValue>,
) -> Option<LoweredValue> {
    let Some(accumulated) = accumulated else {
        return fragment;
    };
    if fragment.is_none() && fragment_start == nodes.len() {
        return Some(accumulated);
    }

    let consequent_value = fragment.unwrap_or_else(|| lower_bool_constant(nodes, span, true));
    let consequent = control_region(nodes, fragment_start, consequent_value.node);
    let alternative_start = nodes.len();
    let otherwise = lower_bool_constant(nodes, span, false);
    let alternative = control_region(nodes, alternative_start, otherwise.node);
    Some(LoweredValue {
        node: push_node(
            nodes,
            span,
            Type::Bool,
            EffectFacts::PURE,
            vec![accumulated.node],
            Op::If {
                consequent,
                alternative,
            },
        ),
        ty: Type::Bool,
    })
}

fn pattern_span(pattern: &ast::Pattern) -> Span {
    match pattern {
        ast::Pattern::Some(pattern) => pattern.span,
        ast::Pattern::None(span) => *span,
        ast::Pattern::Ok(pattern) => pattern.span,
        ast::Pattern::Err(pattern) => pattern.span,
        ast::Pattern::Record(pattern) => pattern.span,
        ast::Pattern::Variant(pattern) => pattern.span,
        ast::Pattern::Binding(pattern) => pattern.span,
        ast::Pattern::Str(pattern) => pattern.span,
        ast::Pattern::Number(pattern) => pattern.span,
        ast::Pattern::Wildcard(span) => *span,
        ast::Pattern::Tuple(pattern) => pattern.span,
    }
}

// r[impl lang.pattern.record]
fn require_record_pattern_owner(
    pattern: &ast::RecordPattern,
    record: &RecordType,
) -> Result<(), Diagnostics> {
    let supplied = path_name(&pattern.ty);
    if supplied == record.name {
        Ok(())
    } else {
        Err(type_mismatch(
            pattern.ty.span,
            record.name.clone(),
            supplied,
        ))
    }
}

fn select_record_pattern_fields<'pattern, 'declared>(
    pattern: &'pattern ast::RecordPatternFields,
    declared: &'declared [RecordField],
    owner: &str,
) -> Result<Vec<(usize, &'declared RecordField, &'pattern ast::PatternField)>, Diagnostics> {
    let mut supplied = BTreeMap::new();
    for field in &pattern.fields {
        if supplied.insert(field.name.value.clone(), field).is_some() {
            return Err(field_diagnostic(
                DiagnosticCode::DuplicateField,
                field.name.span,
                owner,
                &field.name.value,
            ));
        }
    }

    let mut selected = Vec::with_capacity(supplied.len());
    for (index, field) in declared.iter().enumerate() {
        if let Some(pattern) = supplied.remove(&field.name) {
            selected.push((index, field, pattern));
        } else if pattern.rest.is_none() {
            return Err(field_diagnostic(
                DiagnosticCode::MissingField,
                pattern.span,
                owner,
                &field.name,
            ));
        }
    }
    if let Some((name, field)) = supplied.into_iter().next() {
        return Err(field_diagnostic(
            DiagnosticCode::UnknownField,
            field.name.span,
            owner,
            &name,
        ));
    }
    Ok(selected)
}

fn pattern_field_span(field: &ast::PatternField) -> Span {
    field
        .pattern
        .as_ref()
        .map(pattern_span)
        .unwrap_or(field.name.span)
}

fn project_record_field(
    nodes: &mut Vec<Node>,
    scrutinee: &LoweredValue,
    field: usize,
    ty: &Type,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    let field = u32::try_from(field)
        .map_err(|_| type_mismatch(span, "record field index", field.to_string()))?;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            ty.clone(),
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::Project { index: field },
        ),
        ty: ty.clone(),
    })
}

fn bind_name(
    bindings: &mut BTreeMap<String, LoweredValue>,
    value: &LoweredValue,
    binding: &Spanned<String>,
) -> Result<(), Diagnostics> {
    if bindings.contains_key(&binding.value) {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::DuplicateBinding,
            primary: binding.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: binding.value.clone(),
            },
        }));
    }
    bindings.insert(binding.value.clone(), value.clone());
    Ok(())
}

fn lower_pattern_guard(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    pattern: LoweredValue,
    guard: &ast::Expr,
) -> Result<LoweredValue, Diagnostics> {
    let guard_span = expr_span(guard);
    let consequent_start = nodes.len();
    let guard_value = lower_value(nodes, bindings, context, guard)?;
    require_type(&guard_value, &Type::Bool, guard_span)?;
    let consequent = control_region(nodes, consequent_start, guard_value.node);
    let alternative_start = nodes.len();
    let otherwise = lower_bool_constant(nodes, guard_span, false);
    let alternative = control_region(nodes, alternative_start, otherwise.node);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            guard_span,
            Type::Bool,
            EffectFacts::PURE,
            vec![pattern.node],
            Op::If {
                consequent,
                alternative,
            },
        ),
        ty: Type::Bool,
    })
}

fn find_variant<'a>(
    enumeration: &'a EnumType,
    path: &ast::VariantPath,
) -> Result<(usize, &'a EnumVariant), Diagnostics> {
    let owner = nominal_base_name(&enumeration.name);
    if path.type_name.value != owner {
        return Err(type_mismatch(
            path.type_name.span,
            owner,
            &path.type_name.value,
        ));
    }
    enumeration
        .variants
        .iter()
        .enumerate()
        .find(|(_, variant)| variant.name == path.variant.value)
        .ok_or_else(|| {
            variant_diagnostic(
                DiagnosticCode::UnknownVariant,
                path.variant.span,
                &enumeration.name,
                &path.variant.value,
            )
        })
}

#[derive(Clone, Copy)]
enum EnumPattern<'a> {
    Some(&'a ast::SomePattern),
    None(Span),
    Ok(&'a ast::OkPattern),
    Err(&'a ast::ErrPattern),
    Variant(&'a ast::VariantPattern),
    Record(&'a ast::RecordPattern),
}

fn enum_pattern(pattern: &ast::Pattern) -> Option<EnumPattern<'_>> {
    match pattern {
        ast::Pattern::Some(pattern) => Some(EnumPattern::Some(pattern)),
        ast::Pattern::None(span) => Some(EnumPattern::None(*span)),
        ast::Pattern::Ok(pattern) => Some(EnumPattern::Ok(pattern)),
        ast::Pattern::Err(pattern) => Some(EnumPattern::Err(pattern)),
        ast::Pattern::Variant(pattern) => Some(EnumPattern::Variant(pattern)),
        ast::Pattern::Record(pattern) => Some(EnumPattern::Record(pattern)),
        _ => None,
    }
}

fn enum_pattern_span(pattern: EnumPattern<'_>) -> Span {
    match pattern {
        EnumPattern::Some(pattern) => pattern.span,
        EnumPattern::None(span) => span,
        EnumPattern::Ok(pattern) => pattern.span,
        EnumPattern::Err(pattern) => pattern.span,
        EnumPattern::Variant(pattern) => pattern.span,
        EnumPattern::Record(pattern) => pattern.span,
    }
}

fn find_enum_pattern_variant<'a>(
    enumeration: &'a EnumType,
    pattern: EnumPattern<'_>,
) -> Result<(usize, &'a EnumVariant, Span), Diagnostics> {
    match pattern {
        EnumPattern::Some(pattern) => {
            if enumeration.option_inner().is_none() {
                return Err(type_mismatch(
                    pattern.span,
                    "Option<_>",
                    enumeration.name.clone(),
                ));
            }
            Ok((
                OPTION_SOME_VARIANT as usize,
                &enumeration.variants[OPTION_SOME_VARIANT as usize],
                pattern.span,
            ))
        }
        EnumPattern::None(span) => {
            if enumeration.option_inner().is_none() {
                return Err(type_mismatch(span, "Option<_>", enumeration.name.clone()));
            }
            Ok((
                OPTION_NONE_VARIANT as usize,
                &enumeration.variants[OPTION_NONE_VARIANT as usize],
                span,
            ))
        }
        EnumPattern::Ok(pattern) => {
            if enumeration.result_inner().is_none() {
                return Err(type_mismatch(
                    pattern.span,
                    "Result<_, _>",
                    enumeration.name.clone(),
                ));
            }
            Ok((
                RESULT_OK_VARIANT as usize,
                &enumeration.variants[RESULT_OK_VARIANT as usize],
                pattern.span,
            ))
        }
        EnumPattern::Err(pattern) => {
            if enumeration.result_inner().is_none() {
                return Err(type_mismatch(
                    pattern.span,
                    "Result<_, _>",
                    enumeration.name.clone(),
                ));
            }
            Ok((
                RESULT_ERR_VARIANT as usize,
                &enumeration.variants[RESULT_ERR_VARIANT as usize],
                pattern.span,
            ))
        }
        EnumPattern::Variant(pattern) => {
            let (index, variant) = find_variant(enumeration, &pattern.path)?;
            Ok((index, variant, pattern.path.variant.span))
        }
        EnumPattern::Record(pattern) => {
            let Some((variant_name, owner)) = pattern.ty.segments.split_last() else {
                return Err(type_mismatch(
                    pattern.ty.span,
                    enumeration.name.clone(),
                    path_name(&pattern.ty),
                ));
            };
            let owner = owner
                .iter()
                .map(|segment| segment.value.as_str())
                .collect::<Vec<_>>()
                .join("::");
            if owner != enumeration.name {
                return Err(type_mismatch(
                    pattern.ty.span,
                    enumeration.name.clone(),
                    path_name(&pattern.ty),
                ));
            }
            enumeration
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name == variant_name.value)
                .map(|(index, variant)| (index, variant, variant_name.span))
                .ok_or_else(|| {
                    variant_diagnostic(
                        DiagnosticCode::UnknownVariant,
                        variant_name.span,
                        &enumeration.name,
                        &variant_name.value,
                    )
                })
        }
    }
}

fn bind_enum_pattern(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    scrutinee: &LoweredValue,
    enumeration: &EnumType,
    variant_index: usize,
    variant: &EnumVariant,
    pattern: EnumPattern<'_>,
) -> Result<(), Diagnostics> {
    match (pattern, &variant.payload) {
        (EnumPattern::Some(pattern), VariantPayload::Tuple(types)) if types.len() == 1 => {
            let projected = project_variant_field(
                nodes,
                scrutinee,
                variant_index,
                0,
                &types[0],
                pattern_span(&pattern.payload),
            )?;
            bind_irrefutable_pattern(nodes, bindings, &pattern.payload, &projected)
        }
        (EnumPattern::None(_), VariantPayload::Unit) => Ok(()),
        (EnumPattern::Ok(pattern), VariantPayload::Tuple(types)) if types.len() == 1 => {
            let projected = project_variant_field(
                nodes,
                scrutinee,
                variant_index,
                0,
                &types[0],
                pattern_span(&pattern.payload),
            )?;
            bind_irrefutable_pattern(nodes, bindings, &pattern.payload, &projected)
        }
        (EnumPattern::Err(pattern), VariantPayload::Tuple(types)) if types.len() == 1 => {
            let projected = project_variant_field(
                nodes,
                scrutinee,
                variant_index,
                0,
                &types[0],
                pattern_span(&pattern.payload),
            )?;
            bind_irrefutable_pattern(nodes, bindings, &pattern.payload, &projected)
        }
        (EnumPattern::Variant(pattern), VariantPayload::Unit)
            if pattern.tuple_payload.is_none() =>
        {
            Ok(())
        }
        (EnumPattern::Variant(pattern), VariantPayload::Tuple(types)) => {
            let Some(tuple) = &pattern.tuple_payload else {
                return Err(variant_diagnostic(
                    DiagnosticCode::VariantPayloadMismatch,
                    pattern.span,
                    &enumeration.name,
                    &variant.name,
                ));
            };
            if types.len() != tuple.elems.len() {
                return Err(invalid_arity(tuple.span, types.len(), tuple.elems.len()));
            }
            for (field, (ty, element)) in types.iter().zip(&tuple.elems).enumerate() {
                let projected = project_variant_field(
                    nodes,
                    scrutinee,
                    variant_index,
                    field,
                    ty,
                    pattern_span(element),
                )?;
                bind_irrefutable_pattern(nodes, bindings, element, &projected)?;
            }
            Ok(())
        }
        (EnumPattern::Record(pattern), VariantPayload::Record(fields)) => {
            let owner = format!("{}::{}", enumeration.name, variant.name);
            for (field_index, declared, field) in
                select_record_pattern_fields(&pattern.fields, fields, &owner)?
            {
                let projected = project_variant_field(
                    nodes,
                    scrutinee,
                    variant_index,
                    field_index,
                    &declared.ty,
                    pattern_field_span(field),
                )?;
                if let Some(field_pattern) = &field.pattern {
                    bind_irrefutable_pattern(nodes, bindings, field_pattern, &projected)?;
                } else {
                    bind_name(bindings, &projected, &field.name)?;
                }
            }
            Ok(())
        }
        _ => Err(variant_diagnostic(
            DiagnosticCode::VariantPayloadMismatch,
            enum_pattern_span(pattern),
            &enumeration.name,
            &variant.name,
        )),
    }
}

fn project_variant_field(
    nodes: &mut Vec<Node>,
    scrutinee: &LoweredValue,
    variant: usize,
    field: usize,
    ty: &Type,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    let variant = u32::try_from(variant)
        .map_err(|_| type_mismatch(span, "variant index", variant.to_string()))?;
    let field = u32::try_from(field)
        .map_err(|_| type_mismatch(span, "variant field index", field.to_string()))?;
    Ok(LoweredValue {
        node: push_node(
            nodes,
            span,
            ty.clone(),
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::VariantProject { variant, field },
        ),
        ty: ty.clone(),
    })
}

/// Lower a subject-less named call `callee where { ... }`. The only such
/// builtin today is `range where { from, to } -> [Int]`, the dense half-open
/// integer array construct.
fn lower_where_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::WhereCall,
) -> Result<LoweredValue, Diagnostics> {
    if call.callee.value != "range" {
        return Err(unknown_name(call.callee.span, &call.callee.value));
    }
    let from = named_field_value(&call.named_args, "from")?;
    let to = named_field_value(&call.named_args, "to")?;
    if call.named_args.fields.len() != 2 {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.named_args.span,
            "range accepts exactly the named bounds `from` and `to`",
        )));
    }
    let from = lower_value_expected(nodes, bindings, context, from, Some(&Type::Int))?;
    require_type(
        &from,
        &Type::Int,
        expr_span_of_named(&call.named_args, "from"),
    )?;
    let to = lower_value_expected(nodes, bindings, context, to, Some(&Type::Int))?;
    require_type(&to, &Type::Int, expr_span_of_named(&call.named_args, "to"))?;
    let ty = Type::array(Type::Int);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            ty.clone(),
            EffectFacts {
                fallible: true,
                ..EffectFacts::PURE
            },
            vec![from.node, to.node],
            Op::Range,
        ),
        ty,
    })
}

/// The value expression of one named argument, resolving field punning
/// (`{ from }`) to the identifier of the same name.
fn named_field_value<'a>(
    named: &'a ast::WhereArgs,
    name: &str,
) -> Result<&'a ast::Expr, Diagnostics> {
    let field = named
        .fields
        .iter()
        .find(|field| field.name.value == name)
        .ok_or_else(|| {
            Diagnostics::one(Diagnostic::unsupported(
                named.span,
                format!("range requires the named bound `{name}`"),
            ))
        })?;
    match &field.value {
        Some(value) => Ok(value),
        // Field punning `{ from }` denotes the in-scope binding `from`.
        None => Err(Diagnostics::one(Diagnostic::unsupported(
            field.span,
            "range bounds must be explicit `name: value` arguments",
        ))),
    }
}

fn expr_span_of_named(named: &ast::WhereArgs, name: &str) -> Span {
    named
        .fields
        .iter()
        .find(|field| field.name.value == name)
        .and_then(|field| field.value.as_ref())
        .map_or(named.span, expr_span)
}

/// Resolve a command template's capability tag. The tag is an ordinary value
/// reference — a root injects a capability value or a solve returns one; there
/// is no ambient tool namespace to fall back to — so a tag that names nothing
/// is an unbound identifier, not a special capability error.
///
/// r[impl machine.primitive.capabilities-by-identity]
fn resolve_command_capability(
    bindings: &BTreeMap<String, LoweredValue>,
    command: &ast::CommandExpr,
) -> Result<LoweredValue, Diagnostics> {
    bindings.get(&command.tag.value).cloned().ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnboundIdentifier,
            primary: command.tag.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: command.tag.value.clone(),
            },
        })
    })
}

/// A bare command template names a `Command<A>` value. Constructing one
/// without demanding it awaits a later rung; the capability tag still resolves
/// first so an undeclared tool fails as an unbound identifier.
fn lower_command(
    _nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    _context: &ModuleContext<'_>,
    command: &ast::CommandExpr,
) -> Result<LoweredValue, Diagnostics> {
    let _capability = resolve_command_capability(bindings, command)?;
    Err(Diagnostics::one(Diagnostic::unsupported(
        command.span,
        "a command value outside `exec`",
    )))
}

/// The ratchet capability packages' command grammar: whitespace-separated
/// argv elements; a double-quoted element keeps interior whitespace and drops
/// its quotes. The band's templates are fully literal (no `{expr}` splices
/// yet), so the parse is closed at compile time and the argv enters the
/// canonical recipe.
///
/// r[impl lang.command.typed]
fn parse_command_template(template: &Spanned<String>) -> Result<Vec<String>, Diagnostics> {
    let text = template
        .value
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(&template.value);
    let mut argv = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&next) = chars.peek() else {
            break;
        };
        if next == '"' {
            chars.next();
            let mut element = String::new();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some(ch) => element.push(ch),
                    None => {
                        return Err(Diagnostics::one(Diagnostic::unsupported(
                            template.span,
                            "unterminated quoted command argument",
                        )));
                    }
                }
            }
            argv.push(element);
        } else {
            let mut element = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                element.push(ch);
                chars.next();
            }
            argv.push(element);
        }
    }
    Ok(argv)
}

/// `exec command` — an effect demand. The capability value is the node's only
/// input, so its identity enters the demand preimage; the parsed argv enters
/// the canonical recipe. The result is the `ExecOutcome` value; a nonzero exit
/// is a typed language failure at this node's site.
fn lower_exec(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    _context: &ModuleContext<'_>,
    exec: &ast::ExecExpr,
) -> Result<LoweredValue, Diagnostics> {
    let capability = resolve_command_capability(bindings, &exec.command)?;
    if !is_capability_type(&capability.ty) {
        return Err(type_mismatch(
            exec.command.tag.span,
            "a capability value",
            capability.ty.name(),
        ));
    }
    let argv = parse_command_template(&exec.command.template)?;
    let ty = exec_outcome_type();
    Ok(LoweredValue {
        node: push_node(
            nodes,
            exec.span,
            ty.clone(),
            EffectFacts::EFFECT,
            vec![capability.node],
            Op::Exec { argv },
        ),
        ty,
    })
}

fn lower_try(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    try_expr: &ast::TryExpr,
) -> Result<LoweredValue, Diagnostics> {
    let operand = lower_value(nodes, bindings, context, &try_expr.value)?;
    let ty = Type::Enum(EnumType::result(
        operand.ty.clone(),
        EnumType::failure_value(),
    ));
    Ok(LoweredValue {
        node: push_node(
            nodes,
            try_expr.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![operand.node],
            Op::Try,
        ),
        ty,
    })
}

fn lower_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<LoweredValue, Diagnostics> {
    if let Some(callee) = bindings.get(&call.callee.value) {
        return lower_value_call(nodes, bindings, context, call, callee.clone());
    }
    let signature = context
        .signatures
        .get(&call.callee.value)
        .ok_or_else(|| unknown_name(call.callee.span, &call.callee.value))?;
    lower_direct_call(nodes, bindings, context, call, signature)
}

fn lower_direct_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
    signature: &FunctionSignature,
) -> Result<LoweredValue, Diagnostics> {
    if signature.is_test {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "calling a test function",
        )));
    }

    let positional = signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == ParameterKind::Positional)
        .collect::<Vec<_>>();
    if call.args.args.len() != positional.len() {
        return Err(invalid_arity(
            call.span,
            positional.len(),
            call.args.args.len(),
        ));
    }

    let mut inputs = Vec::with_capacity(signature.parameters.len());
    for (parameter, argument) in positional.into_iter().zip(&call.args.args) {
        let value = lower_value_expected(nodes, bindings, context, argument, Some(&parameter.ty))?;
        require_type(&value, &parameter.ty, expr_span(argument))?;
        inputs.push(value.node);
    }

    let mut named_values = BTreeMap::new();
    if let Some(named_args) = &call.named_args {
        for field in &named_args.fields {
            if named_values
                .insert(field.name.value.clone(), field)
                .is_some()
            {
                return Err(Diagnostics::one(Diagnostic {
                    code: DiagnosticCode::DuplicateBinding,
                    primary: field.name.span,
                    labels: Vec::new(),
                    payload: DiagnosticPayload::Name {
                        name: field.name.value.clone(),
                    },
                }));
            }
        }
    }

    for parameter in signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == ParameterKind::Named)
    {
        let field = named_values.remove(&parameter.name).ok_or_else(|| {
            invalid_arity(
                call.span,
                signature.parameters.len(),
                inputs.len() + named_values.len(),
            )
        })?;
        let value = if let Some(expression) = &field.value {
            lower_value_expected(nodes, bindings, context, expression, Some(&parameter.ty))?
        } else {
            lookup_binding(bindings, &field.name.value, field.name.span)?
        };
        require_type(&value, &parameter.ty, field.span)?;
        inputs.push(value.node);
    }

    if let Some((name, field)) = named_values.into_iter().next() {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownName,
            primary: field.name.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name { name },
        }));
    }

    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            signature.return_type.clone(),
            EffectFacts::PURE,
            inputs,
            Op::Call(signature.id),
        ),
        ty: signature.return_type.clone(),
    })
}

fn lower_value_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
    callee: LoweredValue,
) -> Result<LoweredValue, Diagnostics> {
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a function value",
        )));
    }
    let Type::Function { parameter, result } = &callee.ty else {
        return Err(type_mismatch(
            call.callee.span,
            "callable value",
            callee.ty.name(),
        ));
    };
    if call.args.args.len() != 1 {
        return Err(invalid_arity(call.span, 1, call.args.args.len()));
    }
    let argument = lower_value_expected(
        nodes,
        bindings,
        context,
        &call.args.args[0],
        Some(parameter),
    )?;
    require_type(&argument, parameter, expr_span(&call.args.args[0]))?;
    let result = result.as_ref().clone();
    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            result.clone(),
            EffectFacts::PURE,
            vec![callee.node, argument.node],
            Op::CallValue,
        ),
        ty: result,
    })
}

/// Resolve a bare identifier that names a top-level function to a first-class
/// callable value. A function reference is only well-typed as `fn(T) -> U` when
/// the function declares exactly one positional parameter and no named ones.
fn lower_function_reference(
    nodes: &mut Vec<Node>,
    context: &ModuleContext<'_>,
    identifier: &crate::support::Spanned<String>,
) -> Result<LoweredValue, Diagnostics> {
    let signature = context
        .signatures
        .get(&identifier.value)
        .ok_or_else(|| unknown_name(identifier.span, &identifier.value))?;
    if signature.is_test {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            identifier.span,
            "referencing a test function as a value",
        )));
    }
    let [parameter] = signature.parameters.as_slice() else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            identifier.span,
            "referencing a function with other than one parameter as a value",
        )));
    };
    if parameter.kind != ParameterKind::Positional {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            identifier.span,
            "referencing a function with a named parameter as a value",
        )));
    }
    let ty = Type::Function {
        parameter: Box::new(parameter.ty.clone()),
        result: Box::new(signature.return_type.clone()),
    };
    Ok(LoweredValue {
        node: push_node(
            nodes,
            identifier.span,
            ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
            Op::Closure(signature.id),
        ),
        ty,
    })
}

fn lookup_binding(
    bindings: &BTreeMap<String, LoweredValue>,
    name: &str,
    span: Span,
) -> Result<LoweredValue, Diagnostics> {
    bindings.get(name).cloned().ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownName,
            primary: span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: name.to_owned(),
            },
        })
    })
}

fn lower_binary(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    binary: &ast::Binary,
) -> Result<LoweredValue, Diagnostics> {
    let left = lower_value(nodes, bindings, context, &binary.left)?;
    match binary.op.value.as_str() {
        "&&" => {
            return lower_short_circuit_boolean(
                nodes,
                bindings,
                context,
                binary,
                left,
                BooleanOperator::And,
            );
        }
        "||" => {
            return lower_short_circuit_boolean(
                nodes,
                bindings,
                context,
                binary,
                left,
                BooleanOperator::Or,
            );
        }
        _ => {}
    }
    let right = lower_value(nodes, bindings, context, &binary.right)?;
    let (ty, op) = match binary.op.value.as_str() {
        "+" => match &left.ty {
            Type::Int => {
                require_type(&right, &Type::Int, expr_span(&binary.right))?;
                (Type::Int, Op::Add)
            }
            Type::Array(element) => {
                require_type(&right, element, expr_span(&binary.right))?;
                (left.ty.clone(), Op::ArrayAppend)
            }
            Type::String => {
                require_type(&right, &Type::String, expr_span(&binary.right))?;
                (Type::String, Op::StringConcat)
            }
            Type::Map { key, value } => {
                require_type(
                    &right,
                    &Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]),
                    expr_span(&binary.right),
                )?;
                (left.ty.clone(), Op::MapAdd)
            }
            Type::Set(element) => {
                require_type(&right, element, expr_span(&binary.right))?;
                (left.ty.clone(), Op::SetAdd)
            }
            _ => {
                return Err(type_mismatch(
                    expr_span(&binary.left),
                    "Int or collection value",
                    left.ty.name(),
                ));
            }
        },
        "++" => match &left.ty {
            Type::Array(_) => {
                require_type(&right, &left.ty, expr_span(&binary.right))?;
                (left.ty.clone(), Op::ArrayConcat)
            }
            Type::Map { .. } => {
                require_type(&right, &left.ty, expr_span(&binary.right))?;
                (left.ty.clone(), Op::MapConcat)
            }
            Type::Set(_) => {
                require_type(&right, &left.ty, expr_span(&binary.right))?;
                (left.ty.clone(), Op::SetConcat)
            }
            _ => {
                return Err(type_mismatch(
                    expr_span(&binary.left),
                    "collection value",
                    left.ty.name(),
                ));
            }
        },
        "-" => {
            require_type(&left, &Type::Int, expr_span(&binary.left))?;
            require_type(&right, &Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Sub)
        }
        "*" => {
            require_type(&left, &Type::Int, expr_span(&binary.left))?;
            require_type(&right, &Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Mul)
        }
        "/" if matches!(
            left.ty,
            Type::Extern(ExternKind::Tree) | Type::Extern(ExternKind::TreeEntry)
        ) =>
        {
            // Tree projection: one Name segment (a string literal) or a Path
            // (projection through several maps). The projection resolves
            // lazily; undemanded siblings are never read.
            let projector = match &right.ty {
                Type::String => {
                    let literal = nodes
                        .iter()
                        .find(|node| node.id == right.node)
                        .and_then(|node| match &node.op {
                            Op::String(value) => Some(value.clone()),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            Diagnostics::one(Diagnostic::unsupported(
                                expr_span(&binary.right),
                                "dynamic tree Name segments",
                            ))
                        })?;
                    validate_path_segment(&literal, expr_span(&binary.right))?;
                    right.node
                }
                Type::Path => right.node,
                _ => {
                    return Err(type_mismatch(
                        expr_span(&binary.right),
                        "a Name segment literal or Path",
                        right.ty.name(),
                    ));
                }
            };
            let ty = Type::Extern(ExternKind::TreeEntry);
            return Ok(LoweredValue {
                node: push_node(
                    nodes,
                    binary.span,
                    ty.clone(),
                    EffectFacts::EFFECT,
                    vec![left.node, projector],
                    Op::TreeProject,
                ),
                ty,
            });
        }
        "/" if left.ty == Type::Path => {
            let ast::Expr::Str(segment) = &binary.right else {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    expr_span(&binary.right),
                    "dynamic Path segments",
                )));
            };
            require_type(&right, &Type::String, segment.span)?;
            validate_path_segment(segment.value.as_str(), segment.span)?;
            let suffix = push_node(
                nodes,
                segment.span,
                Type::Path,
                EffectFacts::PURE,
                Vec::new(),
                Op::Path(segment.value.clone()),
            );
            let joined = push_node(
                nodes,
                binary.span,
                Type::Path,
                EffectFacts::PURE,
                vec![left.node, suffix],
                Op::PathJoin,
            );
            return Ok(LoweredValue {
                node: joined,
                ty: Type::Path,
            });
        }
        "/" => {
            require_type(&left, &Type::Int, expr_span(&binary.left))?;
            require_type(&right, &Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Div)
        }
        "==" => {
            require_same_type(&left, &right, binary.span)?;
            let node = push_equality_condition(nodes, binary.span, &left, &right, false);
            return Ok(LoweredValue {
                node,
                ty: Type::Bool,
            });
        }
        "!=" => {
            require_same_type(&left, &right, binary.span)?;
            let node = push_equality_condition(nodes, binary.span, &left, &right, true);
            return Ok(LoweredValue {
                node,
                ty: Type::Bool,
            });
        }
        "<=>" => {
            require_same_type(&left, &right, binary.span)?;
            if !left.ty.structural_order_is_defined() {
                return Err(type_mismatch(
                    binary.span,
                    "structurally ordered value",
                    left.ty.name(),
                ));
            }
            (Type::ordering(), Op::Compare)
        }
        "<" => {
            return lower_derived_relation(nodes, binary, left, right, DerivedRelation::Less);
        }
        "<=" => {
            return lower_derived_relation(
                nodes,
                binary,
                left,
                right,
                DerivedRelation::LessOrEqual,
            );
        }
        ">" => {
            return lower_derived_relation(nodes, binary, left, right, DerivedRelation::Greater);
        }
        ">=" => {
            return lower_derived_relation(
                nodes,
                binary,
                left,
                right,
                DerivedRelation::GreaterOrEqual,
            );
        }
        _ => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                binary.op.span,
                format!("binary operator `{}`", binary.op.value),
            )));
        }
    };
    let effect = if matches!(op, Op::MapAdd | Op::MapConcat) {
        EffectFacts {
            fallible: true,
            ..EffectFacts::PURE
        }
    } else {
        EffectFacts::PURE
    };
    Ok(LoweredValue {
        node: push_node(
            nodes,
            binary.span,
            ty.clone(),
            effect,
            vec![left.node, right.node],
            op,
        ),
        ty,
    })
}

#[derive(Clone, Copy)]
enum BooleanOperator {
    And,
    Or,
}

fn lower_short_circuit_boolean(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    binary: &ast::Binary,
    left: LoweredValue,
    operator: BooleanOperator,
) -> Result<LoweredValue, Diagnostics> {
    require_type(&left, &Type::Bool, expr_span(&binary.left))?;

    let consequent_start = nodes.len();
    let consequent_value = match operator {
        BooleanOperator::And => {
            let right = lower_value(nodes, bindings, context, &binary.right)?;
            require_type(&right, &Type::Bool, expr_span(&binary.right))?;
            right
        }
        BooleanOperator::Or => lower_bool_constant(nodes, binary.op.span, true),
    };
    let consequent = control_region(nodes, consequent_start, consequent_value.node);

    let alternative_start = nodes.len();
    let alternative_value = match operator {
        BooleanOperator::And => lower_bool_constant(nodes, binary.op.span, false),
        BooleanOperator::Or => {
            let right = lower_value(nodes, bindings, context, &binary.right)?;
            require_type(&right, &Type::Bool, expr_span(&binary.right))?;
            right
        }
    };
    let alternative = control_region(nodes, alternative_start, alternative_value.node);

    Ok(LoweredValue {
        node: push_node(
            nodes,
            binary.span,
            Type::Bool,
            EffectFacts::PURE,
            vec![left.node],
            Op::If {
                consequent,
                alternative,
            },
        ),
        ty: Type::Bool,
    })
}

fn lower_bool_constant(nodes: &mut Vec<Node>, span: Span, value: bool) -> LoweredValue {
    LoweredValue {
        node: push_node(
            nodes,
            span,
            Type::Bool,
            EffectFacts::PURE,
            Vec::new(),
            Op::Bool(value),
        ),
        ty: Type::Bool,
    }
}

/// Build the equality condition for two same-typed values.
///
/// Ordered collections have structural value identity, but their VIR handles
/// are opaque: two maps built by different recipes (a `collect` and a literal)
/// share no handle. Structural equality desugars to comparison of the canonical
/// projections that already lower through the verified machine — a map is equal
/// iff its canonical key array and canonical value array both match, a set iff
/// its canonical element array matches. Scalars and structural aggregates keep
/// the direct `Op::Eq`/`Op::Ne` primitive. `negate` requests the `!=` sense.
fn push_equality_condition(
    nodes: &mut Vec<Node>,
    span: Span,
    left: &LoweredValue,
    right: &LoweredValue,
    negate: bool,
) -> NodeId {
    if left.ty.equality_is_structural() {
        return push_node(
            nodes,
            span,
            Type::Bool,
            EffectFacts::PURE,
            vec![left.node, right.node],
            if negate { Op::Ne } else { Op::Eq },
        );
    }
    let equal = push_structural_equality(nodes, span, left.node, right.node, &left.ty);
    if negate {
        let alternative = lower_bool_constant(nodes, span, false);
        push_eq(nodes, span, equal, alternative.node)
    } else {
        equal
    }
}

/// One Bool node deciding `left == right` for `ty`, decomposed at compile time
/// when the type's in-frame equality is not one structural word walk. Maps and
/// Sets compare their canonical key/value array projections; a non-structural
/// enum compares variant tags and, only inside the both-same-variant region,
/// each payload field recursively; non-structural tuples and records compare
/// field by field with short-circuiting conjunction. Structural types remain a
/// single `Op::Eq`. Every guarded projection lives inside its `If` region, so a
/// mismatched variant never projects a payload and a failed prefix never
/// compares a later field. This builds an ordinary pure recipe — no new
/// execution primitive, island, or demand shape.
fn push_structural_equality(
    nodes: &mut Vec<Node>,
    span: Span,
    left: NodeId,
    right: NodeId,
    ty: &Type,
) -> NodeId {
    if ty.equality_is_structural() {
        return push_eq(nodes, span, left, right);
    }
    match ty {
        Type::Map { key, value } => {
            let key_array = Type::array(key.as_ref().clone());
            let value_array = Type::array(value.as_ref().clone());
            let keys_left = push_project(nodes, span, left, key_array.clone(), Op::MapKeys);
            let keys_right = push_project(nodes, span, right, key_array.clone(), Op::MapKeys);
            let keys_equal =
                push_structural_equality(nodes, span, keys_left, keys_right, &key_array);
            // The value projections and their comparison form the `then` region
            // of a short-circuiting `keys_equal && values_equal`.
            let values_start = nodes.len();
            let values_left = push_project(nodes, span, left, value_array.clone(), Op::MapValues);
            let values_right = push_project(nodes, span, right, value_array.clone(), Op::MapValues);
            let values_equal =
                push_structural_equality(nodes, span, values_left, values_right, &value_array);
            push_guarded_equality(nodes, span, keys_equal, values_start, values_equal)
        }
        Type::Set(element) => {
            let element_array = Type::array(element.as_ref().clone());
            let left_values = push_project(nodes, span, left, element_array.clone(), Op::SetValues);
            let right_values =
                push_project(nodes, span, right, element_array.clone(), Op::SetValues);
            push_structural_equality(nodes, span, left_values, right_values, &element_array)
        }
        Type::Enum(enumeration) => {
            let enumeration = enumeration.clone();
            push_enum_equality(nodes, span, left, right, &enumeration, 0)
        }
        Type::Tuple(elements) => {
            let elements = elements.clone();
            push_field_conjunction(nodes, span, &elements, 0, &mut |nodes, index, ty| {
                (
                    push_project(nodes, span, left, ty.clone(), Op::Project { index }),
                    push_project(nodes, span, right, ty.clone(), Op::Project { index }),
                )
            })
        }
        Type::Record(record) => {
            let fields: Vec<Type> = record.fields.iter().map(|field| field.ty.clone()).collect();
            push_field_conjunction(nodes, span, &fields, 0, &mut |nodes, index, ty| {
                (
                    push_project(nodes, span, left, ty.clone(), Op::Project { index }),
                    push_project(nodes, span, right, ty.clone(), Op::Project { index }),
                )
            })
        }
        // Functions, streams, and other non-value shapes keep today's typed
        // lowering boundary rather than inventing an equality for them.
        _ => push_eq(nodes, span, left, right),
    }
}

/// `if condition { <consequent region from `consequent_start`> } else { false }`
/// — the short-circuit conjunction step every decomposed equality uses. The
/// consequent region is captured before the `false` constant is pushed, so the
/// guarded projections belong to exactly one region.
fn push_guarded_equality(
    nodes: &mut Vec<Node>,
    span: Span,
    condition: NodeId,
    consequent_start: usize,
    consequent_output: NodeId,
) -> NodeId {
    let consequent = control_region(nodes, consequent_start, consequent_output);
    let alternative_start = nodes.len();
    let alternative_value = lower_bool_constant(nodes, span, false);
    let alternative = control_region(nodes, alternative_start, alternative_value.node);
    push_node(
        nodes,
        span,
        Type::Bool,
        EffectFacts::PURE,
        vec![condition],
        Op::If {
            consequent,
            alternative,
        },
    )
}

/// Variant-tag chain for a non-structural enum equality: for each variant, when
/// the left value holds it, the right value must hold it too and the payloads
/// must compare equal inside that doubly-guarded region. The final variant needs
/// no left-tag test — exhaustiveness makes it the only remaining case.
fn push_enum_equality(
    nodes: &mut Vec<Node>,
    span: Span,
    left: NodeId,
    right: NodeId,
    enumeration: &EnumType,
    index: usize,
) -> NodeId {
    let variant = u32::try_from(index).expect("variant index fits u32");
    let last = index + 1 == enumeration.variants.len();
    let both_here = |nodes: &mut Vec<Node>| -> NodeId {
        let right_is = push_node(
            nodes,
            span,
            Type::Bool,
            EffectFacts::PURE,
            vec![right],
            Op::IsVariant { variant },
        );
        let payload_start = nodes.len();
        let payload_equal = push_variant_payload_equality(
            nodes,
            span,
            left,
            right,
            variant,
            &enumeration.variants[index].payload,
        );
        push_guarded_equality(nodes, span, right_is, payload_start, payload_equal)
    };
    if last {
        return both_here(nodes);
    }
    let left_is = push_node(
        nodes,
        span,
        Type::Bool,
        EffectFacts::PURE,
        vec![left],
        Op::IsVariant { variant },
    );
    let consequent_start = nodes.len();
    let here = both_here(nodes);
    let consequent = control_region(nodes, consequent_start, here);
    let alternative_start = nodes.len();
    let rest = push_enum_equality(nodes, span, left, right, enumeration, index + 1);
    let alternative = control_region(nodes, alternative_start, rest);
    push_node(
        nodes,
        span,
        Type::Bool,
        EffectFacts::PURE,
        vec![left_is],
        Op::If {
            consequent,
            alternative,
        },
    )
}

/// Field-wise payload comparison for one shared variant. Both values are known
/// to hold `variant` where this is emitted, so each `VariantProject` is safe. A
/// unit payload is trivially equal.
fn push_variant_payload_equality(
    nodes: &mut Vec<Node>,
    span: Span,
    left: NodeId,
    right: NodeId,
    variant: u32,
    payload: &VariantPayload,
) -> NodeId {
    let field_types: Vec<Type> = match payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Tuple(fields) => fields.clone(),
        VariantPayload::Record(fields) => fields.iter().map(|field| field.ty.clone()).collect(),
    };
    push_field_conjunction(nodes, span, &field_types, 0, &mut |nodes, field, ty| {
        (
            push_project(
                nodes,
                span,
                left,
                ty.clone(),
                Op::VariantProject { variant, field },
            ),
            push_project(
                nodes,
                span,
                right,
                ty.clone(),
                Op::VariantProject { variant, field },
            ),
        )
    })
}

type FieldProject<'a> = dyn FnMut(&mut Vec<Node>, u32, &Type) -> (NodeId, NodeId) + 'a;

/// Short-circuit conjunction over per-field equalities: field `i + 1` is
/// projected and compared only inside the region where field `i` already
/// compared equal. An empty field list is `true`.
fn push_field_conjunction(
    nodes: &mut Vec<Node>,
    span: Span,
    field_types: &[Type],
    index: usize,
    project: &mut FieldProject<'_>,
) -> NodeId {
    let Some(ty) = field_types.get(index) else {
        return lower_bool_constant(nodes, span, true).node;
    };
    let field = u32::try_from(index).expect("field index fits u32");
    let (left, right) = project(nodes, field, ty);
    let field_equal = push_structural_equality(nodes, span, left, right, ty);
    if index + 1 == field_types.len() {
        return field_equal;
    }
    let rest_start = nodes.len();
    let rest = push_field_conjunction(nodes, span, field_types, index + 1, project);
    push_guarded_equality(nodes, span, field_equal, rest_start, rest)
}

fn push_project(nodes: &mut Vec<Node>, span: Span, source: NodeId, ty: Type, op: Op) -> NodeId {
    push_node(nodes, span, ty, EffectFacts::PURE, vec![source], op)
}

fn push_eq(nodes: &mut Vec<Node>, span: Span, left: NodeId, right: NodeId) -> NodeId {
    push_node(
        nodes,
        span,
        Type::Bool,
        EffectFacts::PURE,
        vec![left, right],
        Op::Eq,
    )
}

#[derive(Clone, Copy)]
enum DerivedRelation {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

fn lower_derived_relation(
    nodes: &mut Vec<Node>,
    binary: &ast::Binary,
    left: LoweredValue,
    right: LoweredValue,
    relation: DerivedRelation,
) -> Result<LoweredValue, Diagnostics> {
    require_same_type(&left, &right, binary.span)?;
    if !left.ty.structural_order_is_defined() {
        return Err(type_mismatch(
            binary.span,
            "structurally ordered value",
            left.ty.name(),
        ));
    }
    let (variant, relation) = match relation {
        DerivedRelation::Less => (ORDERING_LESS_VARIANT, Op::Eq),
        DerivedRelation::LessOrEqual => (ORDERING_GREATER_VARIANT, Op::Ne),
        DerivedRelation::Greater => (ORDERING_GREATER_VARIANT, Op::Eq),
        DerivedRelation::GreaterOrEqual => (ORDERING_LESS_VARIANT, Op::Ne),
    };
    let ordering = Type::ordering();
    let compared = push_node(
        nodes,
        binary.span,
        ordering.clone(),
        EffectFacts::PURE,
        vec![left.node, right.node],
        Op::Compare,
    );
    let expected = push_node(
        nodes,
        binary.op.span,
        ordering,
        EffectFacts::PURE,
        Vec::new(),
        Op::Variant { variant },
    );
    Ok(LoweredValue {
        node: push_node(
            nodes,
            binary.span,
            Type::Bool,
            EffectFacts::PURE,
            vec![compared, expected],
            relation,
        ),
        ty: Type::Bool,
    })
}

fn check_arity(call: &ast::Call, expected: usize) -> Result<(), Diagnostics> {
    let found = call.args.args.len();
    if found != expected {
        return Err(invalid_arity(call.span, expected, found));
    }
    Ok(())
}

fn invalid_arity(span: Span, expected: usize, found: usize) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::InvalidArity,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Arity {
            expected: u32::try_from(expected).expect("arity fits u32"),
            found: u32::try_from(found).expect("arity fits u32"),
        },
    })
}

fn require_type(value: &LoweredValue, expected: &Type, span: Span) -> Result<(), Diagnostics> {
    if &value.ty != expected {
        let mut diagnostic = type_mismatch(span, expected.name(), value.ty.name());
        if matches!((expected, &value.ty), (Type::Path, Type::String)) {
            diagnostic.entries[0].code = DiagnosticCode::StringIsNotPath;
        }
        return Err(diagnostic);
    }
    Ok(())
}

fn require_same_type(
    left: &LoweredValue,
    right: &LoweredValue,
    span: Span,
) -> Result<(), Diagnostics> {
    if left.ty != right.ty {
        return Err(type_mismatch(span, left.ty.name(), right.ty.name()));
    }
    Ok(())
}

fn type_mismatch(span: Span, expected: impl Into<String>, found: impl Into<String>) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::TypeMismatch,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Type {
            expected: expected.into(),
            found: found.into(),
        },
    })
}

fn push_node(
    nodes: &mut Vec<Node>,
    span: Span,
    ty: Type,
    effect: EffectFacts,
    inputs: Vec<NodeId>,
    op: Op,
) -> NodeId {
    let id = NodeId(nodes.len() as u32);
    nodes.push(Node {
        id,
        span,
        ty,
        effect,
        inputs,
        op,
    });
    id
}

fn expr_span(expression: &ast::Expr) -> Span {
    match expression {
        ast::Expr::Closure(value) => value.span,
        ast::Expr::If(value) => value.span,
        ast::Expr::Match(value) => value.span,
        ast::Expr::Binary(value) => value.span,
        ast::Expr::Unary(value) => value.span,
        ast::Expr::Call(value) => value.span,
        ast::Expr::WhereCall(value) => value.span,
        ast::Expr::MethodCall(value) => value.span,
        ast::Expr::Field(value) => value.span,
        ast::Expr::Index(value) => value.span,
        ast::Expr::Array(value) => value.span,
        ast::Expr::Map(value) => value.span,
        ast::Expr::Set(value) => value.span,
        ast::Expr::Variant(value) => value.span,
        ast::Expr::Record(value) => value.span,
        ast::Expr::Tuple(value) => value.span,
        ast::Expr::Paren(value) => value.span,
        ast::Expr::Exec(value) => value.span,
        ast::Expr::Command(value) => value.span,
        ast::Expr::Try(value) => value.span,
        ast::Expr::Identifier(value) => value.span,
        ast::Expr::Path(value) => value.span,
        ast::Expr::Str(value) => value.span,
        ast::Expr::Number(value) => value.span,
        ast::Expr::Quantity(value) => value.span,
        ast::Expr::Bool(value) => value.span,
    }
}

fn type_span(ty: &ast::Type) -> Span {
    match ty {
        ast::Type::Function(value) => value.span,
        ast::Type::Array(value) => value.span,
        ast::Type::Generic(value) => value.span,
        ast::Type::Tuple(value) => value.span,
        ast::Type::Path(value) => value.span,
    }
}
