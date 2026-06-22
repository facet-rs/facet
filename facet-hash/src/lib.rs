//! Reusable Weavy-backed hashing plans for Facet values.
//!
//! `HashPlan` lowers a Facet shape once, then hashes values of that shape by
//! interpreting the lowered program. The interpreter reads typed fields through
//! Facet metadata, so padding bytes are never part of the hash.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use facet_core::{
    ArrayDef, ConstTypeId, Def, Facet, ListDef, MapDef, OptionDef, PointerDef, PtrConst, PtrMut,
    ResultDef, ScalarType, SetDef, Shape, SliceDef, StructKind, Type, UserType,
};
use weavy::ir::{
    ControlOp, EffectContract, EffectResource, EffectStats, IntrinsicDescriptor, IntrinsicOp,
    LoweredEffectStats, MemoryRegion, TypedMemoryAccess, WeavyOp,
};
use weavy::{BlockRef, Control, DenseLowered, Lowered, Program, RunError, RunStats, Step};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HashBlockId {
    Shape(&'static Shape),
}

type BlockId = HashBlockId;
type ExecBlock = BlockRef;
type SymbolicOp = WeavyOp<BlockId, HashIntrinsic<BlockId>>;
type ExecOp = WeavyOp<ExecBlock, HashIntrinsic<ExecBlock>>;

/// A reusable Weavy-backed hashing plan for `T`.
///
/// Build a plan once, then use [`HashPlan::hash`] for repeated hashing without
/// repeatedly walking `T::SHAPE`.
#[derive(Debug)]
pub struct HashPlan<T> {
    lowered: DenseLowered<ExecOp>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> HashPlan<T>
where
    T: Facet<'static>,
{
    /// Lower `T::SHAPE` into value-hash bytecode.
    ///
    /// Value mode assumes callers compare hashes produced by the same plan
    /// shape. It avoids repeatedly hashing type ids and field names, matching
    /// the repeated same-type use case.
    pub fn build() -> Result<Self, HashError> {
        Self::build_with_mode(HashMode::Value)
    }

    /// Lower `T::SHAPE` into structural-hash bytecode.
    ///
    /// Structural mode includes type ids, struct kinds, and field names in the
    /// stream. It costs more, but keeps hashes discriminated across different
    /// Facet shapes.
    pub fn build_structural() -> Result<Self, HashError> {
        Self::build_with_mode(HashMode::Structural)
    }

    /// Lower `T::SHAPE` into hash bytecode using `mode`.
    pub fn build_with_mode(mode: HashMode) -> Result<Self, HashError> {
        let symbolic = Lowering::new(mode).lower(T::SHAPE)?;
        Ok(Self {
            lowered: resolve_hash_lowered(symbolic)?,
            _marker: PhantomData,
        })
    }

    /// Hash `value` into `hasher` through this pre-lowered plan.
    pub fn hash<H>(&self, value: &T, hasher: &mut H) -> Result<(), HashError>
    where
        H: Hasher,
    {
        let ptr = PtrConst::new_sized(value as *const T);
        let mut interp = HashInterp::new(ptr, hasher);
        weavy::run_dense(&self.lowered, &mut interp).map_err(run_error)
    }

    /// Hash `value` and return Weavy runner counters.
    pub fn hash_with_stats<H>(&self, value: &T, hasher: &mut H) -> Result<RunStats, HashError>
    where
        H: Hasher,
    {
        let ptr = PtrConst::new_sized(value as *const T);
        let mut interp = HashInterp::new(ptr, hasher);
        weavy::run_dense_with_stats(&self.lowered, &mut interp).map_err(run_error)
    }

    /// Return conservative effect counters for the lowered hash program.
    pub fn effect_stats(&self) -> LoweredEffectStats {
        hash_lowered_effect_stats(&self.lowered)
    }

    /// Hash `value` with [`std::collections::hash_map::DefaultHasher`].
    pub fn hash64(&self, value: &T) -> Result<u64, HashError> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(value, &mut hasher)?;
        Ok(hasher.finish())
    }
}

/// Hash stream shape for a [`HashPlan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashMode {
    /// Hash only value content under the already-known plan shape.
    Value,
    /// Include type ids, struct kinds, and field names while hashing.
    Structural,
}

/// Build a temporary plan for `T` and hash `value` into `hasher`.
///
/// Use [`HashPlan`] directly when hashing more than one value of the same type.
pub fn hash_into<T, H>(value: &T, hasher: &mut H) -> Result<(), HashError>
where
    T: Facet<'static>,
    H: Hasher,
{
    HashPlan::<T>::build()?.hash(value, hasher)
}

/// Build a temporary plan for `T` and hash `value` with
/// [`std::collections::hash_map::DefaultHasher`].
pub fn hash64<T>(value: &T) -> Result<u64, HashError>
where
    T: Facet<'static>,
{
    HashPlan::<T>::build()?.hash64(value)
}

fn run_error(err: RunError<ExecBlock, HashError>) -> HashError {
    match err {
        RunError::Step(err) => err,
        RunError::MissingBlock(block) => HashError::MissingBlock { block },
    }
}

/// Error returned while building or running a hash plan.
#[derive(Debug)]
pub enum HashError {
    /// The shape cannot be lowered by this backend yet.
    Unsupported {
        /// Shape being lowered.
        shape: &'static Shape,
        /// Missing feature or metadata hook.
        feature: &'static str,
    },
    /// A symbolic block survived lowering into executable bytecode.
    MissingBlock {
        /// Dense block reference that was not present.
        block: BlockRef,
    },
    /// The lowered bytecode contains an op this interpreter does not emit.
    MalformedProgram {
        /// Human-readable invariant violation.
        reason: &'static str,
    },
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashError::Unsupported { shape, feature } => {
                write!(f, "facet-hash does not yet support {feature} for {shape}")
            }
            HashError::MissingBlock { block } => {
                write!(f, "facet-hash program referenced missing block {block:?}")
            }
            HashError::MalformedProgram { reason } => {
                write!(f, "facet-hash lowered an invalid program: {reason}")
            }
        }
    }
}

impl std::error::Error for HashError {}

#[derive(Clone, Debug)]
enum HashIntrinsic<Block> {
    Shape(&'static Shape),
    Scalar {
        shape: &'static Shape,
        scalar: ScalarType,
    },
    Struct {
        mode: HashMode,
        kind: StructKind,
        fields: Box<[FieldPlan<Block>]>,
    },
    Option {
        option: OptionDef,
        some: ChildPlan<Block>,
    },
    Result {
        result: ResultDef,
        ok: ChildPlan<Block>,
        err: ChildPlan<Block>,
    },
    List {
        list_shape: &'static Shape,
        list: ListDef,
        element_layout: Layout,
        element: ChildPlan<Block>,
    },
    Array {
        array: ArrayDef,
        element_layout: Layout,
        element: ChildPlan<Block>,
    },
    Slice {
        slice: SliceDef,
        element_layout: Layout,
        element: ChildPlan<Block>,
    },
    Set {
        set: SetDef,
        element: ChildPlan<Block>,
    },
    Map {
        map: MapDef,
        key: ChildPlan<Block>,
        value: ChildPlan<Block>,
    },
    Pointer {
        pointer: PointerDef,
        pointee: ChildPlan<Block>,
    },
}

#[derive(Clone, Debug)]
enum ChildPlan<Block> {
    Scalar {
        include_shape: bool,
        shape: &'static Shape,
        scalar: ScalarType,
    },
    Program(Program<WeavyOp<Block, HashIntrinsic<Block>>>),
}

impl<Block> IntrinsicOp for HashIntrinsic<Block> {
    fn descriptor(&self) -> IntrinsicDescriptor {
        let name = match self {
            HashIntrinsic::Shape(_) => "shape",
            HashIntrinsic::Scalar { .. } => "scalar",
            HashIntrinsic::Struct { .. } => "struct",
            HashIntrinsic::Option { .. } => "option",
            HashIntrinsic::Result { .. } => "result",
            HashIntrinsic::List { .. } => "list",
            HashIntrinsic::Array { .. } => "array",
            HashIntrinsic::Slice { .. } => "slice",
            HashIntrinsic::Set { .. } => "set",
            HashIntrinsic::Map { .. } => "map",
            HashIntrinsic::Pointer { .. } => "pointer",
        };
        IntrinsicDescriptor {
            dialect: "facet-hash",
            name,
        }
    }

    fn effect(&self) -> EffectContract {
        match self {
            HashIntrinsic::Shape(_) => hash_sink_effect(),
            HashIntrinsic::Scalar { shape, scalar } => scalar_hash_effect(shape, *scalar),
            HashIntrinsic::Struct { mode, fields, .. } => {
                let mut effect = if *mode == HashMode::Structural || !fields.is_empty() {
                    hash_sink_effect()
                } else {
                    EffectContract::new()
                };
                if !fields.is_empty() {
                    effect = effect.barrier();
                }
                effect
            }
            HashIntrinsic::Option { .. } | HashIntrinsic::Result { .. } => {
                thunked_read_effect().barrier()
            }
            HashIntrinsic::List { .. }
            | HashIntrinsic::Array { .. }
            | HashIntrinsic::Slice { .. }
            | HashIntrinsic::Pointer { .. } => thunked_read_effect().barrier(),
            HashIntrinsic::Set { .. } | HashIntrinsic::Map { .. } => {
                thunked_read_effect().may_allocate().barrier()
            }
        }
    }
}

fn hash_sink_effect() -> EffectContract {
    EffectContract::new().write_resource(EffectResource::Sink("hash"))
}

fn scalar_hash_effect(shape: &'static Shape, scalar: ScalarType) -> EffectContract {
    let effect = hash_sink_effect();
    match scalar {
        ScalarType::Unit => effect,
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => effect
            .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::Read)
            .ordered(),
        #[cfg(feature = "net")]
        ScalarType::SocketAddr
        | ScalarType::IpAddr
        | ScalarType::Ipv4Addr
        | ScalarType::Ipv6Addr => effect
            .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::Read)
            .ordered(),
        _ => effect.typed_memory(shape_memory_region(shape), TypedMemoryAccess::Read),
    }
}

fn thunked_read_effect() -> EffectContract {
    hash_sink_effect()
        .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::Read)
        .calls_user_code()
}

fn shape_memory_region(shape: &'static Shape) -> MemoryRegion {
    match shape.layout.sized_layout() {
        Ok(layout) => MemoryRegion::base_relative(0, layout.size()),
        Err(_) => MemoryRegion::unknown(),
    }
}

#[derive(Clone, Debug)]
struct FieldPlan<Block> {
    name: &'static str,
    offset: usize,
    child: ChildPlan<Block>,
}

struct Lowering {
    lowered: Lowered<BlockId, SymbolicOp>,
    in_progress: Vec<&'static Shape>,
    needed_blocks: BTreeSet<BlockId>,
    mode: HashMode,
}

impl Lowering {
    fn new(mode: HashMode) -> Self {
        Self {
            lowered: Lowered {
                program: Vec::new(),
                blocks: BTreeMap::new(),
            },
            in_progress: Vec::new(),
            needed_blocks: BTreeSet::new(),
            mode,
        }
    }

    fn lower(mut self, root: &'static Shape) -> Result<Lowered<BlockId, SymbolicOp>, HashError> {
        self.lowered.program = self.lower_shape(root)?;
        Ok(self.lowered)
    }

    fn lower_shape(&mut self, shape: &'static Shape) -> Result<Program<SymbolicOp>, HashError> {
        let block_id = HashBlockId::Shape(shape);
        if self.lowered.blocks.contains_key(&block_id) || self.in_progress.contains(&shape) {
            if self.in_progress.contains(&shape) {
                self.needed_blocks.insert(block_id);
            }
            return Ok(vec![WeavyOp::Control(ControlOp::CallBlock {
                block: block_id,
                base_offset: 0,
            })]);
        }

        self.in_progress.push(shape);
        let program = self.lower_shape_body(shape)?;
        self.in_progress.pop();
        if self.needed_blocks.remove(&block_id) {
            self.lowered.blocks.insert(block_id, program.clone());
        }
        Ok(program)
    }

    fn lower_shape_body(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Program<SymbolicOp>, HashError> {
        let mut program = Vec::new();
        if self.mode == HashMode::Structural {
            program.push(WeavyOp::Intrinsic(HashIntrinsic::Shape(shape)));
        }

        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            if !supported_scalar(scalar) {
                return Err(unsupported(shape, "scalar"));
            }
            program.push(WeavyOp::Intrinsic(HashIntrinsic::Scalar { shape, scalar }));
            return Ok(program);
        }

        match shape.def {
            Def::Option(option) => {
                let some = self.lower_child(option.t())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Option { option, some }));
            }
            Def::Result(result) => {
                let ok = self.lower_child(result.t())?;
                let err = self.lower_child(result.e())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Result {
                    result,
                    ok,
                    err,
                }));
            }
            Def::List(list) => {
                if list.vtable.as_ptr.is_none() {
                    return Err(unsupported(shape, "list as_ptr"));
                }
                let element_layout = sized_layout(list.t())?;
                let element = self.lower_child(list.t())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::List {
                    list_shape: shape,
                    list,
                    element_layout,
                    element,
                }));
            }
            Def::Array(array) => {
                let element_layout = sized_layout(array.t())?;
                let element = self.lower_child(array.t())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Array {
                    array,
                    element_layout,
                    element,
                }));
            }
            Def::Slice(slice) => {
                let element_layout = sized_layout(slice.t())?;
                let element = self.lower_child(slice.t())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Slice {
                    slice,
                    element_layout,
                    element,
                }));
            }
            Def::Set(set) => {
                if set.vtable.iter_vtable.init_with_value.is_none() {
                    return Err(unsupported(shape, "set iterator init"));
                }
                let element = self.lower_child(set.t())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Set { set, element }));
            }
            Def::Map(map) => {
                if map.vtable.iter_vtable.init_with_value.is_none() {
                    return Err(unsupported(shape, "map iterator init"));
                }
                let key = self.lower_child(map.k())?;
                let value = self.lower_child(map.v())?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Map { map, key, value }));
            }
            Def::Pointer(pointer) => {
                let pointee = pointer
                    .pointee()
                    .ok_or_else(|| unsupported(shape, "opaque pointer"))?;
                if pointer.vtable.borrow_fn.is_none() {
                    return Err(unsupported(shape, "pointer borrow"));
                }
                let pointee = self.lower_child(pointee)?;
                program.push(WeavyOp::Intrinsic(HashIntrinsic::Pointer {
                    pointer,
                    pointee,
                }));
            }
            _ => match shape.ty {
                Type::User(UserType::Struct(struct_type)) => {
                    let mut fields = Vec::with_capacity(struct_type.fields.len());
                    for field in struct_type.fields {
                        if field.is_metadata() {
                            continue;
                        }
                        fields.push(FieldPlan {
                            name: field.name,
                            offset: field.offset,
                            child: self.lower_child(field.shape())?,
                        });
                    }
                    program.push(WeavyOp::Intrinsic(HashIntrinsic::Struct {
                        mode: self.mode,
                        kind: struct_type.kind,
                        fields: fields.into_boxed_slice(),
                    }));
                }
                Type::User(UserType::Enum(_)) => {
                    return Err(unsupported(shape, "enum"));
                }
                _ => return Err(unsupported(shape, "shape")),
            },
        }

        Ok(program)
    }

    fn lower_child(&mut self, shape: &'static Shape) -> Result<ChildPlan<BlockId>, HashError> {
        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            if !supported_scalar(scalar) {
                return Err(unsupported(shape, "scalar"));
            }
            return Ok(ChildPlan::Scalar {
                include_shape: self.mode == HashMode::Structural,
                shape,
                scalar,
            });
        }

        Ok(ChildPlan::Program(self.lower_shape(shape)?))
    }
}

fn resolve_hash_lowered(
    symbolic: Lowered<BlockId, SymbolicOp>,
) -> Result<DenseLowered<ExecOp>, HashError> {
    let refs = symbolic.block_refs();
    let program = resolve_hash_program(symbolic.program, &refs)?;
    let mut blocks = Vec::with_capacity(symbolic.blocks.len());
    for (_, block) in symbolic.blocks {
        blocks.push(resolve_hash_program(block, &refs)?);
    }
    Ok(DenseLowered::new(program, blocks))
}

fn resolve_hash_program(
    program: Program<SymbolicOp>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Program<ExecOp>, HashError> {
    program
        .into_iter()
        .map(|op| resolve_hash_op(op, refs))
        .collect()
}

fn resolve_hash_op(
    op: SymbolicOp,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ExecOp, HashError> {
    match op {
        WeavyOp::Intrinsic(intrinsic) => {
            Ok(WeavyOp::Intrinsic(resolve_hash_intrinsic(intrinsic, refs)?))
        }
        WeavyOp::Control(ControlOp::CallBlock { block, base_offset }) => {
            Ok(WeavyOp::Control(ControlOp::CallBlock {
                block: resolve_block_ref(block, refs)?,
                base_offset,
            }))
        }
        WeavyOp::Control(ControlOp::Return) => Ok(WeavyOp::Control(ControlOp::Return)),
        WeavyOp::Memory(op) => Ok(WeavyOp::Memory(op)),
        WeavyOp::Init(op) => Ok(WeavyOp::Init(op)),
        WeavyOp::Aggregate(_) => Err(HashError::MalformedProgram {
            reason: "hash lowering does not emit canonical aggregate ops",
        }),
        _ => Err(HashError::MalformedProgram {
            reason: "hash lowering saw an unknown canonical op",
        }),
    }
}

fn resolve_hash_intrinsic(
    intrinsic: HashIntrinsic<BlockId>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<HashIntrinsic<ExecBlock>, HashError> {
    Ok(match intrinsic {
        HashIntrinsic::Shape(shape) => HashIntrinsic::Shape(shape),
        HashIntrinsic::Scalar { shape, scalar } => HashIntrinsic::Scalar { shape, scalar },
        HashIntrinsic::Struct { mode, kind, fields } => HashIntrinsic::Struct {
            mode,
            kind,
            fields: resolve_field_plans(fields, refs)?,
        },
        HashIntrinsic::Option { option, some } => HashIntrinsic::Option {
            option,
            some: resolve_child_plan(some, refs)?,
        },
        HashIntrinsic::Result { result, ok, err } => HashIntrinsic::Result {
            result,
            ok: resolve_child_plan(ok, refs)?,
            err: resolve_child_plan(err, refs)?,
        },
        HashIntrinsic::List {
            list_shape,
            list,
            element_layout,
            element,
        } => HashIntrinsic::List {
            list_shape,
            list,
            element_layout,
            element: resolve_child_plan(element, refs)?,
        },
        HashIntrinsic::Array {
            array,
            element_layout,
            element,
        } => HashIntrinsic::Array {
            array,
            element_layout,
            element: resolve_child_plan(element, refs)?,
        },
        HashIntrinsic::Slice {
            slice,
            element_layout,
            element,
        } => HashIntrinsic::Slice {
            slice,
            element_layout,
            element: resolve_child_plan(element, refs)?,
        },
        HashIntrinsic::Set { set, element } => HashIntrinsic::Set {
            set,
            element: resolve_child_plan(element, refs)?,
        },
        HashIntrinsic::Map { map, key, value } => HashIntrinsic::Map {
            map,
            key: resolve_child_plan(key, refs)?,
            value: resolve_child_plan(value, refs)?,
        },
        HashIntrinsic::Pointer { pointer, pointee } => HashIntrinsic::Pointer {
            pointer,
            pointee: resolve_child_plan(pointee, refs)?,
        },
    })
}

fn resolve_child_plan(
    child: ChildPlan<BlockId>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ChildPlan<ExecBlock>, HashError> {
    Ok(match child {
        ChildPlan::Scalar {
            include_shape,
            shape,
            scalar,
        } => ChildPlan::Scalar {
            include_shape,
            shape,
            scalar,
        },
        ChildPlan::Program(program) => ChildPlan::Program(resolve_hash_program(program, refs)?),
    })
}

fn resolve_field_plans(
    fields: Box<[FieldPlan<BlockId>]>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Box<[FieldPlan<ExecBlock>]>, HashError> {
    fields
        .into_vec()
        .into_iter()
        .map(|field| {
            Ok(FieldPlan {
                name: field.name,
                offset: field.offset,
                child: resolve_child_plan(field.child, refs)?,
            })
        })
        .collect()
}

fn resolve_block_ref(
    block: BlockId,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ExecBlock, HashError> {
    refs.get(&block).copied().ok_or(HashError::MissingBlock {
        block: BlockRef::new(usize::MAX),
    })
}

fn hash_lowered_effect_stats(lowered: &DenseLowered<ExecOp>) -> LoweredEffectStats {
    let root = hash_program_effect_stats(&lowered.program);
    let mut blocks = EffectStats::default();
    for block in &lowered.blocks {
        blocks.accumulate(hash_program_effect_stats(block));
    }
    LoweredEffectStats::new(root, blocks, lowered.blocks.len())
}

fn hash_program_effect_stats(program: &[ExecOp]) -> EffectStats {
    let mut stats = weavy::ir::effect_stats(program);
    for op in program {
        if let WeavyOp::Intrinsic(intrinsic) = op {
            add_hash_intrinsic_effect_stats(intrinsic, &mut stats);
        }
    }
    stats
}

fn add_hash_intrinsic_effect_stats(intrinsic: &HashIntrinsic<ExecBlock>, stats: &mut EffectStats) {
    match intrinsic {
        HashIntrinsic::Shape(_) | HashIntrinsic::Scalar { .. } => {}
        HashIntrinsic::Struct { fields, .. } => {
            for field in fields {
                stats.accumulate(hash_child_effect_stats(&field.child));
            }
        }
        HashIntrinsic::Option { some, .. } => {
            stats.accumulate(hash_child_effect_stats(some));
        }
        HashIntrinsic::Result { ok, err, .. } => {
            stats.accumulate(hash_child_effect_stats(ok));
            stats.accumulate(hash_child_effect_stats(err));
        }
        HashIntrinsic::List { element, .. }
        | HashIntrinsic::Array { element, .. }
        | HashIntrinsic::Slice { element, .. }
        | HashIntrinsic::Set { element, .. } => {
            stats.accumulate(hash_child_effect_stats(element));
        }
        HashIntrinsic::Map { key, value, .. } => {
            stats.accumulate(hash_child_effect_stats(key));
            stats.accumulate(hash_child_effect_stats(value));
        }
        HashIntrinsic::Pointer { pointee, .. } => {
            stats.accumulate(hash_child_effect_stats(pointee));
        }
    }
}

fn hash_child_effect_stats(child: &ChildPlan<ExecBlock>) -> EffectStats {
    match child {
        ChildPlan::Scalar {
            include_shape,
            shape,
            scalar,
        } => {
            let mut stats = EffectStats::default();
            if *include_shape {
                let shape_op: ExecOp = WeavyOp::Intrinsic(HashIntrinsic::Shape(shape));
                stats.accumulate(weavy::ir::effect_stats(core::slice::from_ref(&shape_op)));
            }
            let scalar_op: ExecOp = WeavyOp::Intrinsic(HashIntrinsic::Scalar {
                shape,
                scalar: *scalar,
            });
            stats.accumulate(weavy::ir::effect_stats(core::slice::from_ref(&scalar_op)));
            stats
        }
        ChildPlan::Program(program) => hash_program_effect_stats(program),
    }
}

fn sized_layout(shape: &'static Shape) -> Result<Layout, HashError> {
    shape
        .layout
        .sized_layout()
        .map_err(|_| unsupported(shape, "unsized shape"))
}

fn unsupported(shape: &'static Shape, feature: &'static str) -> HashError {
    HashError::Unsupported { shape, feature }
}

struct HashInterp<'a, H>
where
    H: Hasher,
{
    base: PtrConst,
    hasher: &'a mut H,
}

impl<'a, H> HashInterp<'a, H>
where
    H: Hasher,
{
    fn new(base: PtrConst, hasher: &'a mut H) -> Self {
        Self { base, hasher }
    }
}

enum HashContinuation<'program> {
    RestoreBase(PtrConst),
    StructFields {
        original_base: PtrConst,
        mode: HashMode,
        fields: &'program [FieldPlan<ExecBlock>],
        next_index: usize,
    },
    Sequence {
        original_base: PtrConst,
        data: PtrConst,
        len: usize,
        next_index: usize,
        stride: usize,
        element_program: &'program Program<ExecOp>,
    },
    Set {
        original_base: PtrConst,
        iter: PtrMut,
        set: SetDef,
        element: &'program ChildPlan<ExecBlock>,
    },
    MapAfterKey {
        original_base: PtrConst,
        iter: PtrMut,
        map: MapDef,
        key: &'program ChildPlan<ExecBlock>,
        value_plan: &'program ChildPlan<ExecBlock>,
        value_ptr: PtrConst,
    },
    MapAfterValue {
        original_base: PtrConst,
        iter: PtrMut,
        map: MapDef,
        key: &'program ChildPlan<ExecBlock>,
        value: &'program ChildPlan<ExecBlock>,
    },
}

impl<'program, H> Step<'program, ExecBlock, ExecOp> for HashInterp<'_, H>
where
    H: Hasher,
{
    type Error = HashError;
    type Continuation = HashContinuation<'program>;

    fn step(
        &mut self,
        op: &'program ExecOp,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match op {
            WeavyOp::Control(ControlOp::CallBlock { block, base_offset }) => {
                if *base_offset != 0 {
                    return Err(HashError::MalformedProgram {
                        reason: "hash block calls must not adjust base",
                    });
                }
                Ok(Control::CallBlock(*block))
            }
            WeavyOp::Control(ControlOp::Return) => Ok(Control::Return),
            WeavyOp::Intrinsic(intrinsic) => self.step_intrinsic(intrinsic),
            WeavyOp::Memory(_) | WeavyOp::Init(_) | WeavyOp::Aggregate(_) => {
                Err(HashError::MalformedProgram {
                    reason: "hash interpreter only accepts control and hash intrinsic ops",
                })
            }
            _ => Err(HashError::MalformedProgram {
                reason: "hash interpreter saw an unknown canonical op",
            }),
        }
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match continuation {
            HashContinuation::RestoreBase(base) => {
                self.base = base;
                Ok(Control::Continue)
            }
            HashContinuation::StructFields {
                original_base,
                mode,
                fields,
                next_index,
            } => self.call_next_struct_field(original_base, mode, fields, next_index),
            HashContinuation::Sequence {
                original_base,
                data,
                len,
                next_index,
                stride,
                element_program,
            } => self.call_next_sequence(
                original_base,
                data,
                len,
                next_index,
                stride,
                element_program,
            ),
            HashContinuation::Set {
                original_base,
                iter,
                set,
                element,
            } => self.call_next_set(original_base, iter, set, element),
            HashContinuation::MapAfterKey {
                original_base,
                iter,
                map,
                key,
                value_plan,
                value_ptr,
            } => match self.call_map_value(original_base, iter, map, key, value_plan, value_ptr)? {
                Control::Continue => self.call_next_map(original_base, iter, map, key, value_plan),
                control => Ok(control),
            },
            HashContinuation::MapAfterValue {
                original_base,
                iter,
                map,
                key,
                value,
            } => self.call_next_map(original_base, iter, map, key, value),
        }
    }
}

impl<'program, H> HashInterp<'_, H>
where
    H: Hasher,
{
    fn step_intrinsic(
        &mut self,
        intrinsic: &'program HashIntrinsic<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        match intrinsic {
            HashIntrinsic::Shape(shape) => {
                shape.id.hash(self.hasher);
                Ok(Control::Continue)
            }
            HashIntrinsic::Scalar { shape, scalar } => {
                unsafe { hash_scalar(shape, *scalar, self.base, self.hasher) };
                Ok(Control::Continue)
            }
            HashIntrinsic::Struct { mode, kind, fields } => {
                if *mode == HashMode::Structural {
                    (*kind as u8).hash(self.hasher);
                }
                self.call_next_struct_field(self.base, *mode, fields, 0)
            }
            HashIntrinsic::Option { option, some } => {
                if unsafe { (option.vtable.is_some)(self.base) } {
                    true.hash(self.hasher);
                    let value = unsafe { (option.vtable.get_value)(self.base) };
                    self.call_child(some, PtrConst::new_sized(value), self.base)
                } else {
                    false.hash(self.hasher);
                    Ok(Control::Continue)
                }
            }
            HashIntrinsic::Result { result, ok, err } => {
                if unsafe { (result.vtable.is_ok)(self.base) } {
                    0u8.hash(self.hasher);
                    let value = unsafe { (result.vtable.get_ok)(self.base) };
                    self.call_child(ok, PtrConst::new_sized(value), self.base)
                } else {
                    1u8.hash(self.hasher);
                    let value = unsafe { (result.vtable.get_err)(self.base) };
                    self.call_child(err, PtrConst::new_sized(value), self.base)
                }
            }
            HashIntrinsic::List {
                list_shape,
                list,
                element_layout,
                element,
            } => {
                let len = unsafe { (list.vtable.len)(self.base) };
                len.hash(self.hasher);
                if len == 0 {
                    return Ok(Control::Continue);
                }
                let as_ptr = list
                    .vtable
                    .as_ptr
                    .ok_or_else(|| unsupported(list_shape, "list as_ptr"))?;
                let data = unsafe { as_ptr(self.base) };
                self.hash_or_call_sequence(data, len, element_layout.size(), element)
            }
            HashIntrinsic::Array {
                array,
                element_layout,
                element,
            } => {
                array.n.hash(self.hasher);
                if array.n == 0 {
                    return Ok(Control::Continue);
                }
                let data = unsafe { (array.vtable.as_ptr)(self.base) };
                self.hash_or_call_sequence(data, array.n, element_layout.size(), element)
            }
            HashIntrinsic::Slice {
                slice,
                element_layout,
                element,
            } => {
                let len = unsafe { (slice.vtable.len)(self.base) };
                len.hash(self.hasher);
                if len == 0 {
                    return Ok(Control::Continue);
                }
                let data = unsafe { (slice.vtable.as_ptr)(self.base) };
                self.hash_or_call_sequence(data, len, element_layout.size(), element)
            }
            HashIntrinsic::Set { set, element } => {
                let len = unsafe { (set.vtable.len)(self.base) };
                len.hash(self.hasher);
                let iter_init = set
                    .vtable
                    .iter_vtable
                    .init_with_value
                    .ok_or_else(|| unsupported(set.t(), "set iterator init"))?;
                let iter = unsafe { iter_init(self.base) };
                self.call_next_set(self.base, iter, *set, element)
            }
            HashIntrinsic::Map { map, key, value } => {
                let len = unsafe { (map.vtable.len)(self.base) };
                len.hash(self.hasher);
                let iter_init = map
                    .vtable
                    .iter_vtable
                    .init_with_value
                    .ok_or_else(|| unsupported(map.k(), "map iterator init"))?;
                let iter = unsafe { iter_init(self.base) };
                self.call_next_map(self.base, iter, *map, key, value)
            }
            HashIntrinsic::Pointer { pointer, pointee } => {
                let borrow = pointer.vtable.borrow_fn.ok_or_else(|| {
                    unsupported(
                        pointer.pointee().expect("lowering rejects opaque pointers"),
                        "pointer borrow",
                    )
                })?;
                self.call_child(pointee, unsafe { borrow(self.base) }, self.base)
            }
        }
    }

    fn call_child(
        &mut self,
        child: &'program ChildPlan<ExecBlock>,
        child_base: PtrConst,
        original_base: PtrConst,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        match child {
            ChildPlan::Scalar {
                include_shape,
                shape,
                scalar,
            } => {
                unsafe {
                    hash_child_scalar(*include_shape, shape, *scalar, child_base, self.hasher)
                };
                Ok(Control::Continue)
            }
            ChildPlan::Program(program) => {
                self.base = child_base;
                Ok(Control::CallProgramThen(
                    program,
                    HashContinuation::RestoreBase(original_base),
                ))
            }
        }
    }

    fn hash_or_call_sequence(
        &mut self,
        data: PtrConst,
        len: usize,
        stride: usize,
        element: &'program ChildPlan<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        match element {
            ChildPlan::Scalar {
                include_shape,
                shape,
                scalar,
            } => {
                for index in 0..len {
                    let ptr = unsafe { sequence_element(data, index, stride) };
                    unsafe { hash_child_scalar(*include_shape, shape, *scalar, ptr, self.hasher) };
                }
                Ok(Control::Continue)
            }
            ChildPlan::Program(program) => {
                self.call_next_sequence(self.base, data, len, 0, stride, program)
            }
        }
    }

    fn call_next_sequence(
        &mut self,
        original_base: PtrConst,
        data: PtrConst,
        len: usize,
        next_index: usize,
        stride: usize,
        element_program: &'program Program<ExecOp>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        if next_index >= len {
            self.base = original_base;
            return Ok(Control::Continue);
        }

        self.base = unsafe { sequence_element(data, next_index, stride) };
        Ok(Control::CallProgramThen(
            element_program,
            HashContinuation::Sequence {
                original_base,
                data,
                len,
                next_index: next_index + 1,
                stride,
                element_program,
            },
        ))
    }

    fn call_next_struct_field(
        &mut self,
        original_base: PtrConst,
        mode: HashMode,
        fields: &'program [FieldPlan<ExecBlock>],
        next_index: usize,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        let mut index = next_index;
        while index < fields.len() {
            let field = &fields[index];
            if mode == HashMode::Structural {
                field.name.hash(self.hasher);
            }
            let field_base = unsafe { original_base.field(field.offset) };
            match &field.child {
                ChildPlan::Scalar {
                    include_shape,
                    shape,
                    scalar,
                } => {
                    unsafe {
                        hash_child_scalar(*include_shape, shape, *scalar, field_base, self.hasher)
                    };
                    index += 1;
                }
                ChildPlan::Program(program) => {
                    self.base = field_base;
                    return Ok(Control::CallProgramThen(
                        program,
                        HashContinuation::StructFields {
                            original_base,
                            mode,
                            fields,
                            next_index: index + 1,
                        },
                    ));
                }
            }
        }

        self.base = original_base;
        Ok(Control::Continue)
    }

    fn call_next_set(
        &mut self,
        original_base: PtrConst,
        iter: PtrMut,
        set: SetDef,
        element: &'program ChildPlan<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        loop {
            match unsafe { (set.vtable.iter_vtable.next)(iter) } {
                Some(value) => match element {
                    ChildPlan::Scalar {
                        include_shape,
                        shape,
                        scalar,
                    } => unsafe {
                        hash_child_scalar(*include_shape, shape, *scalar, value, self.hasher)
                    },
                    ChildPlan::Program(program) => {
                        self.base = value;
                        return Ok(Control::CallProgramThen(
                            program,
                            HashContinuation::Set {
                                original_base,
                                iter,
                                set,
                                element,
                            },
                        ));
                    }
                },
                None => {
                    unsafe { (set.vtable.iter_vtable.dealloc)(iter) };
                    self.base = original_base;
                    return Ok(Control::Continue);
                }
            }
        }
    }

    fn call_next_map(
        &mut self,
        original_base: PtrConst,
        iter: PtrMut,
        map: MapDef,
        key: &'program ChildPlan<ExecBlock>,
        value: &'program ChildPlan<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        loop {
            match unsafe { (map.vtable.iter_vtable.next)(iter) } {
                Some((key_ptr, value_ptr)) => match key {
                    ChildPlan::Scalar {
                        include_shape,
                        shape,
                        scalar,
                    } => {
                        unsafe {
                            hash_child_scalar(*include_shape, shape, *scalar, key_ptr, self.hasher)
                        };
                        match self.call_map_value(
                            original_base,
                            iter,
                            map,
                            key,
                            value,
                            value_ptr,
                        )? {
                            Control::Continue => {}
                            control => return Ok(control),
                        }
                    }
                    ChildPlan::Program(program) => {
                        self.base = key_ptr;
                        return Ok(Control::CallProgramThen(
                            program,
                            HashContinuation::MapAfterKey {
                                original_base,
                                iter,
                                map,
                                key,
                                value_plan: value,
                                value_ptr,
                            },
                        ));
                    }
                },
                None => {
                    unsafe { (map.vtable.iter_vtable.dealloc)(iter) };
                    self.base = original_base;
                    return Ok(Control::Continue);
                }
            }
        }
    }

    fn call_map_value(
        &mut self,
        original_base: PtrConst,
        iter: PtrMut,
        map: MapDef,
        key: &'program ChildPlan<ExecBlock>,
        value: &'program ChildPlan<ExecBlock>,
        value_ptr: PtrConst,
    ) -> Result<Control<'program, ExecBlock, ExecOp, HashContinuation<'program>>, HashError> {
        match value {
            ChildPlan::Scalar {
                include_shape,
                shape,
                scalar,
            } => {
                unsafe {
                    hash_child_scalar(*include_shape, shape, *scalar, value_ptr, self.hasher)
                };
                Ok(Control::Continue)
            }
            ChildPlan::Program(program) => {
                self.base = value_ptr;
                Ok(Control::CallProgramThen(
                    program,
                    HashContinuation::MapAfterValue {
                        original_base,
                        iter,
                        map,
                        key,
                        value,
                    },
                ))
            }
        }
    }
}

unsafe fn sequence_element(data: PtrConst, index: usize, stride: usize) -> PtrConst {
    PtrConst::new_sized(unsafe { data.as_byte_ptr().add(index * stride) })
}

unsafe fn hash_child_scalar<H>(
    include_shape: bool,
    shape: &'static Shape,
    scalar: ScalarType,
    ptr: PtrConst,
    hasher: &mut H,
) where
    H: Hasher,
{
    if include_shape {
        shape.id.hash(hasher);
    }
    unsafe { hash_scalar(shape, scalar, ptr, hasher) };
}

unsafe fn hash_scalar<H>(shape: &'static Shape, scalar: ScalarType, ptr: PtrConst, hasher: &mut H)
where
    H: Hasher,
{
    match scalar {
        ScalarType::Unit => 0u8.hash(hasher),
        ScalarType::Bool => unsafe { ptr.get::<bool>() }.hash(hasher),
        ScalarType::Char => unsafe { ptr.get::<char>() }.hash(hasher),
        ScalarType::Str => unsafe { hash_str_scalar(shape, ptr, hasher) },
        ScalarType::String => unsafe { ptr.get::<String>() }.hash(hasher),
        ScalarType::CowStr => unsafe { ptr.get::<Cow<'static, str>>() }.hash(hasher),
        ScalarType::F32 => unsafe { ptr.get::<f32>() }.to_bits().hash(hasher),
        ScalarType::F64 => unsafe { ptr.get::<f64>() }.to_bits().hash(hasher),
        ScalarType::U8 => unsafe { ptr.get::<u8>() }.hash(hasher),
        ScalarType::U16 => unsafe { ptr.get::<u16>() }.hash(hasher),
        ScalarType::U32 => unsafe { ptr.get::<u32>() }.hash(hasher),
        ScalarType::U64 => unsafe { ptr.get::<u64>() }.hash(hasher),
        ScalarType::U128 => unsafe { ptr.get::<u128>() }.hash(hasher),
        ScalarType::USize => unsafe { ptr.get::<usize>() }.hash(hasher),
        ScalarType::I8 => unsafe { ptr.get::<i8>() }.hash(hasher),
        ScalarType::I16 => unsafe { ptr.get::<i16>() }.hash(hasher),
        ScalarType::I32 => unsafe { ptr.get::<i32>() }.hash(hasher),
        ScalarType::I64 => unsafe { ptr.get::<i64>() }.hash(hasher),
        ScalarType::I128 => unsafe { ptr.get::<i128>() }.hash(hasher),
        ScalarType::ISize => unsafe { ptr.get::<isize>() }.hash(hasher),
        ScalarType::ConstTypeId => unsafe { ptr.get::<ConstTypeId>() }.hash(hasher),
        #[cfg(feature = "net")]
        ScalarType::SocketAddr => unsafe { ptr.get::<core::net::SocketAddr>() }.hash(hasher),
        #[cfg(feature = "net")]
        ScalarType::IpAddr => unsafe { ptr.get::<core::net::IpAddr>() }.hash(hasher),
        #[cfg(feature = "net")]
        ScalarType::Ipv4Addr => unsafe { ptr.get::<core::net::Ipv4Addr>() }.hash(hasher),
        #[cfg(feature = "net")]
        ScalarType::Ipv6Addr => unsafe { ptr.get::<core::net::Ipv6Addr>() }.hash(hasher),
        _ => unreachable!("unsupported scalar types are rejected while lowering"),
    }
}

unsafe fn hash_str_scalar<H>(shape: &'static Shape, ptr: PtrConst, hasher: &mut H)
where
    H: Hasher,
{
    if shape.is_type::<&'static str>() {
        unsafe { ptr.get::<&'static str>() }.hash(hasher);
    } else {
        unsafe { ptr.get::<str>() }.hash(hasher);
    }
}

fn supported_scalar(scalar: ScalarType) -> bool {
    match scalar {
        ScalarType::Unit
        | ScalarType::Bool
        | ScalarType::Char
        | ScalarType::Str
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
        | ScalarType::ISize
        | ScalarType::ConstTypeId => true,
        #[cfg(feature = "net")]
        ScalarType::SocketAddr
        | ScalarType::IpAddr
        | ScalarType::Ipv4Addr
        | ScalarType::Ipv6Addr => true,
        _ => false,
    }
}
