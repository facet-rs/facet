use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;

use facet_core::{Facet, PtrConst, PtrMut, ScalarType, Shape, StructKind, Type, UserType};
use weavy::{BlockRef, Control, DenseLowered, Program, RunError, RunStats, Step};

use crate::SyntaxKind;
use crate::ast::{self, AstNode, BinaryExpr, Block, ElseClause, Expr, IfStmt, Stmt, UnaryExpr};
use crate::{ParseError, parse};

/// A reusable lowered Fable program for `T`.
///
/// Build a plan once with [`FablePlan::compile`], then apply it repeatedly to
/// mutable values of the same Facet-reflected type.
pub struct FablePlan<T> {
    lowered: DenseLowered<FableOp>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> FablePlan<T>
where
    T: Facet<'static>,
{
    /// Parse and lower Fable source for values of type `T`.
    pub fn compile(src: &str) -> Result<Self, FableError> {
        let parsed = parse(src);
        if !parsed.errors().is_empty() {
            return Err(FableError::Parse {
                errors: parsed.errors().to_vec(),
            });
        }

        let root = ast::Root::cast(parsed.syntax().clone()).ok_or(FableError::MalformedSyntax {
            reason: "parse root was not a Fable root node",
        })?;
        let program = Lowerer::new(T::SHAPE).lower_root(&root)?;

        Ok(Self {
            lowered: DenseLowered::new(program, Vec::new()),
            _marker: PhantomData,
        })
    }

    /// Run this plan against `value`.
    pub fn apply(&self, value: &mut T) -> Result<(), FableError> {
        let root = PtrMut::new_sized(value as *mut T);
        let mut interp = FableInterp { root };
        weavy::run_dense(&self.lowered, &mut interp).map_err(run_error)
    }

    /// Run this plan and return Weavy execution counters.
    pub fn apply_with_stats(&self, value: &mut T) -> Result<RunStats, FableError> {
        let root = PtrMut::new_sized(value as *mut T);
        let mut interp = FableInterp { root };
        weavy::run_dense_with_stats(&self.lowered, &mut interp).map_err(run_error)
    }
}

/// Compile and immediately apply a Fable program to `value`.
pub fn apply<T>(value: &mut T, src: &str) -> Result<(), FableError>
where
    T: Facet<'static>,
{
    FablePlan::<T>::compile(src)?.apply(value)
}

fn run_error(err: RunError<BlockRef, FableError>) -> FableError {
    match err {
        RunError::Step(err) => err,
        RunError::MissingBlock(block) => FableError::MissingBlock { block },
    }
}

/// Error returned while parsing, lowering, or running Fable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FableError {
    /// The parser recovered from invalid source.
    Parse {
        /// Collected parse errors.
        errors: Vec<ParseError>,
    },
    /// The CST shape was not one produced by the parser.
    MalformedSyntax {
        /// Human-readable invariant violation.
        reason: &'static str,
    },
    /// This first lowering slice does not support a syntax or type feature yet.
    Unsupported {
        /// Unsupported feature name.
        feature: String,
    },
    /// A path did not start at `root`.
    ExpectedRoot {
        /// The first path segment that was present.
        found: String,
    },
    /// A field name was not present on a named struct.
    UnknownField {
        /// Shape being searched.
        shape: &'static Shape,
        /// Missing field name.
        field: String,
    },
    /// A value had the wrong runtime type for an operator or assignment.
    TypeMismatch {
        /// Expected runtime type.
        expected: String,
        /// Actual runtime type.
        actual: &'static str,
    },
    /// A literal token could not be decoded.
    InvalidLiteral {
        /// Literal source text.
        literal: String,
        /// Reason it was rejected.
        reason: &'static str,
    },
    /// A numeric value could not fit the destination scalar.
    NumberOutOfRange {
        /// Destination scalar.
        target: ScalarType,
        /// Source value.
        value: String,
    },
    /// The lowered bytecode contains an impossible state.
    MalformedProgram {
        /// Human-readable invariant violation.
        reason: &'static str,
    },
    /// A dense block reference was missing.
    MissingBlock {
        /// Missing block reference.
        block: BlockRef,
    },
}

impl fmt::Display for FableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FableError::Parse { errors } => {
                if let Some(error) = errors.first() {
                    write!(
                        f,
                        "Fable parse failed with {} error(s), first at byte {}: {}",
                        errors.len(),
                        error.offset,
                        error.message
                    )
                } else {
                    write!(f, "Fable parse failed")
                }
            }
            FableError::MalformedSyntax { reason } => {
                write!(f, "Fable CST was malformed: {reason}")
            }
            FableError::Unsupported { feature } => {
                write!(f, "Fable lowering does not support {feature} yet")
            }
            FableError::ExpectedRoot { found } => {
                write!(f, "Fable paths must start at root, found {found}")
            }
            FableError::UnknownField { shape, field } => {
                write!(f, "{shape} has no field named {field}")
            }
            FableError::TypeMismatch { expected, actual } => {
                write!(f, "expected {expected}, found {actual}")
            }
            FableError::InvalidLiteral { literal, reason } => {
                write!(f, "invalid Fable literal {literal:?}: {reason}")
            }
            FableError::NumberOutOfRange { target, value } => {
                write!(f, "{value} is out of range for {target:?}")
            }
            FableError::MalformedProgram { reason } => {
                write!(f, "Fable lowered an invalid program: {reason}")
            }
            FableError::MissingBlock { block } => {
                write!(f, "Fable program referenced missing block {block:?}")
            }
        }
    }
}

impl std::error::Error for FableError {}

#[derive(Debug)]
enum FableOp {
    Assign {
        target: FieldPath,
        value: ExprPlan,
    },
    Eval(ExprPlan),
    Branch {
        condition: ExprPlan,
        then_program: Program<FableOp>,
        else_program: Program<FableOp>,
    },
}

#[derive(Debug)]
enum ExprPlan {
    Literal(Value),
    Read(FieldPath),
    Unary {
        op: UnaryOp,
        operand: Box<ExprPlan>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<ExprPlan>,
        rhs: Box<ExprPlan>,
    },
}

#[derive(Clone, Copy, Debug)]
enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug)]
enum BinaryOp {
    Or,
    And,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
}

#[derive(Debug)]
struct FieldPath {
    shape: &'static Shape,
    scalar: ScalarType,
    steps: Box<[FieldStep]>,
}

impl FieldPath {
    unsafe fn ptr_mut(&self, mut ptr: PtrMut) -> PtrMut {
        for step in self.steps.iter() {
            ptr = unsafe { ptr.field(step.offset) };
        }
        ptr
    }

    unsafe fn ptr_const(&self, mut ptr: PtrConst) -> PtrConst {
        for step in self.steps.iter() {
            ptr = unsafe { ptr.field(step.offset) };
        }
        ptr
    }
}

#[derive(Debug)]
struct FieldStep {
    offset: usize,
}

#[derive(Clone, Debug)]
enum Value {
    Unit,
    Null,
    Bool(bool),
    Char(char),
    String(String),
    Int(i128),
    UInt(u128),
    Float(f64),
}

impl Value {
    fn kind_name(&self) -> &'static str {
        match self {
            Value::Unit => "unit",
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Char(_) => "char",
            Value::String(_) => "string",
            Value::Int(_) => "integer",
            Value::UInt(_) => "integer",
            Value::Float(_) => "float",
        }
    }

    fn into_bool(self) -> Result<bool, FableError> {
        match self {
            Value::Bool(value) => Ok(value),
            other => Err(FableError::TypeMismatch {
                expected: "bool".into(),
                actual: other.kind_name(),
            }),
        }
    }
}

struct Lowerer {
    root_shape: &'static Shape,
}

impl Lowerer {
    fn new(root_shape: &'static Shape) -> Self {
        Self { root_shape }
    }

    fn lower_root(&self, root: &ast::Root) -> Result<Program<FableOp>, FableError> {
        self.lower_statements(root.statements())
    }

    fn lower_block(&self, block: &Block) -> Result<Program<FableOp>, FableError> {
        self.lower_statements(block.statements())
    }

    fn lower_statements(
        &self,
        statements: impl IntoIterator<Item = Stmt>,
    ) -> Result<Program<FableOp>, FableError> {
        statements
            .into_iter()
            .map(|stmt| self.lower_stmt(&stmt))
            .collect()
    }

    fn lower_stmt(&self, stmt: &Stmt) -> Result<FableOp, FableError> {
        match stmt {
            Stmt::Assign(assign) => {
                let target = assign.target().ok_or(FableError::MalformedSyntax {
                    reason: "assignment without target expression",
                })?;
                let value = assign.value().ok_or(FableError::MalformedSyntax {
                    reason: "assignment without value expression",
                })?;
                Ok(FableOp::Assign {
                    target: self.lower_writable_path(&target)?,
                    value: self.lower_expr(&value)?,
                })
            }
            Stmt::Expr(expr_stmt) => {
                let expr = expr_stmt.expr().ok_or(FableError::MalformedSyntax {
                    reason: "expression statement without expression",
                })?;
                Ok(FableOp::Eval(self.lower_expr(&expr)?))
            }
            Stmt::If(if_stmt) => self.lower_if(if_stmt),
        }
    }

    fn lower_if(&self, if_stmt: &IfStmt) -> Result<FableOp, FableError> {
        let condition = if_stmt.condition().ok_or(FableError::MalformedSyntax {
            reason: "if statement without condition",
        })?;
        let then_block = if_stmt.then_block().ok_or(FableError::MalformedSyntax {
            reason: "if statement without then block",
        })?;

        let else_program = if let Some(else_clause) = if_stmt.else_clause() {
            self.lower_else(&else_clause)?
        } else {
            Vec::new()
        };

        Ok(FableOp::Branch {
            condition: self.lower_expr(&condition)?,
            then_program: self.lower_block(&then_block)?,
            else_program,
        })
    }

    fn lower_else(&self, else_clause: &ElseClause) -> Result<Program<FableOp>, FableError> {
        if let Some(if_stmt) = else_clause.if_stmt() {
            Ok(vec![self.lower_if(&if_stmt)?])
        } else if let Some(block) = else_clause.block() {
            self.lower_block(&block)
        } else {
            Err(FableError::MalformedSyntax {
                reason: "else clause without if statement or block",
            })
        }
    }

    fn lower_expr(&self, expr: &Expr) -> Result<ExprPlan, FableError> {
        match expr {
            Expr::Literal(literal) => self.lower_literal(literal),
            Expr::Var(_) | Expr::Field(_) => Ok(ExprPlan::Read(self.lower_readable_path(expr)?)),
            Expr::Paren(paren) => {
                let expr = paren.expr().ok_or(FableError::MalformedSyntax {
                    reason: "parenthesized expression without inner expression",
                })?;
                self.lower_expr(&expr)
            }
            Expr::Unary(unary) => Ok(ExprPlan::Unary {
                op: unary_op(unary)?,
                operand: Box::new(self.lower_unary_operand(unary)?),
            }),
            Expr::Binary(binary) => Ok(ExprPlan::Binary {
                op: binary_op(binary)?,
                lhs: Box::new(self.lower_binary_lhs(binary)?),
                rhs: Box::new(self.lower_binary_rhs(binary)?),
            }),
            Expr::Index(_) => Err(FableError::Unsupported {
                feature: "index expressions".into(),
            }),
            Expr::Call(_) => Err(FableError::Unsupported {
                feature: "call expressions".into(),
            }),
        }
    }

    fn lower_literal(&self, literal: &ast::Literal) -> Result<ExprPlan, FableError> {
        let token = literal.token().ok_or(FableError::MalformedSyntax {
            reason: "literal node without token",
        })?;
        let text = token.text();
        let value = match token.kind() {
            SyntaxKind::True => Value::Bool(true),
            SyntaxKind::False => Value::Bool(false),
            SyntaxKind::Null => Value::Null,
            SyntaxKind::Int => {
                Value::UInt(text.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.to_owned(),
                    reason: "integer literal is out of range",
                })?)
            }
            SyntaxKind::Float => {
                Value::Float(text.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.to_owned(),
                    reason: "float literal is invalid",
                })?)
            }
            SyntaxKind::Str => Value::String(decode_string(text)?),
            _ => {
                return Err(FableError::MalformedSyntax {
                    reason: "literal node contained a non-literal token",
                });
            }
        };
        Ok(ExprPlan::Literal(value))
    }

    fn lower_unary_operand(&self, unary: &UnaryExpr) -> Result<ExprPlan, FableError> {
        let operand = unary.operand().ok_or(FableError::MalformedSyntax {
            reason: "unary expression without operand",
        })?;
        self.lower_expr(&operand)
    }

    fn lower_binary_lhs(&self, binary: &BinaryExpr) -> Result<ExprPlan, FableError> {
        let lhs = binary.lhs().ok_or(FableError::MalformedSyntax {
            reason: "binary expression without left operand",
        })?;
        self.lower_expr(&lhs)
    }

    fn lower_binary_rhs(&self, binary: &BinaryExpr) -> Result<ExprPlan, FableError> {
        let rhs = binary.rhs().ok_or(FableError::MalformedSyntax {
            reason: "binary expression without right operand",
        })?;
        self.lower_expr(&rhs)
    }

    fn lower_writable_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        let path = self.resolve_path(expr)?;
        ensure_writable(path.scalar, path.shape)?;
        Ok(path)
    }

    fn lower_readable_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        let path = self.resolve_path(expr)?;
        ensure_readable(path.scalar, path.shape)?;
        Ok(path)
    }

    fn resolve_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        let names = collect_path(expr)?;
        let Some((first, fields)) = names.split_first() else {
            return Err(FableError::MalformedSyntax {
                reason: "empty field path",
            });
        };
        if first != "root" {
            return Err(FableError::ExpectedRoot {
                found: first.clone(),
            });
        }

        let mut shape = self.root_shape;
        let mut steps = Vec::with_capacity(fields.len());
        for field_name in fields {
            let field = find_field(shape, field_name)?;
            let field_shape = field.shape.get();
            steps.push(FieldStep {
                offset: field.offset,
            });
            shape = field_shape;
        }

        let scalar = ScalarType::try_from_shape(shape).ok_or_else(|| FableError::Unsupported {
            feature: format!("non-scalar path ending at {shape}"),
        })?;
        Ok(FieldPath {
            shape,
            scalar,
            steps: steps.into_boxed_slice(),
        })
    }
}

struct FableInterp {
    root: PtrMut,
}

impl<'program> Step<'program, BlockRef, FableOp> for FableInterp {
    type Error = FableError;
    type Continuation = ();

    fn step(
        &mut self,
        op: &'program FableOp,
    ) -> Result<Control<'program, BlockRef, FableOp>, Self::Error> {
        match op {
            FableOp::Assign { target, value } => {
                let value = self.eval(value)?;
                let ptr = unsafe { target.ptr_mut(self.root) };
                unsafe { write_scalar(target.scalar, ptr, value) }
            }
            FableOp::Eval(expr) => {
                self.eval(expr)?;
                Ok(Control::Continue)
            }
            FableOp::Branch {
                condition,
                then_program,
                else_program,
            } => {
                let condition = self.eval(condition)?.into_bool()?;
                let program = if condition {
                    then_program.as_slice()
                } else {
                    else_program.as_slice()
                };
                if program.is_empty() {
                    Ok(Control::Continue)
                } else {
                    Ok(Control::CallProgram(program))
                }
            }
        }
    }
}

impl FableInterp {
    fn eval(&self, expr: &ExprPlan) -> Result<Value, FableError> {
        match expr {
            ExprPlan::Literal(value) => Ok(value.clone()),
            ExprPlan::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                unsafe { read_scalar(path.shape, path.scalar, ptr) }
            }
            ExprPlan::Unary { op, operand } => {
                let operand = self.eval(operand)?;
                eval_unary(*op, operand)
            }
            ExprPlan::Binary { op, lhs, rhs } => {
                let lhs = self.eval(lhs)?;
                match op {
                    BinaryOp::And => {
                        if !lhs.into_bool()? {
                            return Ok(Value::Bool(false));
                        }
                        Ok(Value::Bool(self.eval(rhs)?.into_bool()?))
                    }
                    BinaryOp::Or => {
                        if lhs.into_bool()? {
                            return Ok(Value::Bool(true));
                        }
                        Ok(Value::Bool(self.eval(rhs)?.into_bool()?))
                    }
                    _ => {
                        let rhs = self.eval(rhs)?;
                        eval_binary(*op, lhs, rhs)
                    }
                }
            }
        }
    }
}

fn collect_path(expr: &Expr) -> Result<Vec<String>, FableError> {
    match expr {
        Expr::Var(var) => {
            let name = var.name().ok_or(FableError::MalformedSyntax {
                reason: "variable reference without identifier",
            })?;
            Ok(vec![name])
        }
        Expr::Field(field) => {
            let base = field.base().ok_or(FableError::MalformedSyntax {
                reason: "field expression without base",
            })?;
            let mut path = collect_path(&base)?;
            let field_name = field.field_name().ok_or(FableError::MalformedSyntax {
                reason: "field expression without field name",
            })?;
            path.push(field_name);
            Ok(path)
        }
        Expr::Paren(paren) => {
            let inner = paren.expr().ok_or(FableError::MalformedSyntax {
                reason: "parenthesized path without inner expression",
            })?;
            collect_path(&inner)
        }
        Expr::Index(_) => Err(FableError::Unsupported {
            feature: "index paths".into(),
        }),
        Expr::Call(_) => Err(FableError::Unsupported {
            feature: "call paths".into(),
        }),
        _ => Err(FableError::Unsupported {
            feature: "non-path assignment targets".into(),
        }),
    }
}

fn find_field(
    shape: &'static Shape,
    field_name: &str,
) -> Result<&'static facet_core::Field, FableError> {
    let Type::User(UserType::Struct(struct_type)) = shape.ty else {
        return Err(FableError::Unsupported {
            feature: format!("field access on non-struct shape {shape}"),
        });
    };
    if struct_type.kind != StructKind::Struct {
        return Err(FableError::Unsupported {
            feature: format!("field access on {shape}"),
        });
    }

    struct_type
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .ok_or_else(|| FableError::UnknownField {
            shape,
            field: field_name.to_owned(),
        })
}

fn unary_op(unary: &UnaryExpr) -> Result<UnaryOp, FableError> {
    let kind = first_operator_kind(unary.syntax()).ok_or(FableError::MalformedSyntax {
        reason: "unary expression without operator",
    })?;
    match kind {
        SyntaxKind::NotKw => Ok(UnaryOp::Not),
        SyntaxKind::Minus => Ok(UnaryOp::Neg),
        _ => Err(FableError::MalformedSyntax {
            reason: "unexpected unary operator",
        }),
    }
}

fn binary_op(binary: &BinaryExpr) -> Result<BinaryOp, FableError> {
    let kind = first_operator_kind(binary.syntax()).ok_or(FableError::MalformedSyntax {
        reason: "binary expression without operator",
    })?;
    match kind {
        SyntaxKind::OrKw => Ok(BinaryOp::Or),
        SyntaxKind::AndKw => Ok(BinaryOp::And),
        SyntaxKind::EqEq => Ok(BinaryOp::Eq),
        SyntaxKind::Neq => Ok(BinaryOp::Neq),
        SyntaxKind::Lt => Ok(BinaryOp::Lt),
        SyntaxKind::Gt => Ok(BinaryOp::Gt),
        SyntaxKind::Le => Ok(BinaryOp::Le),
        SyntaxKind::Ge => Ok(BinaryOp::Ge),
        SyntaxKind::Plus => Ok(BinaryOp::Add),
        SyntaxKind::Minus => Ok(BinaryOp::Sub),
        _ => Err(FableError::MalformedSyntax {
            reason: "unexpected binary operator",
        }),
    }
}

fn first_operator_kind(node: &crate::ResolvedNode) -> Option<SyntaxKind> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.kind())
        .find(|kind| {
            matches!(
                kind,
                SyntaxKind::NotKw
                    | SyntaxKind::Minus
                    | SyntaxKind::OrKw
                    | SyntaxKind::AndKw
                    | SyntaxKind::EqEq
                    | SyntaxKind::Neq
                    | SyntaxKind::Lt
                    | SyntaxKind::Gt
                    | SyntaxKind::Le
                    | SyntaxKind::Ge
                    | SyntaxKind::Plus
            )
        })
}

fn ensure_readable(scalar: ScalarType, shape: &'static Shape) -> Result<(), FableError> {
    match scalar {
        ScalarType::Unit
        | ScalarType::Bool
        | ScalarType::Char
        | ScalarType::String
        | ScalarType::CowStr
        | ScalarType::F32
        | ScalarType::F64
        | ScalarType::U8
        | ScalarType::U16
        | ScalarType::U32
        | ScalarType::U64
        | ScalarType::U128
        | ScalarType::USize
        | ScalarType::I8
        | ScalarType::I16
        | ScalarType::I32
        | ScalarType::I64
        | ScalarType::I128
        | ScalarType::ISize => Ok(()),
        ScalarType::Str if shape.is_type::<&'static str>() => Ok(()),
        _ => Err(FableError::Unsupported {
            feature: format!("reading {scalar:?}"),
        }),
    }
}

fn ensure_writable(scalar: ScalarType, _shape: &'static Shape) -> Result<(), FableError> {
    match scalar {
        ScalarType::Unit
        | ScalarType::Bool
        | ScalarType::Char
        | ScalarType::String
        | ScalarType::CowStr
        | ScalarType::F32
        | ScalarType::F64
        | ScalarType::U8
        | ScalarType::U16
        | ScalarType::U32
        | ScalarType::U64
        | ScalarType::U128
        | ScalarType::USize
        | ScalarType::I8
        | ScalarType::I16
        | ScalarType::I32
        | ScalarType::I64
        | ScalarType::I128
        | ScalarType::ISize => Ok(()),
        _ => Err(FableError::Unsupported {
            feature: format!("writing {scalar:?}"),
        }),
    }
}

unsafe fn read_scalar(
    shape: &'static Shape,
    scalar: ScalarType,
    ptr: PtrConst,
) -> Result<Value, FableError> {
    let value = match scalar {
        ScalarType::Unit => Value::Unit,
        ScalarType::Bool => Value::Bool(*unsafe { ptr.get::<bool>() }),
        ScalarType::Char => Value::Char(*unsafe { ptr.get::<char>() }),
        ScalarType::Str if shape.is_type::<&'static str>() => {
            Value::String((*unsafe { ptr.get::<&'static str>() }).to_owned())
        }
        ScalarType::String => Value::String(unsafe { ptr.get::<String>() }.clone()),
        ScalarType::CowStr => Value::String(
            unsafe { ptr.get::<Cow<'static, str>>() }
                .clone()
                .into_owned(),
        ),
        ScalarType::F32 => Value::Float((*unsafe { ptr.get::<f32>() }).into()),
        ScalarType::F64 => Value::Float(*unsafe { ptr.get::<f64>() }),
        ScalarType::U8 => Value::UInt((*unsafe { ptr.get::<u8>() }).into()),
        ScalarType::U16 => Value::UInt((*unsafe { ptr.get::<u16>() }).into()),
        ScalarType::U32 => Value::UInt((*unsafe { ptr.get::<u32>() }).into()),
        ScalarType::U64 => Value::UInt((*unsafe { ptr.get::<u64>() }).into()),
        ScalarType::U128 => Value::UInt(*unsafe { ptr.get::<u128>() }),
        ScalarType::USize => Value::UInt((*unsafe { ptr.get::<usize>() }) as u128),
        ScalarType::I8 => Value::Int((*unsafe { ptr.get::<i8>() }).into()),
        ScalarType::I16 => Value::Int((*unsafe { ptr.get::<i16>() }).into()),
        ScalarType::I32 => Value::Int((*unsafe { ptr.get::<i32>() }).into()),
        ScalarType::I64 => Value::Int((*unsafe { ptr.get::<i64>() }).into()),
        ScalarType::I128 => Value::Int(*unsafe { ptr.get::<i128>() }),
        ScalarType::ISize => Value::Int((*unsafe { ptr.get::<isize>() }) as i128),
        _ => {
            return Err(FableError::Unsupported {
                feature: format!("reading {scalar:?}"),
            });
        }
    };
    Ok(value)
}

unsafe fn write_scalar(
    scalar: ScalarType,
    ptr: PtrMut,
    value: Value,
) -> Result<Control<'static, BlockRef, FableOp>, FableError> {
    match scalar {
        ScalarType::Unit => {
            expect_unit(value)?;
        }
        ScalarType::Bool => {
            *unsafe { ptr.as_mut::<bool>() } = expect_bool(value)?;
        }
        ScalarType::Char => {
            *unsafe { ptr.as_mut::<char>() } = expect_char(value)?;
        }
        ScalarType::String => {
            *unsafe { ptr.as_mut::<String>() } = expect_string(value)?;
        }
        ScalarType::CowStr => {
            *unsafe { ptr.as_mut::<Cow<'static, str>>() } = Cow::Owned(expect_string(value)?);
        }
        ScalarType::F32 => {
            *unsafe { ptr.as_mut::<f32>() } = expect_f64(value)? as f32;
        }
        ScalarType::F64 => {
            *unsafe { ptr.as_mut::<f64>() } = expect_f64(value)?;
        }
        ScalarType::U8 => unsafe { write_unsigned::<u8>(ptr, scalar, value) }?,
        ScalarType::U16 => unsafe { write_unsigned::<u16>(ptr, scalar, value) }?,
        ScalarType::U32 => unsafe { write_unsigned::<u32>(ptr, scalar, value) }?,
        ScalarType::U64 => unsafe { write_unsigned::<u64>(ptr, scalar, value) }?,
        ScalarType::U128 => unsafe { write_unsigned::<u128>(ptr, scalar, value) }?,
        ScalarType::USize => unsafe { write_unsigned::<usize>(ptr, scalar, value) }?,
        ScalarType::I8 => unsafe { write_signed::<i8>(ptr, scalar, value) }?,
        ScalarType::I16 => unsafe { write_signed::<i16>(ptr, scalar, value) }?,
        ScalarType::I32 => unsafe { write_signed::<i32>(ptr, scalar, value) }?,
        ScalarType::I64 => unsafe { write_signed::<i64>(ptr, scalar, value) }?,
        ScalarType::I128 => unsafe { write_signed::<i128>(ptr, scalar, value) }?,
        ScalarType::ISize => unsafe { write_signed::<isize>(ptr, scalar, value) }?,
        _ => {
            return Err(FableError::Unsupported {
                feature: format!("writing {scalar:?}"),
            });
        }
    }
    Ok(Control::Continue)
}

unsafe fn write_unsigned<T>(ptr: PtrMut, target: ScalarType, value: Value) -> Result<(), FableError>
where
    T: TryFrom<u128>,
{
    let source = expect_u128(value)?;
    let converted = T::try_from(source).map_err(|_| FableError::NumberOutOfRange {
        target,
        value: source.to_string(),
    })?;
    *unsafe { ptr.as_mut::<T>() } = converted;
    Ok(())
}

unsafe fn write_signed<T>(ptr: PtrMut, target: ScalarType, value: Value) -> Result<(), FableError>
where
    T: TryFrom<i128>,
{
    let source = expect_i128(value)?;
    let converted = T::try_from(source).map_err(|_| FableError::NumberOutOfRange {
        target,
        value: source.to_string(),
    })?;
    *unsafe { ptr.as_mut::<T>() } = converted;
    Ok(())
}

fn expect_unit(value: Value) -> Result<(), FableError> {
    match value {
        Value::Unit | Value::Null => Ok(()),
        other => Err(FableError::TypeMismatch {
            expected: "unit".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_bool(value: Value) -> Result<bool, FableError> {
    match value {
        Value::Bool(value) => Ok(value),
        other => Err(FableError::TypeMismatch {
            expected: "bool".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_char(value: Value) -> Result<char, FableError> {
    match value {
        Value::Char(value) => Ok(value),
        Value::String(value) => {
            let mut chars = value.chars();
            let Some(ch) = chars.next() else {
                return Err(FableError::TypeMismatch {
                    expected: "single-character string".into(),
                    actual: "empty string",
                });
            };
            if chars.next().is_some() {
                return Err(FableError::TypeMismatch {
                    expected: "single-character string".into(),
                    actual: "string",
                });
            }
            Ok(ch)
        }
        other => Err(FableError::TypeMismatch {
            expected: "char".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_string(value: Value) -> Result<String, FableError> {
    match value {
        Value::String(value) => Ok(value),
        Value::Char(value) => Ok(value.to_string()),
        other => Err(FableError::TypeMismatch {
            expected: "string".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_f64(value: Value) -> Result<f64, FableError> {
    match value {
        Value::Float(value) => Ok(value),
        Value::Int(value) => Ok(value as f64),
        Value::UInt(value) => Ok(value as f64),
        other => Err(FableError::TypeMismatch {
            expected: "number".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_i128(value: Value) -> Result<i128, FableError> {
    match value {
        Value::Int(value) => Ok(value),
        Value::UInt(value) => i128::try_from(value).map_err(|_| FableError::NumberOutOfRange {
            target: ScalarType::I128,
            value: value.to_string(),
        }),
        other => Err(FableError::TypeMismatch {
            expected: "integer".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_u128(value: Value) -> Result<u128, FableError> {
    match value {
        Value::UInt(value) => Ok(value),
        Value::Int(value) => u128::try_from(value).map_err(|_| FableError::NumberOutOfRange {
            target: ScalarType::U128,
            value: value.to_string(),
        }),
        other => Err(FableError::TypeMismatch {
            expected: "unsigned integer".into(),
            actual: other.kind_name(),
        }),
    }
}

fn eval_unary(op: UnaryOp, operand: Value) -> Result<Value, FableError> {
    match op {
        UnaryOp::Not => Ok(Value::Bool(!operand.into_bool()?)),
        UnaryOp::Neg => {
            match operand {
                Value::Int(value) => value.checked_neg().map(Value::Int).ok_or_else(|| {
                    FableError::NumberOutOfRange {
                        target: ScalarType::I128,
                        value: value.to_string(),
                    }
                }),
                Value::UInt(value) => {
                    let signed =
                        i128::try_from(value).map_err(|_| FableError::NumberOutOfRange {
                            target: ScalarType::I128,
                            value: format!("-{value}"),
                        })?;
                    signed.checked_neg().map(Value::Int).ok_or_else(|| {
                        FableError::NumberOutOfRange {
                            target: ScalarType::I128,
                            value: format!("-{value}"),
                        }
                    })
                }
                Value::Float(value) => Ok(Value::Float(-value)),
                other => Err(FableError::TypeMismatch {
                    expected: "number".into(),
                    actual: other.kind_name(),
                }),
            }
        }
    }
}

fn eval_binary(op: BinaryOp, lhs: Value, rhs: Value) -> Result<Value, FableError> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(values_equal(&lhs, &rhs)?)),
        BinaryOp::Neq => Ok(Value::Bool(!values_equal(&lhs, &rhs)?)),
        BinaryOp::Lt => Ok(Value::Bool(compare_values(&lhs, &rhs)? == Ordering::Less)),
        BinaryOp::Gt => Ok(Value::Bool(
            compare_values(&lhs, &rhs)? == Ordering::Greater,
        )),
        BinaryOp::Le => {
            let ordering = compare_values(&lhs, &rhs)?;
            Ok(Value::Bool(matches!(
                ordering,
                Ordering::Less | Ordering::Equal
            )))
        }
        BinaryOp::Ge => {
            let ordering = compare_values(&lhs, &rhs)?;
            Ok(Value::Bool(matches!(
                ordering,
                Ordering::Greater | Ordering::Equal
            )))
        }
        BinaryOp::Add => eval_add(lhs, rhs),
        BinaryOp::Sub => eval_sub(lhs, rhs),
        BinaryOp::And | BinaryOp::Or => Err(FableError::MalformedProgram {
            reason: "boolean connective reached eager binary evaluator",
        }),
    }
}

fn values_equal(lhs: &Value, rhs: &Value) -> Result<bool, FableError> {
    match (lhs, rhs) {
        (Value::Unit, Value::Unit) | (Value::Null, Value::Null) => Ok(true),
        (Value::Bool(lhs), Value::Bool(rhs)) => Ok(lhs == rhs),
        (Value::Char(lhs), Value::Char(rhs)) => Ok(lhs == rhs),
        (Value::String(lhs), Value::String(rhs)) => Ok(lhs == rhs),
        (Value::Char(lhs), Value::String(rhs)) | (Value::String(rhs), Value::Char(lhs)) => {
            let mut chars = rhs.chars();
            Ok(chars.next() == Some(*lhs) && chars.next().is_none())
        }
        _ if both_numeric(lhs, rhs) => Ok(compare_numbers(lhs, rhs)? == Ordering::Equal),
        _ => Ok(false),
    }
}

fn compare_values(lhs: &Value, rhs: &Value) -> Result<Ordering, FableError> {
    if both_numeric(lhs, rhs) {
        compare_numbers(lhs, rhs)
    } else {
        Err(FableError::TypeMismatch {
            expected: "comparable numbers".into(),
            actual: lhs.kind_name(),
        })
    }
}

fn both_numeric(lhs: &Value, rhs: &Value) -> bool {
    matches!(lhs, Value::Int(_) | Value::UInt(_) | Value::Float(_))
        && matches!(rhs, Value::Int(_) | Value::UInt(_) | Value::Float(_))
}

fn compare_numbers(lhs: &Value, rhs: &Value) -> Result<Ordering, FableError> {
    match (lhs, rhs) {
        (Value::Float(lhs), rhs) => compare_f64(*lhs, numeric_to_f64(rhs)?),
        (lhs, Value::Float(rhs)) => compare_f64(numeric_to_f64(lhs)?, *rhs),
        (Value::Int(lhs), Value::Int(rhs)) => Ok(lhs.cmp(rhs)),
        (Value::UInt(lhs), Value::UInt(rhs)) => Ok(lhs.cmp(rhs)),
        (Value::Int(lhs), Value::UInt(rhs)) => {
            if *lhs < 0 {
                Ok(Ordering::Less)
            } else {
                Ok((*lhs as u128).cmp(rhs))
            }
        }
        (Value::UInt(lhs), Value::Int(rhs)) => {
            if *rhs < 0 {
                Ok(Ordering::Greater)
            } else {
                Ok(lhs.cmp(&(*rhs as u128)))
            }
        }
        _ => Err(FableError::TypeMismatch {
            expected: "number".into(),
            actual: lhs.kind_name(),
        }),
    }
}

fn compare_f64(lhs: f64, rhs: f64) -> Result<Ordering, FableError> {
    lhs.partial_cmp(&rhs)
        .ok_or_else(|| FableError::TypeMismatch {
            expected: "ordered float".into(),
            actual: "NaN",
        })
}

fn numeric_to_f64(value: &Value) -> Result<f64, FableError> {
    match value {
        Value::Float(value) => Ok(*value),
        Value::Int(value) => Ok(*value as f64),
        Value::UInt(value) => Ok(*value as f64),
        other => Err(FableError::TypeMismatch {
            expected: "number".into(),
            actual: other.kind_name(),
        }),
    }
}

fn eval_add(lhs: Value, rhs: Value) -> Result<Value, FableError> {
    match (lhs, rhs) {
        (Value::String(mut lhs), Value::String(rhs)) => {
            lhs.push_str(&rhs);
            Ok(Value::String(lhs))
        }
        (lhs, rhs) if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) => {
            Ok(Value::Float(numeric_to_f64(&lhs)? + numeric_to_f64(&rhs)?))
        }
        (Value::UInt(lhs), Value::UInt(rhs)) => {
            lhs.checked_add(rhs)
                .map(Value::UInt)
                .ok_or_else(|| FableError::NumberOutOfRange {
                    target: ScalarType::U128,
                    value: format!("{lhs} + {rhs}"),
                })
        }
        (lhs, rhs) => {
            let lhs = expect_i128(lhs)?;
            let rhs = expect_i128(rhs)?;
            lhs.checked_add(rhs)
                .map(Value::Int)
                .ok_or_else(|| FableError::NumberOutOfRange {
                    target: ScalarType::I128,
                    value: format!("{lhs} + {rhs}"),
                })
        }
    }
}

fn eval_sub(lhs: Value, rhs: Value) -> Result<Value, FableError> {
    if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) {
        return Ok(Value::Float(numeric_to_f64(&lhs)? - numeric_to_f64(&rhs)?));
    }
    let lhs = expect_i128(lhs)?;
    let rhs = expect_i128(rhs)?;
    lhs.checked_sub(rhs)
        .map(Value::Int)
        .ok_or_else(|| FableError::NumberOutOfRange {
            target: ScalarType::I128,
            value: format!("{lhs} - {rhs}"),
        })
}

fn decode_string(text: &str) -> Result<String, FableError> {
    let Some(quote) = text.as_bytes().first().copied() else {
        return Err(FableError::InvalidLiteral {
            literal: text.to_owned(),
            reason: "empty string literal",
        });
    };
    if quote != b'"' && quote != b'\'' {
        return Err(FableError::InvalidLiteral {
            literal: text.to_owned(),
            reason: "missing opening quote",
        });
    }
    if text.as_bytes().last().copied() != Some(quote) || text.len() < 2 {
        return Err(FableError::InvalidLiteral {
            literal: text.to_owned(),
            reason: "missing closing quote",
        });
    }

    let mut out = String::with_capacity(text.len().saturating_sub(2));
    let mut chars = text[1..text.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(FableError::InvalidLiteral {
                literal: text.to_owned(),
                reason: "trailing escape",
            });
        };
        match escaped {
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            _ => {
                return Err(FableError::InvalidLiteral {
                    literal: text.to_owned(),
                    reason: "unsupported escape",
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use facet::Facet;

    use super::*;

    #[derive(Debug, Facet, PartialEq)]
    struct User {
        name: String,
        age: i32,
        active: bool,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct State {
        user: User,
        visits: u32,
        score: f64,
        marker: char,
    }

    fn state() -> State {
        State {
            user: User {
                name: "Ada".into(),
                age: 17,
                active: false,
            },
            visits: 1,
            score: 1.5,
            marker: 'a',
        }
    }

    #[test]
    fn applies_scalar_assignments_to_nested_struct_fields() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                root.user.name = "Grace";
                root.user.age = root.user.age + 1;
                root.visits = root.visits + 2;
                root.score = root.score + 0.5;
                root.marker = "G";
            "#,
        )
        .unwrap();

        assert_eq!(value.user.name, "Grace");
        assert_eq!(value.user.age, 18);
        assert_eq!(value.visits, 3);
        assert_eq!(value.score, 2.0);
        assert_eq!(value.marker, 'G');
    }

    #[test]
    fn applies_if_else_with_boolean_and_comparison_expressions() {
        let mut value = state();
        let plan = FablePlan::<State>::compile(
            r#"
                if root.user.age >= 18 and not root.user.active {
                    root.user.name = "adult";
                } else {
                    root.user.name = "minor";
                }
            "#,
        )
        .unwrap();

        let stats = plan.apply_with_stats(&mut value).unwrap();

        assert_eq!(value.user.name, "minor");
        assert!(stats.step_count >= 1);
    }

    #[test]
    fn else_if_uses_inline_child_programs() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                if root.user.age > 30 {
                    root.user.name = "older";
                } else if root.user.age == 17 {
                    root.user.name = "exact";
                } else {
                    root.user.name = "other";
                }
            "#,
        )
        .unwrap();

        assert_eq!(value.user.name, "exact");
    }

    #[test]
    fn reports_unknown_fields_during_lowering() {
        let err = compile_err("root.user.missing = true");

        assert!(matches!(
            err,
            FableError::UnknownField {
                field,
                ..
            } if field == "missing"
        ));
    }

    #[test]
    fn reports_type_mismatches_while_running() {
        let mut value = state();
        let err = apply(&mut value, r#"root.user.age = "old""#).unwrap_err();

        assert!(matches!(
            err,
            FableError::TypeMismatch {
                actual: "string",
                ..
            }
        ));
    }

    #[test]
    fn reports_unsupported_index_lowering() {
        let err = compile_err("root.users[0].name = \"Ada\"");

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "index paths"
        ));
    }

    fn compile_err(src: &str) -> FableError {
        match FablePlan::<State>::compile(src) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        }
    }
}
