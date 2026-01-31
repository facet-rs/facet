#[cfg(not(feature = "standalone"))]
use afl::fuzz;
use arbitrary::Arbitrary;
use facet::Facet;
use facet_core::{PtrConst, Shape};
use facet_reflect2::{Build, Imm, Op, Partial, Path, Source};
use std::collections::HashMap;

// ============================================================================
// Compound types for fuzzing (these need Facet derive)
// ============================================================================

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct Point {
    x: Box<i32>,
    y: Box<i32>,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct Nested {
    name: String,
    point: Point,
    value: Box<u64>,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct WithOption {
    required: Box<u32>,
    optional: Option<String>,
}

#[derive(Clone, Debug, Facet, Arbitrary)]
pub struct WithVec {
    items: Vec<Box<u32>>,
    label: String,
}

// ============================================================================
// Enum types for fuzzing
// ============================================================================

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
// Macro to generate FuzzValue and FuzzTargetType
// ============================================================================

macro_rules! fuzz_types {
    (
        // Types that can be both values and targets (implement Clone + Arbitrary)
        values {
            $(
                $val_variant:ident => $val_type:ty
            ),* $(,)?
        }
        // Types that can only be targets (don't implement Clone or Arbitrary)
        targets_only {
            $(
                $tgt_variant:ident => $tgt_type:ty
            ),* $(,)?
        }
    ) => {
        /// A value that can be used in fuzzing.
        /// Each variant holds an owned value that we can get a pointer to.
        #[derive(Clone, Arbitrary)]
        pub enum FuzzValue {
            $( $val_variant($val_type), )*
        }

        impl FuzzValue {
            /// Get a pointer and shape for this value.
            /// The pointer is only valid while self is alive.
            fn as_ptr_and_shape(&self) -> (PtrConst, &'static Shape) {
                match self {
                    $( FuzzValue::$val_variant(v) => (PtrConst::new(v), <$val_type>::SHAPE), )*
                }
            }
        }

        impl std::fmt::Debug for FuzzValue {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( FuzzValue::$val_variant(v) => write!(f, "{}({:?})", stringify!($val_variant), v), )*
                }
            }
        }

        /// The target type to allocate.
        #[derive(Debug, Clone, Copy, Arbitrary)]
        pub enum FuzzTargetType {
            $( $val_variant, )*
            $( $tgt_variant, )*
        }

        impl FuzzTargetType {
            fn shape(&self) -> &'static Shape {
                match self {
                    $( FuzzTargetType::$val_variant => <$val_type>::SHAPE, )*
                    $( FuzzTargetType::$tgt_variant => <$tgt_type>::SHAPE, )*
                }
            }
        }
    };
}

fuzz_types! {
    values {
        // Scalars
        Bool => bool,
        U8 => u8,
        U16 => u16,
        U32 => u32,
        U64 => u64,
        U128 => u128,
        Usize => usize,
        I8 => i8,
        I16 => i16,
        I32 => i32,
        I64 => i64,
        I128 => i128,
        Isize => isize,
        F32 => f32,
        F64 => f64,
        Char => char,
        String => String,

        // Custom structs
        Point => Point,
        Nested => Nested,
        WithOption => WithOption,
        WithVec => WithVec,

        // Option
        OptionU32 => Option<u32>,
        OptionString => Option<String>,

        // Vec
        VecU8 => Vec<u8>,
        VecU32 => Vec<u32>,
        VecString => Vec<String>,
        VecPoint => Vec<Point>,
        VecVecU32 => Vec<Vec<u32>>,

        // Box
        BoxU32 => Box<u32>,
        BoxString => Box<String>,
        BoxPoint => Box<Point>,

        // Rc
        RcU32 => std::rc::Rc<u32>,
        RcString => std::rc::Rc<String>,
        RcPoint => std::rc::Rc<Point>,

        // Arc
        ArcU32 => std::sync::Arc<u32>,
        ArcString => std::sync::Arc<String>,
        ArcPoint => std::sync::Arc<Point>,

        // Tuples
        Tuple2U32 => (u32, u32),
        Tuple3Mixed => (u8, String, bool),

        // Unit
        Unit => (),

        // Unit enums
        UnitEnumU8 => UnitEnumU8,
        UnitEnumU16 => UnitEnumU16,
        UnitEnumU32 => UnitEnumU32,
        UnitEnumU64 => UnitEnumU64,
        UnitEnumI8 => UnitEnumI8,
        UnitEnumI16 => UnitEnumI16,
        UnitEnumI32 => UnitEnumI32,
        UnitEnumI64 => UnitEnumI64,

        // Data enums
        DataEnumU8 => DataEnumU8,
        DataEnumU16 => DataEnumU16,
        DataEnumU32 => DataEnumU32,
        DataEnumI8 => DataEnumI8,
        DataEnumI32 => DataEnumI32,

        // Mixed enums
        MixedEnumU8 => MixedEnumU8,
        MixedEnumU32 => MixedEnumU32,
        MixedEnumI16 => MixedEnumI16,
        MixedEnumI64 => MixedEnumI64,

        // Nested enums
        NestedEnumU8 => NestedEnumU8,
        NestedEnumU32 => NestedEnumU32,
        NestedEnumI32 => NestedEnumI32,
        DeepEnum => DeepEnum,

        // HashMaps
        HashMapStringU32 => HashMap<String, u32>,
        HashMapStringString => HashMap<String, String>,
        HashMapU32String => HashMap<u32, String>,
        HashMapStringPoint => HashMap<String, Point>,
        HashMapStringVecU32 => HashMap<String, Vec<u32>>,
        HashMapStringBoxU32 => HashMap<String, Box<u32>>,
    }
    targets_only {
        // Mutex (no Clone/Arbitrary)
        MutexU32 => std::sync::Mutex<u32>,
        MutexString => std::sync::Mutex<String>,
        MutexPoint => std::sync::Mutex<Point>,

        // RwLock (no Clone/Arbitrary)
        RwLockU32 => std::sync::RwLock<u32>,
        RwLockString => std::sync::RwLock<String>,
        RwLockPoint => std::sync::RwLock<Point>,
    }
}

// ============================================================================
// FuzzSource - how to fill a value
// ============================================================================

/// Source for a fuzz 'Set' operation
#[derive(Clone, Arbitrary)]
pub enum FuzzSource {
    /// Immediate value (copy bytes from the FuzzValue).
    Imm(FuzzValue),
    /// Use the type's default value.
    Default,
    /// Build incrementally - pushes a frame.
    Build { len_hint: Option<u8> },
}

impl std::fmt::Debug for FuzzSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzSource::Imm(v) => write!(f, "Imm({:?})", v),
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
    /// Push an element to the current list.
    Push { source: FuzzSource },
    /// Insert a key-value pair into the current map.
    Insert { key: FuzzValue, value: FuzzSource },
    /// End the current frame.
    End,
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

#[cfg(not(feature = "standalone"))]
fn main() {
    fuzz!(|input: FuzzInput| {
        run_fuzz(input, false);
    });
}

#[cfg(feature = "standalone")]
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
                    FuzzSource::Imm(value) => {
                        if log {
                            eprintln!("  [{i}] Set dst={:?} src=Imm({:?})", path, value);
                        }
                        let (ptr, shape) = value.as_ptr_and_shape();
                        // SAFETY: ptr points to value which is valid and initialized,
                        // and remains valid until apply() returns
                        let imm = unsafe { Imm::new(ptr, shape) };
                        let op = Op::Set {
                            dst: path.to_path(),
                            src: Source::Imm(imm),
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
                            eprintln!("  [{i}] Set dst={:?} src=Default", path);
                        }
                        let op = Op::Set {
                            dst: path.to_path(),
                            src: Source::Default,
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        result
                    }
                    FuzzSource::Build { len_hint } => {
                        if log {
                            eprintln!("  [{i}] Set dst={:?} src=Build({:?})", path, len_hint);
                        }
                        let op = Op::Set {
                            dst: path.to_path(),
                            src: Source::Build(Build {
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
            FuzzOp::Push { source } => {
                match source {
                    FuzzSource::Imm(value) => {
                        if log {
                            eprintln!("  [{i}] Push src=Imm({:?})", value);
                        }
                        let (ptr, shape) = value.as_ptr_and_shape();
                        // SAFETY: ptr points to value which is valid and initialized,
                        // and remains valid until apply() returns
                        let imm = unsafe { Imm::new(ptr, shape) };
                        let op = Op::Push {
                            src: Source::Imm(imm),
                        };

                        let result = partial.apply(&[op]);

                        if log {
                            eprintln!("    result: {result:?}");
                        }

                        if result.is_ok() {
                            // Success! The value's bytes have been copied into the list.
                            // We must forget the original to avoid double-free.
                            std::mem::forget(value);
                        }
                        // On failure, value is dropped normally (no bytes were copied)
                        result
                    }
                    FuzzSource::Default => {
                        if log {
                            eprintln!("  [{i}] Push src=Default");
                        }
                        let op = Op::Push {
                            src: Source::Default,
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        result
                    }
                    FuzzSource::Build { len_hint } => {
                        if log {
                            eprintln!("  [{i}] Push src=Build({:?})", len_hint);
                        }
                        let op = Op::Push {
                            src: Source::Build(Build {
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
            FuzzOp::Insert { key, value } => {
                let (key_ptr, key_shape) = key.as_ptr_and_shape();
                // SAFETY: key_ptr points to key which is valid and initialized
                let key_imm = unsafe { Imm::new(key_ptr, key_shape) };

                match value {
                    FuzzSource::Imm(val) => {
                        if log {
                            eprintln!("  [{i}] Insert key={:?} value=Imm({:?})", key, val);
                        }
                        let (val_ptr, val_shape) = val.as_ptr_and_shape();
                        // SAFETY: val_ptr points to val which is valid and initialized
                        let val_imm = unsafe { Imm::new(val_ptr, val_shape) };
                        let op = Op::Insert {
                            key: key_imm,
                            value: Source::Imm(val_imm),
                        };

                        let result = partial.apply(&[op]);

                        if log {
                            eprintln!("    result: {result:?}");
                        }

                        if result.is_ok() {
                            // Success! Both key and value bytes have been copied into the map.
                            // We must forget the originals to avoid double-free.
                            std::mem::forget(key);
                            std::mem::forget(val);
                        }
                        result
                    }
                    FuzzSource::Default => {
                        if log {
                            eprintln!("  [{i}] Insert key={:?} value=Default", key);
                        }
                        let op = Op::Insert {
                            key: key_imm,
                            value: Source::Default,
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        if result.is_ok() {
                            // Key was copied into the map
                            std::mem::forget(key);
                        }
                        result
                    }
                    FuzzSource::Build { len_hint } => {
                        if log {
                            eprintln!("  [{i}] Insert key={:?} value=Build({:?})", key, len_hint);
                        }
                        let op = Op::Insert {
                            key: key_imm,
                            value: Source::Build(Build {
                                len_hint: len_hint.map(|h| h as usize),
                            }),
                        };
                        let result = partial.apply(&[op]);
                        if log {
                            eprintln!("    result: {result:?}");
                        }
                        if result.is_ok() {
                            // Key was copied into the pending key storage
                            std::mem::forget(key);
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
