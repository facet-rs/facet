//! Surface AST checking and lowering to Vix IR.

use std::collections::BTreeSet;

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::support::Span;
use crate::surface::{SurfaceParser, ast};
use crate::vir::{EffectFacts, Function, FunctionId, Module, Node, NodeId, Op, Test, Type};

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

fn lower_module(source: &ast::SourceFile) -> Result<Module, Diagnostics> {
    let mut module = Module::default();
    let mut names = BTreeSet::new();

    for item in &source.items {
        let ast::Item::Fn(function) = item;
        if !names.insert(function.name.value.clone()) {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::DuplicateDefinition,
                primary: function.name.span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Name {
                    name: function.name.value.clone(),
                },
            }));
        }

        let function_id = FunctionId(module.functions.len() as u32);
        let is_test = function
            .attributes
            .iter()
            .any(|attribute| attribute.name.value == "test");
        if is_test {
            check_test_signature(function)?;
        }

        let lowered = lower_function(function_id, function, is_test)?;
        if is_test {
            module.tests.push(Test {
                name: function.name.value.clone(),
                function: function_id,
            });
        }
        module.functions.push(lowered);
    }

    Ok(module)
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

fn lower_function(
    id: FunctionId,
    function: &ast::FnItem,
    is_test: bool,
) -> Result<Function, Diagnostics> {
    let mut nodes = Vec::new();
    let mut yielded_checks = Vec::new();

    for statement in &function.body.stmts {
        match statement {
            ast::Stmt::Yield(statement) if is_test => {
                let check = lower_check(&mut nodes, &statement.value)?;
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
                return Err(Diagnostics::one(Diagnostic::unsupported(
                    statement.span,
                    "let bindings are not yet lowered",
                )));
            }
        }
    }

    if let Some(tail) = &function.body.tail {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(tail),
            "test tail expression",
        )));
    }

    Ok(Function {
        id,
        name: function.name.value.clone(),
        span: function.span,
        nodes,
        yielded_checks,
    })
}

fn lower_check(nodes: &mut Vec<Node>, expression: &ast::Expr) -> Result<NodeId, Diagnostics> {
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
            let condition = lower_value(nodes, &call.args.args[0])?;
            require_type(condition, Type::Bool, expr_span(&call.args.args[0]))?;
            condition.node
        }
        "expect_eq" | "expect_ne" => {
            check_arity(call, 2)?;
            let left = lower_value(nodes, &call.args.args[0])?;
            let right = lower_value(nodes, &call.args.args[1])?;
            require_same_type(left, right, call.span)?;
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

#[derive(Clone, Copy)]
struct LoweredValue {
    node: NodeId,
    ty: Type,
}

fn lower_value(nodes: &mut Vec<Node>, expression: &ast::Expr) -> Result<LoweredValue, Diagnostics> {
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
        ast::Expr::Binary(binary) => lower_binary(nodes, binary),
        ast::Expr::Paren(paren) => lower_value(nodes, &paren.inner),
        _ => Err(Diagnostics::one(Diagnostic::unsupported(
            expr_span(expression),
            "value expression",
        ))),
    }
}

fn lower_binary(nodes: &mut Vec<Node>, binary: &ast::Binary) -> Result<LoweredValue, Diagnostics> {
    let left = lower_value(nodes, &binary.left)?;
    let right = lower_value(nodes, &binary.right)?;
    let (ty, op) = match binary.op.value.as_str() {
        "+" => {
            require_type(left, Type::Int, expr_span(&binary.left))?;
            require_type(right, Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Add)
        }
        "-" => {
            require_type(left, Type::Int, expr_span(&binary.left))?;
            require_type(right, Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Sub)
        }
        "*" => {
            require_type(left, Type::Int, expr_span(&binary.left))?;
            require_type(right, Type::Int, expr_span(&binary.right))?;
            (Type::Int, Op::Mul)
        }
        "==" => {
            require_same_type(left, right, binary.span)?;
            (Type::Bool, Op::Eq)
        }
        "!=" => {
            require_same_type(left, right, binary.span)?;
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
            ty,
            EffectFacts::PURE,
            vec![left.node, right.node],
            op,
        ),
        ty,
    })
}

fn check_arity(call: &ast::Call, expected: u32) -> Result<(), Diagnostics> {
    let found = call.args.args.len() as u32;
    if found != expected {
        return Err(Diagnostics::one(Diagnostic {
            code: DiagnosticCode::InvalidArity,
            primary: call.span,
            labels: Vec::new(),
            payload: DiagnosticPayload::Arity { expected, found },
        }));
    }
    Ok(())
}

fn require_type(value: LoweredValue, expected: Type, span: Span) -> Result<(), Diagnostics> {
    if value.ty != expected {
        return Err(type_mismatch(span, expected.name(), value.ty.name()));
    }
    Ok(())
}

fn require_same_type(
    left: LoweredValue,
    right: LoweredValue,
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
        ast::Expr::Tuple(value) => value.span,
        ast::Expr::Paren(value) => value.span,
        ast::Expr::Identifier(value) => value.span,
        ast::Expr::Str(value) => value.span,
        ast::Expr::Number(value) => value.span,
        ast::Expr::Bool(value) => value.span,
    }
}
