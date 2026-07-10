//! Surface AST checking and lowering to Vix IR.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics, Label};
use crate::support::{Span, Spanned};
use crate::surface::{SurfaceParser, ast};
use crate::vir::{
    ControlRegion, EffectFacts, EnumType, EnumVariant, Function, FunctionId,
    MatchArm as VirMatchArm, Module, Node, NodeId, ORDERING_GREATER_VARIANT, ORDERING_LESS_VARIANT,
    Op, OrderedMatchArm, Parameter, ParameterId, ParameterKind, RecordField, RecordType, Test,
    Type, VariantPayload,
};

pub struct Compiler {
    parser: SurfaceParser,
}

impl Compiler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: SurfaceParser::new(),
        }
    }

    /// Parse, check, and lower to architecture-neutral VIR.
    pub fn compile(&self, source: &str) -> Result<Module, Diagnostics> {
        let ast = self.parser.parse(source)?;
        lower_module(&ast)
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
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
}

#[derive(Clone, Copy)]
enum TypeDeclaration<'a> {
    Record(&'a ast::StructItem),
    Enum(&'a ast::EnumItem),
}

impl TypeDeclaration<'_> {
    fn span(self) -> Span {
        match self {
            Self::Record(record) => record.span,
            Self::Enum(enumeration) => enumeration.span,
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

    fn resolve_all(mut self) -> Result<BTreeMap<String, Type>, Diagnostics> {
        let names = self.declarations.keys().cloned().collect::<Vec<_>>();
        for name in names {
            self.resolve_nominal(&name)?;
        }
        Ok(self.resolved)
    }

    fn resolve_nominal(&mut self, name: &str) -> Result<Type, Diagnostics> {
        if let Some(ty) = self.resolved.get(name) {
            return Ok(ty.clone());
        }
        let declaration = *self
            .declarations
            .get(name)
            .ok_or_else(|| unknown_name(Span { start: 0, end: 0 }, name))?;
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

    fn resolve_record_fields(
        &mut self,
        owner: &str,
        declared_fields: &[ast::RecordField],
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
                ty: self.resolve_type(&field.ty)?,
            });
        }
        Ok(fields)
    }

    fn resolve_type(&mut self, ty: &ast::Type) -> Result<Type, Diagnostics> {
        match ty {
            ast::Type::Path(path) if path_is(path, "Bool") => Ok(Type::Bool),
            ast::Type::Path(path) if path_is(path, "Int") => Ok(Type::Int),
            ast::Type::Path(path) if path_is(path, "String") => Ok(Type::String),
            ast::Type::Path(path) if path_is(path, "Check") => Ok(Type::Check),
            ast::Type::Generic(_) if is_stream_check_type(ty) => Ok(Type::StreamCheck),
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
            ast::Type::Generic(generic) => Err(Diagnostics::one(Diagnostic::unsupported(
                generic.span,
                "generic type",
            ))),
            ast::Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .map(|element| self.resolve_type(element))
                .collect::<Result<Vec<_>, _>>()
                .map(Type::Tuple),
        }
    }
}

fn lower_module(source: &ast::SourceFile) -> Result<Module, Diagnostics> {
    let types = TypeResolver::new(source)?.resolve_all()?;
    let mut signatures = BTreeMap::new();
    let mut ordered_signatures = Vec::new();

    for item in &source.items {
        let ast::Item::Fn(function) = item else {
            continue;
        };
        if types.contains_key(&function.name.value) || signatures.contains_key(&function.name.value)
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
        enums: source
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Enum(enumeration) => match types.get(&enumeration.name.value) {
                    Some(Type::Enum(enumeration)) => Some(enumeration.clone()),
                    _ => None,
                },
                ast::Item::Struct(_) | ast::Item::Fn(_) => None,
            })
            .collect(),
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
        let lowered = lower_function(signature, function, &context)
            .map_err(|diagnostics| anchor_function_diagnostics(function, diagnostics))?;
        if signature.is_test {
            module.tests.push(Test {
                name: function.name.value.clone(),
                function: signature.id,
            });
        }
        module.functions.push(lowered);
    }

    Ok(module)
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
        ast::Type::Path(path) => types
            .get(&path_name(path))
            .cloned()
            .ok_or_else(|| unknown_name(path.span, path_name(path))),
        ast::Type::Generic(generic) => Err(Diagnostics::one(Diagnostic::unsupported(
            generic.span,
            "generic type",
        ))),
        ast::Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .map(|element| lower_declared_type(element, types))
            .collect::<Result<Vec<_>, _>>()
            .map(Type::Tuple),
    }
}

fn lower_function(
    signature: &FunctionSignature,
    function: &ast::FnItem,
    context: &ModuleContext<'_>,
) -> Result<Function, Diagnostics> {
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

    for statement in &function.body.stmts {
        match statement {
            ast::Stmt::Expression(statement) => {
                return Err(expression_statement_diagnostic(statement.span));
            }
            ast::Stmt::Yield(statement) if signature.is_test => {
                let check = lower_check(&mut nodes, &bindings, context, &statement.value)?;
                yielded_checks.push(check);
                push_node(
                    &mut nodes,
                    statement.span,
                    Type::StreamCheck,
                    EffectFacts::CODATA,
                    vec![check],
                    Op::Yield,
                );
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

    let output = match (&function.body.tail, signature.is_test) {
        (Some(tail), true) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                expr_span(tail),
                "test tail expression",
            )));
        }
        (None, true) => None,
        (Some(tail), false) => {
            let value = lower_value(&mut nodes, &bindings, context, tail)?;
            require_type(&value, &signature.return_type, expr_span(tail))?;
            Some(value.node)
        }
        (None, false) => {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                function.body.span,
                "function without a tail value",
            )));
        }
    };

    Ok(Function {
        id: signature.id,
        name: function.name.value.clone(),
        span: function.span,
        parameters,
        return_type: signature.return_type.clone(),
        nodes,
        output,
        yielded_checks,
    })
}

fn lower_let_statement(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    statement: &ast::LetStmt,
) -> Result<(), Diagnostics> {
    let value = lower_value(nodes, bindings, context, &statement.value)?;
    if let Some(annotation) = &statement.ty {
        let expected = lower_declared_type(annotation, context.types)?;
        require_type(&value, &expected, type_span(annotation))?;
    }
    bind_irrefutable_let_pattern(nodes, bindings, &statement.pattern, &value)
}

fn bind_irrefutable_let_pattern(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    pattern: &ast::Pattern,
    value: &LoweredValue,
) -> Result<(), Diagnostics> {
    match pattern {
        ast::Pattern::Binding(pattern) => bind_name(bindings, value, &pattern.binding),
        ast::Pattern::Tuple(pattern) => {
            let Type::Tuple(elements) = &value.ty else {
                return Err(type_mismatch(pattern.span, "tuple", value.ty.name()));
            };
            if pattern.bindings.len() != elements.len() {
                return Err(type_mismatch(
                    pattern.span,
                    format!("tuple pattern with {} elements", elements.len()),
                    format!("tuple pattern with {} elements", pattern.bindings.len()),
                ));
            }
            let elements = elements.clone();
            for (index, (binding, ty)) in pattern.bindings.iter().zip(elements).enumerate() {
                let index = u32::try_from(index).map_err(|_| {
                    type_mismatch(pattern.span, "tuple field index", index.to_string())
                })?;
                let projected = LoweredValue {
                    node: push_node(
                        nodes,
                        binding.span,
                        ty.clone(),
                        EffectFacts::PURE,
                        vec![value.node],
                        Op::Project { index },
                    ),
                    ty,
                };
                bind_name(bindings, &projected, binding)?;
            }
            Ok(())
        }
        ast::Pattern::Wildcard(_) => Ok(()),
        ast::Pattern::Variant(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable let pattern",
        ))),
        ast::Pattern::Number(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "refutable let pattern",
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
            let right = lower_value(nodes, bindings, context, &call.args.args[1])?;
            require_same_type(&left, &right, call.span)?;
            push_node(
                nodes,
                call.span,
                Type::Bool,
                EffectFacts::PURE,
                vec![left.node, right.node],
                if call.callee.value == "expect_eq" {
                    Op::Eq
                } else {
                    Op::Ne
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

fn lower_value(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::Expr,
) -> Result<LoweredValue, Diagnostics> {
    match expression {
        ast::Expr::If(expression) => lower_if(nodes, bindings, context, expression),
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
        ast::Expr::Identifier(identifier) => {
            lookup_binding(bindings, &identifier.value, identifier.span)
        }
        ast::Expr::Call(call) => lower_call(nodes, bindings, context, call),
        ast::Expr::Binary(binary) => lower_binary(nodes, bindings, context, binary),
        ast::Expr::Variant(variant) => lower_variant(nodes, bindings, context, variant),
        ast::Expr::Match(match_expr) => lower_match(nodes, bindings, context, match_expr),
        ast::Expr::Tuple(tuple) => {
            let values = tuple
                .elems
                .iter()
                .map(|element| lower_value(nodes, bindings, context, element))
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
        ast::Expr::Paren(paren) => lower_value(nodes, bindings, context, &paren.inner),
        ast::Expr::Unary(unary) => lower_unary_value(nodes, bindings, context, unary),
    }
}

fn lower_if(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::IfExpr,
) -> Result<LoweredValue, Diagnostics> {
    let condition = lower_value(nodes, bindings, context, &expression.condition)?;
    require_type(&condition, &Type::Bool, expr_span(&expression.condition))?;

    let consequent_start = nodes.len();
    let consequent_value = lower_value_block(nodes, bindings, context, &expression.consequent)?;
    let consequent = control_region(nodes, consequent_start, consequent_value.node);

    let alternative_start = nodes.len();
    let alternative_value = match &expression.alternative {
        ast::IfBranch::Block(block) => lower_value_block(nodes, bindings, context, block)?,
        ast::IfBranch::If(expression) => lower_if(nodes, bindings, context, expression)?,
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
    lower_value(nodes, &bindings, context, tail)
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
) -> Result<LoweredValue, Diagnostics> {
    let enumeration = context
        .types
        .get(&expression.path.type_name.value)
        .ok_or_else(|| {
            unknown_name(
                expression.path.type_name.span,
                &expression.path.type_name.value,
            )
        })?;
    let Type::Enum(enumeration) = enumeration else {
        return Err(type_mismatch(
            expression.path.type_name.span,
            "enum type",
            enumeration.name(),
        ));
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
                let value = lower_value(nodes, bindings, context, argument)?;
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
) -> Result<LoweredValue, Diagnostics> {
    let scrutinee = lower_value(nodes, bindings, context, &expression.scrutinee)?;
    let enumeration = match &scrutinee.ty {
        Type::Enum(enumeration)
            if expression.arms.arms.iter().all(|arm| {
                matches!(arm.pattern, ast::Pattern::Variant(_)) && arm.guard.is_none()
            }) =>
        {
            Some(enumeration.clone())
        }
        _ => None,
    };
    if let Some(enumeration) = enumeration {
        return lower_enum_match(nodes, bindings, context, expression, scrutinee, enumeration);
    }
    lower_ordered_match(nodes, bindings, context, expression, scrutinee)
}

fn lower_enum_match(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    expression: &ast::MatchExpr,
    scrutinee: LoweredValue,
    enumeration: EnumType,
) -> Result<LoweredValue, Diagnostics> {
    let mut seen = BTreeSet::new();
    let mut arms = Vec::with_capacity(expression.arms.arms.len());
    let mut result_type = None;

    for arm in &expression.arms.arms {
        let ast::Pattern::Variant(pattern) = &arm.pattern else {
            unreachable!("enum match shape was checked by lower_match")
        };
        let (variant_index, variant) = find_variant(&enumeration, &pattern.path)?;
        if !seen.insert(variant_index) {
            return Err(variant_diagnostic(
                DiagnosticCode::DuplicateVariant,
                pattern.path.variant.span,
                &enumeration.name,
                &variant.name,
            ));
        }
        let first_arm_node = nodes.len();
        let mut arm_bindings = bindings.clone();
        bind_variant_pattern(
            nodes,
            &mut arm_bindings,
            &scrutinee,
            &enumeration,
            variant_index,
            variant,
            pattern,
        )?;
        let output = lower_value(nodes, &arm_bindings, context, &arm.body)?;
        if let Some(expected) = &result_type {
            require_type(&output, expected, expr_span(&arm.body))?;
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
                    pattern.span,
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
        let output = lower_value(nodes, &arm_bindings, context, &arm.body)?;
        if let Some(expected) = &result_type {
            require_type(&output, expected, expr_span(&arm.body))?;
        } else {
            result_type = Some(output.ty.clone());
        }
        let body = control_region(nodes, body_start, output.node);

        if let Some(condition) = condition {
            arms.push(OrderedMatchArm { condition, body });
        } else {
            fallback = Some(body);
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
        ast::Pattern::Wildcard(_) => Ok(None),
        ast::Pattern::Variant(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "guarded enum pattern",
        ))),
        ast::Pattern::Tuple(pattern) => Err(Diagnostics::one(Diagnostic::unsupported(
            pattern.span,
            "tuple match pattern",
        ))),
    }
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
    if path.type_name.value != enumeration.name {
        return Err(type_mismatch(
            path.type_name.span,
            &enumeration.name,
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

fn bind_variant_pattern(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    scrutinee: &LoweredValue,
    enumeration: &EnumType,
    variant_index: usize,
    variant: &EnumVariant,
    pattern: &ast::VariantPattern,
) -> Result<(), Diagnostics> {
    match (&variant.payload, &pattern.payload) {
        (VariantPayload::Unit, None) => Ok(()),
        (VariantPayload::Tuple(types), Some(ast::VariantPatternPayload::Tuple(tuple))) => {
            if types.len() != tuple.bindings.len() {
                return Err(invalid_arity(tuple.span, types.len(), tuple.bindings.len()));
            }
            for (field, (ty, binding)) in types.iter().zip(&tuple.bindings).enumerate() {
                bind_variant_field(
                    nodes,
                    bindings,
                    scrutinee,
                    variant_index,
                    field,
                    ty,
                    binding,
                )?;
            }
            Ok(())
        }
        (VariantPayload::Record(fields), Some(ast::VariantPatternPayload::Record(record))) => {
            let mut supplied = BTreeMap::new();
            for field in &record.fields {
                if supplied.insert(field.name.value.clone(), field).is_some() {
                    return Err(field_diagnostic(
                        DiagnosticCode::DuplicateField,
                        field.name.span,
                        &format!("{}::{}", enumeration.name, variant.name),
                        &field.name.value,
                    ));
                }
            }
            for (field_index, declared) in fields.iter().enumerate() {
                let field = supplied.remove(&declared.name).ok_or_else(|| {
                    field_diagnostic(
                        DiagnosticCode::MissingField,
                        record.span,
                        &format!("{}::{}", enumeration.name, variant.name),
                        &declared.name,
                    )
                })?;
                let binding = field.binding.as_ref().unwrap_or(&field.name);
                bind_variant_field(
                    nodes,
                    bindings,
                    scrutinee,
                    variant_index,
                    field_index,
                    &declared.ty,
                    binding,
                )?;
            }
            if let Some((name, field)) = supplied.into_iter().next() {
                return Err(field_diagnostic(
                    DiagnosticCode::UnknownField,
                    field.name.span,
                    &format!("{}::{}", enumeration.name, variant.name),
                    &name,
                ));
            }
            Ok(())
        }
        _ => Err(variant_diagnostic(
            DiagnosticCode::VariantPayloadMismatch,
            pattern.span,
            &enumeration.name,
            &variant.name,
        )),
    }
}

fn bind_variant_field(
    nodes: &mut Vec<Node>,
    bindings: &mut BTreeMap<String, LoweredValue>,
    scrutinee: &LoweredValue,
    variant: usize,
    field: usize,
    ty: &Type,
    binding: &crate::support::Spanned<String>,
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
    let variant = u32::try_from(variant)
        .map_err(|_| type_mismatch(binding.span, "variant index", variant.to_string()))?;
    let field = u32::try_from(field)
        .map_err(|_| type_mismatch(binding.span, "variant field index", field.to_string()))?;
    let value = LoweredValue {
        node: push_node(
            nodes,
            binding.span,
            ty.clone(),
            EffectFacts::PURE,
            vec![scrutinee.node],
            Op::VariantProject { variant, field },
        ),
        ty: ty.clone(),
    };
    bindings.insert(binding.value.clone(), value);
    Ok(())
}

fn lower_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::Call,
) -> Result<LoweredValue, Diagnostics> {
    let signature = context.signatures.get(&call.callee.value).ok_or_else(|| {
        Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownName,
            primary: call.callee.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: call.callee.value.clone(),
            },
        })
    })?;
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
        let value = lower_value(nodes, bindings, context, argument)?;
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
            lower_value(nodes, bindings, context, expression)?
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
        "+" => {
            require_type(&left, &Type::Int, expr_span(&binary.left))?;
            require_type(&right, &Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Add)
        }
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
        "==" => {
            require_same_type(&left, &right, binary.span)?;
            (Type::Bool, Op::Eq)
        }
        "!=" => {
            require_same_type(&left, &right, binary.span)?;
            (Type::Bool, Op::Ne)
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
    Ok(LoweredValue {
        node: push_node(
            nodes,
            binary.span,
            ty.clone(),
            EffectFacts::PURE,
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
        ast::Expr::If(value) => value.span,
        ast::Expr::Match(value) => value.span,
        ast::Expr::Binary(value) => value.span,
        ast::Expr::Unary(value) => value.span,
        ast::Expr::Call(value) => value.span,
        ast::Expr::Field(value) => value.span,
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
        ast::Type::Generic(value) => value.span,
        ast::Type::Tuple(value) => value.span,
        ast::Type::Path(value) => value.span,
    }
}
