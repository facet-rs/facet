use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::ptr::copy_nonoverlapping;

use facet_core::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, ScalarType, Shape, StructKind, Type, UserType,
};
use weavy::{BlockRef, Control, DenseLowered, Program, RunError, RunStats, Step};

use crate::SyntaxKind;
use crate::ast::{
    self, AstNode, BinaryExpr, Block, CallExpr, ElseClause, Expr, IfStmt, Stmt, StructLiteral,
    UnaryExpr,
};
use crate::{ParseError, parse};

/// A reusable lowered Fable program for `T`.
///
/// Build a plan once with [`FablePlan::compile`], then apply it repeatedly to
/// mutable values of the same Facet-reflected type.
pub struct FablePlan<T> {
    lowered: DenseLowered<FableOp>,
    _marker: PhantomData<fn() -> T>,
}

/// Host-call registry used while lowering Fable calls.
#[derive(Clone, Debug)]
pub struct FableIntrinsics {
    signatures: Vec<IntrinsicSignature>,
}

/// Host function for `string -> string` intrinsics.
pub type FableStringUnary = fn(&str) -> Result<String, FableError>;
/// Host function for `(string, string) -> bool` intrinsics.
pub type FableStringBinaryPredicate = fn(&str, &str) -> Result<bool, FableError>;
/// Host function for `signed number -> signed number` intrinsics.
pub type FableSignedUnary = fn(i128) -> Result<i128, FableError>;
/// Host function for `unsigned number -> unsigned number` intrinsics.
pub type FableUnsignedUnary = fn(u128) -> Result<u128, FableError>;
/// Host function for `float -> float` intrinsics.
pub type FableFloatUnary = fn(f64) -> Result<f64, FableError>;
/// Host function for `field -> string` intrinsics.
pub type FableFieldStringUnary = for<'field> fn(FableField<'field>) -> Result<String, FableError>;
/// Host function for `field -> bool` intrinsics.
pub type FableFieldBoolUnary = for<'field> fn(FableField<'field>) -> Result<bool, FableError>;
/// Host function for `field_mut -> unit` intrinsics.
pub type FableFieldMutUnary = for<'field> fn(FableFieldMut<'field>) -> Result<(), FableError>;

/// Read-only handle passed to field-aware Fable host intrinsics.
pub struct FableField<'field> {
    path: &'field str,
    shape: &'static Shape,
    scalar: ScalarType,
    ptr: PtrConst,
}

impl<'field> fmt::Debug for FableField<'field> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FableField")
            .field("path", &self.path)
            .field("shape", &self.shape)
            .field("scalar", &self.scalar)
            .finish_non_exhaustive()
    }
}

impl<'field> FableField<'field> {
    /// Dot-separated field path, starting at `root`.
    #[must_use]
    pub fn path(&self) -> &'field str {
        self.path
    }

    /// Reflected shape of the referenced field.
    #[must_use]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Scalar kind of the referenced field.
    #[must_use]
    pub fn scalar(&self) -> ScalarType {
        self.scalar
    }

    /// Read the field as a bool.
    pub fn read_bool(&self) -> Result<bool, FableError> {
        match self.scalar {
            ScalarType::Bool => Ok(*unsafe { self.ptr.get::<bool>() }),
            _ => Err(FableError::TypeMismatch {
                expected: "bool".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Read the field as a char.
    pub fn read_char(&self) -> Result<char, FableError> {
        match self.scalar {
            ScalarType::Char => Ok(*unsafe { self.ptr.get::<char>() }),
            _ => Err(FableError::TypeMismatch {
                expected: "char".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Read the field as an owned string.
    pub fn read_string(&self) -> Result<String, FableError> {
        match self.scalar {
            ScalarType::Str if self.shape.is_type::<&'static str>() => {
                Ok((*unsafe { self.ptr.get::<&'static str>() }).to_owned())
            }
            ScalarType::String => Ok(unsafe { self.ptr.get::<String>() }.clone()),
            ScalarType::CowStr => Ok(unsafe { self.ptr.get::<Cow<'static, str>>() }
                .clone()
                .into_owned()),
            _ => Err(FableError::TypeMismatch {
                expected: "string".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Read the field as a signed integer.
    pub fn read_i128(&self) -> Result<i128, FableError> {
        read_signed_scalar(self.scalar, self.ptr)
    }

    /// Read the field as an unsigned integer.
    pub fn read_u128(&self) -> Result<u128, FableError> {
        read_unsigned_scalar(self.scalar, self.ptr)
    }

    /// Read the field as an f64.
    pub fn read_f64(&self) -> Result<f64, FableError> {
        read_float_scalar(self.scalar, self.ptr)
    }
}

/// Mutable handle passed to field-aware Fable host intrinsics.
pub struct FableFieldMut<'field> {
    path: &'field str,
    shape: &'static Shape,
    scalar: ScalarType,
    ptr: PtrMut,
}

impl<'field> fmt::Debug for FableFieldMut<'field> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FableFieldMut")
            .field("path", &self.path)
            .field("shape", &self.shape)
            .field("scalar", &self.scalar)
            .finish_non_exhaustive()
    }
}

impl<'field> FableFieldMut<'field> {
    /// Dot-separated field path, starting at `root`.
    #[must_use]
    pub fn path(&self) -> &'field str {
        self.path
    }

    /// Reflected shape of the referenced field.
    #[must_use]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Scalar kind of the referenced field.
    #[must_use]
    pub fn scalar(&self) -> ScalarType {
        self.scalar
    }

    /// Read the field as a bool.
    pub fn read_bool(&self) -> Result<bool, FableError> {
        FableField {
            path: self.path,
            shape: self.shape,
            scalar: self.scalar,
            ptr: self.ptr.as_const(),
        }
        .read_bool()
    }

    /// Read the field as a char.
    pub fn read_char(&self) -> Result<char, FableError> {
        FableField {
            path: self.path,
            shape: self.shape,
            scalar: self.scalar,
            ptr: self.ptr.as_const(),
        }
        .read_char()
    }

    /// Read the field as an owned string.
    pub fn read_string(&self) -> Result<String, FableError> {
        FableField {
            path: self.path,
            shape: self.shape,
            scalar: self.scalar,
            ptr: self.ptr.as_const(),
        }
        .read_string()
    }

    /// Read the field as a signed integer.
    pub fn read_i128(&self) -> Result<i128, FableError> {
        read_signed_scalar(self.scalar, self.ptr.as_const())
    }

    /// Read the field as an unsigned integer.
    pub fn read_u128(&self) -> Result<u128, FableError> {
        read_unsigned_scalar(self.scalar, self.ptr.as_const())
    }

    /// Read the field as an f64.
    pub fn read_f64(&self) -> Result<f64, FableError> {
        read_float_scalar(self.scalar, self.ptr.as_const())
    }

    /// Write the field as a bool.
    pub fn write_bool(&mut self, value: bool) -> Result<(), FableError> {
        match self.scalar {
            ScalarType::Bool => {
                *unsafe { self.ptr.as_mut::<bool>() } = value;
                Ok(())
            }
            _ => Err(FableError::TypeMismatch {
                expected: "bool".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Write the field as a char.
    pub fn write_char(&mut self, value: char) -> Result<(), FableError> {
        match self.scalar {
            ScalarType::Char => {
                *unsafe { self.ptr.as_mut::<char>() } = value;
                Ok(())
            }
            _ => Err(FableError::TypeMismatch {
                expected: "char".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Write the field as an owned string.
    pub fn write_string(&mut self, value: impl Into<String>) -> Result<(), FableError> {
        match self.scalar {
            ScalarType::String => {
                *unsafe { self.ptr.as_mut::<String>() } = value.into();
                Ok(())
            }
            ScalarType::CowStr => {
                *unsafe { self.ptr.as_mut::<Cow<'static, str>>() } = Cow::Owned(value.into());
                Ok(())
            }
            ScalarType::Str => Err(FableError::Unsupported {
                feature: "writing Str".into(),
            }),
            _ => Err(FableError::TypeMismatch {
                expected: "string".into(),
                actual: scalar_kind_name(self.scalar),
            }),
        }
    }

    /// Write the field as a signed integer.
    pub fn write_i128(&mut self, value: i128) -> Result<(), FableError> {
        write_signed_scalar(self.scalar, self.ptr, value)
    }

    /// Write the field as an unsigned integer.
    pub fn write_u128(&mut self, value: u128) -> Result<(), FableError> {
        write_unsigned_scalar(self.scalar, self.ptr, value)
    }

    /// Write the field as an f64.
    pub fn write_f64(&mut self, value: f64) -> Result<(), FableError> {
        write_float_scalar(self.scalar, self.ptr, value)
    }
}

impl<T> FablePlan<T>
where
    T: Facet<'static>,
{
    /// Parse and lower Fable source for values of type `T`.
    pub fn compile(src: &str) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, &FableIntrinsics::standard())
    }

    /// Parse and lower Fable source with an explicit host-call registry.
    pub fn compile_with_intrinsics(
        src: &str,
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        let parsed = parse(src);
        if !parsed.errors().is_empty() {
            return Err(FableError::Parse {
                errors: parsed.errors().to_vec(),
            });
        }

        let root = ast::Root::cast(parsed.syntax().clone()).ok_or(FableError::MalformedSyntax {
            reason: "parse root was not a Fable root node",
        })?;
        let mut lowerer = Lowerer::new(T::SHAPE, intrinsics);
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

/// Compile with explicit host intrinsics and immediately apply to `value`.
pub fn apply_with_intrinsics<T>(
    value: &mut T,
    src: &str,
    intrinsics: &FableIntrinsics,
) -> Result<(), FableError>
where
    T: Facet<'static>,
{
    FablePlan::<T>::compile_with_intrinsics(src, intrinsics)?.apply(value)
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
    /// A named type was not visible from the root shape.
    UnknownType {
        /// Requested type name.
        name: String,
    },
    /// A named type matched more than one shape visible from the root shape.
    AmbiguousType {
        /// Requested type name.
        name: String,
    },
    /// A struct literal omitted a required field.
    MissingStructField {
        /// Shape being constructed.
        shape: &'static Shape,
        /// Missing field name.
        field: String,
    },
    /// A struct literal tried to construct a non-POD type directly.
    NonPodStructLiteral {
        /// Shape being constructed.
        shape: &'static Shape,
    },
    /// An indexed path step was out of bounds at runtime.
    IndexOutOfBounds {
        /// Path prefix being indexed.
        path: String,
        /// Requested index.
        index: usize,
        /// Runtime sequence length.
        len: usize,
    },
    /// A typed expression was used in a context that expects another type.
    TypeMismatch {
        /// Expected expression type.
        expected: String,
        /// Actual expression type.
        actual: &'static str,
    },
    /// An intrinsic call was malformed.
    InvalidCall {
        /// Intrinsic name.
        function: &'static str,
        /// Reason it was rejected.
        reason: &'static str,
    },
    /// A host intrinsic registration was invalid.
    InvalidIntrinsic {
        /// Intrinsic name.
        name: &'static str,
        /// Reason it was rejected.
        reason: &'static str,
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
    /// A struct literal specified the same field more than once.
    DuplicateStructField {
        /// Duplicate field name.
        field: String,
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
            FableError::UnknownType { name } => {
                write!(f, "Fable could not resolve type {name}")
            }
            FableError::AmbiguousType { name } => {
                write!(f, "Fable type name {name} is ambiguous")
            }
            FableError::MissingStructField { shape, field } => {
                write!(f, "{shape} literal is missing field {field}")
            }
            FableError::NonPodStructLiteral { shape } => {
                write!(f, "{shape} is not POD and cannot be constructed by literal")
            }
            FableError::IndexOutOfBounds { path, index, len } => {
                write!(f, "{path} index {index} is out of bounds for length {len}")
            }
            FableError::TypeMismatch { expected, actual } => {
                write!(f, "expected {expected}, found {actual}")
            }
            FableError::InvalidCall { function, reason } => {
                write!(f, "invalid call to {function}: {reason}")
            }
            FableError::InvalidIntrinsic { name, reason } => {
                write!(f, "invalid intrinsic {name}: {reason}")
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
            FableError::DuplicateStructField { field } => {
                write!(f, "struct literal field {field} is already initialized")
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
    Value(ValueExpr),
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
            ExprPlan::Value(_) => "typed value",
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
    Value { index: usize, shape: &'static Shape },
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
            LocalRef::Value { .. } => "typed value",
        }
    }
}

#[derive(Debug)]
enum UnitExpr {
    Null,
    Read(FieldPath),
    Local(LocalRef),
    HostFieldMut {
        function: FableFieldMutUnary,
        field: FieldPath,
    },
}

#[derive(Debug)]
enum BoolExpr {
    Literal(bool),
    Read(FieldPath),
    Local(LocalRef),
    HostFieldPredicate {
        function: FableFieldBoolUnary,
        field: FieldPath,
    },
    HostStringPredicate {
        function: FableStringBinaryPredicate,
        lhs: Box<StringExpr>,
        rhs: Box<StringExpr>,
    },
    StringContains {
        haystack: Box<StringExpr>,
        needle: Box<StringExpr>,
    },
    StringStartsWith {
        haystack: Box<StringExpr>,
        prefix: Box<StringExpr>,
    },
    StringEndsWith {
        haystack: Box<StringExpr>,
        suffix: Box<StringExpr>,
    },
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
    HostFieldString {
        function: FableFieldStringUnary,
        field: FieldPath,
    },
    HostUnary {
        function: FableStringUnary,
        value: Box<StringExpr>,
    },
    Trim(Box<StringExpr>),
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
    HostUnary {
        function: FableSignedUnary,
        value: Box<NumberExpr>,
    },
    Min(Box<NumberExpr>, Box<NumberExpr>),
    Max(Box<NumberExpr>, Box<NumberExpr>),
    Clamp {
        value: Box<NumberExpr>,
        min: Box<NumberExpr>,
        max: Box<NumberExpr>,
    },
    Neg(Box<NumberExpr>),
    Add(Box<NumberExpr>, Box<NumberExpr>),
    Sub(Box<NumberExpr>, Box<NumberExpr>),
}

#[derive(Debug)]
enum UIntExpr {
    Read(FieldPath),
    Local(LocalRef),
    Literal(u128),
    HostUnary {
        function: FableUnsignedUnary,
        value: Box<NumberExpr>,
    },
    StringLen(Box<StringExpr>),
    Min(Box<UIntExpr>, Box<UIntExpr>),
    Max(Box<UIntExpr>, Box<UIntExpr>),
    Clamp {
        value: Box<UIntExpr>,
        min: Box<UIntExpr>,
        max: Box<UIntExpr>,
    },
    Add(Box<UIntExpr>, Box<UIntExpr>),
}

#[derive(Debug)]
enum FloatExpr {
    Read(FieldPath),
    Local(LocalRef),
    Literal(f64),
    HostUnary {
        function: FableFloatUnary,
        value: Box<NumberExpr>,
    },
    Min(Box<NumberExpr>, Box<NumberExpr>),
    Max(Box<NumberExpr>, Box<NumberExpr>),
    Clamp {
        value: Box<NumberExpr>,
        min: Box<NumberExpr>,
        max: Box<NumberExpr>,
    },
    Neg(Box<NumberExpr>),
    Add(Box<NumberExpr>, Box<NumberExpr>),
    Sub(Box<NumberExpr>, Box<NumberExpr>),
}

#[derive(Debug)]
enum ValueExpr {
    Struct(StructExpr),
    Local(LocalRef),
}

#[derive(Debug)]
struct StructExpr {
    shape: &'static Shape,
    fields: Box<[StructFieldInit]>,
}

#[derive(Debug)]
struct StructFieldInit {
    offset: usize,
    shape: &'static Shape,
    scalar: ScalarType,
    value: ExprPlan,
}

impl ValueExpr {
    fn shape(&self) -> &'static Shape {
        match self {
            Self::Struct(expr) => expr.shape,
            Self::Local(LocalRef::Value { shape, .. }) => shape,
            Self::Local(_) => unreachable!("value expression local must refer to a value slot"),
        }
    }

    fn kind_name(&self) -> &'static str {
        "typed value"
    }
}

impl StructExpr {
    fn kind_name(&self) -> &'static str {
        "typed value"
    }
}

#[derive(Debug)]
struct FieldPath {
    source: Box<str>,
    shape: &'static Shape,
    scalar: Option<ScalarType>,
    steps: Box<[FieldStep]>,
}

impl FieldPath {
    fn ptr_mut(&self, mut ptr: PtrMut) -> Result<PtrMut, FableError> {
        for step in self.steps.iter() {
            ptr = unsafe { step.ptr_mut(ptr)? };
        }
        Ok(ptr)
    }

    fn ptr_const(&self, mut ptr: PtrConst) -> Result<PtrConst, FableError> {
        for step in self.steps.iter() {
            ptr = unsafe { step.ptr_const(ptr)? };
        }
        Ok(ptr)
    }
}

#[derive(Debug)]
enum FieldStep {
    Field {
        offset: usize,
    },
    ListIndex {
        source: Box<str>,
        shape: &'static Shape,
        index: usize,
    },
    ArrayIndex {
        source: Box<str>,
        shape: &'static Shape,
        len: usize,
        stride: usize,
        index: usize,
    },
    SliceIndex {
        source: Box<str>,
        shape: &'static Shape,
        len: unsafe extern "C" fn(PtrConst) -> usize,
        stride: usize,
        index: usize,
    },
}

impl FieldStep {
    unsafe fn ptr_mut(&self, ptr: PtrMut) -> Result<PtrMut, FableError> {
        match self {
            Self::Field { offset } => Ok(unsafe { ptr.field(*offset) }),
            Self::ListIndex {
                source,
                shape,
                index,
            } => {
                let Def::List(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "list index step did not point to a list shape",
                    });
                };
                let Some(get_mut) = def.vtable.get_mut else {
                    return Err(FableError::Unsupported {
                        feature: format!("mutable index access on {shape}"),
                    });
                };
                unsafe { get_mut(ptr, *index, shape) }.ok_or_else(|| {
                    let len = unsafe { (def.vtable.len)(ptr.as_const()) };
                    index_out_of_bounds(source, *index, len)
                })
            }
            Self::ArrayIndex {
                source,
                shape,
                len,
                stride,
                index,
            } => {
                if *index >= *len {
                    return Err(index_out_of_bounds(source, *index, *len));
                }
                let Def::Array(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "array index step did not point to an array shape",
                    });
                };
                let base = unsafe { (def.vtable.as_mut_ptr)(ptr) };
                Ok(unsafe { base.field(index * stride) })
            }
            Self::SliceIndex {
                source,
                shape,
                len,
                stride,
                index,
            } => {
                let runtime_len = unsafe { len(ptr.as_const()) };
                if *index >= runtime_len {
                    return Err(index_out_of_bounds(source, *index, runtime_len));
                }
                let Def::Slice(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "slice index step did not point to a slice shape",
                    });
                };
                let base = unsafe { (def.vtable.as_mut_ptr)(ptr) };
                Ok(unsafe { base.field(index * stride) })
            }
        }
    }

    unsafe fn ptr_const(&self, ptr: PtrConst) -> Result<PtrConst, FableError> {
        match self {
            Self::Field { offset } => Ok(unsafe { ptr.field(*offset) }),
            Self::ListIndex {
                source,
                shape,
                index,
            } => {
                let Def::List(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "list index step did not point to a list shape",
                    });
                };
                unsafe { (def.vtable.get)(ptr, *index, shape) }.ok_or_else(|| {
                    let len = unsafe { (def.vtable.len)(ptr) };
                    index_out_of_bounds(source, *index, len)
                })
            }
            Self::ArrayIndex {
                source,
                shape,
                len,
                stride,
                index,
            } => {
                if *index >= *len {
                    return Err(index_out_of_bounds(source, *index, *len));
                }
                let Def::Array(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "array index step did not point to an array shape",
                    });
                };
                let base = unsafe { (def.vtable.as_ptr)(ptr) };
                Ok(unsafe { base.field(index * stride) })
            }
            Self::SliceIndex {
                source,
                shape,
                len,
                stride,
                index,
            } => {
                let runtime_len = unsafe { len(ptr) };
                if *index >= runtime_len {
                    return Err(index_out_of_bounds(source, *index, runtime_len));
                }
                let Def::Slice(def) = shape.def else {
                    return Err(FableError::MalformedProgram {
                        reason: "slice index step did not point to a slice shape",
                    });
                };
                let base = unsafe { (def.vtable.as_ptr)(ptr) };
                Ok(unsafe { base.field(index * stride) })
            }
        }
    }
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
    value_count: usize,
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
            ExprPlan::Value(expr) => {
                let index = self.value_count;
                self.value_count += 1;
                LocalRef::Value {
                    index,
                    shape: expr.shape(),
                }
            }
        }
    }
}

struct Lowerer<'intrinsics> {
    root_shape: &'static Shape,
    intrinsics: &'intrinsics FableIntrinsics,
    scopes: Vec<BTreeMap<String, LocalRef>>,
    locals: LocalAllocator,
    type_shapes: Vec<&'static Shape>,
}

impl<'intrinsics> Lowerer<'intrinsics> {
    fn new(root_shape: &'static Shape, intrinsics: &'intrinsics FableIntrinsics) -> Self {
        let mut type_shapes = Vec::new();
        collect_reachable_shapes(root_shape, &mut type_shapes);
        Self {
            root_shape,
            intrinsics,
            scopes: vec![BTreeMap::new()],
            locals: LocalAllocator::default(),
            type_shapes,
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
                validate_assignment(target.scalar, target.shape, &value)?;
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
            Expr::Index(_) => {
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::StructLiteral(literal) => self.lower_struct_literal(literal),
            Expr::Call(call) => self.lower_call(call),
        }
    }

    fn lower_struct_literal(&mut self, literal: &StructLiteral) -> Result<ExprPlan, FableError> {
        let type_name = literal.type_name().ok_or(FableError::MalformedSyntax {
            reason: "struct literal without type name",
        })?;
        let shape = self.resolve_type_name(&type_name)?;
        if !shape.is_pod() {
            return Err(FableError::NonPodStructLiteral { shape });
        }
        let Type::User(UserType::Struct(struct_type)) = shape.ty else {
            return Err(FableError::Unsupported {
                feature: format!("struct literal for non-struct type {shape}"),
            });
        };
        if struct_type.kind != StructKind::Struct {
            return Err(FableError::Unsupported {
                feature: format!("struct literal for {shape}"),
            });
        }

        let mut supplied = BTreeMap::new();
        for field in literal.fields() {
            let name = field.name().ok_or(FableError::MalformedSyntax {
                reason: "struct literal field without name",
            })?;
            if supplied.contains_key(&name) {
                return Err(FableError::DuplicateStructField { field: name });
            }
            let value = field.value().ok_or(FableError::MalformedSyntax {
                reason: "struct literal field without value",
            })?;
            supplied.insert(name, self.lower_expr(&value)?);
        }

        let mut fields = Vec::with_capacity(struct_type.fields.len());
        for field in struct_type.fields {
            let field_shape = field.shape.get();
            let scalar =
                ScalarType::try_from_shape(field_shape).ok_or_else(|| FableError::Unsupported {
                    feature: format!("non-scalar POD literal field {}.{}", shape, field.name),
                })?;
            let Some(value) = supplied.remove(field.name) else {
                return Err(FableError::MissingStructField {
                    shape,
                    field: field.name.to_owned(),
                });
            };
            validate_assignment(Some(scalar), field_shape, &value)?;
            fields.push(StructFieldInit {
                offset: field.offset,
                shape: field_shape,
                scalar,
                value,
            });
        }

        if let Some((name, _)) = supplied.into_iter().next() {
            return Err(FableError::UnknownField { shape, field: name });
        }

        Ok(ExprPlan::Value(ValueExpr::Struct(StructExpr {
            shape,
            fields: fields.into_boxed_slice(),
        })))
    }

    fn resolve_type_name(&self, name: &str) -> Result<&'static Shape, FableError> {
        let mut matches = self
            .type_shapes
            .iter()
            .copied()
            .filter(|shape| shape_name_matches(shape, name));
        let Some(first) = matches.next() else {
            return Err(FableError::UnknownType {
                name: name.to_owned(),
            });
        };
        if matches.next().is_some() {
            return Err(FableError::AmbiguousType {
                name: name.to_owned(),
            });
        }
        Ok(first)
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

    fn lower_call(&mut self, call: &CallExpr) -> Result<ExprPlan, FableError> {
        let callee = call.callee().ok_or(FableError::MalformedSyntax {
            reason: "call expression without callee",
        })?;
        let name = call_callee_name(&callee)?;
        let signature =
            self.intrinsics
                .signature(&name)
                .ok_or_else(|| FableError::Unsupported {
                    feature: format!("intrinsic {name}"),
                })?;

        let mut raw_args = Vec::new();
        if let Some(arg_list) = call.args() {
            for arg in arg_list.args() {
                let expr = arg.expr().ok_or(FableError::MalformedSyntax {
                    reason: "argument without expression",
                })?;
                raw_args.push(expr);
            }
        }
        signature.validate_arity(raw_args.len())?;

        let mut args = Vec::with_capacity(raw_args.len());
        for expr in raw_args {
            args.push(match signature.arg_kind {
                IntrinsicArgKind::Expr => IntrinsicArgPlan::Expr(self.lower_expr(&expr)?),
                IntrinsicArgKind::FieldRead => {
                    IntrinsicArgPlan::Field(self.lower_readable_path(&expr)?)
                }
                IntrinsicArgKind::FieldMut => {
                    IntrinsicArgPlan::Field(self.lower_writable_path(&expr)?)
                }
            });
        }
        lower_intrinsic(signature, args)
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
        if let Some(scalar) = path.scalar {
            ensure_writable(scalar)?;
        } else if !path.shape.is_pod() {
            return Err(FableError::Unsupported {
                feature: format!("writing non-POD path ending at {}", path.shape),
            });
        }
        Ok(path)
    }

    fn lower_readable_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        let path = self.resolve_path(expr)?;
        let Some(scalar) = path.scalar else {
            return Err(FableError::Unsupported {
                feature: format!("reading non-scalar path ending at {}", path.shape),
            });
        };
        ensure_readable(scalar, path.shape)?;
        Ok(path)
    }

    fn resolve_path(&self, expr: &Expr) -> Result<FieldPath, FableError> {
        let segments = collect_path(expr)?;
        let Some((first, rest)) = segments.split_first() else {
            return Err(FableError::MalformedSyntax {
                reason: "empty field path",
            });
        };
        let PathSegment::Name(first) = first else {
            return Err(FableError::MalformedSyntax {
                reason: "path did not start with a variable reference",
            });
        };
        if first != "root" {
            return Err(FableError::ExpectedRoot {
                found: first.clone(),
            });
        }

        let mut shape = self.root_shape;
        let mut source = first.clone();
        let mut steps = Vec::with_capacity(rest.len());
        for segment in rest {
            match segment {
                PathSegment::Name(field_name) => {
                    let field = find_field(shape, field_name)?;
                    let field_shape = field.shape.get();
                    source.push('.');
                    source.push_str(field_name);
                    steps.push(FieldStep::Field {
                        offset: field.offset,
                    });
                    shape = field_shape;
                }
                PathSegment::Index { index, literal } => {
                    source.push('[');
                    source.push_str(literal);
                    source.push(']');
                    let (step, element_shape) =
                        index_step(shape, *index, source.clone().into_boxed_str())?;
                    steps.push(step);
                    shape = element_shape;
                }
            }
        }

        let scalar = ScalarType::try_from_shape(shape);
        Ok(FieldPath {
            source: source.into_boxed_str(),
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

#[derive(Clone, Copy, Debug)]
enum Intrinsic {
    Min,
    Max,
    Clamp,
    Len,
    Contains,
    StartsWith,
    EndsWith,
    Trim,
    FieldString(FableFieldStringUnary),
    FieldBool(FableFieldBoolUnary),
    FieldMut(FableFieldMutUnary),
    StringUnary(FableStringUnary),
    StringBinaryPredicate(FableStringBinaryPredicate),
    SignedUnary(FableSignedUnary),
    UnsignedUnary(FableUnsignedUnary),
    FloatUnary(FableFloatUnary),
}

#[derive(Clone, Copy, Debug)]
struct IntrinsicSignature {
    name: &'static str,
    intrinsic: Intrinsic,
    arity: usize,
    arg_kind: IntrinsicArgKind,
}

impl IntrinsicSignature {
    fn validate_arity(self, actual: usize) -> Result<(), FableError> {
        if actual == self.arity {
            Ok(())
        } else {
            Err(invalid_call(self.name, arity_reason(self.arity)))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericLane {
    Signed,
    Unsigned,
    Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntrinsicArgKind {
    Expr,
    FieldRead,
    FieldMut,
}

enum IntrinsicArgPlan {
    Expr(ExprPlan),
    Field(FieldPath),
}

impl Default for FableIntrinsics {
    fn default() -> Self {
        Self::standard()
    }
}

impl FableIntrinsics {
    /// Return a registry with Fable's builtin intrinsics.
    #[must_use]
    pub fn standard() -> Self {
        let mut intrinsics = Self::empty();
        intrinsics.add_builtin("min", Intrinsic::Min, 2);
        intrinsics.add_builtin("max", Intrinsic::Max, 2);
        intrinsics.add_builtin("clamp", Intrinsic::Clamp, 3);
        intrinsics.add_builtin("len", Intrinsic::Len, 1);
        intrinsics.add_builtin("contains", Intrinsic::Contains, 2);
        intrinsics.add_builtin("starts_with", Intrinsic::StartsWith, 2);
        intrinsics.add_builtin("ends_with", Intrinsic::EndsWith, 2);
        intrinsics.add_builtin("trim", Intrinsic::Trim, 1);
        intrinsics
    }

    /// Return an empty registry with no builtin intrinsics.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            signatures: Vec::new(),
        }
    }

    /// Register a `string -> string` host intrinsic.
    pub fn add_string_unary(
        &mut self,
        name: &'static str,
        function: FableStringUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::StringUnary(function),
            1,
            IntrinsicArgKind::Expr,
        )
    }

    /// Register a `(string, string) -> bool` host intrinsic.
    pub fn add_string_binary_predicate(
        &mut self,
        name: &'static str,
        function: FableStringBinaryPredicate,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::StringBinaryPredicate(function),
            2,
            IntrinsicArgKind::Expr,
        )
    }

    /// Register a `signed number -> signed number` host intrinsic.
    pub fn add_signed_unary(
        &mut self,
        name: &'static str,
        function: FableSignedUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::SignedUnary(function),
            1,
            IntrinsicArgKind::Expr,
        )
    }

    /// Register an `unsigned number -> unsigned number` host intrinsic.
    pub fn add_unsigned_unary(
        &mut self,
        name: &'static str,
        function: FableUnsignedUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::UnsignedUnary(function),
            1,
            IntrinsicArgKind::Expr,
        )
    }

    /// Register a `float -> float` host intrinsic.
    pub fn add_float_unary(
        &mut self,
        name: &'static str,
        function: FableFloatUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::FloatUnary(function),
            1,
            IntrinsicArgKind::Expr,
        )
    }

    /// Register a `field -> string` host intrinsic.
    pub fn add_field_string_unary(
        &mut self,
        name: &'static str,
        function: FableFieldStringUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::FieldString(function),
            1,
            IntrinsicArgKind::FieldRead,
        )
    }

    /// Register a `field -> bool` host intrinsic.
    pub fn add_field_bool_unary(
        &mut self,
        name: &'static str,
        function: FableFieldBoolUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::FieldBool(function),
            1,
            IntrinsicArgKind::FieldRead,
        )
    }

    /// Register a `field_mut -> unit` host intrinsic.
    pub fn add_field_mut_unary(
        &mut self,
        name: &'static str,
        function: FableFieldMutUnary,
    ) -> Result<&mut Self, FableError> {
        self.insert(
            name,
            Intrinsic::FieldMut(function),
            1,
            IntrinsicArgKind::FieldMut,
        )
    }

    fn add_builtin(&mut self, name: &'static str, intrinsic: Intrinsic, arity: usize) {
        self.insert(name, intrinsic, arity, IntrinsicArgKind::Expr)
            .expect("builtin Fable intrinsic metadata is valid");
    }

    fn insert(
        &mut self,
        name: &'static str,
        intrinsic: Intrinsic,
        arity: usize,
        arg_kind: IntrinsicArgKind,
    ) -> Result<&mut Self, FableError> {
        validate_intrinsic_name(name)?;
        if self
            .signatures
            .iter()
            .any(|signature| signature.name == name)
        {
            return Err(invalid_intrinsic(name, "duplicate intrinsic name"));
        }
        self.signatures.push(IntrinsicSignature {
            name,
            intrinsic,
            arity,
            arg_kind,
        });
        Ok(self)
    }

    fn signature(&self, name: &str) -> Option<IntrinsicSignature> {
        self.signatures
            .iter()
            .find(|signature| signature.name == name)
            .copied()
    }
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

fn lower_intrinsic(
    signature: IntrinsicSignature,
    args: Vec<IntrinsicArgPlan>,
) -> Result<ExprPlan, FableError> {
    let intrinsic = signature.intrinsic;
    match intrinsic {
        Intrinsic::Min | Intrinsic::Max | Intrinsic::Clamp => {
            lower_numeric_intrinsic(signature.name, intrinsic, expr_args(signature.name, args)?)
        }
        Intrinsic::Len => {
            let string = only_string_arg("len", expr_args(signature.name, args)?)?;
            Ok(ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::StringLen(
                Box::new(string),
            ))))
        }
        Intrinsic::Contains | Intrinsic::StartsWith | Intrinsic::EndsWith => {
            let (lhs, rhs) = two_string_args(signature.name, expr_args(signature.name, args)?)?;
            let expr = match intrinsic {
                Intrinsic::Contains => BoolExpr::StringContains {
                    haystack: Box::new(lhs),
                    needle: Box::new(rhs),
                },
                Intrinsic::StartsWith => BoolExpr::StringStartsWith {
                    haystack: Box::new(lhs),
                    prefix: Box::new(rhs),
                },
                Intrinsic::EndsWith => BoolExpr::StringEndsWith {
                    haystack: Box::new(lhs),
                    suffix: Box::new(rhs),
                },
                _ => unreachable!("string predicate branch only receives string predicates"),
            };
            Ok(ExprPlan::Bool(expr))
        }
        Intrinsic::Trim => {
            let string = only_string_arg("trim", expr_args(signature.name, args)?)?;
            Ok(ExprPlan::String(StringExpr::Trim(Box::new(string))))
        }
        Intrinsic::FieldString(function) => {
            let field = only_field_arg(signature.name, args)?;
            Ok(ExprPlan::String(StringExpr::HostFieldString {
                function,
                field,
            }))
        }
        Intrinsic::FieldBool(function) => {
            let field = only_field_arg(signature.name, args)?;
            Ok(ExprPlan::Bool(BoolExpr::HostFieldPredicate {
                function,
                field,
            }))
        }
        Intrinsic::FieldMut(function) => {
            let field = only_field_arg(signature.name, args)?;
            Ok(ExprPlan::Unit(UnitExpr::HostFieldMut { function, field }))
        }
        Intrinsic::StringUnary(function) => {
            let value = only_string_arg(signature.name, expr_args(signature.name, args)?)?;
            Ok(ExprPlan::String(StringExpr::HostUnary {
                function,
                value: Box::new(value),
            }))
        }
        Intrinsic::StringBinaryPredicate(function) => {
            let (lhs, rhs) = two_string_args(signature.name, expr_args(signature.name, args)?)?;
            Ok(ExprPlan::Bool(BoolExpr::HostStringPredicate {
                function,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            }))
        }
        Intrinsic::SignedUnary(function) => {
            let value = only_number_arg(signature.name, expr_args(signature.name, args)?)?;
            Ok(ExprPlan::Number(NumberExpr::Signed(IntExpr::HostUnary {
                function,
                value: Box::new(value),
            })))
        }
        Intrinsic::UnsignedUnary(function) => {
            let value = only_number_arg(signature.name, expr_args(signature.name, args)?)?;
            Ok(ExprPlan::Number(NumberExpr::Unsigned(
                UIntExpr::HostUnary {
                    function,
                    value: Box::new(value),
                },
            )))
        }
        Intrinsic::FloatUnary(function) => {
            let value = only_number_arg(signature.name, expr_args(signature.name, args)?)?;
            Ok(ExprPlan::Number(NumberExpr::Float(FloatExpr::HostUnary {
                function,
                value: Box::new(value),
            })))
        }
    }
}

fn expr_args(
    function: &'static str,
    args: Vec<IntrinsicArgPlan>,
) -> Result<Vec<ExprPlan>, FableError> {
    args.into_iter()
        .map(|arg| match arg {
            IntrinsicArgPlan::Expr(expr) => Ok(expr),
            IntrinsicArgPlan::Field(_) => {
                Err(invalid_call(function, "expected expression argument"))
            }
        })
        .collect()
}

fn lower_numeric_intrinsic(
    function: &'static str,
    intrinsic: Intrinsic,
    args: Vec<ExprPlan>,
) -> Result<ExprPlan, FableError> {
    let numbers = args
        .into_iter()
        .map(expect_number_plan)
        .collect::<Result<Vec<_>, _>>()?;
    let lane = numeric_lane(&numbers);

    match intrinsic {
        Intrinsic::Min => {
            let (lhs, rhs) = two_numbers(function, numbers)?;
            Ok(ExprPlan::Number(match lane {
                NumericLane::Signed => {
                    NumberExpr::Signed(IntExpr::Min(Box::new(lhs), Box::new(rhs)))
                }
                NumericLane::Unsigned => NumberExpr::Unsigned(UIntExpr::Min(
                    Box::new(expect_unsigned_number(function, lhs)?),
                    Box::new(expect_unsigned_number(function, rhs)?),
                )),
                NumericLane::Float => {
                    NumberExpr::Float(FloatExpr::Min(Box::new(lhs), Box::new(rhs)))
                }
            }))
        }
        Intrinsic::Max => {
            let (lhs, rhs) = two_numbers(function, numbers)?;
            Ok(ExprPlan::Number(match lane {
                NumericLane::Signed => {
                    NumberExpr::Signed(IntExpr::Max(Box::new(lhs), Box::new(rhs)))
                }
                NumericLane::Unsigned => NumberExpr::Unsigned(UIntExpr::Max(
                    Box::new(expect_unsigned_number(function, lhs)?),
                    Box::new(expect_unsigned_number(function, rhs)?),
                )),
                NumericLane::Float => {
                    NumberExpr::Float(FloatExpr::Max(Box::new(lhs), Box::new(rhs)))
                }
            }))
        }
        Intrinsic::Clamp => {
            let (value, min, max) = three_numbers(function, numbers)?;
            Ok(ExprPlan::Number(match lane {
                NumericLane::Signed => NumberExpr::Signed(IntExpr::Clamp {
                    value: Box::new(value),
                    min: Box::new(min),
                    max: Box::new(max),
                }),
                NumericLane::Unsigned => NumberExpr::Unsigned(UIntExpr::Clamp {
                    value: Box::new(expect_unsigned_number(function, value)?),
                    min: Box::new(expect_unsigned_number(function, min)?),
                    max: Box::new(expect_unsigned_number(function, max)?),
                }),
                NumericLane::Float => NumberExpr::Float(FloatExpr::Clamp {
                    value: Box::new(value),
                    min: Box::new(min),
                    max: Box::new(max),
                }),
            }))
        }
        _ => unreachable!("numeric intrinsic branch only receives numeric intrinsics"),
    }
}

fn only_number_arg(function: &'static str, args: Vec<ExprPlan>) -> Result<NumberExpr, FableError> {
    let [arg]: [ExprPlan; 1] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(1)))?;
    expect_number_plan(arg)
}

fn only_field_arg(
    function: &'static str,
    args: Vec<IntrinsicArgPlan>,
) -> Result<FieldPath, FableError> {
    let [arg]: [IntrinsicArgPlan; 1] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(1)))?;
    match arg {
        IntrinsicArgPlan::Field(field) => Ok(field),
        IntrinsicArgPlan::Expr(_) => Err(invalid_call(function, "expected field argument")),
    }
}

fn only_string_arg(function: &'static str, args: Vec<ExprPlan>) -> Result<StringExpr, FableError> {
    let [arg]: [ExprPlan; 1] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(1)))?;
    expect_string_plan(arg)
}

fn two_string_args(
    function: &'static str,
    args: Vec<ExprPlan>,
) -> Result<(StringExpr, StringExpr), FableError> {
    let [lhs, rhs]: [ExprPlan; 2] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(2)))?;
    Ok((expect_string_plan(lhs)?, expect_string_plan(rhs)?))
}

fn two_numbers(
    function: &'static str,
    args: Vec<NumberExpr>,
) -> Result<(NumberExpr, NumberExpr), FableError> {
    let [lhs, rhs]: [NumberExpr; 2] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(2)))?;
    Ok((lhs, rhs))
}

fn three_numbers(
    function: &'static str,
    args: Vec<NumberExpr>,
) -> Result<(NumberExpr, NumberExpr, NumberExpr), FableError> {
    let [value, min, max]: [NumberExpr; 3] = args
        .try_into()
        .map_err(|_| invalid_call(function, arity_reason(3)))?;
    Ok((value, min, max))
}

fn expect_unsigned_number(
    function: &'static str,
    number: NumberExpr,
) -> Result<UIntExpr, FableError> {
    match number {
        NumberExpr::Unsigned(expr) => Ok(expr),
        _ => Err(invalid_call(
            function,
            "unsigned numeric lane contained a non-unsigned argument",
        )),
    }
}

fn numeric_lane(args: &[NumberExpr]) -> NumericLane {
    if args.iter().any(|arg| matches!(arg, NumberExpr::Float(_))) {
        NumericLane::Float
    } else if args
        .iter()
        .all(|arg| matches!(arg, NumberExpr::Unsigned(_)))
    {
        NumericLane::Unsigned
    } else {
        NumericLane::Signed
    }
}

fn call_callee_name(callee: &Expr) -> Result<String, FableError> {
    let Expr::Var(var) = callee else {
        return Err(FableError::Unsupported {
            feature: "non-identifier callees".into(),
        });
    };
    var.name().ok_or(FableError::MalformedSyntax {
        reason: "callee without identifier",
    })
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

fn expect_string_plan(expr: ExprPlan) -> Result<StringExpr, FableError> {
    match expr {
        ExprPlan::String(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "string".into(),
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
        LocalRef::Value { .. } => ExprPlan::Value(ValueExpr::Local(local)),
    }
}

fn path_to_expr(path: FieldPath) -> Result<ExprPlan, FableError> {
    let scalar = path.scalar.ok_or_else(|| FableError::Unsupported {
        feature: format!("reading non-scalar path ending at {}", path.shape),
    })?;
    let expr = match scalar {
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
                feature: format!("reading {scalar:?}"),
            });
        }
    };
    Ok(expr)
}

fn validate_assignment(
    scalar: Option<ScalarType>,
    shape: &'static Shape,
    expr: &ExprPlan,
) -> Result<(), FableError> {
    let Some(scalar) = scalar else {
        return match expr {
            ExprPlan::Value(value) if value.shape() == shape => Ok(()),
            ExprPlan::Value(value) => Err(FableError::TypeMismatch {
                expected: format!("{shape}"),
                actual: value.kind_name(),
            }),
            other => Err(FableError::TypeMismatch {
                expected: format!("{shape}"),
                actual: other.kind_name(),
            }),
        };
    };

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
    values: Vec<Option<OwnedValue>>,
}

struct OwnedValue {
    shape: &'static Shape,
    ptr: PtrMut,
}

impl OwnedValue {
    unsafe fn move_into(self, dst: PtrMut) -> Result<(), FableError> {
        let shape = self.shape;
        let src = self.ptr;
        let layout = shape
            .layout
            .sized_layout()
            .map_err(|_| FableError::Unsupported {
                feature: format!("moving unsized value {shape}"),
            })?;
        unsafe {
            shape.call_drop_in_place(dst);
            copy_nonoverlapping(src.as_byte_ptr(), dst.as_mut_byte_ptr(), layout.size());
        }
        let uninit = src.as_uninit();
        std::mem::forget(self);
        unsafe { shape.deallocate_uninit(uninit) }.map_err(|_| FableError::Unsupported {
            feature: format!("deallocating unsized value {shape}"),
        })
    }
}

impl Drop for OwnedValue {
    fn drop(&mut self) {
        unsafe {
            self.shape.call_drop_in_place(self.ptr);
            let _ = self.shape.deallocate_mut(self.ptr);
        }
    }
}

struct StructInitGuard {
    shape: &'static Shape,
    ptr: PtrUninit,
    initialized: Vec<StructInitializedField>,
}

struct StructInitializedField {
    shape: &'static Shape,
    offset: usize,
}

impl StructInitGuard {
    fn new(shape: &'static Shape, ptr: PtrUninit) -> Self {
        Self {
            shape,
            ptr,
            initialized: Vec::new(),
        }
    }

    fn mark_initialized(&mut self, field: &StructFieldInit) {
        self.initialized.push(StructInitializedField {
            shape: field.shape,
            offset: field.offset,
        });
    }

    unsafe fn finish(self) -> OwnedValue {
        let ptr = unsafe { self.ptr.assume_init() };
        let shape = self.shape;
        std::mem::forget(self);
        OwnedValue { shape, ptr }
    }
}

impl Drop for StructInitGuard {
    fn drop(&mut self) {
        for field in self.initialized.iter().rev() {
            let ptr = unsafe { self.ptr.field_init(field.offset) };
            unsafe { field.shape.call_drop_in_place(ptr) };
        }
        unsafe {
            let _ = self.shape.deallocate_uninit(self.ptr);
        }
    }
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
                let ptr = target.ptr_mut(self.root)?;
                if let Some(scalar) = target.scalar {
                    unsafe { self.write_scalar(scalar, ptr, value) }?;
                } else {
                    unsafe { self.write_value(target.shape, ptr, value) }?;
                }
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
            LocalRef::Value { index, shape } => {
                let value = self.eval_value(shape, expect_value_expr(expr)?)?;
                set_slot(&mut self.locals.values, index, Some(value));
            }
        }
        Ok(())
    }

    fn eval_expr(&mut self, expr: &ExprPlan) -> Result<(), FableError> {
        match expr {
            ExprPlan::Unit(expr) => self.eval_unit(expr),
            ExprPlan::Bool(expr) => self.eval_bool(expr).map(drop),
            ExprPlan::Char(expr) => self.eval_char(expr).map(drop),
            ExprPlan::String(expr) => self.eval_string(expr).map(drop),
            ExprPlan::Number(expr) => self.eval_number_for_effect(expr),
            ExprPlan::Value(expr) => self.eval_value_for_effect(expr),
        }
    }

    fn eval_unit(&self, expr: &UnitExpr) -> Result<(), FableError> {
        match expr {
            UnitExpr::Null => Ok(()),
            UnitExpr::Read(path) => {
                let _ = path.ptr_const(self.root.as_const())?;
                Ok(())
            }
            UnitExpr::Local(local) => self.local_unit(*local),
            UnitExpr::HostFieldMut { function, field } => function(self.field_mut(field)?),
        }
    }

    fn eval_bool(&self, expr: &BoolExpr) -> Result<bool, FableError> {
        match expr {
            BoolExpr::Literal(value) => Ok(*value),
            BoolExpr::Read(path) => {
                let ptr = path.ptr_const(self.root.as_const())?;
                Ok(*unsafe { ptr.get::<bool>() })
            }
            BoolExpr::Local(local) => self.local_bool(*local),
            BoolExpr::HostFieldPredicate { function, field } => function(self.field_ref(field)?),
            BoolExpr::HostStringPredicate { function, lhs, rhs } => {
                let lhs = self.eval_string(lhs)?;
                let rhs = self.eval_string(rhs)?;
                function(&lhs, &rhs)
            }
            BoolExpr::StringContains { haystack, needle } => {
                let haystack = self.eval_string(haystack)?;
                let needle = self.eval_string(needle)?;
                Ok(haystack.contains(&needle))
            }
            BoolExpr::StringStartsWith { haystack, prefix } => {
                let haystack = self.eval_string(haystack)?;
                let prefix = self.eval_string(prefix)?;
                Ok(haystack.starts_with(&prefix))
            }
            BoolExpr::StringEndsWith { haystack, suffix } => {
                let haystack = self.eval_string(haystack)?;
                let suffix = self.eval_string(suffix)?;
                Ok(haystack.ends_with(&suffix))
            }
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
                let ptr = path.ptr_const(self.root.as_const())?;
                Ok(*unsafe { ptr.get::<char>() })
            }
            CharExpr::Local(local) => self.local_char(*local),
        }
    }

    fn eval_string(&self, expr: &StringExpr) -> Result<String, FableError> {
        match expr {
            StringExpr::Literal(value) => Ok(value.clone()),
            StringExpr::Read(path) => {
                let ptr = path.ptr_const(self.root.as_const())?;
                unsafe { self.read_string_path(path, ptr) }
            }
            StringExpr::Local(local) => self.local_string(*local),
            StringExpr::HostFieldString { function, field } => function(self.field_ref(field)?),
            StringExpr::HostUnary { function, value } => {
                let value = self.eval_string(value)?;
                function(&value)
            }
            StringExpr::Trim(expr) => Ok(self.eval_string(expr)?.trim().to_owned()),
            StringExpr::Add(lhs, rhs) => {
                let mut lhs = self.eval_string(lhs)?;
                lhs.push_str(&self.eval_string(rhs)?);
                Ok(lhs)
            }
        }
    }

    fn field_ref<'field>(
        &self,
        field: &'field FieldPath,
    ) -> Result<FableField<'field>, FableError> {
        Ok(FableField {
            path: &field.source,
            shape: field.shape,
            scalar: field_scalar(field)?,
            ptr: field.ptr_const(self.root.as_const())?,
        })
    }

    fn field_mut<'field>(
        &self,
        field: &'field FieldPath,
    ) -> Result<FableFieldMut<'field>, FableError> {
        Ok(FableFieldMut {
            path: &field.source,
            shape: field.shape,
            scalar: field_scalar(field)?,
            ptr: field.ptr_mut(self.root)?,
        })
    }

    unsafe fn read_string_path(
        &self,
        path: &FieldPath,
        ptr: PtrConst,
    ) -> Result<String, FableError> {
        let scalar = field_scalar(path)?;
        match scalar {
            ScalarType::Str if path.shape.is_type::<&'static str>() => {
                Ok((*unsafe { ptr.get::<&'static str>() }).to_owned())
            }
            ScalarType::String => Ok(unsafe { ptr.get::<String>() }.clone()),
            ScalarType::CowStr => Ok(unsafe { ptr.get::<Cow<'static, str>>() }
                .clone()
                .into_owned()),
            _ => Err(FableError::Unsupported {
                feature: format!("reading {scalar:?}"),
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
                let ptr = path.ptr_const(self.root.as_const())?;
                unsafe { self.read_signed_path(field_scalar(path)?, ptr) }
            }
            IntExpr::Local(local) => self.local_i128(*local),
            IntExpr::HostUnary { function, value } => function(self.eval_number_as_i128(value)?),
            IntExpr::Min(lhs, rhs) => {
                let lhs = self.eval_number_as_i128(lhs)?;
                let rhs = self.eval_number_as_i128(rhs)?;
                Ok(lhs.min(rhs))
            }
            IntExpr::Max(lhs, rhs) => {
                let lhs = self.eval_number_as_i128(lhs)?;
                let rhs = self.eval_number_as_i128(rhs)?;
                Ok(lhs.max(rhs))
            }
            IntExpr::Clamp { value, min, max } => {
                let value = self.eval_number_as_i128(value)?;
                let min = self.eval_number_as_i128(min)?;
                let max = self.eval_number_as_i128(max)?;
                clamp_i128(value, min, max)
            }
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
                let ptr = path.ptr_const(self.root.as_const())?;
                unsafe { self.read_unsigned_path(field_scalar(path)?, ptr) }
            }
            UIntExpr::Local(local) => self.local_u128(*local),
            UIntExpr::HostUnary { function, value } => function(self.eval_number_as_u128(value)?),
            UIntExpr::StringLen(expr) => Ok(self.eval_string(expr)?.len() as u128),
            UIntExpr::Min(lhs, rhs) => {
                let lhs = self.eval_u128(lhs)?;
                let rhs = self.eval_u128(rhs)?;
                Ok(lhs.min(rhs))
            }
            UIntExpr::Max(lhs, rhs) => {
                let lhs = self.eval_u128(lhs)?;
                let rhs = self.eval_u128(rhs)?;
                Ok(lhs.max(rhs))
            }
            UIntExpr::Clamp { value, min, max } => {
                let value = self.eval_u128(value)?;
                let min = self.eval_u128(min)?;
                let max = self.eval_u128(max)?;
                clamp_u128(value, min, max)
            }
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
                let ptr = path.ptr_const(self.root.as_const())?;
                match field_scalar(path)? {
                    ScalarType::F32 => Ok((*unsafe { ptr.get::<f32>() }).into()),
                    ScalarType::F64 => Ok(*unsafe { ptr.get::<f64>() }),
                    _ => Err(FableError::MalformedProgram {
                        reason: "float read path did not point to a float scalar",
                    }),
                }
            }
            FloatExpr::Local(local) => self.local_f64(*local),
            FloatExpr::HostUnary { function, value } => function(self.eval_number_as_f64(value)?),
            FloatExpr::Min(lhs, rhs) => {
                min_f64(self.eval_number_as_f64(lhs)?, self.eval_number_as_f64(rhs)?)
            }
            FloatExpr::Max(lhs, rhs) => {
                max_f64(self.eval_number_as_f64(lhs)?, self.eval_number_as_f64(rhs)?)
            }
            FloatExpr::Clamp { value, min, max } => clamp_f64(
                self.eval_number_as_f64(value)?,
                self.eval_number_as_f64(min)?,
                self.eval_number_as_f64(max)?,
            ),
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

    unsafe fn write_value(
        &mut self,
        shape: &'static Shape,
        ptr: PtrMut,
        expr: &ExprPlan,
    ) -> Result<(), FableError> {
        let value = self.eval_value(shape, expect_value_expr(expr)?)?;
        unsafe { value.move_into(ptr) }
    }

    fn eval_value(
        &mut self,
        shape: &'static Shape,
        expr: &ValueExpr,
    ) -> Result<OwnedValue, FableError> {
        match expr {
            ValueExpr::Struct(expr) => {
                if expr.shape != shape {
                    return Err(FableError::TypeMismatch {
                        expected: format!("{shape}"),
                        actual: expr.kind_name(),
                    });
                }
                unsafe { self.init_struct_value(expr) }
            }
            ValueExpr::Local(local) => self.take_local_value(*local, shape),
        }
    }

    fn eval_value_for_effect(&mut self, expr: &ValueExpr) -> Result<(), FableError> {
        match expr {
            ValueExpr::Struct(expr) => unsafe { self.init_struct_value(expr) }.map(drop),
            ValueExpr::Local(local) => {
                let LocalRef::Value { index, .. } = *local else {
                    return Err(local_kind_mismatch("typed value", *local));
                };
                if self
                    .locals
                    .values
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(uninitialized_local())
                }
            }
        }
    }

    unsafe fn init_struct_value(&self, expr: &StructExpr) -> Result<OwnedValue, FableError> {
        let ptr = expr.shape.allocate().map_err(|_| FableError::Unsupported {
            feature: format!("allocating unsized value {}", expr.shape),
        })?;
        let mut guard = StructInitGuard::new(expr.shape, ptr);
        for field in expr.fields.iter() {
            let field_ptr = unsafe { ptr.field_uninit(field.offset) };
            unsafe { self.init_scalar(field.scalar, field_ptr, &field.value) }?;
            guard.mark_initialized(field);
        }
        Ok(unsafe { guard.finish() })
    }

    fn take_local_value(
        &mut self,
        local: LocalRef,
        expected_shape: &'static Shape,
    ) -> Result<OwnedValue, FableError> {
        let LocalRef::Value { index, shape } = local else {
            return Err(local_kind_mismatch("typed value", local));
        };
        if shape != expected_shape {
            return Err(FableError::TypeMismatch {
                expected: format!("{expected_shape}"),
                actual: "typed value",
            });
        }
        let Some(slot) = self.locals.values.get_mut(index) else {
            return Err(uninitialized_local());
        };
        slot.take().ok_or_else(uninitialized_local)
    }

    unsafe fn init_scalar(
        &self,
        scalar: ScalarType,
        ptr: PtrUninit,
        expr: &ExprPlan,
    ) -> Result<(), FableError> {
        match scalar {
            ScalarType::Unit => {
                self.eval_unit(expect_unit_expr(expr)?)?;
                unsafe { ptr.put(()) };
            }
            ScalarType::Bool => {
                unsafe { ptr.put(self.eval_bool(expect_bool_expr(expr)?)?) };
            }
            ScalarType::Char => {
                unsafe { ptr.put(self.eval_char_assign(expr)?) };
            }
            ScalarType::String => {
                unsafe { ptr.put(self.eval_string_assign(expr)?) };
            }
            ScalarType::CowStr => {
                unsafe { ptr.put::<Cow<'static, str>>(Cow::Owned(self.eval_string_assign(expr)?)) };
            }
            ScalarType::F32 => {
                unsafe { ptr.put(self.eval_number_as_f64(expect_number_expr(expr)?)? as f32) };
            }
            ScalarType::F64 => {
                unsafe { ptr.put(self.eval_number_as_f64(expect_number_expr(expr)?)?) };
            }
            ScalarType::U8 => unsafe { self.init_unsigned::<u8>(ptr, scalar, expr) }?,
            ScalarType::U16 => unsafe { self.init_unsigned::<u16>(ptr, scalar, expr) }?,
            ScalarType::U32 => unsafe { self.init_unsigned::<u32>(ptr, scalar, expr) }?,
            ScalarType::U64 => unsafe { self.init_unsigned::<u64>(ptr, scalar, expr) }?,
            ScalarType::U128 => unsafe { self.init_unsigned::<u128>(ptr, scalar, expr) }?,
            ScalarType::USize => unsafe { self.init_unsigned::<usize>(ptr, scalar, expr) }?,
            ScalarType::I8 => unsafe { self.init_signed::<i8>(ptr, scalar, expr) }?,
            ScalarType::I16 => unsafe { self.init_signed::<i16>(ptr, scalar, expr) }?,
            ScalarType::I32 => unsafe { self.init_signed::<i32>(ptr, scalar, expr) }?,
            ScalarType::I64 => unsafe { self.init_signed::<i64>(ptr, scalar, expr) }?,
            ScalarType::I128 => unsafe { self.init_signed::<i128>(ptr, scalar, expr) }?,
            ScalarType::ISize => unsafe { self.init_signed::<isize>(ptr, scalar, expr) }?,
            _ => {
                return Err(FableError::Unsupported {
                    feature: format!("initializing {scalar:?}"),
                });
            }
        }
        Ok(())
    }

    unsafe fn init_unsigned<T>(
        &self,
        ptr: PtrUninit,
        target: ScalarType,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<u128>,
    {
        let value = self.eval_number_as_u128(expect_number_expr(expr)?)?;
        let converted =
            T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
        unsafe { ptr.put(converted) };
        Ok(())
    }

    unsafe fn init_signed<T>(
        &self,
        ptr: PtrUninit,
        target: ScalarType,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<i128>,
    {
        let value = self.eval_number_as_i128(expect_number_expr(expr)?)?;
        let converted =
            T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
        unsafe { ptr.put(converted) };
        Ok(())
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

fn scalar_kind_name(scalar: ScalarType) -> &'static str {
    match scalar {
        ScalarType::Unit => "unit",
        ScalarType::Bool => "bool",
        ScalarType::Char => "char",
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => "string",
        ScalarType::F32 | ScalarType::F64 => "float",
        ScalarType::U8
        | ScalarType::U16
        | ScalarType::U32
        | ScalarType::U64
        | ScalarType::U128
        | ScalarType::USize => "unsigned number",
        ScalarType::I8
        | ScalarType::I16
        | ScalarType::I32
        | ScalarType::I64
        | ScalarType::I128
        | ScalarType::ISize => "signed number",
        _ => "unsupported scalar",
    }
}

fn read_signed_scalar(scalar: ScalarType, ptr: PtrConst) -> Result<i128, FableError> {
    let value = match scalar {
        ScalarType::I8 => (*unsafe { ptr.get::<i8>() }).into(),
        ScalarType::I16 => (*unsafe { ptr.get::<i16>() }).into(),
        ScalarType::I32 => (*unsafe { ptr.get::<i32>() }).into(),
        ScalarType::I64 => (*unsafe { ptr.get::<i64>() }).into(),
        ScalarType::I128 => *unsafe { ptr.get::<i128>() },
        ScalarType::ISize => (*unsafe { ptr.get::<isize>() }) as i128,
        _ => {
            return Err(FableError::TypeMismatch {
                expected: "signed number".into(),
                actual: scalar_kind_name(scalar),
            });
        }
    };
    Ok(value)
}

fn read_unsigned_scalar(scalar: ScalarType, ptr: PtrConst) -> Result<u128, FableError> {
    let value = match scalar {
        ScalarType::U8 => (*unsafe { ptr.get::<u8>() }).into(),
        ScalarType::U16 => (*unsafe { ptr.get::<u16>() }).into(),
        ScalarType::U32 => (*unsafe { ptr.get::<u32>() }).into(),
        ScalarType::U64 => (*unsafe { ptr.get::<u64>() }).into(),
        ScalarType::U128 => *unsafe { ptr.get::<u128>() },
        ScalarType::USize => (*unsafe { ptr.get::<usize>() }) as u128,
        _ => {
            return Err(FableError::TypeMismatch {
                expected: "unsigned number".into(),
                actual: scalar_kind_name(scalar),
            });
        }
    };
    Ok(value)
}

fn read_float_scalar(scalar: ScalarType, ptr: PtrConst) -> Result<f64, FableError> {
    match scalar {
        ScalarType::F32 => Ok((*unsafe { ptr.get::<f32>() }).into()),
        ScalarType::F64 => Ok(*unsafe { ptr.get::<f64>() }),
        _ => Err(FableError::TypeMismatch {
            expected: "float".into(),
            actual: scalar_kind_name(scalar),
        }),
    }
}

fn write_signed_scalar(scalar: ScalarType, ptr: PtrMut, value: i128) -> Result<(), FableError> {
    match scalar {
        ScalarType::I8 => unsafe { write_signed_value::<i8>(ptr, scalar, value) },
        ScalarType::I16 => unsafe { write_signed_value::<i16>(ptr, scalar, value) },
        ScalarType::I32 => unsafe { write_signed_value::<i32>(ptr, scalar, value) },
        ScalarType::I64 => unsafe { write_signed_value::<i64>(ptr, scalar, value) },
        ScalarType::I128 => unsafe { write_signed_value::<i128>(ptr, scalar, value) },
        ScalarType::ISize => unsafe { write_signed_value::<isize>(ptr, scalar, value) },
        _ => Err(FableError::TypeMismatch {
            expected: "signed number".into(),
            actual: scalar_kind_name(scalar),
        }),
    }
}

unsafe fn write_signed_value<T>(
    ptr: PtrMut,
    target: ScalarType,
    value: i128,
) -> Result<(), FableError>
where
    T: TryFrom<i128>,
{
    let converted =
        T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
    *unsafe { ptr.as_mut::<T>() } = converted;
    Ok(())
}

fn write_unsigned_scalar(scalar: ScalarType, ptr: PtrMut, value: u128) -> Result<(), FableError> {
    match scalar {
        ScalarType::U8 => unsafe { write_unsigned_value::<u8>(ptr, scalar, value) },
        ScalarType::U16 => unsafe { write_unsigned_value::<u16>(ptr, scalar, value) },
        ScalarType::U32 => unsafe { write_unsigned_value::<u32>(ptr, scalar, value) },
        ScalarType::U64 => unsafe { write_unsigned_value::<u64>(ptr, scalar, value) },
        ScalarType::U128 => unsafe { write_unsigned_value::<u128>(ptr, scalar, value) },
        ScalarType::USize => unsafe { write_unsigned_value::<usize>(ptr, scalar, value) },
        _ => Err(FableError::TypeMismatch {
            expected: "unsigned number".into(),
            actual: scalar_kind_name(scalar),
        }),
    }
}

unsafe fn write_unsigned_value<T>(
    ptr: PtrMut,
    target: ScalarType,
    value: u128,
) -> Result<(), FableError>
where
    T: TryFrom<u128>,
{
    let converted =
        T::try_from(value).map_err(|_| number_out_of_range(target, value.to_string()))?;
    *unsafe { ptr.as_mut::<T>() } = converted;
    Ok(())
}

fn write_float_scalar(scalar: ScalarType, ptr: PtrMut, value: f64) -> Result<(), FableError> {
    match scalar {
        ScalarType::F32 => {
            *unsafe { ptr.as_mut::<f32>() } = value as f32;
            Ok(())
        }
        ScalarType::F64 => {
            *unsafe { ptr.as_mut::<f64>() } = value;
            Ok(())
        }
        _ => Err(FableError::TypeMismatch {
            expected: "float".into(),
            actual: scalar_kind_name(scalar),
        }),
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

fn expect_value_expr(expr: &ExprPlan) -> Result<&ValueExpr, FableError> {
    match expr {
        ExprPlan::Value(expr) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "typed value".into(),
            actual: other.kind_name(),
        }),
    }
}

#[derive(Debug)]
enum PathSegment {
    Name(String),
    Index { index: usize, literal: String },
}

fn collect_path(expr: &Expr) -> Result<Vec<PathSegment>, FableError> {
    match expr {
        Expr::Var(var) => {
            let name = var.name().ok_or(FableError::MalformedSyntax {
                reason: "variable reference without identifier",
            })?;
            Ok(vec![PathSegment::Name(name)])
        }
        Expr::Field(field) => {
            let base = field.base().ok_or(FableError::MalformedSyntax {
                reason: "field expression without base",
            })?;
            let mut path = collect_path(&base)?;
            let field_name = field.field_name().ok_or(FableError::MalformedSyntax {
                reason: "field expression without field name",
            })?;
            path.push(PathSegment::Name(field_name));
            Ok(path)
        }
        Expr::Index(index) => {
            let base = index.base().ok_or(FableError::MalformedSyntax {
                reason: "index expression without base",
            })?;
            let index_expr = index.index().ok_or(FableError::MalformedSyntax {
                reason: "index expression without index",
            })?;
            let mut path = collect_path(&base)?;
            let (index, literal) = literal_index(&index_expr)?;
            path.push(PathSegment::Index { index, literal });
            Ok(path)
        }
        Expr::Paren(paren) => {
            let inner = paren.expr().ok_or(FableError::MalformedSyntax {
                reason: "parenthesized path without inner expression",
            })?;
            collect_path(&inner)
        }
        Expr::Call(_) => Err(FableError::Unsupported {
            feature: "call paths".into(),
        }),
        _ => Err(FableError::Unsupported {
            feature: "non-path assignment targets".into(),
        }),
    }
}

fn literal_index(expr: &Expr) -> Result<(usize, String), FableError> {
    let Expr::Literal(literal) = expr else {
        return Err(FableError::Unsupported {
            feature: "dynamic index paths".into(),
        });
    };
    let token = literal.token().ok_or(FableError::MalformedSyntax {
        reason: "index literal without token",
    })?;
    if token.kind() != SyntaxKind::Int {
        return Err(FableError::Unsupported {
            feature: "non-integer index paths".into(),
        });
    }
    let text = token.text().to_owned();
    let index = text.parse().map_err(|_| FableError::InvalidLiteral {
        literal: text.clone(),
        reason: "index literal is out of range",
    })?;
    Ok((index, text))
}

fn index_step(
    shape: &'static Shape,
    index: usize,
    source: Box<str>,
) -> Result<(FieldStep, &'static Shape), FableError> {
    match shape.def {
        Def::List(def) => Ok((
            FieldStep::ListIndex {
                source,
                shape,
                index,
            },
            def.t(),
        )),
        Def::Array(def) => Ok((
            FieldStep::ArrayIndex {
                source,
                shape,
                len: def.n,
                stride: element_stride(def.t(), shape)?,
                index,
            },
            def.t(),
        )),
        Def::Slice(def) => Ok((
            FieldStep::SliceIndex {
                source,
                shape,
                len: def.vtable.len,
                stride: element_stride(def.t(), shape)?,
                index,
            },
            def.t(),
        )),
        _ => Err(FableError::Unsupported {
            feature: format!("index access on {shape}"),
        }),
    }
}

fn collect_reachable_shapes(shape: &'static Shape, out: &mut Vec<&'static Shape>) {
    if out.iter().any(|candidate| **candidate == *shape) {
        return;
    }
    out.push(shape);

    if let Type::User(UserType::Struct(struct_type)) = shape.ty {
        for field in struct_type.fields {
            collect_reachable_shapes(field.shape.get(), out);
        }
    }

    match shape.def {
        Def::List(def) => collect_reachable_shapes(def.t(), out),
        Def::Array(def) => collect_reachable_shapes(def.t(), out),
        Def::Slice(def) => collect_reachable_shapes(def.t(), out),
        _ => {}
    }

    if let Some(inner) = shape.inner {
        collect_reachable_shapes(inner, out);
    }
    if let Some(builder_shape) = shape.builder_shape {
        collect_reachable_shapes(builder_shape, out);
    }
}

fn shape_name_matches(shape: &'static Shape, name: &str) -> bool {
    let displayed = format!("{}", shape.type_name());
    displayed == name || displayed.rsplit("::").next() == Some(name)
}

fn element_stride(
    element_shape: &'static Shape,
    owner_shape: &'static Shape,
) -> Result<usize, FableError> {
    let layout = element_shape
        .layout
        .sized_layout()
        .map_err(|_| FableError::Unsupported {
            feature: format!("index access to unsized elements in {owner_shape}"),
        })?;
    Ok(layout.pad_to_align().size())
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

fn min_f64(lhs: f64, rhs: f64) -> Result<f64, FableError> {
    Ok(match compare_f64(lhs, rhs)? {
        Ordering::Less | Ordering::Equal => lhs,
        Ordering::Greater => rhs,
    })
}

fn max_f64(lhs: f64, rhs: f64) -> Result<f64, FableError> {
    Ok(match compare_f64(lhs, rhs)? {
        Ordering::Less => rhs,
        Ordering::Equal | Ordering::Greater => lhs,
    })
}

fn clamp_i128(value: i128, min: i128, max: i128) -> Result<i128, FableError> {
    if min > max {
        return Err(invalid_call(
            "clamp",
            "minimum bound is greater than maximum bound",
        ));
    }
    Ok(value.clamp(min, max))
}

fn clamp_u128(value: u128, min: u128, max: u128) -> Result<u128, FableError> {
    if min > max {
        return Err(invalid_call(
            "clamp",
            "minimum bound is greater than maximum bound",
        ));
    }
    Ok(value.clamp(min, max))
}

fn clamp_f64(value: f64, min: f64, max: f64) -> Result<f64, FableError> {
    if compare_f64(min, max)? == Ordering::Greater {
        return Err(invalid_call(
            "clamp",
            "minimum bound is greater than maximum bound",
        ));
    }
    max_f64(min_f64(value, max)?, min)
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

fn index_out_of_bounds(path: &str, index: usize, len: usize) -> FableError {
    FableError::IndexOutOfBounds {
        path: path.to_owned(),
        index,
        len,
    }
}

fn field_scalar(path: &FieldPath) -> Result<ScalarType, FableError> {
    path.scalar.ok_or(FableError::MalformedProgram {
        reason: "scalar operation referenced a non-scalar path",
    })
}

fn invalid_call(function: &'static str, reason: &'static str) -> FableError {
    FableError::InvalidCall { function, reason }
}

fn invalid_intrinsic(name: &'static str, reason: &'static str) -> FableError {
    FableError::InvalidIntrinsic { name, reason }
}

fn validate_intrinsic_name(name: &'static str) -> Result<(), FableError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_intrinsic(name, "empty intrinsic name"));
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return Err(invalid_intrinsic(
            name,
            "intrinsic name must start with '_' or an ASCII letter",
        ));
    }
    if chars.any(|ch| ch != '_' && !ch.is_ascii_alphanumeric()) {
        return Err(invalid_intrinsic(
            name,
            "intrinsic name must contain only '_' and ASCII alphanumeric characters",
        ));
    }
    Ok(())
}

fn arity_reason(expected: usize) -> &'static str {
    match expected {
        1 => "expected 1 argument",
        2 => "expected 2 arguments",
        3 => "expected 3 arguments",
        _ => "unexpected argument count",
    }
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

    #[derive(Debug, Facet, PartialEq, Clone, Copy)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct User {
        name: String,
        age: i32,
        active: bool,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct State {
        user: User,
        users: Vec<User>,
        checkpoints: [i32; 3],
        position: Point,
        visits: u32,
        score: f64,
        marker: char,
        tag: &'static str,
    }

    fn state() -> State {
        State {
            user: User {
                name: "Ada".into(),
                age: 17,
                active: false,
            },
            users: vec![
                User {
                    name: "Ada".into(),
                    age: 17,
                    active: false,
                },
                User {
                    name: "Grace".into(),
                    age: 30,
                    active: false,
                },
            ],
            checkpoints: [1, 2, 3],
            position: Point { x: 4, y: 5 },
            visits: 1,
            score: 1.5,
            marker: 'a',
            tag: "seed",
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
    fn applies_typed_intrinsic_calls() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                let trimmed = trim("  Ada  ");
                let size = len(trimmed);
                let adult_age = clamp(max(root.user.age, 18), 0, 130);
                let bounded_score = min(max(root.score, 2.0), 3.0);

                root.user.name = trimmed;
                root.visits = max(size, 4);
                root.user.age = adult_age;
                root.score = bounded_score;

                if contains(root.user.name, "da") and starts_with(root.user.name, "A") and ends_with(root.user.name, "a") {
                    root.user.active = true;
                }
            "#,
        )
        .unwrap();

        assert_eq!(value.user.name, "Ada");
        assert_eq!(value.visits, 4);
        assert_eq!(value.user.age, 18);
        assert_eq!(value.score, 2.0);
        assert!(value.user.active);
    }

    #[test]
    fn applies_indexed_paths_to_lists_and_arrays() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                root.users[1].name = root.user.name + " Lovelace";
                root.users[0].age = root.users[1].age + root.checkpoints[2];
                root.checkpoints[1] = root.users[0].age;
                if root.users[0].age == 33 {
                    root.users[1].active = true;
                }
            "#,
        )
        .unwrap();

        assert_eq!(value.users[1].name, "Ada Lovelace");
        assert_eq!(value.users[0].age, 33);
        assert_eq!(value.checkpoints, [1, 33, 3]);
        assert!(value.users[1].active);
    }

    #[test]
    fn applies_pod_struct_literals_through_typed_locals() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                let next = Point {
                    x: root.position.x + root.checkpoints[0],
                    y: root.users[1].age,
                };
                root.position = next;
            "#,
        )
        .unwrap();

        assert_eq!(value.position, Point { x: 5, y: 30 });
    }

    #[test]
    fn applies_pod_struct_literals_directly_to_paths() {
        let mut value = state();

        apply(
            &mut value,
            r#"
                root.position = Point { x: 8, y: root.user.age + 1 };
            "#,
        )
        .unwrap();

        assert_eq!(value.position, Point { x: 8, y: 18 });
    }

    #[test]
    fn rejects_reusing_moved_typed_locals() {
        let mut value = state();

        let err = apply(
            &mut value,
            r#"
                let next = Point { x: 1, y: 2 };
                root.position = next;
                root.position = next;
            "#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            FableError::MalformedProgram {
                reason: "local read before initialization",
            }
        ));
    }

    #[test]
    fn rejects_non_pod_struct_literals() {
        let err = compile_err(
            r#"
                let user = User {
                    name: "Ada",
                    age: 36,
                    active: true,
                };
            "#,
        );

        assert!(matches!(
            err,
            FableError::NonPodStructLiteral {
                shape
            } if shape.type_name().to_string().ends_with("User")
        ));
    }

    #[test]
    fn reports_missing_pod_struct_literal_fields() {
        let err = compile_err("let point = Point { x: 1 };");

        assert!(matches!(
            err,
            FableError::MissingStructField {
                field,
                ..
            } if field == "y"
        ));
    }

    #[test]
    fn reports_duplicate_pod_struct_literal_fields() {
        let err = compile_err("let point = Point { x: 1, x: 2, y: 3 };");

        assert!(matches!(
            err,
            FableError::DuplicateStructField {
                field
            } if field == "x"
        ));
    }

    #[test]
    fn applies_custom_host_intrinsics() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics.add_string_unary("scream", scream).unwrap();
        intrinsics
            .add_string_binary_predicate("contains_ci", contains_ci)
            .unwrap();
        intrinsics.add_signed_unary("plus_ten", plus_ten).unwrap();
        intrinsics
            .add_unsigned_unary("cap_seven", cap_seven)
            .unwrap();
        intrinsics.add_float_unary("half", half).unwrap();

        FablePlan::<State>::compile_with_intrinsics(
            r#"
                let name = scream(root.user.name);
                root.user.name = name;
                root.user.age = plus_ten(root.user.age);
                root.visits = cap_seven(root.visits + 20);
                root.score = half(root.score);
                if contains_ci(root.user.name, "ada!") {
                    root.user.active = true;
                }
            "#,
            &intrinsics,
        )
        .unwrap()
        .apply(&mut value)
        .unwrap();

        assert_eq!(value.user.name, "ADA!");
        assert_eq!(value.user.age, 27);
        assert_eq!(value.visits, 7);
        assert_eq!(value.score, 0.75);
        assert!(value.user.active);
    }

    #[test]
    fn applies_field_host_intrinsics() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_field_string_unary("describe_field", describe_field)
            .unwrap();
        intrinsics
            .add_field_bool_unary("field_is_positive", field_is_positive)
            .unwrap();

        FablePlan::<State>::compile_with_intrinsics(
            r#"
                root.user.name = describe_field(root.user.age);
                if field_is_positive(root.user.age) {
                    root.user.active = true;
                }
            "#,
            &intrinsics,
        )
        .unwrap()
        .apply(&mut value)
        .unwrap();

        assert_eq!(value.user.name, "root.user.age=17");
        assert!(value.user.active);
    }

    #[test]
    fn applies_host_intrinsics_to_indexed_paths() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_field_string_unary("describe_indexed_field", describe_indexed_field)
            .unwrap();
        intrinsics
            .add_field_mut_unary("rewrite_field", rewrite_field)
            .unwrap();

        FablePlan::<State>::compile_with_intrinsics(
            r#"
                root.users[0].name = describe_indexed_field(root.users[1].age);
                rewrite_field(root.users[1].name);
            "#,
            &intrinsics,
        )
        .unwrap()
        .apply(&mut value)
        .unwrap();

        assert_eq!(value.users[0].name, "root.users[1].age=30");
        assert_eq!(value.users[1].name, "GRACE!");
    }

    #[test]
    fn reports_field_host_intrinsic_type_mismatches() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_field_bool_unary("field_is_positive", field_is_positive)
            .unwrap();

        let err = FablePlan::<State>::compile_with_intrinsics(
            r#"
                if field_is_positive(root.user.name) {
                    root.user.active = true;
                }
            "#,
            &intrinsics,
        )
        .unwrap()
        .apply(&mut value)
        .unwrap_err();

        assert!(matches!(
            err,
            FableError::TypeMismatch {
                expected,
                actual: "string",
            } if expected == "signed number"
        ));
    }

    #[test]
    fn applies_field_mut_host_intrinsics() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_field_mut_unary("rewrite_field", rewrite_field)
            .unwrap();

        FablePlan::<State>::compile_with_intrinsics(
            r#"
                rewrite_field(root.user.name);
                rewrite_field(root.user.age);
                rewrite_field(root.visits);
                rewrite_field(root.score);
                rewrite_field(root.user.active);
                rewrite_field(root.marker);
            "#,
            &intrinsics,
        )
        .unwrap()
        .apply(&mut value)
        .unwrap();

        assert_eq!(value.user.name, "ADA!");
        assert_eq!(value.user.age, 22);
        assert_eq!(value.visits, 4);
        assert_eq!(value.score, 3.0);
        assert!(value.user.active);
        assert_eq!(value.marker, 'Z');
        assert_eq!(value.tag, "seed");
    }

    #[test]
    fn rejects_field_mut_intrinsics_for_read_only_fields() {
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_field_mut_unary("rewrite_field", rewrite_field)
            .unwrap();

        let err = match FablePlan::<State>::compile_with_intrinsics(
            "rewrite_field(root.tag);",
            &intrinsics,
        ) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "writing Str"
        ));
    }

    #[test]
    fn reports_indexed_path_out_of_bounds() {
        let mut value = state();
        let err = apply(&mut value, "root.users[5].age = 1").unwrap_err();

        assert!(matches!(
            err,
            FableError::IndexOutOfBounds {
                path,
                index: 5,
                len: 2,
            } if path == "root.users[5]"
        ));
    }

    #[test]
    fn apply_with_intrinsics_uses_custom_registry() {
        let mut value = state();
        let mut intrinsics = FableIntrinsics::empty();
        intrinsics.add_string_unary("scream", scream).unwrap();

        apply_with_intrinsics(
            &mut value,
            r#"
                root.user.name = scream(root.user.name);
            "#,
            &intrinsics,
        )
        .unwrap();

        assert_eq!(value.user.name, "ADA!");
    }

    #[test]
    fn empty_intrinsic_registry_excludes_builtins() {
        let err = match FablePlan::<State>::compile_with_intrinsics(
            "root.user.name = trim(root.user.name)",
            &FableIntrinsics::empty(),
        ) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "intrinsic trim"
        ));
    }

    #[test]
    fn reports_duplicate_host_intrinsics() {
        let mut intrinsics = FableIntrinsics::empty();
        intrinsics.add_string_unary("scream", scream).unwrap();
        let err = intrinsics.add_string_unary("scream", scream).unwrap_err();

        assert!(matches!(
            err,
            FableError::InvalidIntrinsic {
                name: "scream",
                reason: "duplicate intrinsic name",
            }
        ));
    }

    #[test]
    fn reports_invalid_host_intrinsic_names() {
        let mut intrinsics = FableIntrinsics::empty();
        let err = intrinsics
            .add_string_unary("not-valid", scream)
            .unwrap_err();

        assert!(matches!(
            err,
            FableError::InvalidIntrinsic {
                name: "not-valid",
                reason: "intrinsic name must contain only '_' and ASCII alphanumeric characters",
            }
        ));
    }

    #[test]
    fn reports_unknown_intrinsic_calls() {
        let err = compile_err("root.user.age = nope(root.user.age)");

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "intrinsic nope"
        ));
    }

    #[test]
    fn reports_intrinsic_arity_errors() {
        let err = compile_err("root.user.age = clamp(root.user.age, 0)");

        assert!(matches!(
            err,
            FableError::InvalidCall {
                function: "clamp",
                reason: "expected 3 arguments",
            }
        ));
    }

    #[test]
    fn reports_intrinsic_type_mismatches() {
        let err = compile_err("root.user.age = len(root.user.age)");

        assert!(matches!(
            err,
            FableError::TypeMismatch {
                expected,
                actual: "signed number",
            } if expected == "string"
        ));
    }

    #[test]
    fn reports_invalid_runtime_intrinsic_calls() {
        let mut value = state();
        let err = apply(&mut value, "root.user.age = clamp(root.user.age, 10, 0)").unwrap_err();

        assert!(matches!(
            err,
            FableError::InvalidCall {
                function: "clamp",
                reason: "minimum bound is greater than maximum bound",
            }
        ));
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
    fn rejects_dynamic_index_paths() {
        let err = compile_err("root.users[root.visits].name = \"Ada\"");

        assert!(matches!(
            err,
            FableError::Unsupported {
                feature
            } if feature == "dynamic index paths"
        ));
    }

    fn compile_err(src: &str) -> FableError {
        match FablePlan::<State>::compile(src) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        }
    }

    fn scream(value: &str) -> Result<String, FableError> {
        Ok(format!("{}!", value.to_ascii_uppercase()))
    }

    fn contains_ci(haystack: &str, needle: &str) -> Result<bool, FableError> {
        Ok(haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase()))
    }

    fn plus_ten(value: i128) -> Result<i128, FableError> {
        value
            .checked_add(10)
            .ok_or_else(|| number_out_of_range(ScalarType::I128, format!("{value} + 10")))
    }

    fn cap_seven(value: u128) -> Result<u128, FableError> {
        Ok(value.min(7))
    }

    fn half(value: f64) -> Result<f64, FableError> {
        Ok(value / 2.0)
    }

    fn describe_field(field: FableField<'_>) -> Result<String, FableError> {
        assert_eq!(field.path(), "root.user.age");
        assert!(field.shape().is_type::<i32>());
        assert_eq!(field.scalar(), ScalarType::I32);
        Ok(format!("{}={}", field.path(), field.read_i128()?))
    }

    fn describe_indexed_field(field: FableField<'_>) -> Result<String, FableError> {
        assert_eq!(field.path(), "root.users[1].age");
        assert!(field.shape().is_type::<i32>());
        assert_eq!(field.scalar(), ScalarType::I32);
        Ok(format!("{}={}", field.path(), field.read_i128()?))
    }

    fn field_is_positive(field: FableField<'_>) -> Result<bool, FableError> {
        Ok(field.read_i128()? > 0)
    }

    fn rewrite_field(mut field: FableFieldMut<'_>) -> Result<(), FableError> {
        match field.scalar() {
            ScalarType::String => {
                let value = field.read_string()?.to_ascii_uppercase();
                field.write_string(format!("{value}!"))
            }
            ScalarType::I32 => field.write_i128(field.read_i128()? + 5),
            ScalarType::U32 => field.write_u128(field.read_u128()? + 3),
            ScalarType::F64 => field.write_f64(field.read_f64()? * 2.0),
            ScalarType::Bool => field.write_bool(!field.read_bool()?),
            ScalarType::Char => field.write_char('Z'),
            other => Err(FableError::TypeMismatch {
                expected: "known rewrite field".into(),
                actual: scalar_kind_name(other),
            }),
        }
    }
}
