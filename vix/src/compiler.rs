//! Surface AST checking and lowering to Vix IR.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::support::Span;
use crate::surface::{SurfaceParser, ast};
use crate::vir::{
    EffectFacts, Function, FunctionId, Module, Node, NodeId, Op, Parameter, ParameterId,
    ParameterKind, Test, Type,
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

fn lower_module(source: &ast::SourceFile) -> Result<Module, Diagnostics> {
    let mut signatures = BTreeMap::new();
    let mut ordered_signatures = Vec::with_capacity(source.items.len());

    for (index, item) in source.items.iter().enumerate() {
        let ast::Item::Fn(function) = item;
        if signatures.contains_key(&function.name.value) {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::DuplicateDefinition,
                primary: function.name.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name {
                    name: function.name.value.clone(),
                },
            }));
        }
        let id = FunctionId(u32::try_from(index).expect("module function count fits u32"));
        let signature = declare_function(id, function)?;
        signatures.insert(function.name.value.clone(), signature.clone());
        ordered_signatures.push(signature);
    }

    let mut module = Module::default();
    for (item, signature) in source.items.iter().zip(&ordered_signatures) {
        let ast::Item::Fn(function) = item;
        let lowered = lower_function(signature, function, &signatures)?;
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

fn declare_function(
    id: FunctionId,
    function: &ast::FnItem,
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
        .and_then(lower_declared_type)?;
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
        ty: lower_declared_type(ty)?,
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

fn lower_declared_type(ty: &ast::Type) -> Result<Type, Diagnostics> {
    match ty {
        ast::Type::Path(path) if path_is(path, "Bool") => Ok(Type::Bool),
        ast::Type::Path(path) if path_is(path, "Int") => Ok(Type::Int),
        ast::Type::Path(path) if path_is(path, "String") => Ok(Type::String),
        ast::Type::Path(path) if path_is(path, "Check") => Ok(Type::Check),
        ast::Type::Generic(_) if is_stream_check_type(ty) => Ok(Type::StreamCheck),
        ast::Type::Path(path) => Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::UnknownName,
            primary: path.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Name {
                name: path
                    .segments
                    .iter()
                    .map(|segment| segment.value.as_str())
                    .collect::<Vec<_>>()
                    .join("::"),
            },
        })),
        ast::Type::Generic(generic) => Err(Diagnostics::one(Diagnostic::unsupported(
            generic.span,
            "generic type",
        ))),
        ast::Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .map(lower_declared_type)
            .collect::<Result<Vec<_>, _>>()
            .map(Type::Tuple),
    }
}

fn lower_function(
    signature: &FunctionSignature,
    function: &ast::FnItem,
    signatures: &BTreeMap<String, FunctionSignature>,
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
            ast::Stmt::Yield(statement) if signature.is_test => {
                let check = lower_check(&mut nodes, &bindings, signatures, &statement.value)?;
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
                if bindings.contains_key(&statement.name.value) {
                    return Err(Diagnostics::one(Diagnostic {
                        code: DiagnosticCode::DuplicateBinding,
                        primary: statement.name.span,
                        labels: Vec::new(),
                        payload: DiagnosticPayload::Name {
                            name: statement.name.value.clone(),
                        },
                    }));
                }
                let value = lower_value(&mut nodes, &bindings, signatures, &statement.value)?;
                if let Some(annotation) = &statement.ty {
                    let expected = lower_declared_type(annotation)?;
                    require_type(&value, &expected, type_span(annotation))?;
                }
                bindings.insert(statement.name.value.clone(), value);
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
            let value = lower_value(&mut nodes, &bindings, signatures, tail)?;
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

fn lower_check(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    signatures: &BTreeMap<String, FunctionSignature>,
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
            let condition = lower_value(nodes, bindings, signatures, &call.args.args[0])?;
            require_type(&condition, &Type::Bool, expr_span(&call.args.args[0]))?;
            condition.node
        }
        "expect_eq" | "expect_ne" => {
            check_arity(call, 2)?;
            let left = lower_value(nodes, bindings, signatures, &call.args.args[0])?;
            let right = lower_value(nodes, bindings, signatures, &call.args.args[1])?;
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
    signatures: &BTreeMap<String, FunctionSignature>,
    expression: &ast::Expr,
) -> Result<LoweredValue, Diagnostics> {
    match expression {
        ast::Expr::Bool(value) => Ok(LoweredValue {
            node: push_node(
                nodes,
                value.span,
                Type::Bool,
                EffectFacts::PURE,
                Vec::new(),
                Op::Bool(value.value),
            ),
            ty: Type::Bool,
        }),
        ast::Expr::Number(value) => {
            let parsed = value.value.parse::<i64>().map_err(|_| {
                type_mismatch(
                    value.span,
                    "Int",
                    format!("number literal `{}`", value.value),
                )
            })?;
            Ok(LoweredValue {
                node: push_node(
                    nodes,
                    value.span,
                    Type::Int,
                    EffectFacts::PURE,
                    Vec::new(),
                    Op::Int(parsed),
                ),
                ty: Type::Int,
            })
        }
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
        ast::Expr::Call(call) => lower_call(nodes, bindings, signatures, call),
        ast::Expr::Binary(binary) => lower_binary(nodes, bindings, signatures, binary),
        ast::Expr::Tuple(tuple) => {
            let values = tuple
                .elems
                .iter()
                .map(|element| lower_value(nodes, bindings, signatures, element))
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
        ast::Expr::Field(field) => {
            let receiver = lower_value(nodes, bindings, signatures, &field.receiver)?;
            let ast::Member::Index(index) = &field.name else {
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    field.span,
                    "named field projection",
                )));
            };
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
            let index_value = u32::try_from(index_value)
                .map_err(|_| type_mismatch(index.span, "tuple index", index.value.clone()))?;
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
        ast::Expr::Paren(paren) => lower_value(nodes, bindings, signatures, &paren.inner),
        _ => Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(expression),
            "value expression",
        ))),
    }
}

fn lower_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    signatures: &BTreeMap<String, FunctionSignature>,
    call: &ast::Call,
) -> Result<LoweredValue, Diagnostics> {
    let signature = signatures.get(&call.callee.value).ok_or_else(|| {
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
        let value = lower_value(nodes, bindings, signatures, argument)?;
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
            lower_value(nodes, bindings, signatures, expression)?
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
    signatures: &BTreeMap<String, FunctionSignature>,
    binary: &ast::Binary,
) -> Result<LoweredValue, Diagnostics> {
    let left = lower_value(nodes, bindings, signatures, &binary.left)?;
    let right = lower_value(nodes, bindings, signatures, &binary.right)?;
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
        ast::Expr::Binary(value) => value.span,
        ast::Expr::Unary(value) => value.span,
        ast::Expr::Call(value) => value.span,
        ast::Expr::Field(value) => value.span,
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
