#[cfg(not(feature = "cov"))]
use afl::fuzz;
use arbitrary::Arbitrary;
use facet::Facet;
use facet_core::{PtrConst, Shape};
use facet_reflect2::{Move, Op, Partial, Path, Source};

// ============================================================================
// Compound types for fuzzing (these need Facet derive)
// ============================================================================

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct Nested {
    name: String,
    point: Point,
    value: u64,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct WithOption {
    required: u32,
    optional: Option<String>,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct WithVec {
    items: Vec<u32>,
    label: String,
}

// ============================================================================
// Enum types for fuzzing
// ============================================================================

// Unit enums with various reprs
#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u8)]
pub enum UnitEnumU8 {
    A,
    B,
    C,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u16)]
pub enum UnitEnumU16 {
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u32)]
pub enum UnitEnumU32 {
    One,
    Two,
    Three,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u64)]
pub enum UnitEnumU64 {
    Alpha,
    Beta,
    Gamma,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i8)]
pub enum UnitEnumI8 {
    Neg,
    Zero,
    Pos,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i16)]
pub enum UnitEnumI16 {
    Low,
    Mid,
    High,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i32)]
pub enum UnitEnumI32 {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i64)]
pub enum UnitEnumI64 {
    Past,
    Present,
    Future,
}

// Data enums with various reprs
#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u8)]
pub enum DataEnumU8 {
    Empty,
    WithU32(u32),
    WithString(String),
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u16)]
pub enum DataEnumU16 {
    None,
    Bool(bool),
    Pair(u32, u32),
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u32)]
pub enum DataEnumU32 {
    Vacant,
    Single(i64),
    Double(String, String),
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i8)]
pub enum DataEnumI8 {
    Nothing,
    Something(u8),
    Everything(Vec<u8>),
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i32)]
pub enum DataEnumI32 {
    Nil,
    Value(f64),
    Values(Vec<f64>),
}

// Mixed enums (unit + tuple + struct variants) with various reprs
#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u8)]
pub enum MixedEnumU8 {
    Unit,
    Tuple(u32, String),
    Struct { x: i32, y: i32 },
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u32)]
pub enum MixedEnumU32 {
    Empty,
    Wrapped(Box<u32>),
    Named { value: Option<String> },
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i16)]
pub enum MixedEnumI16 {
    Zero,
    One(bool),
    Two { a: u8, b: u8 },
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i64)]
pub enum MixedEnumI64 {
    Void,
    Scalar(usize),
    Record { id: u64, name: String },
}

// Nested enums (enums containing enums)
#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u8)]
pub enum NestedEnumU8 {
    Simple(UnitEnumU8),
    Complex(DataEnumU8),
    Both { unit: UnitEnumU8, data: DataEnumU8 },
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u32)]
pub enum NestedEnumU32 {
    Left(UnitEnumU32),
    Right(DataEnumU32),
    Mixed(MixedEnumU32),
}

#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(i32)]
pub enum NestedEnumI32 {
    A(UnitEnumI32),
    B(DataEnumI32),
    C { inner: MixedEnumI16 },
}

// Deeply nested enum
#[derive(Clone, Debug, Facet, Arbitrary)]
#[repr(u16)]
pub enum DeepEnum {
    Leaf(u32),
    Branch(Box<NestedEnumU8>),
    Tree {
        left: NestedEnumU32,
        right: NestedEnumI32,
    },
}

// ============================================================================
// FuzzValue - values that can be moved into a Partial
// ============================================================================

/// A value that can be used in fuzzing.
/// Each variant holds an owned value that we can get a pointer to.
#[derive(Clone, Arbitrary)]
pub enum FuzzValue {
    // Scalars
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),

    // Compound types
    Point(Point),
    Nested(Nested),
    WithOption(WithOption),
    WithVec(WithVec),

    // Standard library compound types
    OptionU32(Option<u32>),
    OptionString(Option<String>),
    VecU8(Vec<u8>),
    VecU32(Vec<u32>),
    VecString(Vec<String>),
    BoxU32(Box<u32>),
    BoxString(Box<String>),

    // Tuples
    Tuple2U32((u32, u32)),
    Tuple3Mixed((u8, String, bool)),

    // Unit
    Unit(()),

    // Unit enums
    UnitEnumU8(UnitEnumU8),
    UnitEnumU16(UnitEnumU16),
    UnitEnumU32(UnitEnumU32),
    UnitEnumU64(UnitEnumU64),
    UnitEnumI8(UnitEnumI8),
    UnitEnumI16(UnitEnumI16),
    UnitEnumI32(UnitEnumI32),
    UnitEnumI64(UnitEnumI64),

    // Data enums
    DataEnumU8(DataEnumU8),
    DataEnumU16(DataEnumU16),
    DataEnumU32(DataEnumU32),
    DataEnumI8(DataEnumI8),
    DataEnumI32(DataEnumI32),

    // Mixed enums
    MixedEnumU8(MixedEnumU8),
    MixedEnumU32(MixedEnumU32),
    MixedEnumI16(MixedEnumI16),
    MixedEnumI64(MixedEnumI64),

    // Nested enums
    NestedEnumU8(NestedEnumU8),
    NestedEnumU32(NestedEnumU32),
    NestedEnumI32(NestedEnumI32),
    DeepEnum(DeepEnum),
}

impl FuzzValue {
    /// Get a pointer and shape for this value.
    /// The pointer is only valid while self is alive.
    fn as_ptr_and_shape(&self) -> (PtrConst, &'static Shape) {
        match self {
            // Scalars
            FuzzValue::Bool(v) => (PtrConst::new(v), bool::SHAPE),
            FuzzValue::U8(v) => (PtrConst::new(v), u8::SHAPE),
            FuzzValue::U16(v) => (PtrConst::new(v), u16::SHAPE),
            FuzzValue::U32(v) => (PtrConst::new(v), u32::SHAPE),
            FuzzValue::U64(v) => (PtrConst::new(v), u64::SHAPE),
            FuzzValue::U128(v) => (PtrConst::new(v), u128::SHAPE),
            FuzzValue::Usize(v) => (PtrConst::new(v), usize::SHAPE),
            FuzzValue::I8(v) => (PtrConst::new(v), i8::SHAPE),
            FuzzValue::I16(v) => (PtrConst::new(v), i16::SHAPE),
            FuzzValue::I32(v) => (PtrConst::new(v), i32::SHAPE),
            FuzzValue::I64(v) => (PtrConst::new(v), i64::SHAPE),
            FuzzValue::I128(v) => (PtrConst::new(v), i128::SHAPE),
            FuzzValue::Isize(v) => (PtrConst::new(v), isize::SHAPE),
            FuzzValue::F32(v) => (PtrConst::new(v), f32::SHAPE),
            FuzzValue::F64(v) => (PtrConst::new(v), f64::SHAPE),
            FuzzValue::Char(v) => (PtrConst::new(v), char::SHAPE),
            FuzzValue::String(v) => (PtrConst::new(v), String::SHAPE),

            // Compound types
            FuzzValue::Point(v) => (PtrConst::new(v), Point::SHAPE),
            FuzzValue::Nested(v) => (PtrConst::new(v), Nested::SHAPE),
            FuzzValue::WithOption(v) => (PtrConst::new(v), WithOption::SHAPE),
            FuzzValue::WithVec(v) => (PtrConst::new(v), WithVec::SHAPE),

            // Standard library compound types
            FuzzValue::OptionU32(v) => (PtrConst::new(v), <Option<u32>>::SHAPE),
            FuzzValue::OptionString(v) => (PtrConst::new(v), <Option<String>>::SHAPE),
            FuzzValue::VecU8(v) => (PtrConst::new(v), <Vec<u8>>::SHAPE),
            FuzzValue::VecU32(v) => (PtrConst::new(v), <Vec<u32>>::SHAPE),
            FuzzValue::VecString(v) => (PtrConst::new(v), <Vec<String>>::SHAPE),
            FuzzValue::BoxU32(v) => (PtrConst::new(v), <Box<u32>>::SHAPE),
            FuzzValue::BoxString(v) => (PtrConst::new(v), <Box<String>>::SHAPE),

            // Tuples
            FuzzValue::Tuple2U32(v) => (PtrConst::new(v), <(u32, u32)>::SHAPE),
            FuzzValue::Tuple3Mixed(v) => (PtrConst::new(v), <(u8, String, bool)>::SHAPE),

            // Unit
            FuzzValue::Unit(v) => (PtrConst::new(v), <()>::SHAPE),

            // Unit enums
            FuzzValue::UnitEnumU8(v) => (PtrConst::new(v), UnitEnumU8::SHAPE),
            FuzzValue::UnitEnumU16(v) => (PtrConst::new(v), UnitEnumU16::SHAPE),
            FuzzValue::UnitEnumU32(v) => (PtrConst::new(v), UnitEnumU32::SHAPE),
            FuzzValue::UnitEnumU64(v) => (PtrConst::new(v), UnitEnumU64::SHAPE),
            FuzzValue::UnitEnumI8(v) => (PtrConst::new(v), UnitEnumI8::SHAPE),
            FuzzValue::UnitEnumI16(v) => (PtrConst::new(v), UnitEnumI16::SHAPE),
            FuzzValue::UnitEnumI32(v) => (PtrConst::new(v), UnitEnumI32::SHAPE),
            FuzzValue::UnitEnumI64(v) => (PtrConst::new(v), UnitEnumI64::SHAPE),

            // Data enums
            FuzzValue::DataEnumU8(v) => (PtrConst::new(v), DataEnumU8::SHAPE),
            FuzzValue::DataEnumU16(v) => (PtrConst::new(v), DataEnumU16::SHAPE),
            FuzzValue::DataEnumU32(v) => (PtrConst::new(v), DataEnumU32::SHAPE),
            FuzzValue::DataEnumI8(v) => (PtrConst::new(v), DataEnumI8::SHAPE),
            FuzzValue::DataEnumI32(v) => (PtrConst::new(v), DataEnumI32::SHAPE),

            // Mixed enums
            FuzzValue::MixedEnumU8(v) => (PtrConst::new(v), MixedEnumU8::SHAPE),
            FuzzValue::MixedEnumU32(v) => (PtrConst::new(v), MixedEnumU32::SHAPE),
            FuzzValue::MixedEnumI16(v) => (PtrConst::new(v), MixedEnumI16::SHAPE),
            FuzzValue::MixedEnumI64(v) => (PtrConst::new(v), MixedEnumI64::SHAPE),

            // Nested enums
            FuzzValue::NestedEnumU8(v) => (PtrConst::new(v), NestedEnumU8::SHAPE),
            FuzzValue::NestedEnumU32(v) => (PtrConst::new(v), NestedEnumU32::SHAPE),
            FuzzValue::NestedEnumI32(v) => (PtrConst::new(v), NestedEnumI32::SHAPE),
            FuzzValue::DeepEnum(v) => (PtrConst::new(v), DeepEnum::SHAPE),
        }
    }
}

impl std::fmt::Debug for FuzzValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzValue::Bool(v) => write!(f, "bool({v:?})"),
            FuzzValue::U8(v) => write!(f, "u8({v})"),
            FuzzValue::U16(v) => write!(f, "u16({v})"),
            FuzzValue::U32(v) => write!(f, "u32({v})"),
            FuzzValue::U64(v) => write!(f, "u64({v})"),
            FuzzValue::U128(v) => write!(f, "u128({v})"),
            FuzzValue::Usize(v) => write!(f, "usize({v})"),
            FuzzValue::I8(v) => write!(f, "i8({v})"),
            FuzzValue::I16(v) => write!(f, "i16({v})"),
            FuzzValue::I32(v) => write!(f, "i32({v})"),
            FuzzValue::I64(v) => write!(f, "i64({v})"),
            FuzzValue::I128(v) => write!(f, "i128({v})"),
            FuzzValue::Isize(v) => write!(f, "isize({v})"),
            FuzzValue::F32(v) => write!(f, "f32({v})"),
            FuzzValue::F64(v) => write!(f, "f64({v})"),
            FuzzValue::Char(v) => write!(f, "char({v:?})"),
            FuzzValue::String(v) => write!(f, "String({v:?})"),
            FuzzValue::Point(v) => write!(f, "Point({v:?})"),
            FuzzValue::Nested(v) => write!(f, "Nested({v:?})"),
            FuzzValue::WithOption(v) => write!(f, "WithOption({v:?})"),
            FuzzValue::WithVec(v) => write!(f, "WithVec({v:?})"),
            FuzzValue::OptionU32(v) => write!(f, "Option<u32>({v:?})"),
            FuzzValue::OptionString(v) => write!(f, "Option<String>({v:?})"),
            FuzzValue::VecU8(v) => write!(f, "Vec<u8>({v:?})"),
            FuzzValue::VecU32(v) => write!(f, "Vec<u32>({v:?})"),
            FuzzValue::VecString(v) => write!(f, "Vec<String>({v:?})"),
            FuzzValue::BoxU32(v) => write!(f, "Box<u32>({v:?})"),
            FuzzValue::BoxString(v) => write!(f, "Box<String>({v:?})"),
            FuzzValue::Tuple2U32(v) => write!(f, "(u32, u32)({v:?})"),
            FuzzValue::Tuple3Mixed(v) => write!(f, "(u8, String, bool)({v:?})"),
            FuzzValue::Unit(_) => write!(f, "()"),
            FuzzValue::UnitEnumU8(v) => write!(f, "UnitEnumU8::{v:?}"),
            FuzzValue::UnitEnumU16(v) => write!(f, "UnitEnumU16::{v:?}"),
            FuzzValue::UnitEnumU32(v) => write!(f, "UnitEnumU32::{v:?}"),
            FuzzValue::UnitEnumU64(v) => write!(f, "UnitEnumU64::{v:?}"),
            FuzzValue::UnitEnumI8(v) => write!(f, "UnitEnumI8::{v:?}"),
            FuzzValue::UnitEnumI16(v) => write!(f, "UnitEnumI16::{v:?}"),
            FuzzValue::UnitEnumI32(v) => write!(f, "UnitEnumI32::{v:?}"),
            FuzzValue::UnitEnumI64(v) => write!(f, "UnitEnumI64::{v:?}"),
            FuzzValue::DataEnumU8(v) => write!(f, "DataEnumU8::{v:?}"),
            FuzzValue::DataEnumU16(v) => write!(f, "DataEnumU16::{v:?}"),
            FuzzValue::DataEnumU32(v) => write!(f, "DataEnumU32::{v:?}"),
            FuzzValue::DataEnumI8(v) => write!(f, "DataEnumI8::{v:?}"),
            FuzzValue::DataEnumI32(v) => write!(f, "DataEnumI32::{v:?}"),
            FuzzValue::MixedEnumU8(v) => write!(f, "MixedEnumU8::{v:?}"),
            FuzzValue::MixedEnumU32(v) => write!(f, "MixedEnumU32::{v:?}"),
            FuzzValue::MixedEnumI16(v) => write!(f, "MixedEnumI16::{v:?}"),
            FuzzValue::MixedEnumI64(v) => write!(f, "MixedEnumI64::{v:?}"),
            FuzzValue::NestedEnumU8(v) => write!(f, "NestedEnumU8::{v:?}"),
            FuzzValue::NestedEnumU32(v) => write!(f, "NestedEnumU32::{v:?}"),
            FuzzValue::NestedEnumI32(v) => write!(f, "NestedEnumI32::{v:?}"),
            FuzzValue::DeepEnum(v) => write!(f, "DeepEnum::{v:?}"),
        }
    }
}

// ============================================================================
// FuzzSource - how to fill a value
// ============================================================================

/// Source for a fuzz operation.
#[derive(Clone, Arbitrary)]
pub enum FuzzSource {
    /// Move a value (copy bytes from the FuzzValue).
    Move(FuzzValue),
    /// Use the type's default value.
    Default,
    /// Build incrementally - pushes a frame.
    Build { len_hint: Option<u8> },
}

impl std::fmt::Debug for FuzzSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzSource::Move(v) => write!(f, "Move({:?})", v),
            FuzzSource::Default => write!(f, "Default"),
            FuzzSource::Build { len_hint } => write!(f, "Build({:?})", len_hint),
        }
    }
}

// ============================================================================
// FuzzPath - path into nested structures
// ============================================================================

/// A path for accessing nested fields.
/// Uses small indices to keep fuzzing efficient.
#[derive(Clone, Arbitrary)]
pub struct FuzzPath {
    /// Field indices (limited to keep paths reasonable).
    /// Using u8 since structs rarely have more than 256 fields.
    pub indices: Vec<u8>,
}

impl std::fmt::Debug for FuzzPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.indices)
    }
}

impl FuzzPath {
    fn to_path(&self) -> Path {
        let mut path = Path::default();
        // Limit path depth to avoid pathological cases
        for &idx in self.indices.iter().take(4) {
            path.push(idx as u32);
        }
        path
    }
}

// ============================================================================
// FuzzOp - operations to apply
// ============================================================================

/// A fuzzing operation that maps to Op.
#[derive(Clone, Debug, Arbitrary)]
pub enum FuzzOp {
    /// Set a value at a path.
    Set { path: FuzzPath, source: FuzzSource },
    /// End the current frame.
    End,
}

// ============================================================================
// FuzzTargetType - types we can allocate
// ============================================================================

/// The target type to allocate.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum FuzzTargetType {
    // Scalars
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    F32,
    F64,
    Char,
    String,

    // Compound types (custom structs)
    Point,
    Nested,
    WithOption,
    WithVec,

    // Standard library compound types
    OptionU32,
    OptionString,
    VecU8,
    VecU32,
    VecString,
    BoxU32,
    BoxString,

    // Tuples
    Tuple2U32,
    Tuple3Mixed,

    // Unit
    Unit,

    // Unit enums
    UnitEnumU8,
    UnitEnumU16,
    UnitEnumU32,
    UnitEnumU64,
    UnitEnumI8,
    UnitEnumI16,
    UnitEnumI32,
    UnitEnumI64,

    // Data enums
    DataEnumU8,
    DataEnumU16,
    DataEnumU32,
    DataEnumI8,
    DataEnumI32,

    // Mixed enums
    MixedEnumU8,
    MixedEnumU32,
    MixedEnumI16,
    MixedEnumI64,

    // Nested enums
    NestedEnumU8,
    NestedEnumU32,
    NestedEnumI32,
    DeepEnum,
}

impl FuzzTargetType {
    fn shape(&self) -> &'static Shape {
        match self {
            // Scalars
            FuzzTargetType::Bool => bool::SHAPE,
            FuzzTargetType::U8 => u8::SHAPE,
            FuzzTargetType::U16 => u16::SHAPE,
            FuzzTargetType::U32 => u32::SHAPE,
            FuzzTargetType::U64 => u64::SHAPE,
            FuzzTargetType::U128 => u128::SHAPE,
            FuzzTargetType::Usize => usize::SHAPE,
            FuzzTargetType::I8 => i8::SHAPE,
            FuzzTargetType::I16 => i16::SHAPE,
            FuzzTargetType::I32 => i32::SHAPE,
            FuzzTargetType::I64 => i64::SHAPE,
            FuzzTargetType::I128 => i128::SHAPE,
            FuzzTargetType::Isize => isize::SHAPE,
            FuzzTargetType::F32 => f32::SHAPE,
            FuzzTargetType::F64 => f64::SHAPE,
            FuzzTargetType::Char => char::SHAPE,
            FuzzTargetType::String => String::SHAPE,

            // Compound types (custom structs)
            FuzzTargetType::Point => Point::SHAPE,
            FuzzTargetType::Nested => Nested::SHAPE,
            FuzzTargetType::WithOption => WithOption::SHAPE,
            FuzzTargetType::WithVec => WithVec::SHAPE,

            // Standard library compound types
            FuzzTargetType::OptionU32 => <Option<u32>>::SHAPE,
            FuzzTargetType::OptionString => <Option<String>>::SHAPE,
            FuzzTargetType::VecU8 => <Vec<u8>>::SHAPE,
            FuzzTargetType::VecU32 => <Vec<u32>>::SHAPE,
            FuzzTargetType::VecString => <Vec<String>>::SHAPE,
            FuzzTargetType::BoxU32 => <Box<u32>>::SHAPE,
            FuzzTargetType::BoxString => <Box<String>>::SHAPE,

            // Tuples
            FuzzTargetType::Tuple2U32 => <(u32, u32)>::SHAPE,
            FuzzTargetType::Tuple3Mixed => <(u8, String, bool)>::SHAPE,

            // Unit
            FuzzTargetType::Unit => <()>::SHAPE,

            // Unit enums
            FuzzTargetType::UnitEnumU8 => UnitEnumU8::SHAPE,
            FuzzTargetType::UnitEnumU16 => UnitEnumU16::SHAPE,
            FuzzTargetType::UnitEnumU32 => UnitEnumU32::SHAPE,
            FuzzTargetType::UnitEnumU64 => UnitEnumU64::SHAPE,
            FuzzTargetType::UnitEnumI8 => UnitEnumI8::SHAPE,
            FuzzTargetType::UnitEnumI16 => UnitEnumI16::SHAPE,
            FuzzTargetType::UnitEnumI32 => UnitEnumI32::SHAPE,
            FuzzTargetType::UnitEnumI64 => UnitEnumI64::SHAPE,

            // Data enums
            FuzzTargetType::DataEnumU8 => DataEnumU8::SHAPE,
            FuzzTargetType::DataEnumU16 => DataEnumU16::SHAPE,
            FuzzTargetType::DataEnumU32 => DataEnumU32::SHAPE,
            FuzzTargetType::DataEnumI8 => DataEnumI8::SHAPE,
            FuzzTargetType::DataEnumI32 => DataEnumI32::SHAPE,

            // Mixed enums
            FuzzTargetType::MixedEnumU8 => MixedEnumU8::SHAPE,
            FuzzTargetType::MixedEnumU32 => MixedEnumU32::SHAPE,
            FuzzTargetType::MixedEnumI16 => MixedEnumI16::SHAPE,
            FuzzTargetType::MixedEnumI64 => MixedEnumI64::SHAPE,

            // Nested enums
            FuzzTargetType::NestedEnumU8 => NestedEnumU8::SHAPE,
            FuzzTargetType::NestedEnumU32 => NestedEnumU32::SHAPE,
            FuzzTargetType::NestedEnumI32 => NestedEnumI32::SHAPE,
            FuzzTargetType::DeepEnum => DeepEnum::SHAPE,
        }
    }
}

// ============================================================================
// FuzzInput - the complete fuzzer input
// ============================================================================

/// Input for the fuzzer.
#[derive(Debug, Clone, Arbitrary)]
pub struct FuzzInput {
    /// The type to allocate.
    pub target: FuzzTargetType,
    /// Operations to apply.
    pub ops: Vec<FuzzOp>,
}

// ============================================================================
// Main fuzz target
// ============================================================================

#[cfg(not(feature = "cov"))]
fn main() {
    fuzz!(|input: FuzzInput| {
        run_fuzz(input, false);
    });
}

#[cfg(feature = "cov")]
fn main() {
    use arbitrary::Unstructured;
    use std::io::Read;

    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).unwrap();
    if let Ok(input) = FuzzInput::arbitrary(&mut Unstructured::new(&data)) {
        run_fuzz(input, true);
    }
}

fn run_fuzz(input: FuzzInput, log: bool) {
    if log {
        eprintln!("=== Allocating {:?} ===", input.target);
    }

    // Allocate a Partial for the target type
    let mut partial = match Partial::alloc_shape(input.target.shape()) {
        Ok(p) => p,
        Err(e) => {
            if log {
                eprintln!("  alloc failed: {e:?}");
            }
            return;
        }
    };

    // Apply operations (stop on first error)
    for (i, fuzz_op) in input.ops.into_iter().enumerate() {
        let result = match fuzz_op {
            FuzzOp::Set { path, source } => {
                match source {
                    FuzzSource::Move(value) => {
                        if log {
                            eprintln!("  [{i}] Set path={:?} source=Move({:?})", path, value);
                        }
                        let (ptr, shape) = value.as_ptr_and_shape();
                        // SAFETY: ptr points to value which is valid and initialized,
                        // and remains valid until apply() returns
                        let mov = unsafe { Move::new(ptr, shape) };
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Move(mov),
                        };

                        let result = partial.apply(&[op]);

                        if log {
                            eprintln!("    result: {result:?}");
                        }

                        if result.is_ok() {
                            // Success! The value's bytes have been copied into the Partial.
                            // We must forget the original to avoid double-free.
                            std::mem::forget(value);
                        }
                        // On failure, value is dropped normally (no bytes were copied)
                        result
                    }
                    FuzzSource::Default => {
                        if log {
                            eprintln!("  [{i}] Set path={:?} source=Default", path);
                        }
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Default,
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        result
                    }
                    FuzzSource::Build { len_hint } => {
                        if log {
                            eprintln!("  [{i}] Set path={:?} source=Build({:?})", path, len_hint);
                        }
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Build(facet_reflect2::Build {
                                len_hint: len_hint.map(|h| h as usize),
                            }),
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        result
                    }
                }
            }
            FuzzOp::End => {
                if log {
                    eprintln!("  [{i}] End");
                }
                let result = partial.apply(&[Op::End]);
                if log {
                    eprintln!("    result: {result:?}");
                }
                result
            }
        };

        if result.is_err() {
            break;
        }
    }

    if log {
        eprintln!("=== Dropping partial ===");
    }
    // Drop partial - this exercises the Drop impl
    // If it doesn't panic or crash, we're good
}
