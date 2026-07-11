//! Surface AST checking and lowering to Vix IR.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics, Label};
use crate::support::{Span, Spanned};
use crate::surface::{SurfaceParser, ast};
use crate::vir::{
    ArrayMapGrain, ArrayMapGrainKey, ControlRegion, EffectFacts, EnumType, EnumVariant, Function,
    FunctionId, GeneratorArm, GeneratorBody, GeneratorStep, MatchArm as VirMatchArm, Module, Node,
    NodeId, OPTION_NONE_VARIANT, OPTION_SOME_VARIANT, ORDERING_GREATER_VARIANT,
    ORDERING_LESS_VARIANT, Op, OrderedMatchArm, Parameter, ParameterId, ParameterKind, RecordField,
    RecordType, Test, Type, VariantPayload, YieldSite, YieldSiteId,
};

pub struct Compiler {
    parser: SurfaceParser,
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
        }
    }

    /// Parse, check, and lower to architecture-neutral VIR.
    pub fn compile(&self, source: &str) -> Result<Compilation, Diagnostics> {
        let ast = self.parser.parse(source)?;
        let module = lower_module(&ast)?;
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
                ast::Item::Fn(_) => continue,
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
                for argument in &expression.args.args {
                    self.resolve_expr_types(argument)?;
                }
                if let Some(named) = &expression.named_args {
                    self.resolve_named_value_types(&named.fields)?;
                }
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
            ast::Expr::Identifier(_)
            | ast::Expr::Str(_)
            | ast::Expr::Number(_)
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
            ast::Type::Path(path) if path_is(path, "Check") => Ok(Type::Check),
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

fn lower_module(source: &ast::SourceFile) -> Result<Module, Diagnostics> {
    let types = TypeResolver::new(source)?.resolve_all(source)?;
    let declared_type_names = source
        .items
        .iter()
        .filter_map(|item| match item {
            ast::Item::Struct(record) => Some(record.name.value.as_str()),
            ast::Item::Enum(enumeration) => Some(enumeration.name.value.as_str()),
            ast::Item::Fn(_) => None,
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
    };
    let mut module = Module {
        records: source
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Struct(record) => match types.get(&record.name.value) {
                    Some(Type::Record(record)) => Some(record.clone()),
                    _ => None,
                },
                ast::Item::Enum(_) | ast::Item::Fn(_) => None,
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
        if diagnostic.code != DiagnosticCode::NonExhaustiveMatch {
            continue;
        }
        let match_span = diagnostic.primary;
        diagnostic.primary = function.name.span;
        diagnostic.labels.push(Label {
            span: match_span,
            text: "non-exhaustive match occurs here".to_owned(),
        });
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
        check_test_signature(function)?;
        return Ok(FunctionSignature {
            id,
            is_test,
            parameters: Vec::new(),
            return_type: Type::StreamCheck,
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

fn check_test_signature(function: &ast::FnItem) -> Result<(), Diagnostics> {
    let valid_return = function
        .return_type
        .as_ref()
        .is_some_and(is_stream_check_type);
    if !function.params.params.is_empty()
        || function.where_params.is_some()
        || !valid_return
        || function.generics.is_some()
    {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::InvalidTestSignature,
            primary: function.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Type {
                expected: "fn() -> Stream<Check>".to_owned(),
                found: function.name.value.clone(),
            },
        }));
    }
    Ok(())
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
                    if top_level {
                        push_node(
                            nodes,
                            statement.span,
                            Type::StreamCheck,
                            EffectFacts::CODATA,
                            vec![site.check],
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

/// Lower one leaf `yield <check>` into a static [`YieldSite`]. The check is a
/// pure parameterized recipe (`Op::Expect` over captured values), never an
/// evaluated boolean or a host call.
fn lower_yield_check_site(
    nodes: &mut Vec<Node>,
    yielded_checks: &mut Vec<NodeId>,
    site_counter: &mut u32,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    value: &ast::Expr,
    span: Span,
) -> Result<YieldSite, Diagnostics> {
    let check = lower_check(nodes, bindings, context, value)?;
    yielded_checks.push(check);
    let id = YieldSiteId(*site_counter);
    *site_counter = site_counter
        .checked_add(1)
        .expect("yield site count fits u32");
    Ok(YieldSite { id, check, span })
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
) -> Result<NodeId, Diagnostics> {
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
    if call.named_args.is_some() {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.span,
            "named arguments on a check constructor",
        )));
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
    Ok(push_node(
        nodes,
        call.span,
        Type::Check,
        EffectFacts::PURE,
        vec![condition],
        Op::Expect,
    ))
}

#[derive(Clone)]
struct LoweredValue {
    node: NodeId,
    ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreludeReceiverType {
    Array,
    Map,
    Set,
    Stream,
    Int,
}

impl PreludeReceiverType {
    fn from_vir_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(_) => Some(Self::Array),
            Type::Map { .. } => Some(Self::Map),
            Type::Set(_) => Some(Self::Set),
            Type::Stream { .. } => Some(Self::Stream),
            Type::Int => Some(Self::Int),
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
        ast::Expr::Call(call) if call.callee.value == "Some" => {
            lower_some(nodes, bindings, context, call, expected)
        }
        ast::Expr::Call(call) => lower_call(nodes, bindings, context, call),
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
        ast::Expr::MethodCall(call) => lower_method_call(nodes, bindings, context, call),
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

fn lower_method_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::MethodCall,
) -> Result<LoweredValue, Diagnostics> {
    let receiver = lower_value(nodes, bindings, context, &call.receiver)?;
    let Some(entry) = PreludeMethodRegistry::STANDARD.resolve(&receiver.ty, &call.name.value)
    else {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownMethod,
            primary: call.name.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: call.name.value.clone(),
            },
        }));
    };
    if call.args.args.len() != entry.arity {
        return Err(invalid_arity(call.span, entry.arity, call.args.args.len()));
    }
    if let Some(named) = &call.named_args {
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
            let mapper = match &call.args.args[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, element)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &mapper.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> _", element.name()),
                    mapper.ty.name(),
                ));
            };
            if parameter.as_ref() != element.as_ref() {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
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
            let initial = lower_value(nodes, bindings, context, &call.args.args[0])?;
            let parameter_ty = Type::Tuple(vec![initial.ty.clone(), element.as_ref().clone()]);
            let folder = match &call.args.args[1] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, &parameter_ty)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &folder.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[1]),
                    format!("fn({}) -> {}", parameter_ty.name(), initial.ty.name()),
                    folder.ty.name(),
                ));
            };
            if parameter.as_ref() != &parameter_ty || result.as_ref() != &initial.ty {
                return Err(type_mismatch(
                    expr_span(&call.args.args[1]),
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
        PreludeMethod::ArrayAll | PreludeMethod::ArrayAny => {
            let Type::Array(element) = &receiver.ty else {
                unreachable!("array predicate registry entry has an array receiver")
            };
            let predicate = match &call.args.args[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, element)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &predicate.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Bool", element.name()),
                    predicate.ty.name(),
                ));
            };
            if parameter.as_ref() != element.as_ref() || result.as_ref() != &Type::Bool {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
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
            let value = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&value, &element, expr_span(&call.args.args[0]))?;
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
            let divisor = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&divisor, &Type::Int, expr_span(&call.args.args[0]))?;
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
            let lowered_key = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&lowered_key, &key, expr_span(&call.args.args[0]))?;
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
                    let lowered_value = lower_value(nodes, bindings, context, &call.args.args[1])?;
                    require_type(&lowered_value, &value, expr_span(&call.args.args[1]))?;
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
            let candidate = lower_value(nodes, bindings, context, &call.args.args[0])?;
            require_type(&candidate, &element, expr_span(&call.args.args[0]))?;
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
            let predicate = match &call.args.args[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, value)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &predicate.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Bool", value.name()),
                    predicate.ty.name(),
                ));
            };
            if parameter.as_ref() != value || result.as_ref() != &Type::Bool {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
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
            let transform = match &call.args.args[0] {
                ast::Expr::Closure(closure) => {
                    lower_closure_with_parameter(nodes, bindings, context, closure, &value)?
                }
                expression => lower_value_expected(nodes, bindings, context, expression, None)?,
            };
            let Type::Function { parameter, result } = &transform.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Option<_>", value.name()),
                    transform.ty.name(),
                ));
            };
            if parameter.as_ref() != &value {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Option<_>", value.name()),
                    transform.ty.name(),
                ));
            }
            let output = result.option_inner().ok_or_else(|| {
                type_mismatch(
                    expr_span(&call.args.args[0]),
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
            let ast::Expr::Closure(closure) = &call.args.args[0] else {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    expr_span(&call.args.args[0]),
                    "flat_map expects a closure returning an array stream",
                )));
            };
            // The closure is lowered as an array-returning frame `fn(V) -> [W]`;
            // its inner stream keys are the dense positions of that array.
            let transform = lower_array_stream_closure(nodes, bindings, context, closure, &value)?;
            let Type::Function { parameter, result } = &transform.ty else {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Stream<_, _>", value.name()),
                    transform.ty.name(),
                ));
            };
            if parameter.as_ref() != &value {
                return Err(type_mismatch(
                    expr_span(&call.args.args[0]),
                    format!("fn({}) -> Stream<_, _>", value.name()),
                    transform.ty.name(),
                ));
            }
            let inner_value = result.array_element().ok_or_else(|| {
                type_mismatch(
                    expr_span(&call.args.args[0]),
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
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    call.span,
                    ty.clone(),
                    EffectFacts {
                        fallible: true,
                        ..EffectFacts::PURE
                    },
                    vec![receiver.node],
                    Op::StreamCollect,
                ),
                ty,
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

fn lower_closure(
    nodes: &mut Vec<Node>,
    _outer_bindings: &BTreeMap<String, LoweredValue>,
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
        (None, None) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                closure.span,
                "closure parameter without an expected type",
            )));
        }
    };

    lower_closure_typed(
        nodes,
        context,
        closure,
        parameter_ty,
        expected_signature.map(|(_, result)| result),
    )
}

fn lower_closure_with_parameter(
    nodes: &mut Vec<Node>,
    _outer_bindings: &BTreeMap<String, LoweredValue>,
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
    lower_closure_typed(nodes, context, closure, parameter_ty, None)
}

/// Lower a `flat_map` closure `|v| <array>.stream()` to an array-returning
/// callable value of type `fn(V) -> [W]`. The terminal `.stream()` is a codata
/// view that `flat_map` re-derives with fresh position keys at collection time,
/// so the frame itself returns the dense array.
fn lower_array_stream_closure(
    nodes: &mut Vec<Node>,
    _outer_bindings: &BTreeMap<String, LoweredValue>,
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

fn lower_closure_typed(
    nodes: &mut Vec<Node>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter_ty: Type,
    expected_result: Option<&Type>,
) -> Result<LoweredValue, Diagnostics> {
    lower_closure_typed_with_body_kind(
        nodes,
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
}

fn lower_closure_typed_with_body_kind(
    nodes: &mut Vec<Node>,
    context: &ModuleContext<'_>,
    closure: &ast::ClosureExpr,
    parameter_ty: Type,
    expected_result: Option<&Type>,
    body_kind: ClosureBodyKind,
) -> Result<LoweredValue, Diagnostics> {
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
                parameters: vec![Parameter {
                    id: parameter_id,
                    node: parameter_node,
                    name: "$argument".to_owned(),
                    ty: parameter_ty,
                    kind: ParameterKind::Positional,
                }],
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

    Ok(LoweredValue {
        node: push_node(
            nodes,
            closure.span,
            ty.clone(),
            EffectFacts::PURE,
            Vec::new(),
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
                lower_value(nodes, bindings, context, expression)?
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
    Variant(&'a ast::VariantPattern),
    Record(&'a ast::RecordPattern),
}

fn enum_pattern(pattern: &ast::Pattern) -> Option<EnumPattern<'_>> {
    match pattern {
        ast::Pattern::Some(pattern) => Some(EnumPattern::Some(pattern)),
        ast::Pattern::None(span) => Some(EnumPattern::None(*span)),
        ast::Pattern::Variant(pattern) => Some(EnumPattern::Variant(pattern)),
        ast::Pattern::Record(pattern) => Some(EnumPattern::Record(pattern)),
        _ => None,
    }
}

fn enum_pattern_span(pattern: EnumPattern<'_>) -> Span {
    match pattern {
        EnumPattern::Some(pattern) => pattern.span,
        EnumPattern::None(span) => span,
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
    let equal = match &left.ty {
        Type::Map { key, value } => {
            let key_array = Type::array(key.as_ref().clone());
            let value_array = Type::array(value.as_ref().clone());
            let keys_left =
                push_project(nodes, span, left.node, key_array.clone(), Op::MapKeys);
            let keys_right = push_project(nodes, span, right.node, key_array, Op::MapKeys);
            let keys_equal = push_eq(nodes, span, keys_left, keys_right);
            // The value projections and their comparison form the `then` region
            // of a short-circuiting `keys_equal && values_equal`.
            let values_start = nodes.len();
            let values_left =
                push_project(nodes, span, left.node, value_array.clone(), Op::MapValues);
            let values_right = push_project(nodes, span, right.node, value_array, Op::MapValues);
            let values_equal = push_eq(nodes, span, values_left, values_right);
            let consequent = control_region(nodes, values_start, values_equal);
            let alternative_start = nodes.len();
            let alternative_value = lower_bool_constant(nodes, span, false);
            let alternative = control_region(nodes, alternative_start, alternative_value.node);
            push_node(
                nodes,
                span,
                Type::Bool,
                EffectFacts::PURE,
                vec![keys_equal],
                Op::If {
                    consequent,
                    alternative,
                },
            )
        }
        Type::Set(element) => {
            let element_array = Type::array(element.as_ref().clone());
            let left_values =
                push_project(nodes, span, left.node, element_array.clone(), Op::SetValues);
            let right_values = push_project(nodes, span, right.node, element_array, Op::SetValues);
            push_eq(nodes, span, left_values, right_values)
        }
        _ => {
            return push_node(
                nodes,
                span,
                Type::Bool,
                EffectFacts::PURE,
                vec![left.node, right.node],
                if negate { Op::Ne } else { Op::Eq },
            );
        }
    };
    if negate {
        let alternative = lower_bool_constant(nodes, span, false);
        push_eq(nodes, span, equal, alternative.node)
    } else {
        equal
    }
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
        return Err(type_mismatch(span, expected.name(), value.ty.name()));
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
        ast::Expr::Identifier(value) => value.span,
        ast::Expr::Str(value) => value.span,
        ast::Expr::Number(value) => value.span,
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
