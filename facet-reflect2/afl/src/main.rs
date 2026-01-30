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
        }
    }
}

impl std::fmt::Debug for FuzzValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzValue::Bool(v) => write!(f, "Bool({v:?})"),
            FuzzValue::U8(v) => write!(f, "U8({v})"),
            FuzzValue::U16(v) => write!(f, "U16({v})"),
            FuzzValue::U32(v) => write!(f, "U32({v})"),
            FuzzValue::U64(v) => write!(f, "U64({v})"),
            FuzzValue::U128(v) => write!(f, "U128({v})"),
            FuzzValue::Usize(v) => write!(f, "Usize({v})"),
            FuzzValue::I8(v) => write!(f, "I8({v})"),
            FuzzValue::I16(v) => write!(f, "I16({v})"),
            FuzzValue::I32(v) => write!(f, "I32({v})"),
            FuzzValue::I64(v) => write!(f, "I64({v})"),
            FuzzValue::I128(v) => write!(f, "I128({v})"),
            FuzzValue::Isize(v) => write!(f, "Isize({v})"),
            FuzzValue::F32(v) => write!(f, "F32({v})"),
            FuzzValue::F64(v) => write!(f, "F64({v})"),
            FuzzValue::Char(v) => write!(f, "Char({v:?})"),
            FuzzValue::String(v) => write!(f, "String({v:?})"),
            FuzzValue::Point(v) => write!(f, "Point({v:?})"),
            FuzzValue::Nested(v) => write!(f, "Nested({v:?})"),
            FuzzValue::WithOption(v) => write!(f, "WithOption({v:?})"),
            FuzzValue::WithVec(v) => write!(f, "WithVec({v:?})"),
            FuzzValue::OptionU32(v) => write!(f, "OptionU32({v:?})"),
            FuzzValue::OptionString(v) => write!(f, "OptionString({v:?})"),
            FuzzValue::VecU8(v) => write!(f, "VecU8({v:?})"),
            FuzzValue::VecU32(v) => write!(f, "VecU32({v:?})"),
            FuzzValue::VecString(v) => write!(f, "VecString({v:?})"),
            FuzzValue::BoxU32(v) => write!(f, "BoxU32({v:?})"),
            FuzzValue::BoxString(v) => write!(f, "BoxString({v:?})"),
            FuzzValue::Tuple2U32(v) => write!(f, "Tuple2U32({v:?})"),
            FuzzValue::Tuple3Mixed(v) => write!(f, "Tuple3Mixed({v:?})"),
            FuzzValue::Unit(v) => write!(f, "Unit({v:?})"),
        }
    }
}

// ============================================================================
// FuzzSource - how to fill a value
// ============================================================================

/// Source for a fuzz operation.
#[derive(Clone, Debug, Arbitrary)]
pub enum FuzzSource {
    /// Move a value (copy bytes from the FuzzValue).
    Move(FuzzValue),
    /// Use the type's default value.
    Default,
    /// Build incrementally - pushes a frame.
    Build,
}

// ============================================================================
// FuzzPath - path into nested structures
// ============================================================================

/// A path for accessing nested fields.
/// Uses small indices to keep fuzzing efficient.
#[derive(Clone, Debug, Arbitrary)]
pub struct FuzzPath {
    /// Field indices (limited to keep paths reasonable).
    /// Using u8 since structs rarely have more than 256 fields.
    pub indices: Vec<u8>,
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

fn main() {
    fuzz!(|input: FuzzInput| {
        run_fuzz(input);
    });
}

fn run_fuzz(input: FuzzInput) {
    // Allocate a Partial for the target type
    let mut partial = match Partial::alloc_shape(input.target.shape()) {
        Ok(p) => p,
        Err(_) => return, // Allocation failed, that's fine
    };

    // Apply operations
    for fuzz_op in input.ops {
        match fuzz_op {
            FuzzOp::Set { path, source } => {
                match source {
                    FuzzSource::Move(value) => {
                        let (ptr, shape) = value.as_ptr_and_shape();
                        // SAFETY: ptr points to value which is valid and initialized,
                        // and remains valid until apply() returns
                        let mov = unsafe { Move::new(ptr, shape) };
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Move(mov),
                        };

                        // Apply may fail (e.g., shape mismatch, invalid path)
                        let result = partial.apply(&[op]);

                        if result.is_ok() {
                            // Success! The value's bytes have been copied into the Partial.
                            // We must forget the original to avoid double-free.
                            std::mem::forget(value);
                        }
                        // On failure, value is dropped normally (no bytes were copied)
                    }
                    FuzzSource::Default => {
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Default,
                        };
                        // Apply may fail (e.g., type doesn't implement Default, invalid path)
                        let _ = partial.apply(&[op]);
                    }
                    FuzzSource::Build => {
                        let op = Op::Set {
                            path: path.to_path(),
                            source: Source::Build(facet_reflect2::Build { len_hint: None }),
                        };
                        // Apply may fail (e.g., empty path, invalid path)
                        let _ = partial.apply(&[op]);
                    }
                }
            }
            FuzzOp::End => {
                // End may fail (e.g., at root, incomplete children)
                let _ = partial.apply(&[Op::End]);
            }
        }
    }

    // Drop partial - this exercises the Drop impl
    // If it doesn't panic or crash, we're good
}
