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

/// Minimal intrinsic metadata hook.
///
/// Effect contracts are intentionally not modeled here yet; the next step is to
/// grow this surface from names into explicit read/write/failure contracts.
pub trait IntrinsicOp {
    /// Return this intrinsic's stable descriptor.
    fn descriptor(&self) -> IntrinsicDescriptor;
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
}
