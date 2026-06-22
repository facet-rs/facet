//! Canonical typed execution IR shared by Weavy frontends.
//!
//! This module is the convergence target for PHON, facet-json, facet-hash,
//! serializers, validators, and future scripting frontends. Domain-specific
//! work stays in [`WeavyOp::Intrinsic`]; typed memory, initialization,
//! aggregate, and control structure live here so shared optimizers and backends
//! can reason across crate boundaries.

use std::collections::BTreeMap;

use crate::mem::{Layout, ScalarSegment};
use crate::{BlockRef, DenseLowered, Lowered, Program};

/// A canonical Weavy program with caller-defined symbolic block ids.
pub type WeavyProgram<Block, Intrinsic> = Program<WeavyOp<Block, Intrinsic>>;

/// A canonical Weavy lowered program with caller-defined symbolic block ids.
pub type WeavyLowered<Block, Intrinsic> = Lowered<Block, WeavyOp<Block, Intrinsic>>;

/// A canonical Weavy program whose block calls use dense [`BlockRef`]s.
pub type DenseWeavyProgram<Intrinsic> = Program<WeavyOp<BlockRef, Intrinsic>>;

/// A canonical Weavy lowered program whose block calls use dense [`BlockRef`]s.
pub type DenseWeavyLowered<Intrinsic> = DenseLowered<WeavyOp<BlockRef, Intrinsic>>;

/// One canonical Weavy operation.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeavyOp<Block, Intrinsic> {
    /// Program control: block calls, branches, returns.
    Control(ControlOp<Block>),
    /// Typed memory movement and destruction.
    Memory(MemoryOp),
    /// Type-erased initialization and ownership transfer.
    Init(InitOp),
    /// Aggregate construction/tracking such as records and lists.
    Aggregate(AggregateOp<Block>),
    /// Frontend/domain-specific work with a declared contract.
    Intrinsic(Intrinsic),
}

impl<Block, Intrinsic> WeavyOp<Block, Intrinsic> {
    /// Rewrite every canonical block reference in this op.
    pub fn try_map_blocks<MappedBlock, Error>(
        self,
        map: &mut impl FnMut(Block) -> Result<MappedBlock, Error>,
    ) -> Result<WeavyOp<MappedBlock, Intrinsic>, Error> {
        Ok(match self {
            WeavyOp::Control(op) => WeavyOp::Control(op.try_map_blocks(map)?),
            WeavyOp::Memory(op) => WeavyOp::Memory(op),
            WeavyOp::Init(op) => WeavyOp::Init(op),
            WeavyOp::Aggregate(op) => WeavyOp::Aggregate(op.try_map_blocks(map)?),
            WeavyOp::Intrinsic(op) => WeavyOp::Intrinsic(op),
        })
    }
}

/// Canonical control-flow operations.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOp<Block> {
    /// Call a recursive or shared lowered block at `base + base_offset`.
    CallBlock { block: Block, base_offset: usize },
    /// Return from the current program.
    Return,
}

impl<Block> ControlOp<Block> {
    fn try_map_blocks<MappedBlock, Error>(
        self,
        map: &mut impl FnMut(Block) -> Result<MappedBlock, Error>,
    ) -> Result<ControlOp<MappedBlock>, Error> {
        Ok(match self {
            ControlOp::CallBlock { block, base_offset } => ControlOp::CallBlock {
                block: map(block)?,
                base_offset,
            },
            ControlOp::Return => ControlOp::Return,
        })
    }
}

/// Canonical typed-memory operations.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryOp {
    /// Copy a fixed-width scalar at a base-relative offset.
    ScalarCopy {
        offset: usize,
        size: usize,
        align: usize,
    },
    /// Copy a fused run of scalar segments while preserving per-segment facts.
    ScalarRun { segments: Vec<ScalarSegment> },
    /// Zero a base-relative byte range.
    Zero { offset: usize, size: usize },
    /// Move initialized bytes between base-relative ranges.
    Move {
        src_offset: usize,
        dst_offset: usize,
        size: usize,
        align: usize,
    },
    /// Drop an initialized value at a base-relative offset.
    Drop { offset: usize, layout: Layout },
}

/// Canonical initialization and ownership-transfer operations.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitOp {
    /// Initialize a value from its default.
    Default { offset: usize },
    /// Initialize an option-like value to none.
    OptionNone { offset: usize },
    /// Initialize an option-like value from scratch storage.
    OptionSome { offset: usize, inner: Layout },
    /// Adopt a raw element buffer into an owned list-like handle.
    ListFromRawParts {
        offset: usize,
        element: Layout,
        len: usize,
        cap: usize,
    },
    /// Initialize an owning pointer-like value from scratch storage.
    PointerFromScratch { offset: usize, pointee: Layout },
}

/// Canonical aggregate operations.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateOp<Block> {
    /// Begin tracking an aggregate record.
    BeginRecord { field_count: usize },
    /// Mark one record field as initialized.
    RecordField { index: usize, offset: usize },
    /// Finish and validate a tracked record.
    FinishRecord,
    /// Begin an owned list-like aggregate.
    BeginList {
        offset: usize,
        element: Layout,
        loop_block: Block,
    },
    /// Finish and validate an owned list-like aggregate.
    FinishList,
}

impl<Block> AggregateOp<Block> {
    fn try_map_blocks<MappedBlock, Error>(
        self,
        map: &mut impl FnMut(Block) -> Result<MappedBlock, Error>,
    ) -> Result<AggregateOp<MappedBlock>, Error> {
        Ok(match self {
            AggregateOp::BeginRecord { field_count } => AggregateOp::BeginRecord { field_count },
            AggregateOp::RecordField { index, offset } => {
                AggregateOp::RecordField { index, offset }
            }
            AggregateOp::FinishRecord => AggregateOp::FinishRecord,
            AggregateOp::BeginList {
                offset,
                element,
                loop_block,
            } => AggregateOp::BeginList {
                offset,
                element,
                loop_block: map(loop_block)?,
            },
            AggregateOp::FinishList => AggregateOp::FinishList,
        })
    }
}

/// Stable identity for a domain intrinsic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntrinsicDescriptor {
    /// The producer or dialect, such as `phon`, `json`, `hash`, or `script`.
    pub dialect: &'static str,
    /// The intrinsic name inside that dialect.
    pub name: &'static str,
}

/// A resource touched by an op effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EffectResource {
    /// A frontend input stream/cursor, such as `wire` or `json`.
    Input(&'static str),
    /// A frontend output sink, such as `wire`, `json`, or `hash`.
    Sink(&'static str),
    /// Interpreter/backend side state, such as aggregate trackers.
    SideChannel(&'static str),
}

/// How an op touches a non-memory resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceAccess {
    /// Reads from the resource without necessarily consuming it.
    Read,
    /// Advances or consumes a cursor-like resource.
    Advance,
    /// Writes to the resource.
    Write,
}

/// One non-memory resource touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceEffect {
    /// Which resource is touched.
    pub resource: EffectResource,
    /// How the resource is touched.
    pub access: ResourceAccess,
}

/// A typed-memory region relative to the current base pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    /// `None` when the op may touch a region the current IR cannot localize yet.
    pub offset: Option<usize>,
    /// `None` when the op may touch a dynamically-sized or opaque region.
    pub size: Option<usize>,
}

impl MemoryRegion {
    /// A known base-relative memory range.
    #[must_use]
    pub fn base_relative(offset: usize, size: usize) -> Self {
        Self {
            offset: Some(offset),
            size: Some(size),
        }
    }

    /// A known base-relative offset whose exact byte width is opaque.
    #[must_use]
    pub fn base_relative_unknown_size(offset: usize) -> Self {
        Self {
            offset: Some(offset),
            size: None,
        }
    }

    /// A known-width region whose base-relative offset is opaque.
    #[must_use]
    pub fn unknown_offset(size: usize) -> Self {
        Self {
            offset: None,
            size: Some(size),
        }
    }

    /// A region the current IR cannot localize.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            offset: None,
            size: None,
        }
    }
}

/// How an op touches typed memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypedMemoryAccess {
    /// Reads an already-initialized value or bytes.
    Read,
    /// Initializes previously-uninitialized storage.
    Initialize,
    /// Writes storage that may already be initialized.
    Overwrite,
    /// Moves a value out of this region.
    MoveFrom,
    /// Moves a value into this region.
    MoveInto,
    /// Drops an initialized value.
    Drop,
}

/// One typed-memory touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypedMemoryEffect {
    /// Which typed-memory region is touched.
    pub region: MemoryRegion,
    /// How the region is touched.
    pub access: TypedMemoryAccess,
}

/// Whether an op can be moved across neighboring effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum EffectOrdering {
    /// The op has no sequencing constraint beyond memory/resource dependencies.
    #[default]
    Reorderable,
    /// The op has stream/state order and may only be fused or moved by a pass
    /// that explicitly understands that order.
    Ordered,
    /// The op is a hard scheduling barrier.
    Barrier,
}

/// Conservative effect contract for one canonical op.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectContract {
    /// Non-memory resources touched by this op.
    pub resources: Vec<ResourceEffect>,
    /// Typed-memory regions touched by this op.
    pub typed_memory: Vec<TypedMemoryEffect>,
    /// The op may return an error in the interpreter/backend.
    pub may_fail: bool,
    /// The op may allocate.
    pub may_allocate: bool,
    /// The op may call user or registry code.
    pub calls_user_code: bool,
    /// Scheduling constraint exposed to optimizer passes.
    pub ordering: EffectOrdering,
    /// The op's real effects are not fully visible yet.
    pub opaque: bool,
}

impl EffectContract {
    /// Build an empty, reorderable contract.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an explicit opaque barrier contract.
    #[must_use]
    pub fn opaque() -> Self {
        Self::new().opaque_barrier()
    }

    /// Mark this op as reading a resource.
    #[must_use]
    pub fn read_resource(mut self, resource: EffectResource) -> Self {
        self.resources.push(ResourceEffect {
            resource,
            access: ResourceAccess::Read,
        });
        self
    }

    /// Mark this op as advancing a cursor-like resource.
    #[must_use]
    pub fn advance_resource(mut self, resource: EffectResource) -> Self {
        self.resources.push(ResourceEffect {
            resource,
            access: ResourceAccess::Advance,
        });
        self.ordered()
    }

    /// Mark this op as writing a resource.
    #[must_use]
    pub fn write_resource(mut self, resource: EffectResource) -> Self {
        self.resources.push(ResourceEffect {
            resource,
            access: ResourceAccess::Write,
        });
        self.ordered()
    }

    /// Mark this op as touching typed memory.
    #[must_use]
    pub fn typed_memory(mut self, region: MemoryRegion, access: TypedMemoryAccess) -> Self {
        self.typed_memory.push(TypedMemoryEffect { region, access });
        self
    }

    /// Mark this op as fallible.
    #[must_use]
    pub fn may_fail(mut self) -> Self {
        self.may_fail = true;
        self
    }

    /// Mark this op as allocating.
    #[must_use]
    pub fn may_allocate(mut self) -> Self {
        self.may_allocate = true;
        self
    }

    /// Mark this op as calling user or registry code.
    #[must_use]
    pub fn calls_user_code(mut self) -> Self {
        self.calls_user_code = true;
        self.barrier()
    }

    /// Mark this op as order-sensitive.
    #[must_use]
    pub fn ordered(mut self) -> Self {
        if matches!(self.ordering, EffectOrdering::Reorderable) {
            self.ordering = EffectOrdering::Ordered;
        }
        self
    }

    /// Mark this op as a hard ordering barrier.
    #[must_use]
    pub fn barrier(mut self) -> Self {
        self.ordering = EffectOrdering::Barrier;
        self
    }

    /// Mark this op as an opaque hard barrier.
    #[must_use]
    pub fn opaque_barrier(mut self) -> Self {
        self.opaque = true;
        self.barrier()
    }
}

/// Minimal intrinsic metadata hook.
pub trait IntrinsicOp {
    /// Return this intrinsic's stable descriptor.
    fn descriptor(&self) -> IntrinsicDescriptor;

    /// Return a conservative effect contract for this intrinsic.
    fn effect(&self) -> EffectContract {
        EffectContract::opaque()
    }
}

/// Error while resolving symbolic canonical block ids into dense refs.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError<Block> {
    /// An op referenced a block absent from the lowered block table.
    MissingBlock(Block),
}

/// Resolve a canonical lowered program into dense block-ref form.
pub fn resolve_lowered<Block, Intrinsic>(
    lowered: WeavyLowered<Block, Intrinsic>,
) -> Result<DenseWeavyLowered<Intrinsic>, ResolveError<Block>>
where
    Block: Clone + Ord,
{
    let refs = lowered.block_refs();
    let program = resolve_program(lowered.program, &refs)?;
    let mut blocks = Vec::with_capacity(lowered.blocks.len());
    for block in lowered.blocks.into_values() {
        blocks.push(resolve_program(block, &refs)?);
    }
    Ok(DenseLowered::new(program, blocks))
}

fn resolve_program<Block, Intrinsic>(
    program: WeavyProgram<Block, Intrinsic>,
    refs: &BTreeMap<Block, BlockRef>,
) -> Result<DenseWeavyProgram<Intrinsic>, ResolveError<Block>>
where
    Block: Clone + Ord,
{
    program
        .into_iter()
        .map(|op| {
            op.try_map_blocks(&mut |block| {
                refs.get(&block)
                    .copied()
                    .ok_or(ResolveError::MissingBlock(block))
            })
        })
        .collect()
}

/// Shape-only counts for one canonical program.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgramStats {
    pub op_count: usize,
    pub control_op_count: usize,
    pub memory_op_count: usize,
    pub init_op_count: usize,
    pub aggregate_op_count: usize,
    pub intrinsic_op_count: usize,
    pub block_call_count: usize,
    pub return_count: usize,
    pub scalar_copy_count: usize,
    pub scalar_run_count: usize,
    pub scalar_run_segment_count: usize,
    pub zero_count: usize,
    pub move_count: usize,
    pub drop_count: usize,
    pub default_init_count: usize,
    pub option_none_count: usize,
    pub option_some_count: usize,
    pub list_from_raw_parts_count: usize,
    pub pointer_init_count: usize,
    pub record_count: usize,
    pub record_field_count: usize,
    pub list_count: usize,
}

impl ProgramStats {
    /// Add another shape counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.op_count += other.op_count;
        self.control_op_count += other.control_op_count;
        self.memory_op_count += other.memory_op_count;
        self.init_op_count += other.init_op_count;
        self.aggregate_op_count += other.aggregate_op_count;
        self.intrinsic_op_count += other.intrinsic_op_count;
        self.block_call_count += other.block_call_count;
        self.return_count += other.return_count;
        self.scalar_copy_count += other.scalar_copy_count;
        self.scalar_run_count += other.scalar_run_count;
        self.scalar_run_segment_count += other.scalar_run_segment_count;
        self.zero_count += other.zero_count;
        self.move_count += other.move_count;
        self.drop_count += other.drop_count;
        self.default_init_count += other.default_init_count;
        self.option_none_count += other.option_none_count;
        self.option_some_count += other.option_some_count;
        self.list_from_raw_parts_count += other.list_from_raw_parts_count;
        self.pointer_init_count += other.pointer_init_count;
        self.record_count += other.record_count;
        self.record_field_count += other.record_field_count;
        self.list_count += other.list_count;
    }
}

/// Shape-only counts for a canonical lowered program with block table.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoweredProgramStats {
    pub root: ProgramStats,
    pub blocks: ProgramStats,
    pub total: ProgramStats,
    pub block_count: usize,
}

impl LoweredProgramStats {
    /// Add another lowered-program shape counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.root.accumulate(other.root);
        self.blocks.accumulate(other.blocks);
        self.total.accumulate(other.total);
        self.block_count += other.block_count;
    }
}

/// Effect counts for one canonical program.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStats {
    pub op_count: usize,
    pub intrinsic_op_count: usize,
    pub opaque_count: usize,
    pub reorderable_count: usize,
    pub ordered_count: usize,
    pub barrier_count: usize,
    pub input_read_count: usize,
    pub input_advance_count: usize,
    pub sink_write_count: usize,
    pub side_channel_count: usize,
    pub may_fail_count: usize,
    pub may_allocate_count: usize,
    pub calls_user_code_count: usize,
    pub typed_memory_read_count: usize,
    pub typed_memory_initialize_count: usize,
    pub typed_memory_overwrite_count: usize,
    pub typed_memory_move_count: usize,
    pub typed_memory_drop_count: usize,
}

impl EffectStats {
    /// Add another effect counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.op_count += other.op_count;
        self.intrinsic_op_count += other.intrinsic_op_count;
        self.opaque_count += other.opaque_count;
        self.reorderable_count += other.reorderable_count;
        self.ordered_count += other.ordered_count;
        self.barrier_count += other.barrier_count;
        self.input_read_count += other.input_read_count;
        self.input_advance_count += other.input_advance_count;
        self.sink_write_count += other.sink_write_count;
        self.side_channel_count += other.side_channel_count;
        self.may_fail_count += other.may_fail_count;
        self.may_allocate_count += other.may_allocate_count;
        self.calls_user_code_count += other.calls_user_code_count;
        self.typed_memory_read_count += other.typed_memory_read_count;
        self.typed_memory_initialize_count += other.typed_memory_initialize_count;
        self.typed_memory_overwrite_count += other.typed_memory_overwrite_count;
        self.typed_memory_move_count += other.typed_memory_move_count;
        self.typed_memory_drop_count += other.typed_memory_drop_count;
    }
}

/// Effect counts for a canonical lowered program with block table.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoweredEffectStats {
    pub root: EffectStats,
    pub blocks: EffectStats,
    pub total: EffectStats,
    pub block_count: usize,
}

impl LoweredEffectStats {
    /// Build lowered-program effect counters from already-counted root and block programs.
    #[must_use]
    pub fn new(root: EffectStats, blocks: EffectStats, block_count: usize) -> Self {
        let mut total = root;
        total.accumulate(blocks);
        Self {
            root,
            blocks,
            total,
            block_count,
        }
    }

    /// Add another lowered-program effect counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.root.accumulate(other.root);
        self.blocks.accumulate(other.blocks);
        self.total.accumulate(other.total);
        self.block_count += other.block_count;
    }
}

/// Count the canonical IR shape for one program.
#[must_use]
pub fn program_stats<Block, Intrinsic>(program: &[WeavyOp<Block, Intrinsic>]) -> ProgramStats {
    let mut stats = ProgramStats::default();
    add_program_stats(program, &mut stats);
    stats
}

/// Count the canonical IR shape for a lowered program and its block table.
#[must_use]
pub fn lowered_program_stats<Block, Intrinsic>(
    lowered: &WeavyLowered<Block, Intrinsic>,
) -> LoweredProgramStats
where
    Block: Ord,
{
    let root = program_stats(&lowered.program);
    let mut blocks = ProgramStats::default();
    for block in lowered.blocks.values() {
        blocks.accumulate(program_stats(block));
    }
    let mut total = root;
    total.accumulate(blocks);

    LoweredProgramStats {
        root,
        blocks,
        total,
        block_count: lowered.blocks.len(),
    }
}

/// Return the conservative effect contract for one canonical op.
#[must_use]
pub fn op_effect<Block, Intrinsic>(op: &WeavyOp<Block, Intrinsic>) -> EffectContract
where
    Intrinsic: IntrinsicOp,
{
    match op {
        WeavyOp::Control(op) => control_effect(op),
        WeavyOp::Memory(op) => memory_effect(op),
        WeavyOp::Init(op) => init_effect(op),
        WeavyOp::Aggregate(op) => aggregate_effect(op),
        WeavyOp::Intrinsic(intrinsic) => intrinsic.effect(),
    }
}

/// Count effect contracts for one canonical program.
#[must_use]
pub fn effect_stats<Block, Intrinsic>(program: &[WeavyOp<Block, Intrinsic>]) -> EffectStats
where
    Intrinsic: IntrinsicOp,
{
    let mut stats = EffectStats::default();
    add_effect_stats(program, &mut stats);
    stats
}

/// Count effect contracts for a canonical lowered program and its block table.
#[must_use]
pub fn lowered_effect_stats<Block, Intrinsic>(
    lowered: &WeavyLowered<Block, Intrinsic>,
) -> LoweredEffectStats
where
    Block: Ord,
    Intrinsic: IntrinsicOp,
{
    let root = effect_stats(&lowered.program);
    let mut blocks = EffectStats::default();
    for block in lowered.blocks.values() {
        blocks.accumulate(effect_stats(block));
    }
    let mut total = root;
    total.accumulate(blocks);

    LoweredEffectStats {
        root,
        blocks,
        total,
        block_count: lowered.blocks.len(),
    }
}

fn add_program_stats<Block, Intrinsic>(
    program: &[WeavyOp<Block, Intrinsic>],
    stats: &mut ProgramStats,
) {
    for op in program {
        stats.op_count += 1;
        match op {
            WeavyOp::Control(op) => add_control_stats(op, stats),
            WeavyOp::Memory(op) => add_memory_stats(op, stats),
            WeavyOp::Init(op) => add_init_stats(op, stats),
            WeavyOp::Aggregate(op) => add_aggregate_stats(op, stats),
            WeavyOp::Intrinsic(_) => {
                stats.intrinsic_op_count += 1;
            }
        }
    }
}

fn control_effect<Block>(op: &ControlOp<Block>) -> EffectContract {
    match op {
        ControlOp::CallBlock { .. } | ControlOp::Return => EffectContract::new().barrier(),
    }
}

fn memory_effect(op: &MemoryOp) -> EffectContract {
    match op {
        MemoryOp::ScalarCopy {
            offset,
            size,
            align: _,
        } => scalar_effect(*offset, *size),
        MemoryOp::ScalarRun { segments } => {
            let mut effect = stream_copy_effect();
            for segment in segments {
                effect = effect
                    .typed_memory(
                        MemoryRegion::base_relative(segment.offset, segment.size),
                        TypedMemoryAccess::Read,
                    )
                    .typed_memory(
                        MemoryRegion::base_relative(segment.offset, segment.size),
                        TypedMemoryAccess::Initialize,
                    );
            }
            effect
        }
        MemoryOp::Zero { offset, size } => EffectContract::new().typed_memory(
            MemoryRegion::base_relative(*offset, *size),
            TypedMemoryAccess::Overwrite,
        ),
        MemoryOp::Move {
            src_offset,
            dst_offset,
            size,
            align: _,
        } => EffectContract::new()
            .typed_memory(
                MemoryRegion::base_relative(*src_offset, *size),
                TypedMemoryAccess::MoveFrom,
            )
            .typed_memory(
                MemoryRegion::base_relative(*dst_offset, *size),
                TypedMemoryAccess::MoveInto,
            )
            .ordered(),
        MemoryOp::Drop { offset, layout } => EffectContract::new()
            .typed_memory(
                MemoryRegion::base_relative(*offset, layout.size),
                TypedMemoryAccess::Drop,
            )
            .calls_user_code(),
    }
}

fn scalar_effect(offset: usize, size: usize) -> EffectContract {
    stream_copy_effect()
        .typed_memory(
            MemoryRegion::base_relative(offset, size),
            TypedMemoryAccess::Read,
        )
        .typed_memory(
            MemoryRegion::base_relative(offset, size),
            TypedMemoryAccess::Initialize,
        )
}

fn stream_copy_effect() -> EffectContract {
    EffectContract::new()
        .read_resource(EffectResource::Input("wire"))
        .advance_resource(EffectResource::Input("wire"))
        .write_resource(EffectResource::Sink("wire"))
        .may_fail()
}

fn init_effect(op: &InitOp) -> EffectContract {
    match op {
        InitOp::Default { offset } | InitOp::OptionNone { offset } => EffectContract::new()
            .typed_memory(
                MemoryRegion::base_relative_unknown_size(*offset),
                TypedMemoryAccess::Initialize,
            )
            .calls_user_code(),
        InitOp::OptionSome { offset, inner } => EffectContract::new()
            .typed_memory(
                MemoryRegion::base_relative_unknown_size(*offset),
                TypedMemoryAccess::Initialize,
            )
            .typed_memory(
                MemoryRegion::unknown_offset(inner.size),
                TypedMemoryAccess::MoveFrom,
            )
            .calls_user_code(),
        InitOp::ListFromRawParts {
            offset,
            element: _,
            len: _,
            cap: _,
        } => EffectContract::new()
            .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::MoveFrom)
            .typed_memory(
                MemoryRegion::base_relative_unknown_size(*offset),
                TypedMemoryAccess::Initialize,
            )
            .calls_user_code(),
        InitOp::PointerFromScratch { offset, pointee: _ } => EffectContract::new()
            .typed_memory(MemoryRegion::unknown(), TypedMemoryAccess::MoveFrom)
            .typed_memory(
                MemoryRegion::base_relative_unknown_size(*offset),
                TypedMemoryAccess::Initialize,
            )
            .may_allocate()
            .calls_user_code(),
    }
}

fn aggregate_effect<Block>(op: &AggregateOp<Block>) -> EffectContract {
    match op {
        AggregateOp::BeginRecord { .. }
        | AggregateOp::RecordField { .. }
        | AggregateOp::BeginList { .. }
        | AggregateOp::FinishList => EffectContract::new()
            .write_resource(EffectResource::SideChannel("aggregate_state"))
            .ordered(),
        AggregateOp::FinishRecord => EffectContract::new()
            .read_resource(EffectResource::SideChannel("aggregate_state"))
            .may_fail()
            .ordered(),
    }
}

fn add_effect_stats<Block, Intrinsic>(
    program: &[WeavyOp<Block, Intrinsic>],
    stats: &mut EffectStats,
) where
    Intrinsic: IntrinsicOp,
{
    for op in program {
        stats.op_count += 1;
        if matches!(op, WeavyOp::Intrinsic(_)) {
            stats.intrinsic_op_count += 1;
        }
        add_contract_stats(&op_effect(op), stats);
    }
}

fn add_contract_stats(contract: &EffectContract, stats: &mut EffectStats) {
    if contract.opaque {
        stats.opaque_count += 1;
    }
    match contract.ordering {
        EffectOrdering::Reorderable => stats.reorderable_count += 1,
        EffectOrdering::Ordered => stats.ordered_count += 1,
        EffectOrdering::Barrier => stats.barrier_count += 1,
    }
    if contract.may_fail {
        stats.may_fail_count += 1;
    }
    if contract.may_allocate {
        stats.may_allocate_count += 1;
    }
    if contract.calls_user_code {
        stats.calls_user_code_count += 1;
    }
    for resource in &contract.resources {
        match (resource.resource, resource.access) {
            (EffectResource::Input(_), ResourceAccess::Read) => stats.input_read_count += 1,
            (EffectResource::Input(_), ResourceAccess::Advance) => {
                stats.input_advance_count += 1;
            }
            (EffectResource::Sink(_), ResourceAccess::Write) => stats.sink_write_count += 1,
            (EffectResource::SideChannel(_), _) => stats.side_channel_count += 1,
            _ => {}
        }
    }
    for memory in &contract.typed_memory {
        match memory.access {
            TypedMemoryAccess::Read => stats.typed_memory_read_count += 1,
            TypedMemoryAccess::Initialize => stats.typed_memory_initialize_count += 1,
            TypedMemoryAccess::Overwrite => stats.typed_memory_overwrite_count += 1,
            TypedMemoryAccess::MoveFrom | TypedMemoryAccess::MoveInto => {
                stats.typed_memory_move_count += 1;
            }
            TypedMemoryAccess::Drop => stats.typed_memory_drop_count += 1,
        }
    }
}

fn add_control_stats<Block>(op: &ControlOp<Block>, stats: &mut ProgramStats) {
    stats.control_op_count += 1;
    match op {
        ControlOp::CallBlock { .. } => stats.block_call_count += 1,
        ControlOp::Return => stats.return_count += 1,
    }
}

fn add_memory_stats(op: &MemoryOp, stats: &mut ProgramStats) {
    stats.memory_op_count += 1;
    match op {
        MemoryOp::ScalarCopy { .. } => stats.scalar_copy_count += 1,
        MemoryOp::ScalarRun { segments } => {
            stats.scalar_run_count += 1;
            stats.scalar_run_segment_count += segments.len();
        }
        MemoryOp::Zero { .. } => stats.zero_count += 1,
        MemoryOp::Move { .. } => stats.move_count += 1,
        MemoryOp::Drop { .. } => stats.drop_count += 1,
    }
}

fn add_init_stats(op: &InitOp, stats: &mut ProgramStats) {
    stats.init_op_count += 1;
    match op {
        InitOp::Default { .. } => stats.default_init_count += 1,
        InitOp::OptionNone { .. } => stats.option_none_count += 1,
        InitOp::OptionSome { .. } => stats.option_some_count += 1,
        InitOp::ListFromRawParts { .. } => stats.list_from_raw_parts_count += 1,
        InitOp::PointerFromScratch { .. } => stats.pointer_init_count += 1,
    }
}

fn add_aggregate_stats<Block>(op: &AggregateOp<Block>, stats: &mut ProgramStats) {
    stats.aggregate_op_count += 1;
    match op {
        AggregateOp::BeginRecord { .. } => stats.record_count += 1,
        AggregateOp::RecordField { .. } => stats.record_field_count += 1,
        AggregateOp::FinishRecord => {}
        AggregateOp::BeginList { .. } => stats.list_count += 1,
        AggregateOp::FinishList => {}
    }
}

/// Count intrinsic descriptors in one canonical program.
#[must_use]
pub fn intrinsic_counts<Block, Intrinsic>(
    program: &[WeavyOp<Block, Intrinsic>],
) -> BTreeMap<IntrinsicDescriptor, usize>
where
    Intrinsic: IntrinsicOp,
{
    let mut counts = BTreeMap::new();
    add_intrinsic_counts(program, &mut counts);
    counts
}

/// Count intrinsic descriptors in a lowered canonical program.
#[must_use]
pub fn lowered_intrinsic_counts<Block, Intrinsic>(
    lowered: &WeavyLowered<Block, Intrinsic>,
) -> BTreeMap<IntrinsicDescriptor, usize>
where
    Block: Ord,
    Intrinsic: IntrinsicOp,
{
    let mut counts = intrinsic_counts(&lowered.program);
    for block in lowered.blocks.values() {
        add_intrinsic_counts(block, &mut counts);
    }
    counts
}

fn add_intrinsic_counts<Block, Intrinsic>(
    program: &[WeavyOp<Block, Intrinsic>],
    counts: &mut BTreeMap<IntrinsicDescriptor, usize>,
) where
    Intrinsic: IntrinsicOp,
{
    for op in program {
        if let WeavyOp::Intrinsic(intrinsic) = op {
            *counts.entry(intrinsic.descriptor()).or_default() += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestIntrinsic {
        ReadI32,
        HashField,
    }

    impl IntrinsicOp for TestIntrinsic {
        fn descriptor(&self) -> IntrinsicDescriptor {
            match self {
                TestIntrinsic::ReadI32 => IntrinsicDescriptor {
                    dialect: "json",
                    name: "read_i32",
                },
                TestIntrinsic::HashField => IntrinsicDescriptor {
                    dialect: "hash",
                    name: "feed_field",
                },
            }
        }

        fn effect(&self) -> EffectContract {
            match self {
                TestIntrinsic::ReadI32 => EffectContract::new()
                    .read_resource(EffectResource::Input("json"))
                    .advance_resource(EffectResource::Input("json"))
                    .typed_memory(
                        MemoryRegion::base_relative(0, 4),
                        TypedMemoryAccess::Initialize,
                    )
                    .may_fail(),
                TestIntrinsic::HashField => EffectContract::new()
                    .typed_memory(MemoryRegion::base_relative(4, 8), TypedMemoryAccess::Read)
                    .write_resource(EffectResource::Sink("hash")),
            }
        }
    }

    fn scalar<Block>(offset: usize) -> WeavyOp<Block, TestIntrinsic> {
        WeavyOp::Memory(MemoryOp::ScalarCopy {
            offset,
            size: 4,
            align: 4,
        })
    }

    #[test]
    fn resolve_lowered_rewrites_canonical_block_refs() {
        let lowered = Lowered {
            program: vec![
                WeavyOp::Control(ControlOp::CallBlock {
                    block: "child",
                    base_offset: 4,
                }),
                WeavyOp::Aggregate(AggregateOp::BeginList {
                    offset: 8,
                    element: Layout { size: 4, align: 4 },
                    loop_block: "loop",
                }),
            ],
            blocks: BTreeMap::from([
                ("child", vec![scalar(0)]),
                (
                    "loop",
                    vec![WeavyOp::Control(ControlOp::CallBlock {
                        block: "child",
                        base_offset: 12,
                    })],
                ),
            ]),
        };

        let dense = resolve_lowered(lowered).unwrap();

        assert_eq!(
            dense.program,
            vec![
                WeavyOp::Control(ControlOp::CallBlock {
                    block: BlockRef::new(0),
                    base_offset: 4,
                }),
                WeavyOp::Aggregate(AggregateOp::BeginList {
                    offset: 8,
                    element: Layout { size: 4, align: 4 },
                    loop_block: BlockRef::new(1),
                }),
            ]
        );
        assert_eq!(
            dense.blocks[1],
            vec![WeavyOp::Control(ControlOp::CallBlock {
                block: BlockRef::new(0),
                base_offset: 12,
            })]
        );
    }

    #[test]
    fn resolve_lowered_reports_missing_block_refs() {
        let lowered = Lowered {
            program: vec![WeavyOp::<_, TestIntrinsic>::Control(ControlOp::CallBlock {
                block: "missing",
                base_offset: 0,
            })],
            blocks: BTreeMap::new(),
        };

        let err = resolve_lowered(lowered).unwrap_err();

        assert_eq!(err, ResolveError::MissingBlock("missing"));
    }

    #[test]
    fn canonical_program_stats_count_op_families() {
        let program = vec![
            WeavyOp::Control(ControlOp::CallBlock {
                block: "loop",
                base_offset: 0,
            }),
            WeavyOp::Control(ControlOp::Return),
            WeavyOp::Memory(MemoryOp::ScalarRun {
                segments: vec![
                    ScalarSegment {
                        offset: 0,
                        size: 4,
                        align: 4,
                    },
                    ScalarSegment {
                        offset: 8,
                        size: 2,
                        align: 2,
                    },
                ],
            }),
            WeavyOp::Memory(MemoryOp::Zero {
                offset: 16,
                size: 8,
            }),
            WeavyOp::Init(InitOp::OptionSome {
                offset: 24,
                inner: Layout { size: 4, align: 4 },
            }),
            WeavyOp::Aggregate(AggregateOp::BeginRecord { field_count: 2 }),
            WeavyOp::Aggregate(AggregateOp::RecordField {
                index: 1,
                offset: 24,
            }),
            WeavyOp::Intrinsic(TestIntrinsic::ReadI32),
        ];

        let stats = program_stats(&program);

        assert_eq!(stats.op_count, 8);
        assert_eq!(stats.control_op_count, 2);
        assert_eq!(stats.memory_op_count, 2);
        assert_eq!(stats.init_op_count, 1);
        assert_eq!(stats.aggregate_op_count, 2);
        assert_eq!(stats.intrinsic_op_count, 1);
        assert_eq!(stats.block_call_count, 1);
        assert_eq!(stats.return_count, 1);
        assert_eq!(stats.scalar_run_count, 1);
        assert_eq!(stats.scalar_run_segment_count, 2);
        assert_eq!(stats.zero_count, 1);
        assert_eq!(stats.option_some_count, 1);
        assert_eq!(stats.record_count, 1);
        assert_eq!(stats.record_field_count, 1);
    }

    #[test]
    fn lowered_program_stats_include_blocks() {
        let lowered = Lowered {
            program: vec![WeavyOp::Control(ControlOp::CallBlock {
                block: 7,
                base_offset: 0,
            })],
            blocks: BTreeMap::from([(7, vec![scalar(0), scalar(4)])]),
        };

        let stats = lowered_program_stats(&lowered);

        assert_eq!(stats.block_count, 1);
        assert_eq!(stats.root.op_count, 1);
        assert_eq!(stats.blocks.op_count, 2);
        assert_eq!(stats.total.op_count, 3);
        assert_eq!(stats.total.scalar_copy_count, 2);
        assert_eq!(stats.total.block_call_count, 1);
    }

    #[test]
    fn intrinsic_counts_group_by_descriptor() {
        let program = vec![
            WeavyOp::<(), _>::Intrinsic(TestIntrinsic::ReadI32),
            WeavyOp::Intrinsic(TestIntrinsic::HashField),
            WeavyOp::Intrinsic(TestIntrinsic::ReadI32),
        ];

        let counts = intrinsic_counts(&program);

        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "json",
                name: "read_i32",
            }],
            2
        );
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "hash",
                name: "feed_field",
            }],
            1
        );
    }

    #[test]
    fn canonical_effect_stats_count_builtin_effects() {
        let program = vec![
            WeavyOp::<(), TestIntrinsic>::Memory(MemoryOp::ScalarCopy {
                offset: 0,
                size: 4,
                align: 4,
            }),
            WeavyOp::Memory(MemoryOp::Move {
                src_offset: 8,
                dst_offset: 16,
                size: 8,
                align: 8,
            }),
            WeavyOp::Memory(MemoryOp::Drop {
                offset: 24,
                layout: Layout { size: 16, align: 8 },
            }),
            WeavyOp::Control(ControlOp::CallBlock {
                block: (),
                base_offset: 0,
            }),
        ];

        let stats = effect_stats(&program);

        assert_eq!(stats.op_count, 4);
        assert_eq!(stats.input_read_count, 1);
        assert_eq!(stats.input_advance_count, 1);
        assert_eq!(stats.sink_write_count, 1);
        assert_eq!(stats.may_fail_count, 1);
        assert_eq!(stats.calls_user_code_count, 1);
        assert_eq!(stats.typed_memory_read_count, 1);
        assert_eq!(stats.typed_memory_initialize_count, 1);
        assert_eq!(stats.typed_memory_move_count, 2);
        assert_eq!(stats.typed_memory_drop_count, 1);
        assert_eq!(stats.ordered_count, 2);
        assert_eq!(stats.barrier_count, 2);
        assert_eq!(stats.opaque_count, 0);
    }

    #[test]
    fn intrinsic_effect_stats_are_visible() {
        let program = vec![
            WeavyOp::<(), _>::Intrinsic(TestIntrinsic::ReadI32),
            WeavyOp::Intrinsic(TestIntrinsic::HashField),
        ];

        let stats = effect_stats(&program);

        assert_eq!(stats.op_count, 2);
        assert_eq!(stats.intrinsic_op_count, 2);
        assert_eq!(stats.input_read_count, 1);
        assert_eq!(stats.input_advance_count, 1);
        assert_eq!(stats.sink_write_count, 1);
        assert_eq!(stats.may_fail_count, 1);
        assert_eq!(stats.typed_memory_read_count, 1);
        assert_eq!(stats.typed_memory_initialize_count, 1);
        assert_eq!(stats.opaque_count, 0);
    }
}
