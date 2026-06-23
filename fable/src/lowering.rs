use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
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
        let mut lowerer = Lowerer::new(T::SHAPE);
        let program = lowerer.lower_root(&root)?;

        Ok(Self {
            lowered: DenseLowered::new(program, Vec::new()),
            _marker: PhantomData,
        })
    }

    /// Run this plan against `value`.
    pub fn apply(&self, value: &mut T) -> Result<(), FableError> {
        let root = PtrMut::new_sized(value as *mut T);
        let mut interp = FableInterp {
            root,
            locals: LocalSlots::default(),
        };
        weavy::run_dense(&self.lowered, &mut interp).map_err(run_error)
    }

    /// Run this plan and return Weavy execution counters.
    pub fn apply_with_stats(&self, value: &mut T) -> Result<RunStats, FableError> {
        let root = PtrMut::new_sized(value as *mut T);
        let mut interp = FableInterp {
            root,
            locals: LocalSlots::default(),
        };
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
    /// A typed expression was used in a context that expects another type.
    TypeMismatch {
        /// Expected expression type.
        expected: String,
        /// Actual expression type.
        actual: &'static str,
    },
    /// A local binding attempted to use a reserved name.
    ReservedLocalName {
        /// Reserved binding name.
        name: String,
    },
    /// A local binding name was already used in this scope.
    DuplicateLocal {
        /// Duplicate binding name.
        name: String,
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
            FableError::ReservedLocalName { name } => {
                write!(
                    f,
                    "{name} is reserved and cannot be used as a local binding"
                )
            }
            FableError::DuplicateLocal { name } => {
                write!(f, "local binding {name} is already defined in this scope")
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
    Let {
        local: LocalRef,
        value: ExprPlan,
    },
    Assign {
        target: FieldPath,
        value: ExprPlan,
    },
    Eval(ExprPlan),
    Branch {
        condition: BoolExpr,
        then_program: Program<FableOp>,
        else_program: Program<FableOp>,
    },
}

#[derive(Debug)]
enum ExprPlan {
    Unit(UnitExpr),
    Bool(BoolExpr),
    Char(CharExpr),
    String(StringExpr),
    Number(NumberExpr),
}

impl ExprPlan {
    fn kind_name(&self) -> &'static str {
        match self {
            ExprPlan::Unit(_) => "unit",
            ExprPlan::Bool(_) => "bool",
            ExprPlan::Char(_) => "char",
            ExprPlan::String(_) => "string",
            ExprPlan::Number(NumberExpr::Signed(_)) => "signed number",
            ExprPlan::Number(NumberExpr::Unsigned(_)) => "unsigned number",
            ExprPlan::Number(NumberExpr::Float(_)) => "float",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalRef {
    Unit(usize),
    Bool(usize),
    Char(usize),
    String(usize),
    Signed(usize),
    Unsigned(usize),
    Float(usize),
}

impl LocalRef {
    fn kind_name(self) -> &'static str {
        match self {
            LocalRef::Unit(_) => "unit",
            LocalRef::Bool(_) => "bool",
            LocalRef::Char(_) => "char",
            LocalRef::String(_) => "string",
            LocalRef::Signed(_) => "signed number",
            LocalRef::Unsigned(_) => "unsigned number",
            LocalRef::Float(_) => "float",
        }
    }
}

#[derive(Debug)]
enum UnitExpr {
    Null,
    Read(FieldPath),
    Local(LocalRef),
}

#[derive(Debug)]
enum BoolExpr {
    Literal(bool),
    Read(FieldPath),
    Local(LocalRef),
    Not(Box<BoolExpr>),
    And(Box<BoolExpr>, Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
    Eq(Box<ExprPlan>, Box<ExprPlan>),
    Neq(Box<ExprPlan>, Box<ExprPlan>),
    Cmp {
        op: CmpOp,
        lhs: Box<NumberExpr>,
        rhs: Box<NumberExpr>,
    },
}

#[derive(Clone, Copy, Debug)]
enum CmpOp {
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug)]
enum CharExpr {
    Read(FieldPath),
    Local(LocalRef),
}

#[derive(Debug)]
enum StringExpr {
    Literal(String),
    Read(FieldPath),
    Local(LocalRef),
    Add(Box<StringExpr>, Box<StringExpr>),
}

#[derive(Debug)]
enum NumberExpr {
    Signed(IntExpr),
    Unsigned(UIntExpr),
    Float(FloatExpr),
}

#[derive(Debug)]
enum IntExpr {
    Read(FieldPath),
    Local(LocalRef),
    Neg(Box<NumberExpr>),
    Add(Box<NumberExpr>, Box<NumberExpr>),
    Sub(Box<NumberExpr>, Box<NumberExpr>),
}

#[derive(Debug)]
enum UIntExpr {
    Read(FieldPath),
    Local(LocalRef),
    Literal(u128),
    Add(Box<UIntExpr>, Box<UIntExpr>),
}

#[derive(Debug)]
enum FloatExpr {
    Read(FieldPath),
    Local(LocalRef),
    Literal(f64),
    Neg(Box<NumberExpr>),
    Add(Box<NumberExpr>, Box<NumberExpr>),
    Sub(Box<NumberExpr>, Box<NumberExpr>),
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

#[derive(Default)]
struct LocalAllocator {
    unit_count: usize,
    bool_count: usize,
    char_count: usize,
    string_count: usize,
    signed_count: usize,
    unsigned_count: usize,
    float_count: usize,
}

impl LocalAllocator {
    fn allocate(&mut self, expr: &ExprPlan) -> LocalRef {
        match expr {
            ExprPlan::Unit(_) => {
                let index = self.unit_count;
                self.unit_count += 1;
                LocalRef::Unit(index)
            }
            ExprPlan::Bool(_) => {
                let index = self.bool_count;
                self.bool_count += 1;
                LocalRef::Bool(index)
            }
            ExprPlan::Char(_) => {
                let index = self.char_count;
                self.char_count += 1;
                LocalRef::Char(index)
            }
            ExprPlan::String(_) => {
                let index = self.string_count;
                self.string_count += 1;
                LocalRef::String(index)
            }
            ExprPlan::Number(NumberExpr::Signed(_)) => {
                let index = self.signed_count;
                self.signed_count += 1;
                LocalRef::Signed(index)
            }
            ExprPlan::Number(NumberExpr::Unsigned(_)) => {
                let index = self.unsigned_count;
                self.unsigned_count += 1;
                LocalRef::Unsigned(index)
            }
            ExprPlan::Number(NumberExpr::Float(_)) => {
                let index = self.float_count;
                self.float_count += 1;
                LocalRef::Float(index)
            }
        }
    }
}

struct Lowerer {
    root_shape: &'static Shape,
    scopes: Vec<BTreeMap<String, LocalRef>>,
    locals: LocalAllocator,
}

impl Lowerer {
    fn new(root_shape: &'static Shape) -> Self {
        Self {
            root_shape,
            scopes: vec![BTreeMap::new()],
            locals: LocalAllocator::default(),
        }
    }

    fn lower_root(&mut self, root: &ast::Root) -> Result<Program<FableOp>, FableError> {
        self.lower_statements(root.statements())
    }

    fn lower_block(&mut self, block: &Block) -> Result<Program<FableOp>, FableError> {
        self.scopes.push(BTreeMap::new());
        let result = self.lower_statements(block.statements());
        self.scopes.pop();
        result
    }

    fn lower_statements(
        &mut self,
        statements: impl IntoIterator<Item = Stmt>,
    ) -> Result<Program<FableOp>, FableError> {
        let mut program = Vec::new();
        for stmt in statements {
            program.push(self.lower_stmt(&stmt)?);
        }
        Ok(program)
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<FableOp, FableError> {
        match stmt {
            Stmt::Assign(assign) => {
                let target_expr = assign.target().ok_or(FableError::MalformedSyntax {
                    reason: "assignment without target expression",
                })?;
                let value_expr = assign.value().ok_or(FableError::MalformedSyntax {
                    reason: "assignment without value expression",
                })?;
                let target = self.lower_writable_path(&target_expr)?;
                let value = self.lower_expr(&value_expr)?;
                validate_assignment(target.scalar, &value)?;
                Ok(FableOp::Assign { target, value })
            }
            Stmt::Let(let_stmt) => {
                let name = let_stmt.name().ok_or(FableError::MalformedSyntax {
                    reason: "let statement without binding name",
                })?;
                let value_expr = let_stmt.value().ok_or(FableError::MalformedSyntax {
                    reason: "let statement without value expression",
                })?;
                let value = self.lower_expr(&value_expr)?;
                let local = self.declare_local(name, &value)?;
                Ok(FableOp::Let { local, value })
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

    fn lower_if(&mut self, if_stmt: &IfStmt) -> Result<FableOp, FableError> {
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
            condition: expect_bool_plan(self.lower_expr(&condition)?)?,
            then_program: self.lower_block(&then_block)?,
            else_program,
        })
    }

    fn lower_else(&mut self, else_clause: &ElseClause) -> Result<Program<FableOp>, FableError> {
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

    fn lower_expr(&mut self, expr: &Expr) -> Result<ExprPlan, FableError> {
        match expr {
            Expr::Literal(literal) => self.lower_literal(literal),
            Expr::Var(var) => {
                if let Some(name) = var.name()
                    && let Some(local) = self.find_local(&name)
                {
                    return Ok(local_to_expr(local));
                }
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::Field(_) => {
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::Paren(paren) => {
                let expr = paren.expr().ok_or(FableError::MalformedSyntax {
                    reason: "parenthesized expression without inner expression",
                })?;
                self.lower_expr(&expr)
            }
            Expr::Unary(unary) => self.lower_unary(unary),
            Expr::Binary(binary) => self.lower_binary(binary),
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
        let expr = match token.kind() {
            SyntaxKind::True => ExprPlan::Bool(BoolExpr::Literal(true)),
            SyntaxKind::False => ExprPlan::Bool(BoolExpr::Literal(false)),
            SyntaxKind::Null => ExprPlan::Unit(UnitExpr::Null),
            SyntaxKind::Int => ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::Literal(
                text.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.to_owned(),
                    reason: "integer literal is out of range",
                })?,
            ))),
            SyntaxKind::Float => ExprPlan::Number(NumberExpr::Float(FloatExpr::Literal(
                text.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.to_owned(),
                    reason: "float literal is invalid",
                })?,
            ))),
            SyntaxKind::Str => ExprPlan::String(StringExpr::Literal(decode_string(text)?)),
            _ => {
                return Err(FableError::MalformedSyntax {
                    reason: "literal node contained a non-literal token",
                });
            }
        };
        Ok(expr)
    }

    fn lower_unary(&mut self, unary: &UnaryExpr) -> Result<ExprPlan, FableError> {
        let operand = unary.operand().ok_or(FableError::MalformedSyntax {
            reason: "unary expression without operand",
        })?;
        let operand = self.lower_expr(&operand)?;
        match unary_op(unary)? {
            UnaryOp::Not => Ok(ExprPlan::Bool(BoolExpr::Not(Box::new(expect_bool_plan(
                operand,
            )?)))),
            UnaryOp::Neg => {
                let number = expect_number_plan(operand)?;
                match number {
                    NumberExpr::Float(_) => Ok(ExprPlan::Number(NumberExpr::Float(
                        FloatExpr::Neg(Box::new(number)),
                    ))),
                    _ => Ok(ExprPlan::Number(NumberExpr::Signed(IntExpr::Neg(
                        Box::new(number),
                    )))),
                }
            }
        }
    }

    fn lower_binary(&mut self, binary: &BinaryExpr) -> Result<ExprPlan, FableError> {
        let lhs = binary.lhs().ok_or(FableError::MalformedSyntax {
            reason: "binary expression without left operand",
        })?;
        let rhs = binary.rhs().ok_or(FableError::MalformedSyntax {
            reason: "binary expression without right operand",
        })?;
        let lhs = self.lower_expr(&lhs)?;
        let rhs = self.lower_expr(&rhs)?;

        match binary_op(binary)? {
            BinaryOp::Or => Ok(ExprPlan::Bool(BoolExpr::Or(
                Box::new(expect_bool_plan(lhs)?),
                Box::new(expect_bool_plan(rhs)?),
            ))),
            BinaryOp::And => Ok(ExprPlan::Bool(BoolExpr::And(
                Box::new(expect_bool_plan(lhs)?),
                Box::new(expect_bool_plan(rhs)?),
            ))),
            BinaryOp::Eq => Ok(ExprPlan::Bool(BoolExpr::Eq(Box::new(lhs), Box::new(rhs)))),
            BinaryOp::Neq => Ok(ExprPlan::Bool(BoolExpr::Neq(Box::new(lhs), Box::new(rhs)))),
            BinaryOp::Lt => self.lower_cmp(CmpOp::Lt, lhs, rhs),
            BinaryOp::Gt => self.lower_cmp(CmpOp::Gt, lhs, rhs),
            BinaryOp::Le => self.lower_cmp(CmpOp::Le, lhs, rhs),
            BinaryOp::Ge => self.lower_cmp(CmpOp::Ge, lhs, rhs),
            BinaryOp::Add => lower_add(lhs, rhs),
            BinaryOp::Sub => lower_sub(lhs, rhs),
        }
    }

    fn lower_cmp(&self, op: CmpOp, lhs: ExprPlan, rhs: ExprPlan) -> Result<ExprPlan, FableError> {
        Ok(ExprPlan::Bool(BoolExpr::Cmp {
            op,
            lhs: Box::new(expect_number_plan(lhs)?),
            rhs: Box::new(expect_number_plan(rhs)?),
        }))
    }

    fn lower_writable_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        if let Expr::Var(var) = expr
            && let Some(name) = var.name()
            && self.find_local(&name).is_some()
        {
            return Err(FableError::Unsupported {
                feature: "assignment to let bindings".into(),
            });
        }
        let path = self.resolve_path(expr)?;
        ensure_writable(path.scalar)?;
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

    fn declare_local(&mut self, name: String, expr: &ExprPlan) -> Result<LocalRef, FableError> {
        if name == "root" {
            return Err(FableError::ReservedLocalName { name });
        }
        let scope = self.scopes.last_mut().ok_or(FableError::MalformedProgram {
            reason: "local scope stack was empty",
        })?;
        if scope.contains_key(&name) {
            return Err(FableError::DuplicateLocal { name });
        }
        let local = self.locals.allocate(expr);
        scope.insert(name, local);
        Ok(local)
    }

    fn find_local(&self, name: &str) -> Option<LocalRef> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

#[derive(Clone, Copy)]
enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy)]
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

fn lower_add(lhs: ExprPlan, rhs: ExprPlan) -> Result<ExprPlan, FableError> {
    match (lhs, rhs) {
        (ExprPlan::String(lhs), ExprPlan::String(rhs)) => Ok(ExprPlan::String(StringExpr::Add(
            Box::new(lhs),
            Box::new(rhs),
        ))),
        (ExprPlan::Number(lhs), ExprPlan::Number(rhs)) => {
            Ok(ExprPlan::Number(add_numbers(lhs, rhs)))
        }
        (lhs, rhs) => Err(FableError::TypeMismatch {
            expected: "two strings or two numbers".into(),
            actual: binary_actual(lhs.kind_name(), rhs.kind_name()),
        }),
    }
}

fn lower_sub(lhs: ExprPlan, rhs: ExprPlan) -> Result<ExprPlan, FableError> {
    Ok(ExprPlan::Number(sub_numbers(
        expect_number_plan(lhs)?,
        expect_number_plan(rhs)?,
    )))
}

fn add_numbers(lhs: NumberExpr, rhs: NumberExpr) -> NumberExpr {
    match (lhs, rhs) {
        (NumberExpr::Float(lhs), rhs) => NumberExpr::Float(FloatExpr::Add(
            Box::new(NumberExpr::Float(lhs)),
            Box::new(rhs),
        )),
        (lhs, NumberExpr::Float(rhs)) => NumberExpr::Float(FloatExpr::Add(
            Box::new(lhs),
            Box::new(NumberExpr::Float(rhs)),
        )),
        (NumberExpr::Unsigned(lhs), NumberExpr::Unsigned(rhs)) => {
            NumberExpr::Unsigned(UIntExpr::Add(Box::new(lhs), Box::new(rhs)))
        }
        (lhs, rhs) => NumberExpr::Signed(IntExpr::Add(Box::new(lhs), Box::new(rhs))),
    }
}

fn sub_numbers(lhs: NumberExpr, rhs: NumberExpr) -> NumberExpr {
    match (lhs, rhs) {
        (NumberExpr::Float(lhs), rhs) => NumberExpr::Float(FloatExpr::Sub(
            Box::new(NumberExpr::Float(lhs)),
            Box::new(rhs),
        )),
        (lhs, NumberExpr::Float(rhs)) => NumberExpr::Float(FloatExpr::Sub(
            Box::new(lhs),
            Box::new(NumberExpr::Float(rhs)),
        )),
        (lhs, rhs) => NumberExpr::Signed(IntExpr::Sub(Box::new(lhs), Box::new(rhs))),
    }
}

fn expect_bool_plan(expr: ExprPlan) -> Result<BoolExpr, FableError> {
    match expr {
        ExprPlan::Bool(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "bool".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_number_plan(expr: ExprPlan) -> Result<NumberExpr, FableError> {
    match expr {
        ExprPlan::Number(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "number".into(),
            actual: other.kind_name(),
        }),
    }
}

fn local_to_expr(local: LocalRef) -> ExprPlan {
    match local {
        LocalRef::Unit(_) => ExprPlan::Unit(UnitExpr::Local(local)),
        LocalRef::Bool(_) => ExprPlan::Bool(BoolExpr::Local(local)),
        LocalRef::Char(_) => ExprPlan::Char(CharExpr::Local(local)),
        LocalRef::String(_) => ExprPlan::String(StringExpr::Local(local)),
        LocalRef::Signed(_) => ExprPlan::Number(NumberExpr::Signed(IntExpr::Local(local))),
        LocalRef::Unsigned(_) => ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::Local(local))),
        LocalRef::Float(_) => ExprPlan::Number(NumberExpr::Float(FloatExpr::Local(local))),
    }
}

fn path_to_expr(path: FieldPath) -> Result<ExprPlan, FableError> {
    let expr = match path.scalar {
        ScalarType::Unit => ExprPlan::Unit(UnitExpr::Read(path)),
        ScalarType::Bool => ExprPlan::Bool(BoolExpr::Read(path)),
        ScalarType::Char => ExprPlan::Char(CharExpr::Read(path)),
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            ExprPlan::String(StringExpr::Read(path))
        }
        ScalarType::F32 | ScalarType::F64 => {
            ExprPlan::Number(NumberExpr::Float(FloatExpr::Read(path)))
        }
        ScalarType::U8
        | ScalarType::U16
        | ScalarType::U32
        | ScalarType::U64
        | ScalarType::U128
        | ScalarType::USize => ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::Read(path))),
        ScalarType::I8
        | ScalarType::I16
        | ScalarType::I32
        | ScalarType::I64
        | ScalarType::I128
        | ScalarType::ISize => ExprPlan::Number(NumberExpr::Signed(IntExpr::Read(path))),
        _ => {
            return Err(FableError::Unsupported {
                feature: format!("reading {:?}", path.scalar),
            });
        }
    };
    Ok(expr)
}

fn validate_assignment(scalar: ScalarType, expr: &ExprPlan) -> Result<(), FableError> {
    let ok = match scalar {
        ScalarType::Unit => matches!(expr, ExprPlan::Unit(_)),
        ScalarType::Bool => matches!(expr, ExprPlan::Bool(_)),
        ScalarType::Char => matches!(expr, ExprPlan::Char(_) | ExprPlan::String(_)),
        ScalarType::String | ScalarType::CowStr => {
            matches!(expr, ExprPlan::String(_) | ExprPlan::Char(_))
        }
        ScalarType::F32
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
        | ScalarType::ISize => matches!(expr, ExprPlan::Number(_)),
        _ => {
            return Err(FableError::Unsupported {
                feature: format!("writing {scalar:?}"),
            });
        }
    };

    if ok {
        Ok(())
    } else {
        Err(FableError::TypeMismatch {
            expected: format!("value assignable to {scalar:?}"),
            actual: expr.kind_name(),
        })
    }
}

#[derive(Default)]
struct LocalSlots {
    units: Vec<bool>,
    bools: Vec<Option<bool>>,
    chars: Vec<Option<char>>,
    strings: Vec<Option<String>>,
    signed: Vec<Option<i128>>,
    unsigned: Vec<Option<u128>>,
    floats: Vec<Option<f64>>,
}

struct FableInterp {
    root: PtrMut,
    locals: LocalSlots,
}

impl<'program> Step<'program, BlockRef, FableOp> for FableInterp {
    type Error = FableError;
    type Continuation = ();

    fn step(
        &mut self,
        op: &'program FableOp,
    ) -> Result<Control<'program, BlockRef, FableOp>, Self::Error> {
        match op {
            FableOp::Let { local, value } => {
                self.init_local(*local, value)?;
                Ok(Control::Continue)
            }
            FableOp::Assign { target, value } => {
                let ptr = unsafe { target.ptr_mut(self.root) };
                unsafe { self.write_scalar(target.scalar, ptr, value) }?;
                Ok(Control::Continue)
            }
            FableOp::Eval(expr) => {
                self.eval_expr(expr)?;
                Ok(Control::Continue)
            }
            FableOp::Branch {
                condition,
                then_program,
                else_program,
            } => {
                let condition = self.eval_bool(condition)?;
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
    fn init_local(&mut self, local: LocalRef, expr: &ExprPlan) -> Result<(), FableError> {
        match local {
            LocalRef::Unit(index) => {
                self.eval_unit(expect_unit_expr(expr)?)?;
                set_slot(&mut self.locals.units, index, true);
            }
            LocalRef::Bool(index) => {
                let value = self.eval_bool(expect_bool_expr(expr)?)?;
                set_slot(&mut self.locals.bools, index, Some(value));
            }
            LocalRef::Char(index) => {
                let value = self.eval_char_assign(expr)?;
                set_slot(&mut self.locals.chars, index, Some(value));
            }
            LocalRef::String(index) => {
                let value = self.eval_string_assign(expr)?;
                set_slot(&mut self.locals.strings, index, Some(value));
            }
            LocalRef::Signed(index) => {
                let value = self.eval_number_as_i128(expect_number_expr(expr)?)?;
                set_slot(&mut self.locals.signed, index, Some(value));
            }
            LocalRef::Unsigned(index) => {
                let value = self.eval_number_as_u128(expect_number_expr(expr)?)?;
                set_slot(&mut self.locals.unsigned, index, Some(value));
            }
            LocalRef::Float(index) => {
                let value = self.eval_number_as_f64(expect_number_expr(expr)?)?;
                set_slot(&mut self.locals.floats, index, Some(value));
            }
        }
        Ok(())
    }

    fn eval_expr(&self, expr: &ExprPlan) -> Result<(), FableError> {
        match expr {
            ExprPlan::Unit(expr) => self.eval_unit(expr),
            ExprPlan::Bool(expr) => self.eval_bool(expr).map(drop),
            ExprPlan::Char(expr) => self.eval_char(expr).map(drop),
            ExprPlan::String(expr) => self.eval_string(expr).map(drop),
            ExprPlan::Number(expr) => self.eval_number_for_effect(expr),
        }
    }

    fn eval_unit(&self, expr: &UnitExpr) -> Result<(), FableError> {
        match expr {
            UnitExpr::Null => Ok(()),
            UnitExpr::Read(path) => {
                let _ = unsafe { path.ptr_const(self.root.as_const()) };
                Ok(())
            }
            UnitExpr::Local(local) => self.local_unit(*local),
        }
    }

    fn eval_bool(&self, expr: &BoolExpr) -> Result<bool, FableError> {
        match expr {
            BoolExpr::Literal(value) => Ok(*value),
            BoolExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                Ok(*unsafe { ptr.get::<bool>() })
            }
            BoolExpr::Local(local) => self.local_bool(*local),
            BoolExpr::Not(expr) => Ok(!self.eval_bool(expr)?),
            BoolExpr::And(lhs, rhs) => {
                if !self.eval_bool(lhs)? {
                    return Ok(false);
                }
                self.eval_bool(rhs)
            }
            BoolExpr::Or(lhs, rhs) => {
                if self.eval_bool(lhs)? {
                    return Ok(true);
                }
                self.eval_bool(rhs)
            }
            BoolExpr::Eq(lhs, rhs) => self.exprs_equal(lhs, rhs),
            BoolExpr::Neq(lhs, rhs) => Ok(!self.exprs_equal(lhs, rhs)?),
            BoolExpr::Cmp { op, lhs, rhs } => {
                let ordering = self.compare_numbers(lhs, rhs)?;
                Ok(match op {
                    CmpOp::Lt => ordering == Ordering::Less,
                    CmpOp::Gt => ordering == Ordering::Greater,
                    CmpOp::Le => matches!(ordering, Ordering::Less | Ordering::Equal),
                    CmpOp::Ge => matches!(ordering, Ordering::Greater | Ordering::Equal),
                })
            }
        }
    }

    fn eval_char(&self, expr: &CharExpr) -> Result<char, FableError> {
        match expr {
            CharExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                Ok(*unsafe { ptr.get::<char>() })
            }
            CharExpr::Local(local) => self.local_char(*local),
        }
    }

    fn eval_string(&self, expr: &StringExpr) -> Result<String, FableError> {
        match expr {
            StringExpr::Literal(value) => Ok(value.clone()),
            StringExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                unsafe { self.read_string_path(path, ptr) }
            }
            StringExpr::Local(local) => self.local_string(*local),
            StringExpr::Add(lhs, rhs) => {
                let mut lhs = self.eval_string(lhs)?;
                lhs.push_str(&self.eval_string(rhs)?);
                Ok(lhs)
            }
        }
    }

    unsafe fn read_string_path(
        &self,
        path: &FieldPath,
        ptr: PtrConst,
    ) -> Result<String, FableError> {
        match path.scalar {
            ScalarType::Str if path.shape.is_type::<&'static str>() => {
                Ok((*unsafe { ptr.get::<&'static str>() }).to_owned())
            }
            ScalarType::String => Ok(unsafe { ptr.get::<String>() }.clone()),
            ScalarType::CowStr => Ok(unsafe { ptr.get::<Cow<'static, str>>() }
                .clone()
                .into_owned()),
            _ => Err(FableError::Unsupported {
                feature: format!("reading {:?}", path.scalar),
            }),
        }
    }

    fn eval_number_for_effect(&self, expr: &NumberExpr) -> Result<(), FableError> {
        match expr {
            NumberExpr::Signed(expr) => self.eval_i128(expr).map(drop),
            NumberExpr::Unsigned(expr) => self.eval_u128(expr).map(drop),
            NumberExpr::Float(expr) => self.eval_f64(expr).map(drop),
        }
    }

    fn eval_i128(&self, expr: &IntExpr) -> Result<i128, FableError> {
        match expr {
            IntExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                unsafe { self.read_signed_path(path.scalar, ptr) }
            }
            IntExpr::Local(local) => self.local_i128(*local),
            IntExpr::Neg(expr) => {
                let value = self.eval_number_as_i128(expr)?;
                value
                    .checked_neg()
                    .ok_or_else(|| number_out_of_range(ScalarType::I128, format!("-{value}")))
            }
            IntExpr::Add(lhs, rhs) => {
                let lhs = self.eval_number_as_i128(lhs)?;
                let rhs = self.eval_number_as_i128(rhs)?;
                lhs.checked_add(rhs)
                    .ok_or_else(|| number_out_of_range(ScalarType::I128, format!("{lhs} + {rhs}")))
            }
            IntExpr::Sub(lhs, rhs) => {
                let lhs = self.eval_number_as_i128(lhs)?;
                let rhs = self.eval_number_as_i128(rhs)?;
                lhs.checked_sub(rhs)
                    .ok_or_else(|| number_out_of_range(ScalarType::I128, format!("{lhs} - {rhs}")))
            }
        }
    }

    unsafe fn read_signed_path(
        &self,
        scalar: ScalarType,
        ptr: PtrConst,
    ) -> Result<i128, FableError> {
        let value = match scalar {
            ScalarType::I8 => (*unsafe { ptr.get::<i8>() }).into(),
            ScalarType::I16 => (*unsafe { ptr.get::<i16>() }).into(),
            ScalarType::I32 => (*unsafe { ptr.get::<i32>() }).into(),
            ScalarType::I64 => (*unsafe { ptr.get::<i64>() }).into(),
            ScalarType::I128 => *unsafe { ptr.get::<i128>() },
            ScalarType::ISize => (*unsafe { ptr.get::<isize>() }) as i128,
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "signed read path did not point to a signed scalar",
                });
            }
        };
        Ok(value)
    }

    fn eval_u128(&self, expr: &UIntExpr) -> Result<u128, FableError> {
        match expr {
            UIntExpr::Literal(value) => Ok(*value),
            UIntExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                unsafe { self.read_unsigned_path(path.scalar, ptr) }
            }
            UIntExpr::Local(local) => self.local_u128(*local),
            UIntExpr::Add(lhs, rhs) => {
                let lhs = self.eval_u128(lhs)?;
                let rhs = self.eval_u128(rhs)?;
                lhs.checked_add(rhs)
                    .ok_or_else(|| number_out_of_range(ScalarType::U128, format!("{lhs} + {rhs}")))
            }
        }
    }

    unsafe fn read_unsigned_path(
        &self,
        scalar: ScalarType,
        ptr: PtrConst,
    ) -> Result<u128, FableError> {
        let value = match scalar {
            ScalarType::U8 => (*unsafe { ptr.get::<u8>() }).into(),
            ScalarType::U16 => (*unsafe { ptr.get::<u16>() }).into(),
            ScalarType::U32 => (*unsafe { ptr.get::<u32>() }).into(),
            ScalarType::U64 => (*unsafe { ptr.get::<u64>() }).into(),
            ScalarType::U128 => *unsafe { ptr.get::<u128>() },
            ScalarType::USize => (*unsafe { ptr.get::<usize>() }) as u128,
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "unsigned read path did not point to an unsigned scalar",
                });
            }
        };
        Ok(value)
    }

    fn eval_f64(&self, expr: &FloatExpr) -> Result<f64, FableError> {
        match expr {
            FloatExpr::Literal(value) => Ok(*value),
            FloatExpr::Read(path) => {
                let ptr = unsafe { path.ptr_const(self.root.as_const()) };
                match path.scalar {
                    ScalarType::F32 => Ok((*unsafe { ptr.get::<f32>() }).into()),
                    ScalarType::F64 => Ok(*unsafe { ptr.get::<f64>() }),
                    _ => Err(FableError::MalformedProgram {
                        reason: "float read path did not point to a float scalar",
                    }),
                }
            }
            FloatExpr::Local(local) => self.local_f64(*local),
            FloatExpr::Neg(expr) => Ok(-self.eval_number_as_f64(expr)?),
            FloatExpr::Add(lhs, rhs) => {
                Ok(self.eval_number_as_f64(lhs)? + self.eval_number_as_f64(rhs)?)
            }
            FloatExpr::Sub(lhs, rhs) => {
                Ok(self.eval_number_as_f64(lhs)? - self.eval_number_as_f64(rhs)?)
            }
        }
    }

    fn eval_number_as_i128(&self, expr: &NumberExpr) -> Result<i128, FableError> {
        match expr {
            NumberExpr::Signed(expr) => self.eval_i128(expr),
            NumberExpr::Unsigned(expr) => {
                let value = self.eval_u128(expr)?;
                i128::try_from(value)
                    .map_err(|_| number_out_of_range(ScalarType::I128, value.to_string()))
            }
            NumberExpr::Float(_) => Err(FableError::TypeMismatch {
                expected: "integer".into(),
                actual: "float",
            }),
        }
    }

    fn eval_number_as_u128(&self, expr: &NumberExpr) -> Result<u128, FableError> {
        match expr {
            NumberExpr::Unsigned(expr) => self.eval_u128(expr),
            NumberExpr::Signed(expr) => {
                let value = self.eval_i128(expr)?;
                u128::try_from(value)
                    .map_err(|_| number_out_of_range(ScalarType::U128, value.to_string()))
            }
            NumberExpr::Float(_) => Err(FableError::TypeMismatch {
                expected: "unsigned integer".into(),
                actual: "float",
            }),
        }
    }

    fn eval_number_as_f64(&self, expr: &NumberExpr) -> Result<f64, FableError> {
        match expr {
            NumberExpr::Signed(expr) => Ok(self.eval_i128(expr)? as f64),
            NumberExpr::Unsigned(expr) => Ok(self.eval_u128(expr)? as f64),
            NumberExpr::Float(expr) => self.eval_f64(expr),
        }
    }

    fn compare_numbers(&self, lhs: &NumberExpr, rhs: &NumberExpr) -> Result<Ordering, FableError> {
        match (lhs, rhs) {
            (NumberExpr::Float(_), _) | (_, NumberExpr::Float(_)) => {
                compare_f64(self.eval_number_as_f64(lhs)?, self.eval_number_as_f64(rhs)?)
            }
            (NumberExpr::Signed(lhs), NumberExpr::Signed(rhs)) => {
                Ok(self.eval_i128(lhs)?.cmp(&self.eval_i128(rhs)?))
            }
            (NumberExpr::Unsigned(lhs), NumberExpr::Unsigned(rhs)) => {
                Ok(self.eval_u128(lhs)?.cmp(&self.eval_u128(rhs)?))
            }
            (NumberExpr::Signed(lhs), NumberExpr::Unsigned(rhs)) => {
                let lhs = self.eval_i128(lhs)?;
                let rhs = self.eval_u128(rhs)?;
                if lhs < 0 {
                    Ok(Ordering::Less)
                } else {
                    Ok((lhs as u128).cmp(&rhs))
                }
            }
            (NumberExpr::Unsigned(lhs), NumberExpr::Signed(rhs)) => {
                let lhs = self.eval_u128(lhs)?;
                let rhs = self.eval_i128(rhs)?;
                if rhs < 0 {
                    Ok(Ordering::Greater)
                } else {
                    Ok(lhs.cmp(&(rhs as u128)))
                }
            }
        }
    }

    fn exprs_equal(&self, lhs: &ExprPlan, rhs: &ExprPlan) -> Result<bool, FableError> {
        match (lhs, rhs) {
            (ExprPlan::Unit(lhs), ExprPlan::Unit(rhs)) => {
                self.eval_unit(lhs)?;
                self.eval_unit(rhs)?;
                Ok(true)
            }
            (ExprPlan::Bool(lhs), ExprPlan::Bool(rhs)) => {
                Ok(self.eval_bool(lhs)? == self.eval_bool(rhs)?)
            }
            (ExprPlan::Char(lhs), ExprPlan::Char(rhs)) => {
                Ok(self.eval_char(lhs)? == self.eval_char(rhs)?)
            }
            (ExprPlan::String(lhs), ExprPlan::String(rhs)) => {
                Ok(self.eval_string(lhs)? == self.eval_string(rhs)?)
            }
            (ExprPlan::Char(lhs), ExprPlan::String(rhs)) => Ok(string_is_char(
                &self.eval_string(rhs)?,
                self.eval_char(lhs)?,
            )),
            (ExprPlan::String(lhs), ExprPlan::Char(rhs)) => Ok(string_is_char(
                &self.eval_string(lhs)?,
                self.eval_char(rhs)?,
            )),
            (ExprPlan::Number(lhs), ExprPlan::Number(rhs)) => {
                Ok(self.compare_numbers(lhs, rhs)? == Ordering::Equal)
            }
            _ => Ok(false),
        }
    }

    unsafe fn write_scalar(
        &self,
        scalar: ScalarType,
        ptr: PtrMut,
        expr: &ExprPlan,
    ) -> Result<(), FableError> {
        match scalar {
            ScalarType::Unit => self.eval_unit(expect_unit_expr(expr)?)?,
            ScalarType::Bool => {
                *unsafe { ptr.as_mut::<bool>() } = self.eval_bool(expect_bool_expr(expr)?)?;
            }
            ScalarType::Char => {
                *unsafe { ptr.as_mut::<char>() } = self.eval_char_assign(expr)?;
            }
            ScalarType::String => {
                *unsafe { ptr.as_mut::<String>() } = self.eval_string_assign(expr)?;
            }
            ScalarType::CowStr => {
                *unsafe { ptr.as_mut::<Cow<'static, str>>() } =
                    Cow::Owned(self.eval_string_assign(expr)?);
            }
            ScalarType::F32 => {
                *unsafe { ptr.as_mut::<f32>() } =
                    self.eval_number_as_f64(expect_number_expr(expr)?)? as f32;
            }
            ScalarType::F64 => {
                *unsafe { ptr.as_mut::<f64>() } =
                    self.eval_number_as_f64(expect_number_expr(expr)?)?;
            }
            ScalarType::U8 => unsafe { self.write_unsigned::<u8>(ptr, scalar, expr) }?,
            ScalarType::U16 => unsafe { self.write_unsigned::<u16>(ptr, scalar, expr) }?,
            ScalarType::U32 => unsafe { self.write_unsigned::<u32>(ptr, scalar, expr) }?,
            ScalarType::U64 => unsafe { self.write_unsigned::<u64>(ptr, scalar, expr) }?,
            ScalarType::U128 => unsafe { self.write_unsigned::<u128>(ptr, scalar, expr) }?,
            ScalarType::USize => unsafe { self.write_unsigned::<usize>(ptr, scalar, expr) }?,
            ScalarType::I8 => unsafe { self.write_signed::<i8>(ptr, scalar, expr) }?,
            ScalarType::I16 => unsafe { self.write_signed::<i16>(ptr, scalar, expr) }?,
            ScalarType::I32 => unsafe { self.write_signed::<i32>(ptr, scalar, expr) }?,
            ScalarType::I64 => unsafe { self.write_signed::<i64>(ptr, scalar, expr) }?,
            ScalarType::I128 => unsafe { self.write_signed::<i128>(ptr, scalar, expr) }?,
            ScalarType::ISize => unsafe { self.write_signed::<isize>(ptr, scalar, expr) }?,
            _ => {
                return Err(FableError::Unsupported {
                    feature: format!("writing {scalar:?}"),
                });
            }
        }
        Ok(())
    }

    fn eval_char_assign(&self, expr: &ExprPlan) -> Result<char, FableError> {
        match expr {
            ExprPlan::Char(expr) => self.eval_char(expr),
            ExprPlan::String(expr) => expect_single_char(self.eval_string(expr)?),
            other => Err(FableError::TypeMismatch {
                expected: "char".into(),
                actual: other.kind_name(),
            }),
        }
    }

    fn eval_string_assign(&self, expr: &ExprPlan) -> Result<String, FableError> {
        match expr {
            ExprPlan::String(expr) => self.eval_string(expr),
            ExprPlan::Char(expr) => Ok(self.eval_char(expr)?.to_string()),
            other => Err(FableError::TypeMismatch {
                expected: "string".into(),
                actual: other.kind_name(),
            }),
        }
    }

    unsafe fn write_unsigned<T>(
        &self,
        ptr: PtrMut,
        target: ScalarType,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<u128>,
    {
        let value = self.eval_number_as_u128(expect_number_expr(expr)?)?;
        let converted =
            T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
        *unsafe { ptr.as_mut::<T>() } = converted;
        Ok(())
    }

    unsafe fn write_signed<T>(
        &self,
        ptr: PtrMut,
        target: ScalarType,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<i128>,
    {
        let value = self.eval_number_as_i128(expect_number_expr(expr)?)?;
        let converted =
            T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
        *unsafe { ptr.as_mut::<T>() } = converted;
        Ok(())
    }

    fn local_unit(&self, local: LocalRef) -> Result<(), FableError> {
        let LocalRef::Unit(index) = local else {
            return Err(local_kind_mismatch("unit", local));
        };
        if self.locals.units.get(index).copied().unwrap_or(false) {
            Ok(())
        } else {
            Err(uninitialized_local())
        }
    }

    fn local_bool(&self, local: LocalRef) -> Result<bool, FableError> {
        let LocalRef::Bool(index) = local else {
            return Err(local_kind_mismatch("bool", local));
        };
        self.locals
            .bools
            .get(index)
            .and_then(|value| *value)
            .ok_or_else(uninitialized_local)
    }

    fn local_char(&self, local: LocalRef) -> Result<char, FableError> {
        let LocalRef::Char(index) = local else {
            return Err(local_kind_mismatch("char", local));
        };
        self.locals
            .chars
            .get(index)
            .and_then(|value| *value)
            .ok_or_else(uninitialized_local)
    }

    fn local_string(&self, local: LocalRef) -> Result<String, FableError> {
        let LocalRef::String(index) = local else {
            return Err(local_kind_mismatch("string", local));
        };
        self.locals
            .strings
            .get(index)
            .and_then(|value| value.as_ref())
            .cloned()
            .ok_or_else(uninitialized_local)
    }

    fn local_i128(&self, local: LocalRef) -> Result<i128, FableError> {
        let LocalRef::Signed(index) = local else {
            return Err(local_kind_mismatch("signed number", local));
        };
        self.locals
            .signed
            .get(index)
            .and_then(|value| *value)
            .ok_or_else(uninitialized_local)
    }

    fn local_u128(&self, local: LocalRef) -> Result<u128, FableError> {
        let LocalRef::Unsigned(index) = local else {
            return Err(local_kind_mismatch("unsigned number", local));
        };
        self.locals
            .unsigned
            .get(index)
            .and_then(|value| *value)
            .ok_or_else(uninitialized_local)
    }

    fn local_f64(&self, local: LocalRef) -> Result<f64, FableError> {
        let LocalRef::Float(index) = local else {
            return Err(local_kind_mismatch("float", local));
        };
        self.locals
            .floats
            .get(index)
            .and_then(|value| *value)
            .ok_or_else(uninitialized_local)
    }
}

fn set_slot<T: Default>(slots: &mut Vec<T>, index: usize, value: T) {
    if slots.len() <= index {
        slots.resize_with(index + 1, T::default);
    }
    slots[index] = value;
}

fn local_kind_mismatch(expected: &'static str, actual: LocalRef) -> FableError {
    FableError::TypeMismatch {
        expected: expected.into(),
        actual: actual.kind_name(),
    }
}

fn uninitialized_local() -> FableError {
    FableError::MalformedProgram {
        reason: "local read before initialization",
    }
}

fn expect_unit_expr(expr: &ExprPlan) -> Result<&UnitExpr, FableError> {
    match expr {
        ExprPlan::Unit(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "unit".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_bool_expr(expr: &ExprPlan) -> Result<&BoolExpr, FableError> {
    match expr {
        ExprPlan::Bool(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "bool".into(),
            actual: other.kind_name(),
        }),
    }
}

fn expect_number_expr(expr: &ExprPlan) -> Result<&NumberExpr, FableError> {
    match expr {
        ExprPlan::Number(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "number".into(),
            actual: other.kind_name(),
        }),
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

fn ensure_writable(scalar: ScalarType) -> Result<(), FableError> {
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

fn compare_f64(lhs: f64, rhs: f64) -> Result<Ordering, FableError> {
    lhs.partial_cmp(&rhs)
        .ok_or_else(|| FableError::TypeMismatch {
            expected: "ordered float".into(),
            actual: "NaN",
        })
}

fn expect_single_char(value: String) -> Result<char, FableError> {
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

fn string_is_char(value: &str, ch: char) -> bool {
    let mut chars = value.chars();
    chars.next() == Some(ch) && chars.next().is_none()
}

fn number_out_of_range(target: ScalarType, value: String) -> FableError {
    FableError::NumberOutOfRange { target, value }
}

fn binary_actual(lhs: &'static str, rhs: &'static str) -> &'static str {
    if lhs == rhs {
        lhs
    } else {
        "mixed expression types"
    }
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
    fn applies_typed_scalar_let_bindings() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                let next_age = root.user.age + 1;
                let next_visits = root.visits + 2;
                let next_score = root.score + 0.5;
                let label = root.user.name + " Lovelace";
                let mark = root.marker;
                let adult = next_age >= 18;

                root.user.age = next_age;
                root.visits = next_visits;
                root.score = next_score;
                root.user.name = label;
                root.marker = mark;

                if adult {
                    root.user.active = true;
                }
            "#,
        )
        .unwrap();

        assert_eq!(value.user.age, 18);
        assert_eq!(value.visits, 3);
        assert_eq!(value.score, 2.0);
        assert_eq!(value.user.name, "Ada Lovelace");
        assert_eq!(value.marker, 'a');
        assert!(value.user.active);
    }

    #[test]
    fn lets_are_block_scoped() {
        let err = compile_err(
            r#"
                if true {
                    let inside = 1;
                }
                root.user.age = inside;
            "#,
        );

        assert!(matches!(
            err,
            FableError::ExpectedRoot {
                found
            } if found == "inside"
        ));
    }

    #[test]
    fn lets_can_shadow_outer_bindings_in_child_scopes() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                let label = "outer";
                if true {
                    let label = "inner";
                    root.user.name = label;
                }
                root.user.name = root.user.name + " " + label;
            "#,
        )
        .unwrap();

        assert_eq!(value.user.name, "inner outer");
    }

    #[test]
    fn reports_duplicate_local_in_same_scope() {
        let err = compile_err(
            r#"
                let age = 1;
                let age = 2;
            "#,
        );

        assert!(matches!(
            err,
            FableError::DuplicateLocal {
                name
            } if name == "age"
        ));
    }

    #[test]
    fn reports_reserved_root_local_name() {
        let err = compile_err("let root = 1");

        assert!(matches!(
            err,
            FableError::ReservedLocalName {
                name
            } if name == "root"
        ));
    }

    #[test]
    fn rejects_assignment_to_let_bindings() {
        let err = compile_err(
            r#"
                let age = 1;
                age = 2;
            "#,
        );

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "assignment to let bindings"
        ));
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
    fn reports_type_mismatches_during_lowering() {
        let err = compile_err(r#"root.user.age = "old""#);

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
