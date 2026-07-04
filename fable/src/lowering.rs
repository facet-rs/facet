use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::ptr::copy_nonoverlapping;

use facet_core::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, ScalarType, Shape, StructKind, Type, UserType,
};
use weavy::ir::{
    ControlOp, DenseWeavyLowered, DenseWeavyProgram, EffectContract, EffectResource,
    IntrinsicDescriptor, IntrinsicOp, MemoryRegion, TypedMemoryAccess, WeavyOp,
};
use weavy::mem::declared as declared_mem;
use weavy::mem::runtime::RawScratch;
use weavy::mem::{Access as WeavyAccess, Tag as WeavyTag};
use weavy::task::{Fn as TaskFn, FnId as TaskFnId, HostFn, Op as TaskOp, Program as TaskProgram};
use weavy::{BlockRef, Control, RunError, RunStats, Step};

use crate::ast::{
    self, BinaryExpr, Block, CallExpr, ElseClause, Expr, IfStmt, Item, Literal, Name, Stmt,
    StructLiteral, UnaryExpr,
};
use crate::{ParseError, parse};

/// A reusable lowered Fable program for `T`.
///
/// Build a plan once with [`FablePlan::compile`], then apply it repeatedly to
/// mutable values of the same Facet-reflected type.
pub struct FablePlan<T> {
    plan: FableRootPlan,
    _marker: PhantomData<fn() -> T>,
}

/// A reusable lowered Fable transform from `Input` to `Output`.
///
/// Transform plans expose a read-only `in` root and a read-write `out` root,
/// which is the common shape for serialization/deserialization adapters and
/// data-pipeline transforms.
pub struct FableTransformPlan<Input, Output> {
    plan: FableRootPlan,
    _marker: PhantomData<fn(&Input) -> Output>,
}

/// A reusable lowered Fable predicate for `T`.
///
/// Predicate plans expose a read-only `root` and return the final boolean
/// expression in the source. Build once with [`FablePredicatePlan::compile`],
/// then evaluate repeatedly against values of the same Facet-reflected type.
pub struct FablePredicatePlan<T> {
    plan: FableRootPredicatePlan,
    _marker: PhantomData<fn(&T) -> bool>,
}

/// A reusable lowered Fable query for `T`.
///
/// Query plans expose a read-only `root` and return the final expression in the
/// source as a concrete Rust scalar/string type. Build once with
/// [`FableQueryPlan::compile`], then evaluate repeatedly against values of the
/// same Facet-reflected type.
pub struct FableQueryPlan<T, Output> {
    plan: FableRootQueryPlan<Output>,
    _marker: PhantomData<fn(&T) -> Output>,
}

/// A reusable lowered Fable program over explicitly named roots.
///
/// This is the lower-level form used by transform/debug-shell style callers
/// that expose more than one typed root to a script.
pub struct FableRootPlan {
    lowered: FableLowered,
    roots: Box<[FableRootSpec]>,
    declared_types: FableDeclaredTypes,
}

/// A reusable lowered Fable predicate over explicitly named roots.
///
/// All roots must be read-only. The final top-level statement must be a boolean
/// expression; earlier statements may bind typed locals.
pub struct FableRootPredicatePlan {
    lowered: FableLowered,
    roots: Box<[FableRootSpec]>,
    declared_types: FableDeclaredTypes,
}

/// A reusable lowered Fable query over explicitly named roots.
///
/// All roots must be read-only. The final top-level statement must be an
/// expression compatible with `Output`; earlier statements may bind typed
/// locals.
pub struct FableRootQueryPlan<Output> {
    lowered: FableLowered,
    task_query: Option<FableTaskQueryPlan>,
    roots: Box<[FableRootSpec]>,
    declared_types: FableDeclaredTypes,
    _marker: PhantomData<fn() -> Output>,
}

type FableLowered = DenseWeavyLowered<FableIntrinsic>;
type FableProgram = DenseWeavyProgram<FableIntrinsic>;
type FableWeavyOp = WeavyOp<BlockRef, FableIntrinsic>;

/// Broad result lane expected by a Fable query plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FableQueryType {
    /// Unit expression.
    Unit,
    /// Boolean expression.
    Bool,
    /// Character expression.
    Char,
    /// Owned string expression.
    String,
    /// Signed integer expression.
    Signed,
    /// Unsigned integer expression.
    Unsigned,
    /// Floating-point expression.
    Float,
}

impl FableQueryType {
    fn name(self) -> &'static str {
        match self {
            FableQueryType::Unit => "unit",
            FableQueryType::Bool => "bool",
            FableQueryType::Char => "char",
            FableQueryType::String => "string",
            FableQueryType::Signed => "signed number",
            FableQueryType::Unsigned => "unsigned number",
            FableQueryType::Float => "float",
        }
    }
}

/// A fable-declared type descriptor keyed by fable schema/type names.
pub type FableDeclaredDescriptor = weavy::mem::Descriptor<String>;

/// Type declarations compiled from a fable source file.
#[derive(Clone, Debug, Default)]
pub struct FableDeclaredTypes {
    types: Vec<FableDeclaredType>,
    by_name: BTreeMap<String, usize>,
}

impl FableDeclaredTypes {
    /// Look up a declared type by name.
    pub fn get(&self, name: &str) -> Option<&FableDeclaredType> {
        self.by_name.get(name).map(|&index| &self.types[index])
    }

    /// Iterate declared types in source declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &FableDeclaredType> {
        self.types.iter()
    }

    /// Whether the source declared no fable types.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    fn by_index(&self, index: usize) -> Option<&FableDeclaredType> {
        self.types.get(index)
    }
}

/// One fable-declared type plus its computed memory descriptor.
#[derive(Clone, Debug)]
pub struct FableDeclaredType {
    name: String,
    kind: FableDeclaredTypeKind,
    descriptor: FableDeclaredDescriptor,
}

impl FableDeclaredType {
    /// Declared type name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Fable-owned name/index metadata.
    pub fn kind(&self) -> &FableDeclaredTypeKind {
        &self.kind
    }

    /// The ordinary Weavy memory descriptor for this declared type.
    pub fn descriptor(&self) -> &FableDeclaredDescriptor {
        &self.descriptor
    }
}

/// Fable-owned metadata for a declared type.
#[derive(Clone, Debug)]
pub enum FableDeclaredTypeKind {
    Struct { fields: Vec<FableDeclaredField> },
    Enum { variants: Vec<FableDeclaredVariant> },
}

/// Fable-owned metadata for a declared field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FableDeclaredField {
    name: String,
    type_name: String,
}

impl FableDeclaredField {
    /// Field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Resolved fable scalar or declared type name.
    pub fn type_name(&self) -> &str {
        &self.type_name
    }
}

/// Fable-owned metadata for one enum variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FableDeclaredVariant {
    name: String,
    fields: Vec<FableDeclaredField>,
}

impl FableDeclaredVariant {
    /// Variant name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Variant payload fields in source order.
    pub fn fields(&self) -> &[FableDeclaredField] {
        &self.fields
    }
}

/// Rust result types accepted by [`FableQueryPlan`] and [`FableRootQueryPlan`].
pub trait FableQueryResult: query_result_sealed::Sealed + Sized + 'static {
    /// Return the broad Fable result lane expected by this Rust type.
    fn query_type() -> FableQueryType;

    #[doc(hidden)]
    fn from_unit() -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "unit"))
    }

    #[doc(hidden)]
    fn from_bool(_value: bool) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "bool"))
    }

    #[doc(hidden)]
    fn from_char(_value: char) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "char"))
    }

    #[doc(hidden)]
    fn from_string(_value: String) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "string"))
    }

    #[doc(hidden)]
    fn from_i128(_value: i128) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "signed number"))
    }

    #[doc(hidden)]
    fn from_u128(_value: u128) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "unsigned number"))
    }

    #[doc(hidden)]
    fn from_f64(_value: f64) -> Result<Self, FableError> {
        Err(query_result_mismatch(Self::query_type(), "float"))
    }
}

mod query_result_sealed {
    pub trait Sealed {}
}

impl FableQueryResult for () {
    fn query_type() -> FableQueryType {
        FableQueryType::Unit
    }

    fn from_unit() -> Result<Self, FableError> {
        Ok(())
    }
}

impl query_result_sealed::Sealed for () {}

impl FableQueryResult for bool {
    fn query_type() -> FableQueryType {
        FableQueryType::Bool
    }

    fn from_bool(value: bool) -> Result<Self, FableError> {
        Ok(value)
    }
}

impl query_result_sealed::Sealed for bool {}

impl FableQueryResult for char {
    fn query_type() -> FableQueryType {
        FableQueryType::Char
    }

    fn from_char(value: char) -> Result<Self, FableError> {
        Ok(value)
    }
}

impl query_result_sealed::Sealed for char {}

impl FableQueryResult for String {
    fn query_type() -> FableQueryType {
        FableQueryType::String
    }

    fn from_string(value: String) -> Result<Self, FableError> {
        Ok(value)
    }
}

impl query_result_sealed::Sealed for String {}

macro_rules! impl_signed_query_result {
    ($($ty:ty => $scalar:ident),* $(,)?) => {
        $(
            impl query_result_sealed::Sealed for $ty {}

            impl FableQueryResult for $ty {
                fn query_type() -> FableQueryType {
                    FableQueryType::Signed
                }

                fn from_i128(value: i128) -> Result<Self, FableError> {
                    Self::try_from(value)
                        .map_err(|_| number_out_of_range(ScalarType::$scalar, value.to_string()))
                }

                fn from_u128(value: u128) -> Result<Self, FableError> {
                    Self::try_from(value)
                        .map_err(|_| number_out_of_range(ScalarType::$scalar, value.to_string()))
                }
            }
        )*
    };
}

macro_rules! impl_unsigned_query_result {
    ($($ty:ty => $scalar:ident),* $(,)?) => {
        $(
            impl query_result_sealed::Sealed for $ty {}

            impl FableQueryResult for $ty {
                fn query_type() -> FableQueryType {
                    FableQueryType::Unsigned
                }

                fn from_i128(value: i128) -> Result<Self, FableError> {
                    Self::try_from(value)
                        .map_err(|_| number_out_of_range(ScalarType::$scalar, value.to_string()))
                }

                fn from_u128(value: u128) -> Result<Self, FableError> {
                    Self::try_from(value)
                        .map_err(|_| number_out_of_range(ScalarType::$scalar, value.to_string()))
                }
            }
        )*
    };
}

impl_signed_query_result! {
    i8 => I8,
    i16 => I16,
    i32 => I32,
    i64 => I64,
    i128 => I128,
    isize => ISize,
}

impl_unsigned_query_result! {
    u8 => U8,
    u16 => U16,
    u32 => U32,
    u64 => U64,
    u128 => U128,
    usize => USize,
}

impl FableQueryResult for f32 {
    fn query_type() -> FableQueryType {
        FableQueryType::Float
    }

    fn from_i128(value: i128) -> Result<Self, FableError> {
        Ok(value as f32)
    }

    fn from_u128(value: u128) -> Result<Self, FableError> {
        Ok(value as f32)
    }

    fn from_f64(value: f64) -> Result<Self, FableError> {
        Ok(value as f32)
    }
}

impl query_result_sealed::Sealed for f32 {}

impl FableQueryResult for f64 {
    fn query_type() -> FableQueryType {
        FableQueryType::Float
    }

    fn from_i128(value: i128) -> Result<Self, FableError> {
        Ok(value as f64)
    }

    fn from_u128(value: u128) -> Result<Self, FableError> {
        Ok(value as f64)
    }

    fn from_f64(value: f64) -> Result<Self, FableError> {
        Ok(value)
    }
}

impl query_result_sealed::Sealed for f64 {}

/// Access policy for a Fable root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FableRootAccess {
    /// The script may read this root, but may not assign through it or pass it
    /// to mutable field intrinsics.
    ReadOnly,
    /// The script may read and write this root.
    ReadWrite,
}

/// Compile-time description of a named Fable root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FableRootSpec {
    name: &'static str,
    shape: &'static Shape,
    access: FableRootAccess,
}

impl FableRootSpec {
    /// Create a read-only root spec for `T`.
    #[must_use]
    pub fn read_only<T>(name: &'static str) -> Self
    where
        T: Facet<'static>,
    {
        Self {
            name,
            shape: T::SHAPE,
            access: FableRootAccess::ReadOnly,
        }
    }

    /// Create a read-write root spec for `T`.
    #[must_use]
    pub fn read_write<T>(name: &'static str) -> Self
    where
        T: Facet<'static>,
    {
        Self {
            name,
            shape: T::SHAPE,
            access: FableRootAccess::ReadWrite,
        }
    }

    /// Root name as used in Fable source.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Root shape.
    #[must_use]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Root access policy.
    #[must_use]
    pub fn access(&self) -> FableRootAccess {
        self.access
    }
}

/// Runtime value bound to a named Fable root.
pub struct FableRootValue<'root> {
    name: &'static str,
    shape: &'static Shape,
    ptr: RuntimeRootPtr,
    _marker: PhantomData<&'root mut ()>,
}

impl<'root> FableRootValue<'root> {
    /// Bind an immutable value to a read-only root.
    #[must_use]
    pub fn read_only<T>(name: &'static str, value: &'root T) -> Self
    where
        T: Facet<'static>,
    {
        Self {
            name,
            shape: T::SHAPE,
            ptr: RuntimeRootPtr::Const(PtrConst::new_sized(value as *const T)),
            _marker: PhantomData,
        }
    }

    /// Bind a mutable value to a read-write root.
    #[must_use]
    pub fn read_write<T>(name: &'static str, value: &'root mut T) -> Self
    where
        T: Facet<'static>,
    {
        Self {
            name,
            shape: T::SHAPE,
            ptr: RuntimeRootPtr::Mut(PtrMut::new_sized(value as *mut T)),
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Copy)]
enum RuntimeRootPtr {
    Const(PtrConst),
    Mut(PtrMut),
}

impl RuntimeRootPtr {
    fn as_const(self) -> PtrConst {
        match self {
            Self::Const(ptr) => ptr,
            Self::Mut(ptr) => ptr.as_const(),
        }
    }

    fn as_mut(self) -> Option<PtrMut> {
        match self {
            Self::Const(_) => None,
            Self::Mut(ptr) => Some(ptr),
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeRoot {
    name: &'static str,
    ptr: RuntimeRootPtr,
}

const TRANSFORM_INPUT_ROOT: &str = "in";
const TRANSFORM_OUTPUT_ROOT: &str = "out";

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
        let roots = [FableRootSpec::read_write::<T>("root")];
        let plan = FableRootPlan::compile_with_intrinsics(src, &roots, intrinsics)?;

        Ok(Self {
            plan,
            _marker: PhantomData,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        self.plan.declared_types()
    }

    /// Run this plan against `value`.
    pub fn apply(&self, value: &mut T) -> Result<(), FableError> {
        let mut roots = [FableRootValue::read_write("root", value)];
        self.plan.apply(&mut roots)
    }

    /// Run this plan and return Weavy execution counters.
    pub fn apply_with_stats(&self, value: &mut T) -> Result<RunStats, FableError> {
        let mut roots = [FableRootValue::read_write("root", value)];
        self.plan.apply_with_stats(&mut roots)
    }
}

impl<Input, Output> FableTransformPlan<Input, Output>
where
    Input: Facet<'static>,
    Output: Facet<'static>,
{
    /// Parse and lower Fable source as an `in` to `out` transform.
    pub fn compile(src: &str) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, &FableIntrinsics::standard())
    }

    /// Parse and lower Fable source as an `in` to `out` transform with host intrinsics.
    pub fn compile_with_intrinsics(
        src: &str,
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        let roots = [
            FableRootSpec::read_only::<Input>(TRANSFORM_INPUT_ROOT),
            FableRootSpec::read_write::<Output>(TRANSFORM_OUTPUT_ROOT),
        ];
        let plan = FableRootPlan::compile_with_intrinsics(src, &roots, intrinsics)?;

        Ok(Self {
            plan,
            _marker: PhantomData,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        self.plan.declared_types()
    }

    /// Run this transform from `input` into `output`.
    pub fn apply(&self, input: &Input, output: &mut Output) -> Result<(), FableError> {
        let mut roots = [
            FableRootValue::read_only(TRANSFORM_INPUT_ROOT, input),
            FableRootValue::read_write(TRANSFORM_OUTPUT_ROOT, output),
        ];
        self.plan.apply(&mut roots)
    }

    /// Run this transform and return Weavy execution counters.
    pub fn apply_with_stats(
        &self,
        input: &Input,
        output: &mut Output,
    ) -> Result<RunStats, FableError> {
        let mut roots = [
            FableRootValue::read_only(TRANSFORM_INPUT_ROOT, input),
            FableRootValue::read_write(TRANSFORM_OUTPUT_ROOT, output),
        ];
        self.plan.apply_with_stats(&mut roots)
    }
}

impl<T> FablePredicatePlan<T>
where
    T: Facet<'static>,
{
    /// Parse and lower a read-only Fable predicate for values of type `T`.
    pub fn compile(src: &str) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, &FableIntrinsics::standard())
    }

    /// Parse and lower a read-only Fable predicate with an explicit host-call registry.
    pub fn compile_with_intrinsics(
        src: &str,
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        let roots = [FableRootSpec::read_only::<T>("root")];
        let plan = FableRootPredicatePlan::compile_with_intrinsics(src, &roots, intrinsics)?;

        Ok(Self {
            plan,
            _marker: PhantomData,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        self.plan.declared_types()
    }

    /// Run this predicate against `value`.
    pub fn evaluate(&self, value: &T) -> Result<bool, FableError> {
        let mut roots = [FableRootValue::read_only("root", value)];
        self.plan.evaluate(&mut roots)
    }

    /// Run this predicate and return Weavy execution counters.
    pub fn evaluate_with_stats(&self, value: &T) -> Result<(bool, RunStats), FableError> {
        let mut roots = [FableRootValue::read_only("root", value)];
        self.plan.evaluate_with_stats(&mut roots)
    }
}

impl<T, Output> FableQueryPlan<T, Output>
where
    T: Facet<'static>,
    Output: FableQueryResult,
{
    /// Parse and lower a read-only Fable query for values of type `T`.
    pub fn compile(src: &str) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, &FableIntrinsics::standard())
    }

    /// Parse and lower a read-only Fable query with an explicit host-call registry.
    pub fn compile_with_intrinsics(
        src: &str,
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        let roots = [FableRootSpec::read_only::<T>("root")];
        let plan = FableRootQueryPlan::compile_with_intrinsics(src, &roots, intrinsics)?;

        Ok(Self {
            plan,
            _marker: PhantomData,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        self.plan.declared_types()
    }

    /// Run this query against `value`.
    pub fn evaluate(&self, value: &T) -> Result<Output, FableError> {
        let mut roots = [FableRootValue::read_only("root", value)];
        self.plan.evaluate(&mut roots)
    }

    /// Run this query and return Weavy execution counters.
    pub fn evaluate_with_stats(&self, value: &T) -> Result<(Output, RunStats), FableError> {
        let mut roots = [FableRootValue::read_only("root", value)];
        self.plan.evaluate_with_stats(&mut roots)
    }
}

impl FableRootPlan {
    /// Parse and lower Fable source for an explicit root set.
    pub fn compile(src: &str, roots: &[FableRootSpec]) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, roots, &FableIntrinsics::standard())
    }

    /// Parse and lower Fable source with explicit roots and host intrinsics.
    pub fn compile_with_intrinsics(
        src: &str,
        roots: &[FableRootSpec],
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        validate_root_specs(roots)?;

        let root = parse(src).map_err(|error| FableError::Parse { error })?;
        let mut lowerer = Lowerer::new(roots, intrinsics);
        let program = lowerer.lower_root(&root)?;
        let declared_types = lowerer.declared_types.clone();
        let blocks = lowerer.into_blocks();

        Ok(Self {
            lowered: FableLowered::new(program, blocks),
            roots: roots.into(),
            declared_types,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        &self.declared_types
    }

    /// Run this plan against explicitly bound root values.
    pub fn apply(&self, roots: &mut [FableRootValue<'_>]) -> Result<(), FableError> {
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_apply_in_task(&self.lowered, runtime_roots, self.declared_types.clone())
    }

    /// Run this plan and return Weavy execution counters.
    pub fn apply_with_stats(
        &self,
        roots: &mut [FableRootValue<'_>],
    ) -> Result<RunStats, FableError> {
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_apply_in_task_with_stats(
            &self.lowered,
            runtime_roots,
            self.declared_types.clone(),
        )
    }

    fn runtime_roots(&self, values: &[FableRootValue<'_>]) -> Result<Vec<RuntimeRoot>, FableError> {
        runtime_roots(&self.roots, values)
    }
}

impl FableRootPredicatePlan {
    /// Parse and lower a Fable predicate for an explicit root set.
    pub fn compile(src: &str, roots: &[FableRootSpec]) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, roots, &FableIntrinsics::standard())
    }

    /// Parse and lower a Fable predicate with explicit roots and host intrinsics.
    pub fn compile_with_intrinsics(
        src: &str,
        roots: &[FableRootSpec],
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        validate_root_specs(roots)?;
        validate_predicate_root_specs(roots)?;

        let root = parse(src).map_err(|error| FableError::Parse { error })?;
        let mut lowerer = Lowerer::new(roots, intrinsics);
        let program = lowerer.lower_predicate_root(&root)?;
        let declared_types = lowerer.declared_types.clone();
        let blocks = lowerer.into_blocks();

        Ok(Self {
            lowered: FableLowered::new(program, blocks),
            roots: roots.into(),
            declared_types,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        &self.declared_types
    }

    /// Run this predicate against explicitly bound root values.
    pub fn evaluate(&self, roots: &mut [FableRootValue<'_>]) -> Result<bool, FableError> {
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_predicate_in_task(&self.lowered, runtime_roots, self.declared_types.clone())
    }

    /// Run this predicate and return Weavy execution counters.
    pub fn evaluate_with_stats(
        &self,
        roots: &mut [FableRootValue<'_>],
    ) -> Result<(bool, RunStats), FableError> {
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_predicate_in_task_with_stats(
            &self.lowered,
            runtime_roots,
            self.declared_types.clone(),
        )
    }

    fn runtime_roots(&self, values: &[FableRootValue<'_>]) -> Result<Vec<RuntimeRoot>, FableError> {
        runtime_roots(&self.roots, values)
    }
}

impl<Output> FableRootQueryPlan<Output>
where
    Output: FableQueryResult,
{
    /// Parse and lower a Fable query for an explicit root set.
    pub fn compile(src: &str, roots: &[FableRootSpec]) -> Result<Self, FableError> {
        Self::compile_with_intrinsics(src, roots, &FableIntrinsics::standard())
    }

    /// Parse and lower a Fable query with explicit roots and host intrinsics.
    pub fn compile_with_intrinsics(
        src: &str,
        roots: &[FableRootSpec],
        intrinsics: &FableIntrinsics,
    ) -> Result<Self, FableError> {
        validate_root_specs(roots)?;
        validate_read_only_root_specs(roots, "query roots must be read-only")?;

        let root = parse(src).map_err(|error| FableError::Parse { error })?;
        let task_query = FableTaskQueryPlan::from_source(&root, roots, Output::query_type())?;
        if task_query.is_some() {
            let declared_types = FableDeclaredTypes::from_source(&root)?;
            return Ok(Self {
                lowered: FableLowered::new(Vec::new(), Vec::new()),
                task_query,
                roots: roots.into(),
                declared_types,
                _marker: PhantomData,
            });
        }
        let mut lowerer = Lowerer::new(roots, intrinsics);
        let program = lowerer.lower_query_root(&root, Output::query_type())?;
        let declared_types = lowerer.declared_types.clone();
        let blocks = lowerer.into_blocks();

        Ok(Self {
            lowered: FableLowered::new(program, blocks),
            task_query: None,
            roots: roots.into(),
            declared_types,
            _marker: PhantomData,
        })
    }

    /// Fable-declared types compiled with this plan.
    pub fn declared_types(&self) -> &FableDeclaredTypes {
        &self.declared_types
    }

    /// Run this query against explicitly bound root values.
    pub fn evaluate(&self, roots: &mut [FableRootValue<'_>]) -> Result<Output, FableError> {
        if let Some(task_query) = &self.task_query {
            validate_runtime_roots(roots)?;
            return task_query.evaluate();
        }
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_query_in_task(&self.lowered, runtime_roots, self.declared_types.clone())
    }

    /// Run this query and return Weavy execution counters.
    pub fn evaluate_with_stats(
        &self,
        roots: &mut [FableRootValue<'_>],
    ) -> Result<(Output, RunStats), FableError> {
        if let Some(task_query) = &self.task_query {
            validate_runtime_roots(roots)?;
            return Ok((task_query.evaluate()?, RunStats::default()));
        }
        let runtime_roots = self.runtime_roots(roots)?;
        run_dense_query_in_task_with_stats(
            &self.lowered,
            runtime_roots,
            self.declared_types.clone(),
        )
    }

    fn runtime_roots(&self, values: &[FableRootValue<'_>]) -> Result<Vec<RuntimeRoot>, FableError> {
        runtime_roots(&self.roots, values)
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

/// Compile and immediately evaluate a read-only Fable predicate.
pub fn predicate<T>(value: &T, src: &str) -> Result<bool, FableError>
where
    T: Facet<'static>,
{
    FablePredicatePlan::<T>::compile(src)?.evaluate(value)
}

/// Compile with explicit host intrinsics and immediately evaluate a read-only predicate.
pub fn predicate_with_intrinsics<T>(
    value: &T,
    src: &str,
    intrinsics: &FableIntrinsics,
) -> Result<bool, FableError>
where
    T: Facet<'static>,
{
    FablePredicatePlan::<T>::compile_with_intrinsics(src, intrinsics)?.evaluate(value)
}

/// Compile and immediately evaluate a read-only Fable query.
pub fn query<T, Output>(value: &T, src: &str) -> Result<Output, FableError>
where
    T: Facet<'static>,
    Output: FableQueryResult,
{
    FableQueryPlan::<T, Output>::compile(src)?.evaluate(value)
}

/// Compile with explicit host intrinsics and immediately evaluate a read-only query.
pub fn query_with_intrinsics<T, Output>(
    value: &T,
    src: &str,
    intrinsics: &FableIntrinsics,
) -> Result<Output, FableError>
where
    T: Facet<'static>,
    Output: FableQueryResult,
{
    FableQueryPlan::<T, Output>::compile_with_intrinsics(src, intrinsics)?.evaluate(value)
}

/// Compile and immediately apply a Fable `in` to `out` transform.
pub fn transform<Input, Output>(
    input: &Input,
    output: &mut Output,
    src: &str,
) -> Result<(), FableError>
where
    Input: Facet<'static>,
    Output: Facet<'static>,
{
    FableTransformPlan::<Input, Output>::compile(src)?.apply(input, output)
}

/// Compile with explicit host intrinsics and immediately apply a Fable transform.
pub fn transform_with_intrinsics<Input, Output>(
    input: &Input,
    output: &mut Output,
    src: &str,
    intrinsics: &FableIntrinsics,
) -> Result<(), FableError>
where
    Input: Facet<'static>,
    Output: Facet<'static>,
{
    FableTransformPlan::<Input, Output>::compile_with_intrinsics(src, intrinsics)?
        .apply(input, output)
}

#[derive(Clone)]
struct FableTaskQueryPlan {
    program: TaskProgram,
    result: TaskType,
}

impl FableTaskQueryPlan {
    fn from_source(
        root: &ast::SourceFile,
        roots: &[FableRootSpec],
        query_type: FableQueryType,
    ) -> Result<Option<Self>, FableError> {
        let functions: Vec<&ast::FnDecl> = root
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(function) => Some(function.as_ref()),
                _ => None,
            })
            .collect();
        if functions.is_empty() {
            return Ok(None);
        }
        if !roots.is_empty() {
            return Err(FableError::Unsupported {
                feature: "user functions over root values".into(),
            });
        }

        let statements: Vec<&Stmt> = root
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Stmt(stmt) => Some(stmt),
                Item::Struct(_) | Item::Enum(_) | Item::Fn(_) => None,
            })
            .collect();
        let [Stmt::Expr(final_expr)] = statements.as_slice() else {
            return Err(FableError::Unsupported {
                feature: "function scripts must end with one query expression".into(),
            });
        };

        let mut signatures = BTreeMap::new();
        for (index, function) in functions.iter().enumerate() {
            let name = function.name.value.clone();
            if signatures.contains_key(&name) {
                return Err(FableError::DuplicateLocal { name });
            }
            signatures.insert(
                name,
                TaskSignature {
                    fn_id: TaskFnId(u32::try_from(index + 1).map_err(|_| {
                        FableError::MalformedProgram {
                            reason: "too many task functions",
                        }
                    })?),
                    params: function
                        .params
                        .params
                        .iter()
                        .map(|param| TaskType::from_type_expr(&param.ty))
                        .collect::<Result<Vec<_>, _>>()?,
                    result: TaskType::from_type_expr(&function.return_ty)?,
                },
            );
        }

        let mut program = TaskProgram { fns: Vec::new() };
        let mut root_compiler = TaskFnCompiler::new(&signatures);
        let result = root_compiler.compile_expr(&final_expr.expr)?;
        let expected = TaskType::from_query_type(query_type)?;
        if result.ty != expected {
            return Err(FableError::TypeMismatch {
                expected: expected.name().to_owned(),
                actual: result.ty.name(),
            });
        }
        root_compiler.code.push(TaskOp::Ret {
            src: result.slot,
            size: 8,
        });
        program.fns.push(root_compiler.finish());

        for function in functions {
            let signature = signatures
                .get(function.name.value.as_str())
                .expect("signature exists");
            let mut compiler = TaskFnCompiler::new(&signatures);
            for (param, ty) in function.params.params.iter().zip(&signature.params) {
                let name = name_text(&param.name).to_owned();
                let slot = compiler.alloc_slot(*ty);
                compiler.locals.insert(name, TaskValue { ty: *ty, slot });
            }
            let result = compiler.compile_block_value(&function.body, signature.result)?;
            compiler.copy_slot(result.slot, signature.result, result.slot);
            compiler.code.push(TaskOp::Ret {
                src: result.slot,
                size: 8,
            });
            program.fns.push(compiler.finish());
        }

        Ok(Some(Self {
            program,
            result: expected,
        }))
    }

    fn evaluate<Output>(&self) -> Result<Output, FableError>
    where
        Output: FableQueryResult,
    {
        let mut task = weavy::task::Task::spawn(&self.program, TaskFnId(0));
        match task.run(&self.program, &[], &[]) {
            weavy::task::TaskStep::Done => {}
            weavy::task::TaskStep::Parked { .. } => {
                return Err(FableError::MalformedProgram {
                    reason: "synchronous fable task parked",
                });
            }
        }
        let value = i64::from_le_bytes(task.result[..8].try_into().map_err(|_| {
            FableError::MalformedProgram {
                reason: "task query result was not 8 bytes",
            }
        })?);
        match self.result {
            TaskType::I64 => FableQueryOutput::Signed(value as i128).into_result(),
            TaskType::Bool => FableQueryOutput::Bool(value != 0).into_result(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskType {
    I64,
    Bool,
}

impl TaskType {
    fn from_type_expr(ty: &ast::TypeExpr) -> Result<Self, FableError> {
        match ty {
            ast::TypeExpr::Scalar(name) if name.value == "i64" => Ok(TaskType::I64),
            ast::TypeExpr::Scalar(name) if name.value == "bool" => Ok(TaskType::Bool),
            ast::TypeExpr::Scalar(name) => Err(FableError::Unsupported {
                feature: format!("task function scalar type {}", name.value),
            }),
            ast::TypeExpr::Declared(name) => Err(FableError::Unsupported {
                feature: format!("task function declared type {}", name.name.value),
            }),
        }
    }

    fn from_query_type(query_type: FableQueryType) -> Result<Self, FableError> {
        match query_type {
            FableQueryType::Bool => Ok(TaskType::Bool),
            FableQueryType::Signed => Ok(TaskType::I64),
            other => Err(FableError::Unsupported {
                feature: format!("task function query result {}", other.name()),
            }),
        }
    }

    fn name(self) -> &'static str {
        match self {
            TaskType::I64 => "i64",
            TaskType::Bool => "bool",
        }
    }
}

#[derive(Clone)]
struct TaskSignature {
    fn_id: TaskFnId,
    params: Vec<TaskType>,
    result: TaskType,
}

#[derive(Clone, Copy)]
struct TaskValue {
    ty: TaskType,
    slot: u32,
}

struct TaskFnCompiler<'a> {
    signatures: &'a BTreeMap<String, TaskSignature>,
    locals: BTreeMap<String, TaskValue>,
    code: Vec<TaskOp>,
    slot_count: usize,
}

impl<'a> TaskFnCompiler<'a> {
    fn new(signatures: &'a BTreeMap<String, TaskSignature>) -> Self {
        Self {
            signatures,
            locals: BTreeMap::new(),
            code: Vec::new(),
            slot_count: 0,
        }
    }

    fn alloc_slot(&mut self, ty: TaskType) -> u32 {
        let slot = self.slot_count;
        self.slot_count += 1;
        match ty {
            TaskType::I64 | TaskType::Bool => u32::try_from(slot * 8).expect("frame offset"),
        }
    }

    fn finish(self) -> TaskFn {
        let fields = (0..self.slot_count)
            .map(|_| declared_mem::i64_(()))
            .collect();
        let frame = declared_mem::declared_struct((), fields).layout;
        TaskFn {
            frame,
            code: self.code,
        }
    }

    fn compile_block_value(
        &mut self,
        block: &Block,
        expected: TaskType,
    ) -> Result<TaskValue, FableError> {
        let Some((last, prefix)) = block.stmts.split_last() else {
            return Err(FableError::Unsupported {
                feature: "function block without result expression".into(),
            });
        };
        for stmt in prefix {
            self.compile_stmt(stmt)?;
        }
        self.compile_stmt_value(last, expected)
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), FableError> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let value = self.compile_expr(&let_stmt.value)?;
                let local = self.alloc_slot(value.ty);
                self.copy_slot(value.slot, value.ty, local);
                let name = name_text(&let_stmt.name).to_owned();
                if self
                    .locals
                    .insert(
                        name.clone(),
                        TaskValue {
                            ty: value.ty,
                            slot: local,
                        },
                    )
                    .is_some()
                {
                    return Err(FableError::DuplicateLocal { name });
                }
                Ok(())
            }
            Stmt::Expr(expr) => self.compile_expr(&expr.expr).map(drop),
            Stmt::If(if_stmt) => self.compile_if_value(if_stmt, TaskType::I64).map(drop),
            Stmt::Assign(_) => Err(FableError::Unsupported {
                feature: "assignment inside task functions".into(),
            }),
        }
    }

    fn compile_stmt_value(
        &mut self,
        stmt: &Stmt,
        expected: TaskType,
    ) -> Result<TaskValue, FableError> {
        match stmt {
            Stmt::Expr(expr) => {
                let value = self.compile_expr(&expr.expr)?;
                self.expect_type(expected, value)?;
                Ok(value)
            }
            Stmt::If(if_stmt) => self.compile_if_value(if_stmt, expected),
            Stmt::Let(_) | Stmt::Assign(_) => Err(FableError::Unsupported {
                feature: "function block must end with an expression".into(),
            }),
        }
    }

    fn compile_if_value(
        &mut self,
        if_stmt: &IfStmt,
        expected: TaskType,
    ) -> Result<TaskValue, FableError> {
        let condition = self.compile_expr(&if_stmt.condition)?;
        self.expect_type(TaskType::Bool, condition)?;
        let out = self.alloc_slot(expected);
        let jump_to_else = self.code.len();
        self.code.push(TaskOp::JumpIfZero {
            value: condition.slot,
            target: 0,
        });
        let then_value = self.compile_block_value(&if_stmt.then, expected)?;
        self.copy_slot(then_value.slot, expected, out);
        let jump_to_end = self.code.len();
        self.code.push(TaskOp::Jump { target: 0 });
        let else_start = self.code.len();
        self.patch_jump(jump_to_else, else_start)?;
        let else_value = self.compile_else_value(&if_stmt.else_clause, expected)?;
        self.copy_slot(else_value.slot, expected, out);
        let end = self.code.len();
        self.patch_jump(jump_to_end, end)?;
        Ok(TaskValue {
            ty: expected,
            slot: out,
        })
    }

    fn compile_else_value(
        &mut self,
        else_clause: &Option<ElseClause>,
        expected: TaskType,
    ) -> Result<TaskValue, FableError> {
        let Some(else_clause) = else_clause else {
            return Err(FableError::Unsupported {
                feature: "value-producing if without else".into(),
            });
        };
        if let Some(if_stmt) = &else_clause.if_stmt {
            self.compile_if_value(if_stmt, expected)
        } else if let Some(block) = &else_clause.block {
            self.compile_block_value(block, expected)
        } else {
            Err(FableError::MalformedSyntax {
                reason: "else clause without body",
            })
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<TaskValue, FableError> {
        match expr {
            Expr::Literal(literal) => self.compile_literal(literal),
            Expr::Var(var) => {
                let name = name_text(&var.name);
                self.locals
                    .get(name)
                    .copied()
                    .ok_or_else(|| FableError::ExpectedRoot {
                        found: name.to_owned(),
                    })
            }
            Expr::Paren(paren) => self.compile_expr(&paren.expr),
            Expr::Unary(unary) => self.compile_unary(unary),
            Expr::Binary(binary) => self.compile_binary(binary),
            Expr::Call(call) => self.compile_call(call),
            Expr::Field(_)
            | Expr::Index(_)
            | Expr::StructLiteral(_)
            | Expr::EnumVariant(_)
            | Expr::Match(_) => Err(FableError::Unsupported {
                feature: "task function expression".into(),
            }),
        }
    }

    fn compile_literal(&mut self, literal: &Literal) -> Result<TaskValue, FableError> {
        let (ty, value) = match literal {
            Literal::True(_) => (TaskType::Bool, 1),
            Literal::False(_) => (TaskType::Bool, 0),
            Literal::Int(text) => (
                TaskType::I64,
                text.value
                    .parse::<i64>()
                    .map_err(|_| FableError::InvalidLiteral {
                        literal: text.value.clone(),
                        reason: "integer literal is out of range",
                    })?,
            ),
            Literal::Null(_) | Literal::Float(_) | Literal::Str(_) => {
                return Err(FableError::Unsupported {
                    feature: "task function literal".into(),
                });
            }
        };
        let slot = self.alloc_slot(ty);
        self.code.push(TaskOp::ConstI64 { dst: slot, value });
        Ok(TaskValue { ty, slot })
    }

    fn compile_unary(&mut self, unary: &UnaryExpr) -> Result<TaskValue, FableError> {
        let operand = self.compile_expr(&unary.operand)?;
        match unary_op(unary)? {
            UnaryOp::Not => {
                self.expect_type(TaskType::Bool, operand)?;
                let zero = self.const_i64(0);
                let slot = self.alloc_slot(TaskType::Bool);
                self.code.push(TaskOp::EqI64 {
                    dst: slot,
                    a: operand.slot,
                    b: zero,
                });
                Ok(TaskValue {
                    ty: TaskType::Bool,
                    slot,
                })
            }
            UnaryOp::Neg => {
                self.expect_type(TaskType::I64, operand)?;
                let zero = self.const_i64(0);
                let slot = self.alloc_slot(TaskType::I64);
                self.code.push(TaskOp::SubI64 {
                    dst: slot,
                    a: zero,
                    b: operand.slot,
                });
                Ok(TaskValue {
                    ty: TaskType::I64,
                    slot,
                })
            }
        }
    }

    fn compile_binary(&mut self, binary: &BinaryExpr) -> Result<TaskValue, FableError> {
        let lhs = self.compile_expr(&binary.lhs)?;
        let rhs = self.compile_expr(&binary.rhs)?;
        match binary_op(binary)? {
            BinaryOp::Add | BinaryOp::Sub => {
                self.expect_type(TaskType::I64, lhs)?;
                self.expect_type(TaskType::I64, rhs)?;
                let slot = self.alloc_slot(TaskType::I64);
                let op = match binary_op(binary)? {
                    BinaryOp::Add => TaskOp::AddI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Sub => TaskOp::SubI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    _ => unreachable!(),
                };
                self.code.push(op);
                Ok(TaskValue {
                    ty: TaskType::I64,
                    slot,
                })
            }
            BinaryOp::Eq
            | BinaryOp::Neq
            | BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge => {
                if lhs.ty != rhs.ty {
                    return Err(FableError::TypeMismatch {
                        expected: lhs.ty.name().to_owned(),
                        actual: rhs.ty.name(),
                    });
                }
                let slot = self.alloc_slot(TaskType::Bool);
                let op = match binary_op(binary)? {
                    BinaryOp::Eq => TaskOp::EqI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Neq => TaskOp::NeI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Lt => TaskOp::LtI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Gt => TaskOp::GtI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Le => TaskOp::LeI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    BinaryOp::Ge => TaskOp::GeI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    },
                    _ => unreachable!(),
                };
                self.code.push(op);
                Ok(TaskValue {
                    ty: TaskType::Bool,
                    slot,
                })
            }
            BinaryOp::And | BinaryOp::Or => {
                self.expect_type(TaskType::Bool, lhs)?;
                self.expect_type(TaskType::Bool, rhs)?;
                let slot = self.alloc_slot(TaskType::Bool);
                match binary_op(binary)? {
                    BinaryOp::And => self.code.push(TaskOp::MulI64 {
                        dst: slot,
                        a: lhs.slot,
                        b: rhs.slot,
                    }),
                    BinaryOp::Or => {
                        let sum = self.alloc_slot(TaskType::I64);
                        self.code.push(TaskOp::AddI64 {
                            dst: sum,
                            a: lhs.slot,
                            b: rhs.slot,
                        });
                        let zero = self.const_i64(0);
                        self.code.push(TaskOp::NeI64 {
                            dst: slot,
                            a: sum,
                            b: zero,
                        });
                    }
                    _ => unreachable!(),
                }
                Ok(TaskValue {
                    ty: TaskType::Bool,
                    slot,
                })
            }
        }
    }

    fn compile_call(&mut self, call: &CallExpr) -> Result<TaskValue, FableError> {
        let name = call_callee_name(&call.callee)?;
        let signature = self
            .signatures
            .get(name.as_str())
            .ok_or_else(|| FableError::Unsupported {
                feature: format!("task function call to {name}"),
            })?
            .clone();
        if signature.params.len() != call.args.args.len() {
            return Err(FableError::Unsupported {
                feature: format!("{} argument count", name),
            });
        }
        let mut copies = Vec::with_capacity(signature.params.len());
        for (arg, expected) in call.args.args.iter().zip(&signature.params) {
            let value = self.compile_expr(&arg.expr)?;
            self.expect_type(*expected, value)?;
            let dst =
                u32::try_from(copies.len() * 8).map_err(|_| FableError::MalformedProgram {
                    reason: "argument offset overflow",
                })?;
            copies.push(weavy::task::ArgCopy {
                src: value.slot,
                dst,
                size: 8,
            });
        }
        let slot = self.alloc_slot(signature.result);
        self.code.push(TaskOp::Call {
            callee: signature.fn_id,
            args: copies,
            ret: slot,
        });
        Ok(TaskValue {
            ty: signature.result,
            slot,
        })
    }

    fn copy_slot(&mut self, src: u32, ty: TaskType, dst: u32) {
        let zero = self.const_i64(0);
        match ty {
            TaskType::I64 | TaskType::Bool => self.code.push(TaskOp::AddI64 {
                dst,
                a: src,
                b: zero,
            }),
        }
    }

    fn const_i64(&mut self, value: i64) -> u32 {
        let slot = self.alloc_slot(TaskType::I64);
        self.code.push(TaskOp::ConstI64 { dst: slot, value });
        slot
    }

    fn expect_type(&self, expected: TaskType, value: TaskValue) -> Result<(), FableError> {
        if value.ty == expected {
            Ok(())
        } else {
            Err(FableError::TypeMismatch {
                expected: expected.name().to_owned(),
                actual: value.ty.name(),
            })
        }
    }

    fn patch_jump(&mut self, index: usize, target: usize) -> Result<(), FableError> {
        let target = u32::try_from(target).map_err(|_| FableError::MalformedProgram {
            reason: "jump target overflow",
        })?;
        match self.code.get_mut(index) {
            Some(TaskOp::Jump { target: slot }) | Some(TaskOp::JumpIfZero { target: slot, .. }) => {
                *slot = target;
                Ok(())
            }
            _ => Err(FableError::MalformedProgram {
                reason: "attempted to patch non-jump task op",
            }),
        }
    }
}

fn run_error(err: RunError<BlockRef, FableError>) -> FableError {
    match err {
        RunError::Step(err) => err,
        RunError::MissingBlock(block) => FableError::MissingBlock { block },
    }
}

fn host_task_program(result_size: u32) -> TaskProgram {
    TaskProgram {
        fns: vec![TaskFn {
            frame: weavy::mem::Layout {
                size: result_size as usize,
                align: 1,
            },
            code: vec![
                TaskOp::HostCall { host: 0 },
                TaskOp::Ret {
                    src: 0,
                    size: result_size,
                },
            ],
        }],
    }
}

fn run_single_host_task(host: &mut dyn FnMut(&mut [u8])) {
    let program = host_task_program(0);
    let mut task = weavy::task::Task::spawn(&program, TaskFnId(0));
    let mut hosts: [HostFn<'_>; 1] = [host];
    let step = task.run_hosted(&program, &[], &[], &mut hosts);
    debug_assert_eq!(step, weavy::task::TaskStep::Done);
}

fn dense_interp(roots: Vec<RuntimeRoot>, declared_types: FableDeclaredTypes) -> FableInterp {
    FableInterp {
        roots,
        locals: LocalSlots::default(),
        declared_types,
        predicate_result: None,
        query_result: None,
    }
}

fn run_dense_apply_in_task(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<(), FableError> {
    let mut roots = Some(roots);
    let mut result = Ok(());
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "apply host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense(lowered, &mut interp).map_err(run_error);
        };
        run_single_host_task(&mut host);
    }
    result
}

fn run_dense_apply_in_task_with_stats(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<RunStats, FableError> {
    let mut roots = Some(roots);
    let mut result = Err(FableError::MalformedProgram {
        reason: "apply host task did not run",
    });
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "apply host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense_with_stats(lowered, &mut interp).map_err(run_error);
        };
        run_single_host_task(&mut host);
    }
    result
}

fn run_dense_predicate_in_task(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<bool, FableError> {
    let mut roots = Some(roots);
    let mut result = Err(FableError::MalformedProgram {
        reason: "predicate host task did not run",
    });
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "predicate host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense(lowered, &mut interp)
                .map_err(run_error)
                .and_then(|()| {
                    interp.predicate_result.ok_or(FableError::MalformedProgram {
                        reason: "predicate plan did not write a result",
                    })
                });
        };
        run_single_host_task(&mut host);
    }
    result
}

fn run_dense_predicate_in_task_with_stats(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<(bool, RunStats), FableError> {
    let mut roots = Some(roots);
    let mut result = Err(FableError::MalformedProgram {
        reason: "predicate host task did not run",
    });
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "predicate host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense_with_stats(lowered, &mut interp)
                .map_err(run_error)
                .and_then(|stats| {
                    let value = interp
                        .predicate_result
                        .ok_or(FableError::MalformedProgram {
                            reason: "predicate plan did not write a result",
                        })?;
                    Ok((value, stats))
                });
        };
        run_single_host_task(&mut host);
    }
    result
}

fn run_dense_query_in_task<Output>(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<Output, FableError>
where
    Output: FableQueryResult,
{
    let mut roots = Some(roots);
    let mut result = Err(FableError::MalformedProgram {
        reason: "query host task did not run",
    });
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "query host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense(lowered, &mut interp)
                .map_err(run_error)
                .and_then(|()| {
                    interp
                        .query_result
                        .ok_or(FableError::MalformedProgram {
                            reason: "query plan did not write a result",
                        })?
                        .into_result()
                });
        };
        run_single_host_task(&mut host);
    }
    result
}

fn run_dense_query_in_task_with_stats<Output>(
    lowered: &FableLowered,
    roots: Vec<RuntimeRoot>,
    declared_types: FableDeclaredTypes,
) -> Result<(Output, RunStats), FableError>
where
    Output: FableQueryResult,
{
    let mut roots = Some(roots);
    let mut result = Err(FableError::MalformedProgram {
        reason: "query host task did not run",
    });
    {
        let mut host = |_: &mut [u8]| {
            let Some(roots) = roots.take() else {
                result = Err(FableError::MalformedProgram {
                    reason: "query host task ran more than once",
                });
                return;
            };
            let mut interp = dense_interp(roots, declared_types.clone());
            result = weavy::run_dense_with_stats(lowered, &mut interp)
                .map_err(run_error)
                .and_then(|stats| {
                    let value = interp
                        .query_result
                        .ok_or(FableError::MalformedProgram {
                            reason: "query plan did not write a result",
                        })?
                        .into_result()?;
                    Ok((value, stats))
                });
        };
        run_single_host_task(&mut host);
    }
    result
}

fn validate_root_specs(roots: &[FableRootSpec]) -> Result<(), FableError> {
    for (index, root) in roots.iter().enumerate() {
        validate_root_name(root.name)?;
        if roots[..index].iter().any(|seen| seen.name == root.name) {
            return Err(FableError::DuplicateRoot {
                name: root.name.to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_predicate_root_specs(roots: &[FableRootSpec]) -> Result<(), FableError> {
    validate_read_only_root_specs(roots, "predicate roots must be read-only")
}

fn validate_read_only_root_specs(
    roots: &[FableRootSpec],
    reason: &'static str,
) -> Result<(), FableError> {
    for root in roots {
        if root.access != FableRootAccess::ReadOnly {
            return Err(FableError::InvalidRoot {
                name: root.name.to_owned(),
                reason,
            });
        }
    }
    Ok(())
}

fn validate_runtime_roots(roots: &[FableRootValue<'_>]) -> Result<(), FableError> {
    for (index, root) in roots.iter().enumerate() {
        validate_root_name(root.name)?;
        if roots[..index].iter().any(|seen| seen.name == root.name) {
            return Err(FableError::DuplicateRoot {
                name: root.name.to_owned(),
            });
        }
    }
    Ok(())
}

fn runtime_roots(
    specs: &[FableRootSpec],
    values: &[FableRootValue<'_>],
) -> Result<Vec<RuntimeRoot>, FableError> {
    validate_runtime_roots(values)?;

    let mut roots = Vec::with_capacity(specs.len());
    for spec in specs {
        let value = values
            .iter()
            .find(|value| value.name == spec.name)
            .ok_or_else(|| FableError::MissingRoot {
                name: spec.name.to_owned(),
            })?;
        if value.shape != spec.shape {
            return Err(FableError::RootShapeMismatch {
                name: spec.name.to_owned(),
                expected: spec.shape,
                actual: value.shape,
            });
        }
        if spec.access == FableRootAccess::ReadWrite && value.ptr.as_mut().is_none() {
            return Err(FableError::ReadOnlyRoot {
                name: spec.name.to_owned(),
            });
        }
        roots.push(RuntimeRoot {
            name: spec.name,
            ptr: value.ptr,
        });
    }
    Ok(roots)
}

fn validate_root_name(name: &'static str) -> Result<(), FableError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_root(name, "empty root name"));
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return Err(invalid_root(
            name,
            "root name must start with '_' or an ASCII letter",
        ));
    }
    if chars.any(|ch| ch != '_' && !ch.is_ascii_alphanumeric()) {
        return Err(invalid_root(
            name,
            "root name must contain only '_' and ASCII alphanumeric characters",
        ));
    }
    if matches!(
        name,
        "if" | "else" | "let" | "and" | "or" | "not" | "true" | "false" | "null" | "none"
    ) {
        return Err(invalid_root(name, "root name is a Fable keyword"));
    }
    Ok(())
}

/// Error returned while parsing, lowering, or running Fable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FableError {
    /// The parser rejected invalid source.
    Parse {
        /// Parse error.
        error: ParseError,
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
    /// A root spec or runtime binding was malformed.
    InvalidRoot {
        /// Root name.
        name: String,
        /// Reason it was rejected.
        reason: &'static str,
    },
    /// A root name appeared more than once.
    DuplicateRoot {
        /// Duplicate root name.
        name: String,
    },
    /// A required runtime root binding was not supplied.
    MissingRoot {
        /// Missing root name.
        name: String,
    },
    /// A runtime root binding had a different shape than the compiled plan.
    RootShapeMismatch {
        /// Root name.
        name: String,
        /// Compiled shape.
        expected: &'static Shape,
        /// Runtime shape.
        actual: &'static Shape,
    },
    /// The script attempted to mutate a read-only root.
    ReadOnlyRoot {
        /// Root name.
        name: String,
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
            FableError::Parse { error } => {
                write!(f, "Fable parse failed: {}", error.message)
            }
            FableError::MalformedSyntax { reason } => {
                write!(f, "Fable CST was malformed: {reason}")
            }
            FableError::Unsupported { feature } => {
                write!(f, "Fable lowering does not support {feature} yet")
            }
            FableError::InvalidRoot { name, reason } => {
                write!(f, "invalid Fable root {name}: {reason}")
            }
            FableError::DuplicateRoot { name } => {
                write!(f, "Fable root {name} is already defined")
            }
            FableError::MissingRoot { name } => {
                write!(f, "Fable runtime root {name} was not supplied")
            }
            FableError::RootShapeMismatch {
                name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Fable runtime root {name} has shape {actual}, expected {expected}"
                )
            }
            FableError::ReadOnlyRoot { name } => {
                write!(f, "Fable root {name} is read-only")
            }
            FableError::ExpectedRoot { found } => {
                write!(f, "Fable path root {found} is not available")
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
enum FableIntrinsic {
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
        then_block: BlockRef,
        else_block: Option<BlockRef>,
    },
    Predicate(BoolExpr),
    Query(ExprPlan),
}

impl IntrinsicOp for FableIntrinsic {
    fn descriptor(&self) -> IntrinsicDescriptor {
        IntrinsicDescriptor {
            dialect: "fable",
            name: match self {
                FableIntrinsic::Let { .. } => "let",
                FableIntrinsic::Assign { .. } => "assign",
                FableIntrinsic::Eval(_) => "eval",
                FableIntrinsic::Branch { .. } => "branch",
                FableIntrinsic::Predicate(_) => "predicate",
                FableIntrinsic::Query(_) => "query",
            },
        }
    }

    fn effect(&self) -> EffectContract {
        match self {
            FableIntrinsic::Let { .. } => EffectContract::new()
                .write_resource(EffectResource::SideChannel("fable.locals"))
                .may_fail()
                .may_allocate()
                .calls_user_code(),
            FableIntrinsic::Assign { .. } => EffectContract::new()
                .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::Overwrite)
                .may_fail()
                .may_allocate()
                .calls_user_code(),
            FableIntrinsic::Eval(_) => EffectContract::new()
                .read_resource(EffectResource::SideChannel("fable.locals"))
                .may_fail()
                .may_allocate()
                .calls_user_code(),
            FableIntrinsic::Branch { .. } => EffectContract::new()
                .read_resource(EffectResource::SideChannel("fable.locals"))
                .may_fail()
                .calls_user_code(),
            FableIntrinsic::Predicate(_) => EffectContract::new()
                .read_resource(EffectResource::SideChannel("fable.locals"))
                .write_resource(EffectResource::SideChannel("fable.result"))
                .may_fail()
                .may_allocate()
                .calls_user_code(),
            FableIntrinsic::Query(_) => EffectContract::new()
                .read_resource(EffectResource::SideChannel("fable.locals"))
                .write_resource(EffectResource::SideChannel("fable.result"))
                .may_fail()
                .may_allocate()
                .calls_user_code(),
        }
    }
}

fn fable_op(intrinsic: FableIntrinsic) -> FableWeavyOp {
    WeavyOp::Intrinsic(intrinsic)
}

enum FableQueryOutput {
    Unit,
    Bool(bool),
    Char(char),
    String(String),
    Signed(i128),
    Unsigned(u128),
    Float(f64),
}

impl FableQueryOutput {
    fn into_result<Output>(self) -> Result<Output, FableError>
    where
        Output: FableQueryResult,
    {
        match self {
            FableQueryOutput::Unit => Output::from_unit(),
            FableQueryOutput::Bool(value) => Output::from_bool(value),
            FableQueryOutput::Char(value) => Output::from_char(value),
            FableQueryOutput::String(value) => Output::from_string(value),
            FableQueryOutput::Signed(value) => Output::from_i128(value),
            FableQueryOutput::Unsigned(value) => Output::from_u128(value),
            FableQueryOutput::Float(value) => Output::from_f64(value),
        }
    }
}

#[derive(Debug)]
enum ExprPlan {
    Unit(UnitExpr),
    Bool(BoolExpr),
    Char(CharExpr),
    String(StringExpr),
    Number(NumberExpr),
    Value(ValueExpr),
    Match(Box<MatchExprPlan>),
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
            ExprPlan::Match(expr) => expr.result_kind.name(),
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
    Declared { index: usize, type_index: usize },
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
            LocalRef::Declared { .. } => "declared value",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclaredScalar {
    Unit,
    Bool,
    Char,
    String,
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    F32,
    F64,
}

impl DeclaredScalar {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "unit" => Self::Unit,
            "bool" => Self::Bool,
            "char" => Self::Char,
            "string" => Self::String,
            "i8" => Self::I8,
            "i16" => Self::I16,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "i128" => Self::I128,
            "isize" => Self::ISize,
            "u8" => Self::U8,
            "u16" => Self::U16,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "u128" => Self::U128,
            "usize" => Self::USize,
            "f32" => Self::F32,
            "f64" => Self::F64,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Bool => "bool",
            Self::Char => "char",
            Self::String => "string",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::ISize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::USize => "usize",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DeclaredScalarRead {
    local: LocalRef,
    offset: usize,
    scalar: DeclaredScalar,
}

#[derive(Debug)]
enum UnitExpr {
    Null,
    Read(FieldPath),
    Local(LocalRef),
    DeclaredRead(DeclaredScalarRead),
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
    DeclaredRead(DeclaredScalarRead),
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
    DeclaredRead(DeclaredScalarRead),
}

#[derive(Debug)]
enum StringExpr {
    Literal(String),
    Read(FieldPath),
    Local(LocalRef),
    DeclaredRead(DeclaredScalarRead),
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
    DeclaredRead(DeclaredScalarRead),
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
    DeclaredRead(DeclaredScalarRead),
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
    DeclaredRead(DeclaredScalarRead),
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
    Declared(DeclaredValueExpr),
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

#[derive(Debug)]
enum DeclaredValueExpr {
    Struct(DeclaredStructExpr),
    Enum(DeclaredEnumExpr),
    Local(LocalRef),
    Field(DeclaredValueField),
}

#[derive(Debug)]
struct DeclaredStructExpr {
    type_index: usize,
    fields: Box<[DeclaredFieldInit]>,
}

#[derive(Debug)]
struct DeclaredEnumExpr {
    type_index: usize,
    selector: u64,
    tag_offset: usize,
    tag_width: usize,
    fields: Box<[DeclaredFieldInit]>,
}

#[derive(Clone, Copy, Debug)]
struct DeclaredValueField {
    local: LocalRef,
    type_index: usize,
    offset: usize,
}

#[derive(Debug)]
struct DeclaredFieldInit {
    offset: usize,
    ty: DeclaredFieldType,
    value: ExprPlan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclaredFieldType {
    Scalar(DeclaredScalar),
    Declared(usize),
}

impl ValueExpr {
    fn shape(&self) -> &'static Shape {
        match self {
            Self::Struct(expr) => expr.shape,
            Self::Local(LocalRef::Value { shape, .. }) => shape,
            Self::Declared(_) => unreachable!("declared values do not have Facet shapes"),
            Self::Local(_) => unreachable!("value expression local must refer to a value slot"),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            Self::Declared(_) => "declared value",
            _ => "typed value",
        }
    }
}

impl StructExpr {
    fn kind_name(&self) -> &'static str {
        "typed value"
    }
}

impl DeclaredValueExpr {
    fn type_index(&self) -> usize {
        match self {
            Self::Struct(expr) => expr.type_index,
            Self::Enum(expr) => expr.type_index,
            Self::Local(LocalRef::Declared { type_index, .. }) => *type_index,
            Self::Field(field) => field.type_index,
            Self::Local(_) => unreachable!("declared value local must refer to declared slot"),
        }
    }
}

#[derive(Debug)]
struct MatchExprPlan {
    scrutinee: DeclaredValueExpr,
    enum_type_index: usize,
    result_kind: FableQueryType,
    arms: Box<[MatchArmPlan]>,
}

#[derive(Debug)]
struct MatchArmPlan {
    selector: Option<u64>,
    bindings: Box<[MatchBindingPlan]>,
    prefix: FableProgram,
    result: ExprPlan,
}

#[derive(Debug)]
enum MatchBindingPlan {
    Scalar {
        local: LocalRef,
        offset: usize,
        scalar: DeclaredScalar,
    },
    Declared {
        local: LocalRef,
        offset: usize,
        type_index: usize,
    },
}

struct ExpectedDeclaredField {
    name: String,
    offset: usize,
    ty: DeclaredFieldType,
}

struct ExpectedDeclaredVariant {
    selector: u64,
    tag_offset: usize,
    tag_width: usize,
    fields: Vec<ExpectedDeclaredField>,
}

struct LoweredBlockResult {
    prefix: FableProgram,
    result: ExprPlan,
}

#[derive(Debug)]
struct FieldPath {
    root: usize,
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
    declared_count: usize,
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
            ExprPlan::Value(expr) => match expr {
                ValueExpr::Declared(expr) => {
                    let index = self.declared_count;
                    self.declared_count += 1;
                    LocalRef::Declared {
                        index,
                        type_index: expr.type_index(),
                    }
                }
                _ => {
                    let index = self.value_count;
                    self.value_count += 1;
                    LocalRef::Value {
                        index,
                        shape: expr.shape(),
                    }
                }
            },
            ExprPlan::Match(_) => unreachable!("match expressions cannot allocate locals directly"),
        }
    }

    fn allocate_declared_scalar(&mut self, scalar: DeclaredScalar) -> LocalRef {
        match scalar {
            DeclaredScalar::Unit => {
                let index = self.unit_count;
                self.unit_count += 1;
                LocalRef::Unit(index)
            }
            DeclaredScalar::Bool => {
                let index = self.bool_count;
                self.bool_count += 1;
                LocalRef::Bool(index)
            }
            DeclaredScalar::Char => {
                let index = self.char_count;
                self.char_count += 1;
                LocalRef::Char(index)
            }
            DeclaredScalar::String => {
                let index = self.string_count;
                self.string_count += 1;
                LocalRef::String(index)
            }
            DeclaredScalar::I8
            | DeclaredScalar::I16
            | DeclaredScalar::I32
            | DeclaredScalar::I64
            | DeclaredScalar::I128
            | DeclaredScalar::ISize => {
                let index = self.signed_count;
                self.signed_count += 1;
                LocalRef::Signed(index)
            }
            DeclaredScalar::U8
            | DeclaredScalar::U16
            | DeclaredScalar::U32
            | DeclaredScalar::U64
            | DeclaredScalar::U128
            | DeclaredScalar::USize => {
                let index = self.unsigned_count;
                self.unsigned_count += 1;
                LocalRef::Unsigned(index)
            }
            DeclaredScalar::F32 | DeclaredScalar::F64 => {
                let index = self.float_count;
                self.float_count += 1;
                LocalRef::Float(index)
            }
        }
    }

    fn allocate_declared_value(&mut self, type_index: usize) -> LocalRef {
        let index = self.declared_count;
        self.declared_count += 1;
        LocalRef::Declared { index, type_index }
    }
}

#[derive(Clone, Copy)]
enum DeclRef<'ast> {
    Struct(&'ast ast::StructDecl),
    Enum(&'ast ast::EnumDecl),
}

enum DeclBuildState {
    Pending,
    Building,
    Built(Box<FableDeclaredType>),
}

struct DeclaredTypeBuilder<'ast> {
    decls: Vec<(String, DeclRef<'ast>)>,
    by_name: BTreeMap<String, usize>,
    states: Vec<DeclBuildState>,
}

impl FableDeclaredTypes {
    fn from_source(root: &ast::SourceFile) -> Result<Self, FableError> {
        let mut decls = Vec::new();
        let mut by_name = BTreeMap::new();
        for item in &root.items {
            let (name, decl) = match item {
                Item::Struct(decl) => (decl.name.value.clone(), DeclRef::Struct(decl)),
                Item::Enum(decl) => (decl.name.value.clone(), DeclRef::Enum(decl)),
                Item::Fn(_) | Item::Stmt(_) => continue,
            };
            if by_name.insert(name.clone(), decls.len()).is_some() {
                return Err(FableError::AmbiguousType { name });
            }
            decls.push((name, decl));
        }

        let mut builder = DeclaredTypeBuilder {
            states: (0..decls.len()).map(|_| DeclBuildState::Pending).collect(),
            decls,
            by_name,
        };

        let mut types = Vec::with_capacity(builder.decls.len());
        let mut public_by_name = BTreeMap::new();
        for index in 0..builder.decls.len() {
            let declared = builder.build_index(index)?;
            public_by_name.insert(declared.name.clone(), types.len());
            types.push(declared);
        }

        Ok(Self {
            types,
            by_name: public_by_name,
        })
    }
}

impl<'ast> DeclaredTypeBuilder<'ast> {
    fn build_name(&mut self, name: &str) -> Result<FableDeclaredType, FableError> {
        let index = *self
            .by_name
            .get(name)
            .ok_or_else(|| FableError::UnknownType {
                name: name.to_owned(),
            })?;
        self.build_index(index)
    }

    fn build_index(&mut self, index: usize) -> Result<FableDeclaredType, FableError> {
        match &self.states[index] {
            DeclBuildState::Built(declared) => return Ok((**declared).clone()),
            DeclBuildState::Building => {
                let name = self.decls[index].0.clone();
                return Err(FableError::Unsupported {
                    feature: format!("recursive declared type {name}"),
                });
            }
            DeclBuildState::Pending => {}
        }

        self.states[index] = DeclBuildState::Building;
        let name = self.decls[index].0.clone();
        let decl = self.decls[index].1;
        let declared = match decl {
            DeclRef::Struct(decl) => self.build_struct(name, decl)?,
            DeclRef::Enum(decl) => self.build_enum(name, decl)?,
        };
        self.states[index] = DeclBuildState::Built(Box::new(declared.clone()));
        Ok(declared)
    }

    fn build_struct(
        &mut self,
        name: String,
        decl: &ast::StructDecl,
    ) -> Result<FableDeclaredType, FableError> {
        let (fields, descriptors) = self.build_type_fields(&decl.fields.fields)?;
        let descriptor = declared_mem::declared_struct(name.clone(), descriptors);
        Ok(FableDeclaredType {
            name,
            kind: FableDeclaredTypeKind::Struct { fields },
            descriptor,
        })
    }

    fn build_enum(
        &mut self,
        name: String,
        decl: &ast::EnumDecl,
    ) -> Result<FableDeclaredType, FableError> {
        let mut seen = BTreeMap::new();
        let mut variants = Vec::with_capacity(decl.variants.len());
        let mut variant_descriptors = Vec::with_capacity(decl.variants.len());
        for variant in &decl.variants {
            let variant_name = variant.name.value.clone();
            if seen.insert(variant_name.clone(), variants.len()).is_some() {
                return Err(FableError::DuplicateStructField {
                    field: variant_name,
                });
            }
            let (fields, descriptors) = if let Some(fields) = &variant.fields {
                self.build_type_fields(&fields.fields)?
            } else {
                (Vec::new(), Vec::new())
            };
            variants.push(FableDeclaredVariant {
                name: variant_name,
                fields,
            });
            variant_descriptors.push(descriptors);
        }
        let descriptor = declared_mem::declared_enum(name.clone(), variant_descriptors);
        Ok(FableDeclaredType {
            name,
            kind: FableDeclaredTypeKind::Enum { variants },
            descriptor,
        })
    }

    fn build_type_fields(
        &mut self,
        fields: &[ast::TypeField],
    ) -> Result<(Vec<FableDeclaredField>, Vec<FableDeclaredDescriptor>), FableError> {
        let mut seen = BTreeMap::new();
        let mut metadata = Vec::with_capacity(fields.len());
        let mut descriptors = Vec::with_capacity(fields.len());
        for field in fields {
            let name = name_text(&field.name).to_owned();
            if seen.insert(name.clone(), metadata.len()).is_some() {
                return Err(FableError::DuplicateStructField { field: name });
            }
            let (type_name, descriptor) = self.resolve_type_expr(&field.ty)?;
            metadata.push(FableDeclaredField { name, type_name });
            descriptors.push(descriptor);
        }
        Ok((metadata, descriptors))
    }

    fn resolve_type_expr(
        &mut self,
        ty: &ast::TypeExpr,
    ) -> Result<(String, FableDeclaredDescriptor), FableError> {
        match ty {
            ast::TypeExpr::Scalar(name) => {
                let type_name = name.value.clone();
                let descriptor = scalar_declared_descriptor(&type_name)?;
                Ok((type_name, descriptor))
            }
            ast::TypeExpr::Declared(declared) => {
                let declared = self.build_name(&declared.name.value)?;
                Ok((declared.name.clone(), declared.descriptor))
            }
        }
    }
}

fn scalar_declared_descriptor(name: &str) -> Result<FableDeclaredDescriptor, FableError> {
    let schema = name.to_owned();
    let descriptor = match name {
        "unit" => declared_mem::unit(schema),
        "bool" => declared_mem::bool_(schema),
        "char" => declared_mem::scalar(schema, size_of::<char>(), align_of::<char>()),
        "string" => declared_mem::scalar(schema, size_of::<String>(), align_of::<String>()),
        "i8" => declared_mem::scalar(schema, size_of::<i8>(), align_of::<i8>()),
        "i16" => declared_mem::scalar(schema, size_of::<i16>(), align_of::<i16>()),
        "i32" => declared_mem::scalar(schema, size_of::<i32>(), align_of::<i32>()),
        "i64" => declared_mem::i64_(schema),
        "i128" => declared_mem::scalar(schema, size_of::<i128>(), align_of::<i128>()),
        "isize" => declared_mem::scalar(schema, size_of::<isize>(), align_of::<isize>()),
        "u8" => declared_mem::scalar(schema, size_of::<u8>(), align_of::<u8>()),
        "u16" => declared_mem::scalar(schema, size_of::<u16>(), align_of::<u16>()),
        "u32" => declared_mem::scalar(schema, size_of::<u32>(), align_of::<u32>()),
        "u64" => declared_mem::scalar(schema, size_of::<u64>(), align_of::<u64>()),
        "u128" => declared_mem::scalar(schema, size_of::<u128>(), align_of::<u128>()),
        "usize" => declared_mem::scalar(schema, size_of::<usize>(), align_of::<usize>()),
        "f32" => declared_mem::scalar(schema, size_of::<f32>(), align_of::<f32>()),
        "f64" => declared_mem::f64_(schema),
        other => {
            return Err(FableError::UnknownType {
                name: other.to_owned(),
            });
        }
    };
    Ok(descriptor)
}

struct Lowerer<'intrinsics> {
    roots: Box<[FableRootSpec]>,
    intrinsics: &'intrinsics FableIntrinsics,
    scopes: Vec<BTreeMap<String, LocalRef>>,
    locals: LocalAllocator,
    type_shapes: Vec<&'static Shape>,
    declared_types: FableDeclaredTypes,
    blocks: Vec<FableProgram>,
}

impl<'intrinsics> Lowerer<'intrinsics> {
    fn new(roots: &[FableRootSpec], intrinsics: &'intrinsics FableIntrinsics) -> Self {
        let mut type_shapes = Vec::new();
        for root in roots {
            collect_reachable_shapes(root.shape, &mut type_shapes);
        }
        Self {
            roots: roots.into(),
            intrinsics,
            scopes: vec![BTreeMap::new()],
            locals: LocalAllocator::default(),
            type_shapes,
            declared_types: FableDeclaredTypes::default(),
            blocks: Vec::new(),
        }
    }

    fn into_blocks(self) -> Vec<FableProgram> {
        self.blocks
    }

    fn lower_root(&mut self, root: &ast::SourceFile) -> Result<FableProgram, FableError> {
        let statements = self.prepare_source(root)?;
        self.lower_statements(statements)
    }

    fn lower_predicate_root(&mut self, root: &ast::SourceFile) -> Result<FableProgram, FableError> {
        let statements = self.prepare_source(root)?;
        let Some((last, prefix)) = statements.split_last() else {
            return Err(FableError::MalformedSyntax {
                reason: "predicate source did not contain a final boolean expression",
            });
        };

        let mut program = Vec::with_capacity(statements.len());
        for stmt in prefix {
            program.push(self.lower_stmt(stmt)?);
        }

        let Stmt::Expr(expr_stmt) = *last else {
            return Err(FableError::Unsupported {
                feature: "predicate final statement must be a boolean expression".into(),
            });
        };
        let value = expect_bool_plan(self.lower_expr(&expr_stmt.expr)?)?;
        program.push(fable_op(FableIntrinsic::Predicate(value)));
        Ok(program)
    }

    fn lower_query_root(
        &mut self,
        root: &ast::SourceFile,
        query_type: FableQueryType,
    ) -> Result<FableProgram, FableError> {
        let statements = self.prepare_source(root)?;
        let Some((last, prefix)) = statements.split_last() else {
            return Err(FableError::MalformedSyntax {
                reason: "query source did not contain a final expression",
            });
        };

        let mut program = Vec::with_capacity(statements.len());
        for stmt in prefix {
            program.push(self.lower_stmt(stmt)?);
        }

        let Stmt::Expr(expr_stmt) = *last else {
            return Err(FableError::Unsupported {
                feature: "query final statement must be an expression".into(),
            });
        };
        let value = self.lower_expr(&expr_stmt.expr)?;
        validate_query_expr(query_type, &value)?;
        program.push(fable_op(FableIntrinsic::Query(value)));
        Ok(program)
    }

    fn lower_block(&mut self, block: &Block) -> Result<FableProgram, FableError> {
        self.scopes.push(BTreeMap::new());
        let result = self.lower_statements(block.stmts.iter());
        self.scopes.pop();
        result
    }

    fn prepare_source<'ast>(
        &mut self,
        root: &'ast ast::SourceFile,
    ) -> Result<Vec<&'ast Stmt>, FableError> {
        self.declared_types = FableDeclaredTypes::from_source(root)?;
        let mut statements = Vec::new();
        for item in &root.items {
            match item {
                Item::Stmt(stmt) => statements.push(stmt),
                Item::Struct(_) | Item::Enum(_) | Item::Fn(_) => {}
            }
        }
        Ok(statements)
    }

    fn lower_statements<'ast>(
        &mut self,
        statements: impl IntoIterator<Item = &'ast Stmt>,
    ) -> Result<FableProgram, FableError> {
        let mut program = Vec::new();
        for stmt in statements {
            program.push(self.lower_stmt(stmt)?);
        }
        Ok(program)
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<FableWeavyOp, FableError> {
        match stmt {
            Stmt::Assign(assign) => {
                let target = self.lower_writable_path(&assign.target)?;
                let value = self.lower_expr(&assign.value)?;
                validate_assignment(target.scalar, target.shape, &value)?;
                Ok(fable_op(FableIntrinsic::Assign { target, value }))
            }
            Stmt::Let(let_stmt) => {
                let name = name_text(&let_stmt.name).to_owned();
                let value = self.lower_expr(&let_stmt.value)?;
                let local = self.declare_local(name, &value)?;
                Ok(fable_op(FableIntrinsic::Let { local, value }))
            }
            Stmt::Expr(expr_stmt) => Ok(fable_op(FableIntrinsic::Eval(
                self.lower_expr(&expr_stmt.expr)?,
            ))),
            Stmt::If(if_stmt) => self.lower_if(if_stmt),
        }
    }

    fn lower_if(&mut self, if_stmt: &IfStmt) -> Result<FableWeavyOp, FableError> {
        let condition = expect_bool_plan(self.lower_expr(&if_stmt.condition)?)?;

        let else_block = if let Some(else_clause) = &if_stmt.else_clause {
            self.lower_else(else_clause)?
        } else {
            None
        };
        let then_program = self.lower_block(&if_stmt.then)?;
        let then_block = self.push_block(then_program);

        Ok(fable_op(FableIntrinsic::Branch {
            condition,
            then_block,
            else_block,
        }))
    }

    fn lower_else(&mut self, else_clause: &ElseClause) -> Result<Option<BlockRef>, FableError> {
        if let Some(if_stmt) = &else_clause.if_stmt {
            let program = vec![self.lower_if(if_stmt)?];
            Ok(Some(self.push_block(program)))
        } else if let Some(block) = &else_clause.block {
            let program = self.lower_block(block)?;
            Ok(Some(self.push_block(program)))
        } else {
            Err(FableError::MalformedSyntax {
                reason: "else clause without body",
            })
        }
    }

    fn push_block(&mut self, program: FableProgram) -> BlockRef {
        let block = BlockRef::new(self.blocks.len());
        self.blocks.push(program);
        block
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<ExprPlan, FableError> {
        match expr {
            Expr::Literal(literal) => self.lower_literal(literal),
            Expr::Var(var) => {
                let name = name_text(&var.name);
                if let Some(local) = self.find_local(name) {
                    return Ok(local_to_expr(local));
                }
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::Field(_) => {
                if let Some(expr) = self.lower_declared_field_expr(expr)? {
                    return Ok(expr);
                }
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::Paren(paren) => self.lower_expr(&paren.expr),
            Expr::Unary(unary) => self.lower_unary(unary),
            Expr::Binary(binary) => self.lower_binary(binary),
            Expr::Index(_) => {
                let path = self.lower_readable_path(expr)?;
                path_to_expr(path)
            }
            Expr::StructLiteral(literal) => self.lower_struct_literal(literal),
            Expr::EnumVariant(expr) => self.lower_enum_variant_expr(expr),
            Expr::Match(expr) => self.lower_match_expr(expr),
            Expr::Call(call) => self.lower_call(call),
        }
    }

    fn lower_declared_field_expr(&self, expr: &Expr) -> Result<Option<ExprPlan>, FableError> {
        let path = collect_path(expr)?;
        let Some(PathSegment::Name(root_name)) = path.first() else {
            return Ok(None);
        };
        let Some(local @ LocalRef::Declared { type_index, .. }) = self.find_local(root_name) else {
            return Ok(None);
        };

        let mut current_type_index = type_index;
        let mut offset = 0usize;
        let mut segments = path.iter().skip(1).peekable();
        while let Some(segment) = segments.next() {
            let PathSegment::Name(field_name) = segment else {
                return Err(FableError::Unsupported {
                    feature: "index access on declared values".into(),
                });
            };
            let field = self.declared_field(current_type_index, field_name)?;
            offset += field.offset;
            let is_last = segments.peek().is_none();
            match field.ty {
                DeclaredFieldType::Scalar(scalar) if is_last => {
                    return Ok(Some(declared_scalar_expr(DeclaredScalarRead {
                        local,
                        offset,
                        scalar,
                    })));
                }
                DeclaredFieldType::Scalar(scalar) => {
                    return Err(FableError::Unsupported {
                        feature: format!("field access through scalar {}", scalar.name()),
                    });
                }
                DeclaredFieldType::Declared(next_type_index) if is_last => {
                    return Ok(Some(ExprPlan::Value(ValueExpr::Declared(
                        DeclaredValueExpr::Field(DeclaredValueField {
                            local,
                            type_index: next_type_index,
                            offset,
                        }),
                    ))));
                }
                DeclaredFieldType::Declared(next_type_index) => {
                    current_type_index = next_type_index;
                }
            }
        }

        Ok(Some(ExprPlan::Value(ValueExpr::Declared(
            DeclaredValueExpr::Local(local),
        ))))
    }

    fn lower_struct_literal(&mut self, literal: &StructLiteral) -> Result<ExprPlan, FableError> {
        let type_name = literal.type_name.value.as_str();
        if self.declared_types.get(type_name).is_some() {
            return self.lower_declared_struct_literal(literal);
        }
        let shape = self.resolve_type_name(type_name)?;
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
        for field in &literal.fields {
            let name = name_text(&field.name).to_owned();
            if supplied.contains_key(&name) {
                return Err(FableError::DuplicateStructField { field: name });
            }
            supplied.insert(name, self.lower_expr(&field.value)?);
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

    fn lower_declared_struct_literal(
        &mut self,
        literal: &StructLiteral,
    ) -> Result<ExprPlan, FableError> {
        let type_name = literal.type_name.value.as_str();
        let type_index = self.declared_type_index(type_name)?;
        let expected = self.declared_struct_fields(type_index)?;

        let mut supplied = BTreeMap::new();
        for field in &literal.fields {
            let name = name_text(&field.name).to_owned();
            if supplied.insert(name.clone(), field).is_some() {
                return Err(FableError::DuplicateStructField { field: name });
            }
        }

        let mut fields = Vec::with_capacity(expected.len());
        for expected in expected {
            let Some(field) = supplied.remove(&expected.name) else {
                return Err(FableError::Unsupported {
                    feature: format!("{} literal is missing field {}", type_name, expected.name),
                });
            };
            let value = self.lower_expr(&field.value)?;
            validate_declared_assignment(expected.ty, &value, &self.declared_types)?;
            fields.push(DeclaredFieldInit {
                offset: expected.offset,
                ty: expected.ty,
                value,
            });
        }

        if let Some((name, _)) = supplied.into_iter().next() {
            return Err(FableError::Unsupported {
                feature: format!("{type_name} has no field named {name}"),
            });
        }

        Ok(ExprPlan::Value(ValueExpr::Declared(
            DeclaredValueExpr::Struct(DeclaredStructExpr {
                type_index,
                fields: fields.into_boxed_slice(),
            }),
        )))
    }

    fn lower_enum_variant_expr(
        &mut self,
        expr: &ast::EnumVariantExpr,
    ) -> Result<ExprPlan, FableError> {
        let type_name = expr.path.type_name.value.as_str();
        let variant_name = expr.path.variant_name.value.as_str();
        let type_index = self.declared_type_index(type_name)?;
        let expected = self.declared_variant_fields(type_index, variant_name)?;

        let supplied_fields: &[ast::StructField] = expr
            .fields
            .as_ref()
            .map(|fields| fields.fields.as_slice())
            .unwrap_or(&[]);
        let mut supplied = BTreeMap::new();
        for field in supplied_fields {
            let name = name_text(&field.name).to_owned();
            if supplied.insert(name.clone(), field).is_some() {
                return Err(FableError::DuplicateStructField { field: name });
            }
        }

        let mut fields = Vec::with_capacity(expected.fields.len());
        for expected_field in expected.fields {
            let Some(field) = supplied.remove(&expected_field.name) else {
                return Err(FableError::Unsupported {
                    feature: format!(
                        "{}::{} literal is missing field {}",
                        type_name, variant_name, expected_field.name
                    ),
                });
            };
            let value = self.lower_expr(&field.value)?;
            validate_declared_assignment(expected_field.ty, &value, &self.declared_types)?;
            fields.push(DeclaredFieldInit {
                offset: expected_field.offset,
                ty: expected_field.ty,
                value,
            });
        }

        if let Some((name, _)) = supplied.into_iter().next() {
            return Err(FableError::Unsupported {
                feature: format!("{type_name}::{variant_name} has no field named {name}"),
            });
        }

        Ok(ExprPlan::Value(ValueExpr::Declared(
            DeclaredValueExpr::Enum(DeclaredEnumExpr {
                type_index,
                selector: expected.selector,
                tag_offset: expected.tag_offset,
                tag_width: expected.tag_width,
                fields: fields.into_boxed_slice(),
            }),
        )))
    }

    fn declared_type_index(&self, name: &str) -> Result<usize, FableError> {
        self.declared_types
            .index_of(name)
            .ok_or_else(|| FableError::UnknownType {
                name: name.to_owned(),
            })
    }

    fn declared_type(&self, type_index: usize) -> Result<&FableDeclaredType, FableError> {
        self.declared_types
            .by_index(type_index)
            .ok_or(FableError::MalformedProgram {
                reason: "declared type index was missing",
            })
    }

    fn declared_struct_fields(
        &self,
        type_index: usize,
    ) -> Result<Vec<ExpectedDeclaredField>, FableError> {
        let declared = self.declared_type(type_index)?;
        let FableDeclaredTypeKind::Struct { fields } = declared.kind() else {
            return Err(FableError::Unsupported {
                feature: format!("struct literal for non-struct type {}", declared.name()),
            });
        };
        let WeavyAccess::Record(record) = &declared.descriptor().access else {
            return Err(FableError::MalformedProgram {
                reason: "declared struct did not have record descriptor",
            });
        };
        fields
            .iter()
            .zip(&record.fields)
            .map(|(field, access)| {
                Ok(ExpectedDeclaredField {
                    name: field.name.clone(),
                    offset: access.offset,
                    ty: self.declared_field_type(&field.type_name)?,
                })
            })
            .collect()
    }

    fn declared_variant_fields(
        &self,
        type_index: usize,
        variant_name: &str,
    ) -> Result<ExpectedDeclaredVariant, FableError> {
        let declared = self.declared_type(type_index)?;
        let FableDeclaredTypeKind::Enum { variants } = declared.kind() else {
            return Err(FableError::Unsupported {
                feature: format!("variant construction for non-enum type {}", declared.name()),
            });
        };
        let WeavyAccess::Enum(enum_access) = &declared.descriptor().access else {
            return Err(FableError::MalformedProgram {
                reason: "declared enum did not have enum descriptor",
            });
        };
        let variant_index = variants
            .iter()
            .position(|variant| variant.name == variant_name)
            .ok_or_else(|| FableError::Unsupported {
                feature: format!("{} has no variant named {variant_name}", declared.name()),
            })?;
        let variant = &variants[variant_index];
        let access = &enum_access.variants[variant_index];
        let WeavyTag::Direct {
            offset: tag_offset,
            width: tag_width,
        } = enum_access.tag
        else {
            return Err(FableError::Unsupported {
                feature: "non-direct declared enum tags".into(),
            });
        };
        let fields = variant
            .fields
            .iter()
            .zip(&access.payload.fields)
            .map(|(field, access)| {
                Ok(ExpectedDeclaredField {
                    name: field.name.clone(),
                    offset: access.offset,
                    ty: self.declared_field_type(&field.type_name)?,
                })
            })
            .collect::<Result<Vec<_>, FableError>>()?;
        Ok(ExpectedDeclaredVariant {
            selector: access.selector,
            tag_offset,
            tag_width,
            fields,
        })
    }

    fn declared_field(
        &self,
        type_index: usize,
        field_name: &str,
    ) -> Result<ExpectedDeclaredField, FableError> {
        self.declared_struct_fields(type_index)?
            .into_iter()
            .find(|field| field.name == field_name)
            .ok_or_else(|| {
                let type_name = self
                    .declared_type(type_index)
                    .map(|ty| ty.name().to_owned())
                    .unwrap_or_else(|_| "<missing>".to_owned());
                FableError::Unsupported {
                    feature: format!("{type_name} has no field named {field_name}"),
                }
            })
    }

    fn declared_field_type(&self, type_name: &str) -> Result<DeclaredFieldType, FableError> {
        if let Some(scalar) = DeclaredScalar::from_name(type_name) {
            return Ok(DeclaredFieldType::Scalar(scalar));
        }
        self.declared_type_index(type_name)
            .map(DeclaredFieldType::Declared)
    }

    fn lower_match_expr(&mut self, expr: &ast::MatchExpr) -> Result<ExprPlan, FableError> {
        let scrutinee = self.lower_expr(&expr.scrutinee)?;
        let ExprPlan::Value(ValueExpr::Declared(scrutinee)) = scrutinee else {
            return Err(FableError::TypeMismatch {
                expected: "declared enum".into(),
                actual: scrutinee.kind_name(),
            });
        };
        let enum_type_index = scrutinee.type_index();
        let (enum_name, variant_count) = {
            let declared = self.declared_type(enum_type_index)?;
            let FableDeclaredTypeKind::Enum { variants } = declared.kind() else {
                return Err(FableError::TypeMismatch {
                    expected: "declared enum".into(),
                    actual: "declared struct",
                });
            };
            (declared.name().to_owned(), variants.len())
        };
        let mut covered = vec![false; variant_count];
        let mut wildcard = false;
        let mut arms = Vec::with_capacity(expr.arms.len());
        let mut result_kind: Option<FableQueryType> = None;

        for (arm_index, arm) in expr.arms.iter().enumerate() {
            if wildcard {
                return Err(FableError::Unsupported {
                    feature: "match wildcard arm must be trailing".into(),
                });
            }
            let lowered = self.lower_match_arm(arm, enum_type_index)?;
            if let Some(selector) = lowered.selector {
                let variant_index =
                    usize::try_from(selector).map_err(|_| FableError::MalformedProgram {
                        reason: "variant selector did not fit usize",
                    })?;
                if let Some(slot) = covered.get_mut(variant_index) {
                    *slot = true;
                }
            } else {
                wildcard = true;
            }
            let arm_kind = expr_query_type(&lowered.result)?;
            if let Some(result_kind) = result_kind {
                if result_kind != arm_kind {
                    return Err(FableError::TypeMismatch {
                        expected: result_kind.name().to_owned(),
                        actual: arm_kind.name(),
                    });
                }
            } else {
                result_kind = Some(arm_kind);
            }
            if arm_index + 1 == expr.arms.len() && lowered.selector.is_none() {
                wildcard = true;
            }
            arms.push(lowered);
        }

        if arms.is_empty() {
            return Err(FableError::Unsupported {
                feature: "match without arms".into(),
            });
        }
        if !wildcard && covered.iter().any(|covered| !covered) {
            return Err(FableError::Unsupported {
                feature: format!("non-exhaustive match on {enum_name}"),
            });
        }

        Ok(ExprPlan::Match(Box::new(MatchExprPlan {
            scrutinee,
            enum_type_index,
            result_kind: result_kind.expect("non-empty match has result kind"),
            arms: arms.into_boxed_slice(),
        })))
    }

    fn lower_match_arm(
        &mut self,
        arm: &ast::MatchArm,
        enum_type_index: usize,
    ) -> Result<MatchArmPlan, FableError> {
        self.scopes.push(BTreeMap::new());
        let (selector, bindings) = match &arm.pattern {
            ast::MatchPattern::Wildcard(_) => (None, Vec::new()),
            ast::MatchPattern::Variant(pattern) => {
                self.lower_variant_pattern(pattern, enum_type_index)?
            }
        };
        let result = self.lower_block_result(&arm.body)?;
        self.scopes.pop();
        Ok(MatchArmPlan {
            selector,
            bindings: bindings.into_boxed_slice(),
            prefix: result.prefix,
            result: result.result,
        })
    }

    fn lower_variant_pattern(
        &mut self,
        pattern: &ast::VariantPattern,
        enum_type_index: usize,
    ) -> Result<(Option<u64>, Vec<MatchBindingPlan>), FableError> {
        let enum_type = self.declared_type(enum_type_index)?;
        let pattern_type = pattern.path.type_name.value.as_str();
        if pattern_type != enum_type.name() {
            return Err(FableError::TypeMismatch {
                expected: enum_type.name().to_owned(),
                actual: "different declared enum",
            });
        }
        let variant_name = pattern.path.variant_name.value.as_str();
        let expected = self.declared_variant_fields(enum_type_index, variant_name)?;
        let supplied_fields: &[ast::PatternField] = pattern
            .fields
            .as_ref()
            .map(|fields| fields.fields.as_slice())
            .unwrap_or(&[]);
        let mut supplied = BTreeMap::new();
        for field in supplied_fields {
            let name = name_text(&field.name).to_owned();
            if supplied.insert(name.clone(), field).is_some() {
                return Err(FableError::DuplicateStructField { field: name });
            }
        }

        let mut bindings = Vec::with_capacity(expected.fields.len());
        for field in expected.fields {
            if supplied.remove(&field.name).is_none() {
                return Err(FableError::Unsupported {
                    feature: format!("{variant_name} pattern is missing field {}", field.name),
                });
            }
            let local = match field.ty {
                DeclaredFieldType::Scalar(scalar) => self.locals.allocate_declared_scalar(scalar),
                DeclaredFieldType::Declared(type_index) => {
                    self.locals.allocate_declared_value(type_index)
                }
            };
            self.insert_local(field.name.clone(), local)?;
            let binding = match field.ty {
                DeclaredFieldType::Scalar(scalar) => MatchBindingPlan::Scalar {
                    local,
                    offset: field.offset,
                    scalar,
                },
                DeclaredFieldType::Declared(type_index) => MatchBindingPlan::Declared {
                    local,
                    offset: field.offset,
                    type_index,
                },
            };
            bindings.push(binding);
        }
        if let Some((name, _)) = supplied.into_iter().next() {
            return Err(FableError::Unsupported {
                feature: format!("{variant_name} has no field named {name}"),
            });
        }
        Ok((Some(expected.selector), bindings))
    }

    fn lower_block_result(&mut self, block: &Block) -> Result<LoweredBlockResult, FableError> {
        let Some((last, prefix)) = block.stmts.split_last() else {
            return Err(FableError::Unsupported {
                feature: "match arm without result expression".into(),
            });
        };
        let mut program = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            program.push(self.lower_stmt(stmt)?);
        }
        let Stmt::Expr(expr) = last else {
            return Err(FableError::Unsupported {
                feature: "match arm must end with an expression".into(),
            });
        };
        Ok(LoweredBlockResult {
            prefix: program,
            result: self.lower_expr(&expr.expr)?,
        })
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
        let expr = match literal {
            Literal::True(_) => ExprPlan::Bool(BoolExpr::Literal(true)),
            Literal::False(_) => ExprPlan::Bool(BoolExpr::Literal(false)),
            Literal::Null(_) => ExprPlan::Unit(UnitExpr::Null),
            Literal::Int(text) => ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::Literal(
                text.value.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.value.clone(),
                    reason: "integer literal is out of range",
                })?,
            ))),
            Literal::Float(text) => ExprPlan::Number(NumberExpr::Float(FloatExpr::Literal(
                text.value.parse().map_err(|_| FableError::InvalidLiteral {
                    literal: text.value.clone(),
                    reason: "float literal is invalid",
                })?,
            ))),
            Literal::Str(text) => ExprPlan::String(StringExpr::Literal(text.value.clone())),
        };
        Ok(expr)
    }

    fn lower_unary(&mut self, unary: &UnaryExpr) -> Result<ExprPlan, FableError> {
        let operand = self.lower_expr(&unary.operand)?;
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
        let lhs = self.lower_expr(&binary.lhs)?;
        let rhs = self.lower_expr(&binary.rhs)?;

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
        let name = call_callee_name(&call.callee)?;
        let signature =
            self.intrinsics
                .signature(&name)
                .ok_or_else(|| FableError::Unsupported {
                    feature: format!("intrinsic {name}"),
                })?;

        let mut raw_args = Vec::new();
        for arg in &call.args.args {
            raw_args.push(&arg.expr);
        }
        signature.validate_arity(raw_args.len())?;

        let mut args = Vec::with_capacity(raw_args.len());
        for expr in raw_args {
            args.push(match signature.arg_kind {
                IntrinsicArgKind::Expr => IntrinsicArgPlan::Expr(self.lower_expr(expr)?),
                IntrinsicArgKind::FieldRead => {
                    IntrinsicArgPlan::Field(self.lower_readable_path(expr)?)
                }
                IntrinsicArgKind::FieldMut => {
                    IntrinsicArgPlan::Field(self.lower_writable_path(expr)?)
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
            && self.find_local(name_text(&var.name)).is_some()
        {
            return Err(FableError::Unsupported {
                feature: "assignment to let bindings".into(),
            });
        }
        let path = self.resolve_path(expr)?;
        if self.roots[path.root].access != FableRootAccess::ReadWrite {
            return Err(FableError::ReadOnlyRoot {
                name: self.roots[path.root].name.to_owned(),
            });
        }
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
        let Some(root) = self.roots.iter().position(|root| root.name == first) else {
            return Err(FableError::ExpectedRoot {
                found: first.clone(),
            });
        };

        let mut shape = self.roots[root].shape;
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
            root,
            source: source.into_boxed_str(),
            shape,
            scalar,
            steps: steps.into_boxed_slice(),
        })
    }

    fn declare_local(&mut self, name: String, expr: &ExprPlan) -> Result<LocalRef, FableError> {
        if matches!(expr, ExprPlan::Match(_)) {
            return Err(FableError::Unsupported {
                feature: "binding match results".into(),
            });
        }
        let local = self.locals.allocate(expr);
        self.insert_local(name, local)?;
        Ok(local)
    }

    fn insert_local(&mut self, name: String, local: LocalRef) -> Result<(), FableError> {
        if self.roots.iter().any(|root| root.name == name) {
            return Err(FableError::ReservedLocalName { name });
        }
        let scope = self.scopes.last_mut().ok_or(FableError::MalformedProgram {
            reason: "local scope stack was empty",
        })?;
        if scope.contains_key(&name) {
            return Err(FableError::DuplicateLocal { name });
        }
        scope.insert(name, local);
        Ok(())
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
    Ok(name_text(&var.name).to_owned())
}

fn validate_query_expr(query_type: FableQueryType, expr: &ExprPlan) -> Result<(), FableError> {
    let ok = match query_type {
        FableQueryType::Unit => matches!(expr, ExprPlan::Unit(_) | ExprPlan::Match(_)),
        FableQueryType::Bool => matches!(expr, ExprPlan::Bool(_) | ExprPlan::Match(_)),
        FableQueryType::Char => matches!(expr, ExprPlan::Char(_) | ExprPlan::Match(_)),
        FableQueryType::String => matches!(expr, ExprPlan::String(_) | ExprPlan::Match(_)),
        FableQueryType::Signed | FableQueryType::Unsigned | FableQueryType::Float => {
            matches!(expr, ExprPlan::Number(_) | ExprPlan::Match(_))
        }
    } && expr_query_type(expr)? == query_type;
    if ok {
        Ok(())
    } else {
        Err(FableError::TypeMismatch {
            expected: query_type.name().into(),
            actual: expr.kind_name(),
        })
    }
}

fn expr_query_type(expr: &ExprPlan) -> Result<FableQueryType, FableError> {
    match expr {
        ExprPlan::Unit(_) => Ok(FableQueryType::Unit),
        ExprPlan::Bool(_) => Ok(FableQueryType::Bool),
        ExprPlan::Char(_) => Ok(FableQueryType::Char),
        ExprPlan::String(_) => Ok(FableQueryType::String),
        ExprPlan::Number(NumberExpr::Signed(_)) => Ok(FableQueryType::Signed),
        ExprPlan::Number(NumberExpr::Unsigned(_)) => Ok(FableQueryType::Unsigned),
        ExprPlan::Number(NumberExpr::Float(_)) => Ok(FableQueryType::Float),
        ExprPlan::Match(expr) => Ok(expr.result_kind),
        ExprPlan::Value(_) => Err(FableError::Unsupported {
            feature: "typed value query results".into(),
        }),
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
        LocalRef::Declared { .. } => {
            ExprPlan::Value(ValueExpr::Declared(DeclaredValueExpr::Local(local)))
        }
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

fn declared_scalar_expr(read: DeclaredScalarRead) -> ExprPlan {
    match read.scalar {
        DeclaredScalar::Unit => ExprPlan::Unit(UnitExpr::DeclaredRead(read)),
        DeclaredScalar::Bool => ExprPlan::Bool(BoolExpr::DeclaredRead(read)),
        DeclaredScalar::Char => ExprPlan::Char(CharExpr::DeclaredRead(read)),
        DeclaredScalar::String => ExprPlan::String(StringExpr::DeclaredRead(read)),
        DeclaredScalar::I8
        | DeclaredScalar::I16
        | DeclaredScalar::I32
        | DeclaredScalar::I64
        | DeclaredScalar::I128
        | DeclaredScalar::ISize => {
            ExprPlan::Number(NumberExpr::Signed(IntExpr::DeclaredRead(read)))
        }
        DeclaredScalar::U8
        | DeclaredScalar::U16
        | DeclaredScalar::U32
        | DeclaredScalar::U64
        | DeclaredScalar::U128
        | DeclaredScalar::USize => {
            ExprPlan::Number(NumberExpr::Unsigned(UIntExpr::DeclaredRead(read)))
        }
        DeclaredScalar::F32 | DeclaredScalar::F64 => {
            ExprPlan::Number(NumberExpr::Float(FloatExpr::DeclaredRead(read)))
        }
    }
}

fn validate_declared_assignment(
    expected: DeclaredFieldType,
    expr: &ExprPlan,
    declared_types: &FableDeclaredTypes,
) -> Result<(), FableError> {
    match expected {
        DeclaredFieldType::Scalar(scalar) => validate_declared_scalar_assignment(scalar, expr),
        DeclaredFieldType::Declared(type_index) => {
            let actual = match expr {
                ExprPlan::Value(ValueExpr::Declared(value)) => Some(value.type_index()),
                _ => None,
            };
            if actual == Some(type_index) {
                Ok(())
            } else {
                let expected = declared_types
                    .by_index(type_index)
                    .map(FableDeclaredType::name)
                    .unwrap_or("<missing declared type>");
                Err(FableError::TypeMismatch {
                    expected: expected.to_owned(),
                    actual: expr.kind_name(),
                })
            }
        }
    }
}

fn validate_declared_scalar_assignment(
    scalar: DeclaredScalar,
    expr: &ExprPlan,
) -> Result<(), FableError> {
    let ok = match scalar {
        DeclaredScalar::Unit => matches!(expr, ExprPlan::Unit(_)),
        DeclaredScalar::Bool => matches!(expr, ExprPlan::Bool(_)),
        DeclaredScalar::Char => matches!(expr, ExprPlan::Char(_) | ExprPlan::String(_)),
        DeclaredScalar::String => matches!(expr, ExprPlan::String(_) | ExprPlan::Char(_)),
        DeclaredScalar::I8
        | DeclaredScalar::I16
        | DeclaredScalar::I32
        | DeclaredScalar::I64
        | DeclaredScalar::I128
        | DeclaredScalar::ISize
        | DeclaredScalar::U8
        | DeclaredScalar::U16
        | DeclaredScalar::U32
        | DeclaredScalar::U64
        | DeclaredScalar::U128
        | DeclaredScalar::USize
        | DeclaredScalar::F32
        | DeclaredScalar::F64 => matches!(expr, ExprPlan::Number(_)),
    };
    if ok {
        Ok(())
    } else {
        Err(FableError::TypeMismatch {
            expected: scalar.name().to_owned(),
            actual: expr.kind_name(),
        })
    }
}

trait DeclaredIntBytes {
    fn write_ne_bytes(self, dst: *mut u8);
}

macro_rules! impl_declared_int_bytes {
    ($($ty:ty),* $(,)?) => {
        $(
            impl DeclaredIntBytes for $ty {
                fn write_ne_bytes(self, dst: *mut u8) {
                    let bytes = self.to_ne_bytes();
                    unsafe { copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
                }
            }
        )*
    };
}

impl_declared_int_bytes!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize,
);

fn declared_scalar_number_target(scalar: DeclaredScalar) -> ScalarType {
    match scalar {
        DeclaredScalar::I8 => ScalarType::I8,
        DeclaredScalar::I16 => ScalarType::I16,
        DeclaredScalar::I32 => ScalarType::I32,
        DeclaredScalar::I64 => ScalarType::I64,
        DeclaredScalar::I128 => ScalarType::I128,
        DeclaredScalar::ISize => ScalarType::ISize,
        DeclaredScalar::U8 => ScalarType::U8,
        DeclaredScalar::U16 => ScalarType::U16,
        DeclaredScalar::U32 => ScalarType::U32,
        DeclaredScalar::U64 => ScalarType::U64,
        DeclaredScalar::U128 => ScalarType::U128,
        DeclaredScalar::USize => ScalarType::USize,
        DeclaredScalar::F32 => ScalarType::F32,
        DeclaredScalar::F64 => ScalarType::F64,
        DeclaredScalar::Unit
        | DeclaredScalar::Bool
        | DeclaredScalar::Char
        | DeclaredScalar::String => ScalarType::Unit,
    }
}

fn read_signed_scalar_from_ptr(scalar: DeclaredScalar, ptr: *const u8) -> Result<i128, FableError> {
    Ok(match scalar {
        DeclaredScalar::I8 => i8::from_ne_bytes(read_ptr_array(ptr)) as i128,
        DeclaredScalar::I16 => i16::from_ne_bytes(read_ptr_array(ptr)) as i128,
        DeclaredScalar::I32 => i32::from_ne_bytes(read_ptr_array(ptr)) as i128,
        DeclaredScalar::I64 => i64::from_ne_bytes(read_ptr_array(ptr)) as i128,
        DeclaredScalar::I128 => i128::from_ne_bytes(read_ptr_array(ptr)),
        DeclaredScalar::ISize => isize::from_ne_bytes(read_ptr_array(ptr)) as i128,
        _ => {
            return Err(FableError::MalformedProgram {
                reason: "declared signed binding used non-signed scalar",
            });
        }
    })
}

fn read_unsigned_scalar_from_ptr(
    scalar: DeclaredScalar,
    ptr: *const u8,
) -> Result<u128, FableError> {
    Ok(match scalar {
        DeclaredScalar::U8 => u8::from_ne_bytes(read_ptr_array(ptr)) as u128,
        DeclaredScalar::U16 => u16::from_ne_bytes(read_ptr_array(ptr)) as u128,
        DeclaredScalar::U32 => u32::from_ne_bytes(read_ptr_array(ptr)) as u128,
        DeclaredScalar::U64 => u64::from_ne_bytes(read_ptr_array(ptr)) as u128,
        DeclaredScalar::U128 => u128::from_ne_bytes(read_ptr_array(ptr)),
        DeclaredScalar::USize => usize::from_ne_bytes(read_ptr_array(ptr)) as u128,
        _ => {
            return Err(FableError::MalformedProgram {
                reason: "declared unsigned binding used non-unsigned scalar",
            });
        }
    })
}

fn read_float_scalar_from_ptr(scalar: DeclaredScalar, ptr: *const u8) -> Result<f64, FableError> {
    Ok(match scalar {
        DeclaredScalar::F32 => f32::from_ne_bytes(read_ptr_array(ptr)) as f64,
        DeclaredScalar::F64 => f64::from_ne_bytes(read_ptr_array(ptr)),
        _ => {
            return Err(FableError::MalformedProgram {
                reason: "declared float binding used non-float scalar",
            });
        }
    })
}

fn read_ptr_array<const N: usize>(ptr: *const u8) -> [u8; N] {
    let mut out = [0; N];
    unsafe { copy_nonoverlapping(ptr, out.as_mut_ptr(), N) };
    out
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
    declared: Vec<Option<DeclaredOwnedValue>>,
}

struct OwnedValue {
    shape: &'static Shape,
    ptr: PtrMut,
}

struct DeclaredOwnedValue {
    type_index: usize,
    scratch: RawScratch,
    size: usize,
}

impl DeclaredOwnedValue {
    fn zeroed(type_index: usize, descriptor: &FableDeclaredDescriptor) -> Result<Self, FableError> {
        let scratch =
            RawScratch::new(descriptor.layout.size, descriptor.layout.align).map_err(|_| {
                FableError::Unsupported {
                    feature: format!(
                        "allocating declared value layout size={} align={}",
                        descriptor.layout.size, descriptor.layout.align
                    ),
                }
            })?;
        if descriptor.layout.size != 0 {
            unsafe { std::ptr::write_bytes(scratch.ptr(), 0, descriptor.layout.size) };
        }
        Ok(Self {
            type_index,
            scratch,
            size: descriptor.layout.size,
        })
    }

    fn copy_from(
        type_index: usize,
        descriptor: &FableDeclaredDescriptor,
        src: *const u8,
    ) -> Result<Self, FableError> {
        let value = Self::zeroed(type_index, descriptor)?;
        if descriptor.layout.size != 0 {
            unsafe { copy_nonoverlapping(src, value.scratch.ptr(), descriptor.layout.size) };
        }
        Ok(value)
    }

    fn ptr(&self) -> *mut u8 {
        self.scratch.ptr()
    }
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
    roots: Vec<RuntimeRoot>,
    locals: LocalSlots,
    declared_types: FableDeclaredTypes,
    predicate_result: Option<bool>,
    query_result: Option<FableQueryOutput>,
}

impl<'program> Step<'program, BlockRef, FableWeavyOp> for FableInterp {
    type Error = FableError;
    type Continuation = ();

    fn step(
        &mut self,
        op: &'program FableWeavyOp,
    ) -> Result<Control<'program, BlockRef, FableWeavyOp>, Self::Error> {
        match op {
            WeavyOp::Control(ControlOp::CallBlock { block, base_offset }) => {
                if *base_offset != 0 {
                    return Err(FableError::MalformedProgram {
                        reason: "Fable canonical block calls must use base offset 0",
                    });
                }
                Ok(Control::CallBlock(*block))
            }
            WeavyOp::Control(ControlOp::Return) => Ok(Control::Return),
            WeavyOp::Memory(_) | WeavyOp::Init(_) | WeavyOp::Aggregate(_) => {
                Err(FableError::MalformedProgram {
                    reason: "Fable cannot execute canonical typed-memory ops yet",
                })
            }
            WeavyOp::Intrinsic(intrinsic) => self.step_intrinsic(intrinsic),
            _ => Err(FableError::MalformedProgram {
                reason: "Fable cannot execute this canonical Weavy op",
            }),
        }
    }
}

impl FableInterp {
    fn step_intrinsic<'program>(
        &mut self,
        intrinsic: &'program FableIntrinsic,
    ) -> Result<Control<'program, BlockRef, FableWeavyOp>, FableError> {
        match intrinsic {
            FableIntrinsic::Let { local, value } => {
                self.init_local(*local, value)?;
                Ok(Control::Continue)
            }
            FableIntrinsic::Assign { target, value } => {
                let ptr = self.path_ptr_mut(target)?;
                if let Some(scalar) = target.scalar {
                    unsafe { self.write_scalar(scalar, ptr, value) }?;
                } else {
                    unsafe { self.write_value(target.shape, ptr, value) }?;
                }
                Ok(Control::Continue)
            }
            FableIntrinsic::Eval(expr) => {
                self.eval_expr(expr)?;
                Ok(Control::Continue)
            }
            FableIntrinsic::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.eval_bool(condition)?;
                if condition {
                    Ok(Control::CallBlock(*then_block))
                } else if let Some(block) = else_block {
                    Ok(Control::CallBlock(*block))
                } else {
                    Ok(Control::Continue)
                }
            }
            FableIntrinsic::Predicate(value) => {
                self.predicate_result = Some(self.eval_bool(value)?);
                Ok(Control::Continue)
            }
            FableIntrinsic::Query(value) => {
                self.query_result = Some(self.eval_query(value)?);
                Ok(Control::Continue)
            }
        }
    }
}

impl FableInterp {
    fn path_ptr_const(&self, path: &FieldPath) -> Result<PtrConst, FableError> {
        let root = self
            .roots
            .get(path.root)
            .ok_or(FableError::MalformedProgram {
                reason: "path referenced missing runtime root",
            })?;
        path.ptr_const(root.ptr.as_const())
    }

    fn path_ptr_mut(&self, path: &FieldPath) -> Result<PtrMut, FableError> {
        let root = self
            .roots
            .get(path.root)
            .ok_or(FableError::MalformedProgram {
                reason: "path referenced missing runtime root",
            })?;
        let Some(ptr) = root.ptr.as_mut() else {
            return Err(FableError::ReadOnlyRoot {
                name: root.name.to_owned(),
            });
        };
        path.ptr_mut(ptr)
    }

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
            LocalRef::Declared { index, type_index } => {
                let value =
                    self.eval_declared_value(type_index, expect_declared_value_expr(expr)?)?;
                set_slot(&mut self.locals.declared, index, Some(value));
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
            ExprPlan::Match(expr) => self.eval_match_for_effect(expr),
        }
    }

    fn eval_query(&mut self, expr: &ExprPlan) -> Result<FableQueryOutput, FableError> {
        match expr {
            ExprPlan::Unit(expr) => {
                self.eval_unit(expr)?;
                Ok(FableQueryOutput::Unit)
            }
            ExprPlan::Bool(expr) => Ok(FableQueryOutput::Bool(self.eval_bool(expr)?)),
            ExprPlan::Char(expr) => Ok(FableQueryOutput::Char(self.eval_char(expr)?)),
            ExprPlan::String(expr) => Ok(FableQueryOutput::String(self.eval_string(expr)?)),
            ExprPlan::Number(NumberExpr::Signed(expr)) => {
                Ok(FableQueryOutput::Signed(self.eval_i128(expr)?))
            }
            ExprPlan::Number(NumberExpr::Unsigned(expr)) => {
                Ok(FableQueryOutput::Unsigned(self.eval_u128(expr)?))
            }
            ExprPlan::Number(NumberExpr::Float(expr)) => {
                Ok(FableQueryOutput::Float(self.eval_f64(expr)?))
            }
            ExprPlan::Match(expr) => self.eval_match_query(expr),
            ExprPlan::Value(_) => Err(FableError::Unsupported {
                feature: "querying typed values".into(),
            }),
        }
    }

    fn eval_match_for_effect(&mut self, expr: &MatchExprPlan) -> Result<(), FableError> {
        let scrutinee = self.eval_declared_value(expr.enum_type_index, &expr.scrutinee)?;
        let selector = self.read_declared_tag(&scrutinee, expr.enum_type_index)?;
        let arm = expr
            .arms
            .iter()
            .find(|arm| arm.selector == Some(selector))
            .or_else(|| expr.arms.iter().find(|arm| arm.selector.is_none()))
            .ok_or(FableError::MalformedProgram {
                reason: "match had no selected arm",
            })?;
        self.init_match_bindings(&arm.bindings, &scrutinee)?;
        self.eval_arm_prefix(&arm.prefix)?;
        self.eval_expr(&arm.result)
    }

    fn eval_match_query(&mut self, expr: &MatchExprPlan) -> Result<FableQueryOutput, FableError> {
        let scrutinee = self.eval_declared_value(expr.enum_type_index, &expr.scrutinee)?;
        let selector = self.read_declared_tag(&scrutinee, expr.enum_type_index)?;
        let arm = expr
            .arms
            .iter()
            .find(|arm| arm.selector == Some(selector))
            .or_else(|| expr.arms.iter().find(|arm| arm.selector.is_none()))
            .ok_or(FableError::MalformedProgram {
                reason: "match had no selected arm",
            })?;
        self.init_match_bindings(&arm.bindings, &scrutinee)?;
        self.eval_arm_prefix(&arm.prefix)?;
        self.eval_query(&arm.result)
    }

    fn read_declared_tag(
        &self,
        value: &DeclaredOwnedValue,
        type_index: usize,
    ) -> Result<u64, FableError> {
        let descriptor = self.declared_descriptor(type_index)?;
        let WeavyAccess::Enum(access) = &descriptor.access else {
            return Err(FableError::MalformedProgram {
                reason: "match scrutinee descriptor was not an enum",
            });
        };
        let WeavyTag::Direct { offset, width } = access.tag else {
            return Err(FableError::Unsupported {
                feature: "non-direct declared enum tags".into(),
            });
        };
        let bytes = unsafe { std::slice::from_raw_parts(value.ptr().add(offset), width) };
        Ok(match width {
            0 => 0,
            1 => bytes[0].into(),
            2 => u16::from_ne_bytes(bytes.try_into().expect("tag width checked")).into(),
            4 => u32::from_ne_bytes(bytes.try_into().expect("tag width checked")).into(),
            8 => u64::from_ne_bytes(bytes.try_into().expect("tag width checked")),
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "declared enum tag width was invalid",
                });
            }
        })
    }

    fn init_match_bindings(
        &mut self,
        bindings: &[MatchBindingPlan],
        scrutinee: &DeclaredOwnedValue,
    ) -> Result<(), FableError> {
        for binding in bindings {
            match *binding {
                MatchBindingPlan::Scalar {
                    local,
                    offset,
                    scalar,
                } => {
                    self.init_scalar_binding(local, scalar, unsafe { scrutinee.ptr().add(offset) })?
                }
                MatchBindingPlan::Declared {
                    local,
                    offset,
                    type_index,
                } => {
                    let LocalRef::Declared { index, .. } = local else {
                        return Err(local_kind_mismatch("declared value", local));
                    };
                    let descriptor = self.declared_descriptor(type_index)?;
                    let value = DeclaredOwnedValue::copy_from(type_index, descriptor, unsafe {
                        scrutinee.ptr().add(offset).cast_const()
                    })?;
                    set_slot(&mut self.locals.declared, index, Some(value));
                }
            }
        }
        Ok(())
    }

    fn init_scalar_binding(
        &mut self,
        local: LocalRef,
        scalar: DeclaredScalar,
        ptr: *const u8,
    ) -> Result<(), FableError> {
        match (local, scalar) {
            (LocalRef::Unit(index), DeclaredScalar::Unit) => {
                set_slot(&mut self.locals.units, index, true);
            }
            (LocalRef::Bool(index), DeclaredScalar::Bool) => {
                set_slot(&mut self.locals.bools, index, Some(unsafe { *ptr != 0 }));
            }
            (LocalRef::Char(index), DeclaredScalar::Char) => {
                let bytes = unsafe { std::slice::from_raw_parts(ptr, 4) };
                let value = u32::from_ne_bytes(bytes.try_into().expect("char width checked"));
                let value = char::from_u32(value).ok_or(FableError::MalformedProgram {
                    reason: "declared char bytes were not a valid scalar value",
                })?;
                set_slot(&mut self.locals.chars, index, Some(value));
            }
            (LocalRef::Signed(index), _) => {
                let value = read_signed_scalar_from_ptr(scalar, ptr)?;
                set_slot(&mut self.locals.signed, index, Some(value));
            }
            (LocalRef::Unsigned(index), _) => {
                let value = read_unsigned_scalar_from_ptr(scalar, ptr)?;
                set_slot(&mut self.locals.unsigned, index, Some(value));
            }
            (LocalRef::Float(index), _) => {
                let value = read_float_scalar_from_ptr(scalar, ptr)?;
                set_slot(&mut self.locals.floats, index, Some(value));
            }
            (LocalRef::String(_), DeclaredScalar::String) => {
                return Err(FableError::Unsupported {
                    feature: "declared string field runtime".into(),
                });
            }
            _ => return Err(local_kind_mismatch(scalar.name(), local)),
        }
        Ok(())
    }

    fn eval_arm_prefix(&mut self, prefix: &[FableWeavyOp]) -> Result<(), FableError> {
        for op in prefix {
            let WeavyOp::Intrinsic(intrinsic) = op else {
                return Err(FableError::Unsupported {
                    feature: "non-intrinsic match arm prefix".into(),
                });
            };
            match intrinsic {
                FableIntrinsic::Branch { .. } => {
                    return Err(FableError::Unsupported {
                        feature: "branching inside match arm prefixes".into(),
                    });
                }
                _ => {
                    self.step_intrinsic(intrinsic)?;
                }
            }
        }
        Ok(())
    }

    fn eval_unit(&self, expr: &UnitExpr) -> Result<(), FableError> {
        match expr {
            UnitExpr::Null => Ok(()),
            UnitExpr::Read(path) => {
                let _ = self.path_ptr_const(path)?;
                Ok(())
            }
            UnitExpr::Local(local) => self.local_unit(*local),
            UnitExpr::DeclaredRead(read) => self.read_declared_unit(*read),
            UnitExpr::HostFieldMut { function, field } => function(self.field_mut(field)?),
        }
    }

    fn eval_bool(&self, expr: &BoolExpr) -> Result<bool, FableError> {
        match expr {
            BoolExpr::Literal(value) => Ok(*value),
            BoolExpr::Read(path) => {
                let ptr = self.path_ptr_const(path)?;
                Ok(*unsafe { ptr.get::<bool>() })
            }
            BoolExpr::Local(local) => self.local_bool(*local),
            BoolExpr::DeclaredRead(read) => self.read_declared_bool(*read),
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
                let ptr = self.path_ptr_const(path)?;
                Ok(*unsafe { ptr.get::<char>() })
            }
            CharExpr::Local(local) => self.local_char(*local),
            CharExpr::DeclaredRead(read) => self.read_declared_char(*read),
        }
    }

    fn eval_string(&self, expr: &StringExpr) -> Result<String, FableError> {
        match expr {
            StringExpr::Literal(value) => Ok(value.clone()),
            StringExpr::Read(path) => {
                let ptr = self.path_ptr_const(path)?;
                unsafe { self.read_string_path(path, ptr) }
            }
            StringExpr::Local(local) => self.local_string(*local),
            StringExpr::DeclaredRead(read) => self.read_declared_string(*read),
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
            ptr: self.path_ptr_const(field)?,
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
            ptr: self.path_ptr_mut(field)?,
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
                let ptr = self.path_ptr_const(path)?;
                unsafe { self.read_signed_path(field_scalar(path)?, ptr) }
            }
            IntExpr::Local(local) => self.local_i128(*local),
            IntExpr::DeclaredRead(read) => self.read_declared_i128(*read),
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
                let ptr = self.path_ptr_const(path)?;
                unsafe { self.read_unsigned_path(field_scalar(path)?, ptr) }
            }
            UIntExpr::Local(local) => self.local_u128(*local),
            UIntExpr::DeclaredRead(read) => self.read_declared_u128(*read),
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
                let ptr = self.path_ptr_const(path)?;
                match field_scalar(path)? {
                    ScalarType::F32 => Ok((*unsafe { ptr.get::<f32>() }).into()),
                    ScalarType::F64 => Ok(*unsafe { ptr.get::<f64>() }),
                    _ => Err(FableError::MalformedProgram {
                        reason: "float read path did not point to a float scalar",
                    }),
                }
            }
            FloatExpr::Local(local) => self.local_f64(*local),
            FloatExpr::DeclaredRead(read) => self.read_declared_f64(*read),
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

    fn declared_local(&self, local: LocalRef) -> Result<&DeclaredOwnedValue, FableError> {
        let LocalRef::Declared { index, .. } = local else {
            return Err(local_kind_mismatch("declared value", local));
        };
        self.locals
            .declared
            .get(index)
            .and_then(Option::as_ref)
            .ok_or_else(uninitialized_local)
    }

    fn read_declared_unit(&self, read: DeclaredScalarRead) -> Result<(), FableError> {
        let _ = self.declared_local(read.local)?;
        Ok(())
    }

    fn read_declared_bool(&self, read: DeclaredScalarRead) -> Result<bool, FableError> {
        Ok(self.read_declared_bytes(read, 1)?[0] != 0)
    }

    fn read_declared_char(&self, read: DeclaredScalarRead) -> Result<char, FableError> {
        let value = u32::from_ne_bytes(self.read_declared_array(read)?);
        char::from_u32(value).ok_or(FableError::MalformedProgram {
            reason: "declared char bytes were not a valid scalar value",
        })
    }

    fn read_declared_string(&self, _read: DeclaredScalarRead) -> Result<String, FableError> {
        Err(FableError::Unsupported {
            feature: "declared string field runtime".into(),
        })
    }

    fn read_declared_i128(&self, read: DeclaredScalarRead) -> Result<i128, FableError> {
        Ok(match read.scalar {
            DeclaredScalar::I8 => i8::from_ne_bytes(self.read_declared_array(read)?) as i128,
            DeclaredScalar::I16 => i16::from_ne_bytes(self.read_declared_array(read)?) as i128,
            DeclaredScalar::I32 => i32::from_ne_bytes(self.read_declared_array(read)?) as i128,
            DeclaredScalar::I64 => i64::from_ne_bytes(self.read_declared_array(read)?) as i128,
            DeclaredScalar::I128 => i128::from_ne_bytes(self.read_declared_array(read)?),
            DeclaredScalar::ISize => isize::from_ne_bytes(self.read_declared_array(read)?) as i128,
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "declared signed read used non-signed scalar",
                });
            }
        })
    }

    fn read_declared_u128(&self, read: DeclaredScalarRead) -> Result<u128, FableError> {
        Ok(match read.scalar {
            DeclaredScalar::U8 => u8::from_ne_bytes(self.read_declared_array(read)?) as u128,
            DeclaredScalar::U16 => u16::from_ne_bytes(self.read_declared_array(read)?) as u128,
            DeclaredScalar::U32 => u32::from_ne_bytes(self.read_declared_array(read)?) as u128,
            DeclaredScalar::U64 => u64::from_ne_bytes(self.read_declared_array(read)?) as u128,
            DeclaredScalar::U128 => u128::from_ne_bytes(self.read_declared_array(read)?),
            DeclaredScalar::USize => usize::from_ne_bytes(self.read_declared_array(read)?) as u128,
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "declared unsigned read used non-unsigned scalar",
                });
            }
        })
    }

    fn read_declared_f64(&self, read: DeclaredScalarRead) -> Result<f64, FableError> {
        Ok(match read.scalar {
            DeclaredScalar::F32 => f32::from_ne_bytes(self.read_declared_array(read)?) as f64,
            DeclaredScalar::F64 => f64::from_ne_bytes(self.read_declared_array(read)?),
            _ => {
                return Err(FableError::MalformedProgram {
                    reason: "declared float read used non-float scalar",
                });
            }
        })
    }

    fn read_declared_array<const N: usize>(
        &self,
        read: DeclaredScalarRead,
    ) -> Result<[u8; N], FableError> {
        let bytes = self.read_declared_bytes(read, N)?;
        let mut out = [0; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    fn read_declared_bytes(
        &self,
        read: DeclaredScalarRead,
        len: usize,
    ) -> Result<&[u8], FableError> {
        let value = self.declared_local(read.local)?;
        let start = read.offset;
        let end = start.checked_add(len).ok_or(FableError::MalformedProgram {
            reason: "declared scalar read offset overflowed",
        })?;
        if end > value.size {
            return Err(FableError::MalformedProgram {
                reason: "declared scalar read exceeded value layout",
            });
        }
        Ok(unsafe { std::slice::from_raw_parts(value.ptr().add(start), len) })
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
            ValueExpr::Declared(_) => Err(FableError::TypeMismatch {
                expected: format!("{shape}"),
                actual: "declared value",
            }),
        }
    }

    fn eval_declared_value(
        &mut self,
        type_index: usize,
        expr: &DeclaredValueExpr,
    ) -> Result<DeclaredOwnedValue, FableError> {
        if expr.type_index() != type_index {
            let expected = self.declared_type_name(type_index)?;
            return Err(FableError::TypeMismatch {
                expected,
                actual: "declared value",
            });
        }
        match expr {
            DeclaredValueExpr::Struct(expr) => self.init_declared_struct_value(expr),
            DeclaredValueExpr::Enum(expr) => self.init_declared_enum_value(expr),
            DeclaredValueExpr::Local(local) => self.copy_declared_local(*local),
            DeclaredValueExpr::Field(field) => self.copy_declared_field(*field),
        }
    }

    fn init_declared_struct_value(
        &mut self,
        expr: &DeclaredStructExpr,
    ) -> Result<DeclaredOwnedValue, FableError> {
        let descriptor = self.declared_descriptor(expr.type_index)?.clone();
        let value = DeclaredOwnedValue::zeroed(expr.type_index, &descriptor)?;
        for field in expr.fields.iter() {
            self.write_declared_field(value.ptr(), field)?;
        }
        Ok(value)
    }

    fn init_declared_enum_value(
        &mut self,
        expr: &DeclaredEnumExpr,
    ) -> Result<DeclaredOwnedValue, FableError> {
        let descriptor = self.declared_descriptor(expr.type_index)?.clone();
        let value = DeclaredOwnedValue::zeroed(expr.type_index, &descriptor)?;
        self.write_declared_tag(value.ptr(), expr.tag_offset, expr.tag_width, expr.selector)?;
        for field in expr.fields.iter() {
            self.write_declared_field(value.ptr(), field)?;
        }
        Ok(value)
    }

    fn copy_declared_local(&self, local: LocalRef) -> Result<DeclaredOwnedValue, FableError> {
        let value = self.declared_local(local)?;
        let descriptor = self.declared_descriptor(value.type_index)?;
        DeclaredOwnedValue::copy_from(value.type_index, descriptor, value.ptr().cast_const())
    }

    fn copy_declared_field(
        &self,
        field: DeclaredValueField,
    ) -> Result<DeclaredOwnedValue, FableError> {
        let source = self.declared_local(field.local)?;
        let descriptor = self.declared_descriptor(field.type_index)?;
        DeclaredOwnedValue::copy_from(field.type_index, descriptor, unsafe {
            source.ptr().add(field.offset).cast_const()
        })
    }

    fn declared_descriptor(
        &self,
        type_index: usize,
    ) -> Result<&FableDeclaredDescriptor, FableError> {
        Ok(self
            .declared_types
            .by_index(type_index)
            .ok_or(FableError::MalformedProgram {
                reason: "declared type index was missing at runtime",
            })?
            .descriptor())
    }

    fn declared_type_name(&self, type_index: usize) -> Result<String, FableError> {
        Ok(self
            .declared_types
            .by_index(type_index)
            .ok_or(FableError::MalformedProgram {
                reason: "declared type index was missing at runtime",
            })?
            .name()
            .to_owned())
    }

    fn write_declared_field(
        &mut self,
        base: *mut u8,
        field: &DeclaredFieldInit,
    ) -> Result<(), FableError> {
        let dst = unsafe { base.add(field.offset) };
        match field.ty {
            DeclaredFieldType::Scalar(scalar) => {
                self.write_declared_scalar(dst, scalar, &field.value)
            }
            DeclaredFieldType::Declared(type_index) => {
                let value = self
                    .eval_declared_value(type_index, expect_declared_value_expr(&field.value)?)?;
                if value.size != 0 {
                    unsafe { copy_nonoverlapping(value.ptr(), dst, value.size) };
                }
                Ok(())
            }
        }
    }

    fn write_declared_tag(
        &self,
        base: *mut u8,
        offset: usize,
        width: usize,
        selector: u64,
    ) -> Result<(), FableError> {
        let dst = unsafe { base.add(offset) };
        match width {
            0 => Ok(()),
            1 => {
                let value = u8::try_from(selector)
                    .map_err(|_| number_out_of_range(ScalarType::U8, selector.to_string()))?;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 1) };
                Ok(())
            }
            2 => {
                let value = u16::try_from(selector)
                    .map_err(|_| number_out_of_range(ScalarType::U16, selector.to_string()))?;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 2) };
                Ok(())
            }
            4 => {
                let value = u32::try_from(selector)
                    .map_err(|_| number_out_of_range(ScalarType::U32, selector.to_string()))?;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 4) };
                Ok(())
            }
            8 => {
                unsafe { copy_nonoverlapping(selector.to_ne_bytes().as_ptr(), dst, 8) };
                Ok(())
            }
            _ => Err(FableError::MalformedProgram {
                reason: "declared enum tag width was invalid",
            }),
        }
    }

    fn write_declared_scalar(
        &mut self,
        dst: *mut u8,
        scalar: DeclaredScalar,
        expr: &ExprPlan,
    ) -> Result<(), FableError> {
        match scalar {
            DeclaredScalar::Unit => self.eval_unit(expect_unit_expr(expr)?),
            DeclaredScalar::Bool => {
                let value = u8::from(self.eval_bool(expect_bool_expr(expr)?)?);
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 1) };
                Ok(())
            }
            DeclaredScalar::Char => {
                let value = self.eval_char_assign(expr)? as u32;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 4) };
                Ok(())
            }
            DeclaredScalar::String => Err(FableError::Unsupported {
                feature: "declared string field runtime".into(),
            }),
            DeclaredScalar::I8 => self.write_declared_signed::<i8>(dst, scalar, expr),
            DeclaredScalar::I16 => self.write_declared_signed::<i16>(dst, scalar, expr),
            DeclaredScalar::I32 => self.write_declared_signed::<i32>(dst, scalar, expr),
            DeclaredScalar::I64 => self.write_declared_signed::<i64>(dst, scalar, expr),
            DeclaredScalar::I128 => self.write_declared_signed::<i128>(dst, scalar, expr),
            DeclaredScalar::ISize => self.write_declared_signed::<isize>(dst, scalar, expr),
            DeclaredScalar::U8 => self.write_declared_unsigned::<u8>(dst, scalar, expr),
            DeclaredScalar::U16 => self.write_declared_unsigned::<u16>(dst, scalar, expr),
            DeclaredScalar::U32 => self.write_declared_unsigned::<u32>(dst, scalar, expr),
            DeclaredScalar::U64 => self.write_declared_unsigned::<u64>(dst, scalar, expr),
            DeclaredScalar::U128 => self.write_declared_unsigned::<u128>(dst, scalar, expr),
            DeclaredScalar::USize => self.write_declared_unsigned::<usize>(dst, scalar, expr),
            DeclaredScalar::F32 => {
                let value = self.eval_number_as_f64(expect_number_expr(expr)?)? as f32;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 4) };
                Ok(())
            }
            DeclaredScalar::F64 => {
                let value = self.eval_number_as_f64(expect_number_expr(expr)?)?;
                unsafe { copy_nonoverlapping(value.to_ne_bytes().as_ptr(), dst, 8) };
                Ok(())
            }
        }
    }

    fn write_declared_unsigned<T>(
        &self,
        dst: *mut u8,
        target: DeclaredScalar,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<u128> + Copy,
        T: DeclaredIntBytes,
    {
        let value = self.eval_number_as_u128(expect_number_expr(expr)?)?;
        let converted = T::try_from(value).map_err(|_| {
            number_out_of_range(declared_scalar_number_target(target), value.to_string())
        })?;
        converted.write_ne_bytes(dst);
        Ok(())
    }

    fn write_declared_signed<T>(
        &self,
        dst: *mut u8,
        target: DeclaredScalar,
        expr: &ExprPlan,
    ) -> Result<(), FableError>
    where
        T: TryFrom<i128> + Copy,
        T: DeclaredIntBytes,
    {
        let value = self.eval_number_as_i128(expect_number_expr(expr)?)?;
        let converted = T::try_from(value).map_err(|_| {
            number_out_of_range(declared_scalar_number_target(target), value.to_string())
        })?;
        converted.write_ne_bytes(dst);
        Ok(())
    }

    fn eval_value_for_effect(&mut self, expr: &ValueExpr) -> Result<(), FableError> {
        match expr {
            ValueExpr::Struct(expr) => unsafe { self.init_struct_value(expr) }.map(drop),
            ValueExpr::Declared(expr) => {
                self.eval_declared_value(expr.type_index(), expr).map(drop)
            }
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

fn query_result_mismatch(expected: FableQueryType, actual: &'static str) -> FableError {
    FableError::TypeMismatch {
        expected: expected.name().into(),
        actual,
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

fn expect_declared_value_expr(expr: &ExprPlan) -> Result<&DeclaredValueExpr, FableError> {
    match expr {
        ExprPlan::Value(ValueExpr::Declared(expr)) => Ok(expr),
        other => Err(FableError::TypeMismatch {
            expected: "declared value".into(),
            actual: other.kind_name(),
        }),
    }
}

#[derive(Debug)]
enum PathSegment {
    Name(String),
    Index { index: usize, literal: String },
}

fn name_text(name: &Name) -> &str {
    match name {
        Name::Ident(value) | Name::TypeIdent(value) => &value.value,
    }
}

fn collect_path(expr: &Expr) -> Result<Vec<PathSegment>, FableError> {
    match expr {
        Expr::Var(var) => Ok(vec![PathSegment::Name(name_text(&var.name).to_owned())]),
        Expr::Field(field) => {
            let mut path = collect_path(&field.base)?;
            path.push(PathSegment::Name(name_text(&field.field_name).to_owned()));
            Ok(path)
        }
        Expr::Index(index) => {
            let mut path = collect_path(&index.base)?;
            let (index, literal) = literal_index(&index.index)?;
            path.push(PathSegment::Index { index, literal });
            Ok(path)
        }
        Expr::Paren(paren) => collect_path(&paren.expr),
        Expr::Call(_) => Err(FableError::Unsupported {
            feature: "call paths".into(),
        }),
        _ => Err(FableError::Unsupported {
            feature: "non-path assignment targets".into(),
        }),
    }
}

fn literal_index(expr: &Expr) -> Result<(usize, String), FableError> {
    let Expr::Literal(Literal::Int(text)) = expr else {
        return Err(FableError::Unsupported {
            feature: "dynamic index paths".into(),
        });
    };
    let text = text.value.clone();
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
    match unary.op.as_str() {
        "not" => Ok(UnaryOp::Not),
        "-" => Ok(UnaryOp::Neg),
        _ => Err(FableError::MalformedSyntax {
            reason: "unexpected unary operator",
        }),
    }
}

fn binary_op(binary: &BinaryExpr) -> Result<BinaryOp, FableError> {
    match binary.op.as_str() {
        "or" => Ok(BinaryOp::Or),
        "and" => Ok(BinaryOp::And),
        "==" => Ok(BinaryOp::Eq),
        "!=" => Ok(BinaryOp::Neq),
        "<" => Ok(BinaryOp::Lt),
        ">" => Ok(BinaryOp::Gt),
        "<=" => Ok(BinaryOp::Le),
        ">=" => Ok(BinaryOp::Ge),
        "+" => Ok(BinaryOp::Add),
        "-" => Ok(BinaryOp::Sub),
        _ => Err(FableError::MalformedSyntax {
            reason: "unexpected binary operator",
        }),
    }
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

fn invalid_root(name: &'static str, reason: &'static str) -> FableError {
    FableError::InvalidRoot {
        name: name.to_owned(),
        reason,
    }
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

#[cfg(test)]
mod tests {
    use facet::Facet;
    use weavy::ir::{IntrinsicDescriptor, dense_lowered_analysis};
    use weavy::mem::{Access, Tag};

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

    #[derive(Debug, Facet, PartialEq)]
    struct TransformInput {
        first_name: String,
        last_name: String,
        age: u8,
        deleted: bool,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct TransformOutput {
        name: String,
        age: u8,
        status: String,
        adult: bool,
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

    fn transform_input() -> TransformInput {
        TransformInput {
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            age: 36,
            deleted: false,
        }
    }

    fn transform_output() -> TransformOutput {
        TransformOutput {
            name: String::new(),
            age: 0,
            status: String::new(),
            adult: false,
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
    fn applies_transform_style_named_roots() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_write::<TransformOutput>("out"),
        ];
        let plan = FableRootPlan::compile(
            r#"
                out.name = in.first_name + " " + in.last_name;
                out.age = in.age;
                out.adult = in.age >= 18;

                if in.deleted {
                    out.status = "archived";
                } else {
                    out.status = "active";
                }
            "#,
            &roots,
        )
        .unwrap();

        let input = transform_input();
        let mut output = transform_output();
        let stats = {
            let mut values = [
                FableRootValue::read_only("in", &input),
                FableRootValue::read_write("out", &mut output),
            ];
            plan.apply_with_stats(&mut values).unwrap()
        };

        assert_eq!(
            output,
            TransformOutput {
                name: "Ada Lovelace".into(),
                age: 36,
                status: "active".into(),
                adult: true,
            }
        );
        assert!(stats.step_count >= 1);
    }

    #[test]
    fn exposes_compiled_root_program_as_canonical_weavy_ir() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_write::<TransformOutput>("out"),
        ];
        let plan = FableRootPlan::compile(
            r#"
                let full = in.first_name + " " + in.last_name;
                out.name = full;

                if in.deleted {
                    out.status = "archived";
                } else {
                    out.status = "active";
                }
            "#,
            &roots,
        )
        .unwrap();

        let analysis = dense_lowered_analysis(&plan.lowered);
        let shape = analysis.program_stats;
        assert_eq!(shape.block_count, 2);
        assert_eq!(shape.root.op_count, 3);
        assert_eq!(shape.blocks.op_count, 2);
        assert_eq!(shape.total.intrinsic_op_count, 5);
        assert_eq!(shape.total.control_op_count, 0);
        assert_eq!(shape.total.memory_op_count, 0);

        let counts = analysis.intrinsic_counts;
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "fable",
                name: "let",
            }],
            1
        );
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "fable",
                name: "assign",
            }],
            3
        );
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "fable",
                name: "branch",
            }],
            1
        );

        let effects = analysis.effect_stats;
        assert_eq!(effects.total.intrinsic_op_count, 5);
        assert_eq!(effects.total.typed_memory_overwrite_count, 3);
        assert!(effects.total.side_channel_count >= 2);
        assert_eq!(effects.total.barrier_count, 5);

        let input = transform_input();
        let mut output = transform_output();
        {
            let mut values = [
                FableRootValue::read_only("in", &input),
                FableRootValue::read_write("out", &mut output),
            ];
            plan.apply(&mut values).unwrap();
        }

        assert_eq!(output.name, "Ada Lovelace");
        assert_eq!(output.status, "active");
    }

    #[test]
    fn applies_typed_transform_plan() {
        let plan = FableTransformPlan::<TransformInput, TransformOutput>::compile(
            r#"
                out.name = in.first_name + " " + in.last_name;
                out.age = in.age;
                out.adult = in.age >= 18;

                if in.deleted {
                    out.status = "archived";
                } else {
                    out.status = "active";
                }
            "#,
        )
        .unwrap();

        let input = transform_input();
        let mut output = transform_output();
        let stats = plan.apply_with_stats(&input, &mut output).unwrap();

        assert_eq!(
            output,
            TransformOutput {
                name: "Ada Lovelace".into(),
                age: 36,
                status: "active".into(),
                adult: true,
            }
        );
        assert!(stats.step_count >= 1);
    }

    #[test]
    fn applies_transform_helper_with_intrinsics() {
        let input = transform_input();
        let mut output = transform_output();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics.add_string_unary("scream", scream).unwrap();

        transform_with_intrinsics(
            &input,
            &mut output,
            r#"
                out.name = scream(in.first_name);
                out.age = in.age;
                out.status = scream(in.last_name);
                out.adult = in.age >= 18;
            "#,
            &intrinsics,
        )
        .unwrap();

        assert_eq!(
            output,
            TransformOutput {
                name: "ADA!".into(),
                age: 36,
                status: "LOVELACE!".into(),
                adult: true,
            }
        );
    }

    #[test]
    fn evaluates_typed_predicate_plan_against_read_only_root() {
        let value = state();
        let plan = FablePredicatePlan::<State>::compile(
            r#"
                let next_age = root.user.age + 1;
                next_age >= 18 and not root.user.active
            "#,
        )
        .unwrap();

        let (result, stats) = plan.evaluate_with_stats(&value).unwrap();

        assert!(result);
        assert!(stats.step_count >= 2);

        let analysis = dense_lowered_analysis(&plan.plan.lowered);
        assert_eq!(analysis.program_stats.root.op_count, 2);
        assert_eq!(
            analysis.intrinsic_counts[&IntrinsicDescriptor {
                dialect: "fable",
                name: "predicate",
            }],
            1
        );
    }

    #[test]
    fn evaluates_root_predicate_plan_over_explicit_roots() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_only::<TransformOutput>("out"),
        ];
        let plan = FableRootPredicatePlan::compile(
            r#"
                let expected = in.first_name + " " + in.last_name;
                out.name == expected and out.age == in.age and not in.deleted
            "#,
            &roots,
        )
        .unwrap();

        let input = transform_input();
        let mut output = transform_output();
        output.name = "Ada Lovelace".into();
        output.age = 36;
        let mut values = [
            FableRootValue::read_only("in", &input),
            FableRootValue::read_only("out", &output),
        ];

        assert!(plan.evaluate(&mut values).unwrap());
    }

    #[test]
    fn predicate_helper_supports_custom_intrinsics() {
        let value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics
            .add_string_binary_predicate("contains_ci", contains_ci)
            .unwrap();

        let result = predicate_with_intrinsics(
            &value,
            r#"contains_ci(root.user.name, "ADA") and root.user.age == 17"#,
            &intrinsics,
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn predicate_plans_reject_mutating_the_read_only_root() {
        let err = match FablePredicatePlan::<State>::compile(
            r#"
                root.user.active = true;
                root.user.active
            "#,
        ) {
            Ok(_) => panic!("expected Fable predicate compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::ReadOnlyRoot {
                name
            } if name == "root"
        ));
    }

    #[test]
    fn root_predicate_plans_require_read_only_roots() {
        let roots = [FableRootSpec::read_write::<State>("root")];

        let err = match FableRootPredicatePlan::compile("root.user.active", &roots) {
            Ok(_) => panic!("expected Fable predicate compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::InvalidRoot {
                name,
                reason: "predicate roots must be read-only",
            } if name == "root"
        ));
    }

    #[test]
    fn predicate_source_must_end_with_bool_expression() {
        let err = match FablePredicatePlan::<State>::compile("root.user.name") {
            Ok(_) => panic!("expected Fable predicate compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::TypeMismatch {
                expected,
                actual: "string",
            } if expected == "bool"
        ));
    }

    #[test]
    fn evaluates_typed_query_plan_against_read_only_root() {
        let value = state();
        let plan = FableQueryPlan::<State, String>::compile(
            r#"
                let suffix = " Lovelace";
                trim(root.user.name + suffix)
            "#,
        )
        .unwrap();

        let (result, stats) = plan.evaluate_with_stats(&value).unwrap();

        assert_eq!(result, "Ada Lovelace");
        assert!(stats.step_count >= 2);

        let analysis = dense_lowered_analysis(&plan.plan.lowered);
        assert_eq!(analysis.program_stats.root.op_count, 2);
        assert_eq!(
            analysis.intrinsic_counts[&IntrinsicDescriptor {
                dialect: "fable",
                name: "query",
            }],
            1
        );
    }

    #[test]
    fn evaluates_numeric_query_plan_with_checked_output_conversion() {
        let value = state();
        let visits: u8 = query(&value, "root.visits + 4").unwrap();
        let score: f32 = query(&value, "root.score + 0.25").unwrap();
        let age: i16 = query(&value, "root.user.age + 1").unwrap();

        assert_eq!(visits, 5);
        assert_eq!(score, 1.75);
        assert_eq!(age, 18);
    }

    #[test]
    fn evaluates_root_query_plan_over_explicit_roots() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_only::<TransformOutput>("out"),
        ];
        let plan = FableRootQueryPlan::<String>::compile(
            r#"
                let expected = in.first_name + " " + in.last_name;
                out.status + ":" + expected
            "#,
            &roots,
        )
        .unwrap();

        let input = transform_input();
        let mut output = transform_output();
        output.status = "active".into();
        let mut values = [
            FableRootValue::read_only("in", &input),
            FableRootValue::read_only("out", &output),
        ];

        assert_eq!(plan.evaluate(&mut values).unwrap(), "active:Ada Lovelace");
    }

    #[test]
    fn query_helper_supports_custom_intrinsics() {
        let value = state();
        let mut intrinsics = FableIntrinsics::standard();
        intrinsics.add_string_unary("scream", scream).unwrap();

        let result: String =
            query_with_intrinsics(&value, "scream(root.user.name)", &intrinsics).unwrap();

        assert_eq!(result, "ADA!");
    }

    #[test]
    fn query_plans_reject_mutating_the_read_only_root() {
        let err = match FableQueryPlan::<State, bool>::compile(
            r#"
                root.user.active = true;
                root.user.active
            "#,
        ) {
            Ok(_) => panic!("expected Fable query compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::ReadOnlyRoot {
                name
            } if name == "root"
        ));
    }

    #[test]
    fn root_query_plans_require_read_only_roots() {
        let roots = [FableRootSpec::read_write::<State>("root")];

        let err = match FableRootQueryPlan::<bool>::compile("root.user.active", &roots) {
            Ok(_) => panic!("expected Fable query compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::InvalidRoot {
                name,
                reason: "query roots must be read-only",
            } if name == "root"
        ));
    }

    #[test]
    fn query_source_must_end_with_compatible_expression() {
        let err = match FableQueryPlan::<State, String>::compile("root.user.age") {
            Ok(_) => panic!("expected Fable query compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::TypeMismatch {
                expected,
                actual: "signed number",
            } if expected == "string"
        ));
    }

    #[test]
    fn rejects_writes_to_typed_transform_input() {
        let err =
            match FableTransformPlan::<TransformInput, TransformOutput>::compile("in.age = 1;") {
                Ok(_) => panic!("expected Fable compilation to fail"),
                Err(err) => err,
            };

        assert!(matches!(
            err,
            FableError::ReadOnlyRoot {
                name
            } if name == "in"
        ));
    }

    #[test]
    fn rejects_writes_to_read_only_named_roots() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_write::<TransformOutput>("out"),
        ];

        let err = match FableRootPlan::compile("in.age = 1;", &roots) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            FableError::ReadOnlyRoot {
                name
            } if name == "in"
        ));
    }

    #[test]
    fn rejects_missing_runtime_named_roots() {
        let roots = [
            FableRootSpec::read_only::<TransformInput>("in"),
            FableRootSpec::read_write::<TransformOutput>("out"),
        ];
        let plan = FableRootPlan::compile("out.age = in.age;", &roots).unwrap();

        let mut output = transform_output();
        let mut values = [FableRootValue::read_write("out", &mut output)];
        let err = plan.apply(&mut values).unwrap_err();

        assert!(matches!(
            err,
            FableError::MissingRoot {
                name
            } if name == "in"
        ));
    }

    #[test]
    fn rejects_read_only_runtime_binding_for_read_write_root() {
        let roots = [FableRootSpec::read_write::<TransformOutput>("out")];
        let plan = FableRootPlan::compile("out.age = 1;", &roots).unwrap();

        let output = transform_output();
        let mut values = [FableRootValue::read_only("out", &output)];
        let err = plan.apply(&mut values).unwrap_err();

        assert!(matches!(
            err,
            FableError::ReadOnlyRoot {
                name
            } if name == "out"
        ));
    }

    #[test]
    fn rejects_runtime_root_shape_mismatches() {
        let roots = [FableRootSpec::read_only::<TransformInput>("in")];
        let plan = FableRootPlan::compile("in.age;", &roots).unwrap();

        let output = transform_output();
        let mut values = [FableRootValue::read_only("in", &output)];
        let err = plan.apply(&mut values).unwrap_err();

        assert!(matches!(
            err,
            FableError::RootShapeMismatch {
                name,
                ..
            } if name == "in"
        ));
    }

    #[test]
    fn else_if_uses_dense_child_blocks() {
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

    #[test]
    fn declared_type_descriptors_are_observable_from_rust() {
        let plan = FableRootPlan::compile(
            r#"
struct Packed { small: u8, large: i64, flag: bool }
enum MaybePacked {
  None,
  Some { payload: Packed, code: i32 },
}
"#,
            &[],
        )
        .expect("declared types compile");

        let packed = plan
            .declared_types()
            .get("Packed")
            .expect("Packed descriptor");
        let Access::Record(record) = &packed.descriptor().access else {
            panic!("Packed must be a record descriptor");
        };
        assert!(matches!(record.construct, weavy::mem::Construct::InPlace));
        assert_eq!(packed.descriptor().layout.size, 16);
        assert_eq!(packed.descriptor().layout.align, 8);
        assert_eq!(record.fields.len(), 3);
        assert_eq!(record.fields[0].offset, 8);
        assert_eq!(record.fields[0].descriptor.schema, "u8");
        assert_eq!(record.fields[1].offset, 0);
        assert_eq!(record.fields[1].descriptor.schema, "i64");
        assert_eq!(record.fields[2].offset, 9);
        assert_eq!(record.fields[2].descriptor.schema, "bool");

        let maybe = plan
            .declared_types()
            .get("MaybePacked")
            .expect("MaybePacked descriptor");
        let Access::Enum(access) = &maybe.descriptor().access else {
            panic!("MaybePacked must be an enum descriptor");
        };
        let Tag::Direct { offset, width } = access.tag else {
            panic!("declared enums use direct tags");
        };
        assert_eq!(offset, 0);
        assert_eq!(width, 1);
        assert_eq!(access.variants.len(), 2);
        assert_eq!(access.variants[0].index, 0);
        assert_eq!(access.variants[0].selector, 0);
        assert_eq!(access.variants[1].index, 1);
        assert_eq!(access.variants[1].selector, 1);
        assert!(matches!(
            access.variants[1].payload.construct,
            weavy::mem::Construct::InPlace
        ));
        assert_eq!(access.variants[1].payload.fields.len(), 2);
        assert_eq!(access.variants[1].payload.fields[0].offset, 8);
        assert_eq!(
            access.variants[1].payload.fields[0].descriptor.schema,
            "Packed"
        );
        assert_eq!(access.variants[1].payload.fields[1].offset, 24);
        assert_eq!(
            access.variants[1].payload.fields[1].descriptor.schema,
            "i32"
        );
    }

    #[test]
    fn declared_type_checker_rejects_duplicate_and_unknown_names() {
        let duplicate_type = root_compile_err(
            r#"
struct Same { x: i64 }
enum Same { Other }
"#,
        );
        assert!(matches!(
            duplicate_type,
            FableError::AmbiguousType { name } if name == "Same"
        ));

        let duplicate_field = root_compile_err("struct Bad { x: i64, x: bool }");
        assert!(matches!(
            duplicate_field,
            FableError::DuplicateStructField { field } if field == "x"
        ));

        let duplicate_variant = root_compile_err("enum Bad { Same, Same }");
        assert!(matches!(
            duplicate_variant,
            FableError::DuplicateStructField { field } if field == "Same"
        ));

        let unknown_type = root_compile_err("struct Bad { missing: Missing }");
        assert!(matches!(
            unknown_type,
            FableError::UnknownType { name } if name == "Missing"
        ));
    }

    #[test]
    fn declared_structs_and_enums_construct_match_and_query() {
        let plan = FableRootQueryPlan::<i128>::compile(
            r#"
struct Point { x: i64, y: i64 }
enum Shape {
  Empty,
  Hit { payload: Point },
}
let shape = Shape::Hit { payload: Point { x: 40, y: 2 } };
match shape {
  Shape::Empty => { 0 - 0; },
  Shape::Hit { payload } => { payload.x + payload.y; },
}
"#,
            &[],
        )
        .expect("declared query compiles");
        let mut roots = [];

        assert_eq!(plan.evaluate(&mut roots).expect("query runs"), 42);
    }

    #[test]
    fn declared_match_supports_bare_variants_and_scalar_payload_bindings() {
        let plan = FableRootQueryPlan::<i128>::compile(
            r#"
enum Thing {
  Empty,
  Num { value: i64 },
}
let thing = Thing::Num { value: 7 };
match thing {
  Thing::Empty => { 0 - 0; },
  Thing::Num { value } => { value + 35; },
}
"#,
            &[],
        )
        .expect("declared scalar payload match compiles");
        let mut roots = [];

        assert_eq!(plan.evaluate(&mut roots).expect("query runs"), 42);
    }

    #[test]
    fn declared_struct_let_binding_copies_bytes() {
        let plan = FableRootQueryPlan::<i128>::compile(
            r#"
struct Point { x: i64 }
let a = Point { x: 21 };
let b = a;
a.x + b.x;
"#,
            &[],
        )
        .expect("declared copy query compiles");
        let mut roots = [];

        assert_eq!(plan.evaluate(&mut roots).expect("query runs"), 42);
    }

    #[test]
    fn declared_value_checker_reports_required_errors() {
        assert!(matches!(
            root_query_compile_err::<i128>("struct Point { x: i64 } let p = Point { x: 1, y: 2 }; p.x"),
            FableError::Unsupported { feature } if feature == "Point has no field named y"
        ));
        assert!(matches!(
            root_query_compile_err::<i128>("struct Point { x: i64 } let p = Point { }; p.x"),
            FableError::Unsupported { feature } if feature == "Point literal is missing field x"
        ));
        assert!(matches!(
            root_query_compile_err::<i128>("struct Point { x: i64 } let p = Point { x: 1, x: 2 }; p.x"),
            FableError::DuplicateStructField { field } if field == "x"
        ));
        assert!(matches!(
            root_query_compile_err::<i128>("struct Point { x: i64 } let p = Point { x: true }; p.x"),
            FableError::TypeMismatch { expected, actual } if expected == "i64" && actual == "bool"
        ));
        assert!(matches!(
            root_query_compile_err::<u128>("enum Thing { Empty } let thing = Thing::Missing; 0;"),
            FableError::Unsupported { feature } if feature == "Thing has no variant named Missing"
        ));
        assert!(matches!(
            root_query_compile_err::<u128>(
                "enum Thing { A, B } let thing = Thing::A; match thing { Thing::A => { 1; } }",
            ),
            FableError::Unsupported { feature } if feature == "non-exhaustive match on Thing"
        ));
    }

    #[test]
    fn task_functions_support_deep_direct_recursion() {
        let plan = FableRootQueryPlan::<i128>::compile(
            r#"
fn countdown(n: i64) -> i64 {
  if n == 0 {
    0;
  } else {
    countdown(n - 1);
  }
}
countdown(100000);
"#,
            &[],
        )
        .expect("recursive function query compiles");
        let mut roots = [];

        assert_eq!(plan.evaluate(&mut roots).expect("query runs"), 0);
    }

    #[test]
    fn task_functions_support_mutual_recursion() {
        let plan = FableRootQueryPlan::<bool>::compile(
            r#"
fn even(n: i64) -> bool {
  if n == 0 {
    true;
  } else {
    odd(n - 1);
  }
}

fn odd(n: i64) -> bool {
  if n == 0 {
    false;
  } else {
    even(n - 1);
  }
}

even(101);
"#,
            &[],
        )
        .expect("mutual recursion query compiles");
        let mut roots = [];

        assert!(!plan.evaluate(&mut roots).expect("query runs"));
    }

    fn compile_err(src: &str) -> FableError {
        match FablePlan::<State>::compile(src) {
            Ok(_) => panic!("expected Fable compilation to fail"),
            Err(err) => err,
        }
    }

    fn root_compile_err(src: &str) -> FableError {
        match FableRootPlan::compile(src, &[]) {
            Ok(_) => panic!("expected Fable root compilation to fail"),
            Err(err) => err,
        }
    }

    fn root_query_compile_err<Output>(src: &str) -> FableError
    where
        Output: FableQueryResult,
    {
        match FableRootQueryPlan::<Output>::compile(src, &[]) {
            Ok(_) => panic!("expected Fable root query compilation to fail"),
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
