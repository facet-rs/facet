use afl::fuzz;
use arbitrary::Arbitrary;
use facet_core::{Facet, PtrConst, Shape};
use facet_reflect2::{Move, Op, Partial, Path, Source};

/// A value that can be used in fuzzing.
/// Each variant holds an owned value that we can get a pointer to.
#[derive(Clone, Arbitrary)]
pub enum FuzzValue {
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
}

impl FuzzValue {
    /// Get a pointer and shape for this value.
    /// The pointer is only valid while self is alive.
    fn as_ptr_and_shape(&self) -> (PtrConst, &'static Shape) {
        match self {
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
        }
    }
}

/// Source for a fuzz operation.
#[derive(Clone, Arbitrary)]
pub enum FuzzSource {
    /// Move a value (copy bytes from the FuzzValue).
    Move(FuzzValue),
    // TODO: Build, Default when implemented
}

impl std::fmt::Debug for FuzzSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzSource::Move(v) => write!(f, "Move({v:?})"),
        }
    }
}

/// A fuzzing operation that maps to Op.
#[derive(Clone, Arbitrary)]
pub enum FuzzOp {
    /// Set a value at the root (empty path for now).
    Set(FuzzSource),
}

impl std::fmt::Debug for FuzzOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzOp::Set(source) => write!(f, "Set({source:?})"),
        }
    }
}

/// The target type to allocate.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum FuzzTargetType {
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
}

impl FuzzTargetType {
    fn shape(&self) -> &'static Shape {
        match self {
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
        }
    }
}

/// Input for the fuzzer.
#[derive(Debug, Clone, Arbitrary)]
pub struct FuzzInput {
    /// The type to allocate.
    pub target: FuzzTargetType,
    /// Operations to apply.
    pub ops: Vec<FuzzOp>,
}

fn main() {
    fuzz!(|input: FuzzInput| {
        // Allocate a Partial for the target type
        let mut partial = match Partial::alloc_shape(input.target.shape()) {
            Ok(p) => p,
            Err(_) => return, // Allocation failed, that's fine
        };

        // Apply operations
        // We need to own the FuzzValues so we can forget them after successful moves
        for fuzz_op in input.ops {
            match fuzz_op {
                FuzzOp::Set(source) => {
                    match source {
                        FuzzSource::Move(value) => {
                            let (ptr, shape) = value.as_ptr_and_shape();
                            // SAFETY: ptr points to value which is valid and initialized,
                            // and remains valid until apply() returns
                            let mov = unsafe { Move::new(ptr, shape) };
                            let op = Op::Set {
                                path: Path::default(),
                                source: Source::Move(mov),
                            };

                            // Apply may fail (e.g., shape mismatch)
                            let result = partial.apply(&[op]);

                            if result.is_ok() {
                                // Success! The value's bytes have been copied into the Partial.
                                // We must forget the original to avoid double-free.
                                std::mem::forget(value);
                            }
                            // On failure, value is dropped normally (no bytes were copied)
                        }
                    }
                }
            }
        }

        // Drop partial - this exercises the Drop impl
        // If it doesn't panic or crash, we're good
    });
}
