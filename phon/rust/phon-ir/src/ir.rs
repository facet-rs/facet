//! The intermediate representation: a decode plan lowered to a straight,
//! pre-sequenced run of [`Op`]s.
//!
//! Compatibility planning (in `phon-engine`) reconciles a writer schema with a
//! reader schema into a value-shaped *tree*; lowering flattens that tree into a
//! `Program`. Every type-directed decision — which primitive, which field order,
//! which fields to skip or default, how enum variants map — is made once, during
//! lowering, and frozen into the op sequence. What remains in the program is only
//! data-directed control flow that genuinely cannot be precomputed: the element
//! count of a sequence, the active variant of an enum, the presence bit of an
//! option.
//!
//! Two consumers run the same `Program`: the interpreter (a stack machine, in
//! `phon-engine::interp`) and, later, the JIT (copy-and-patch, in `phon-jit`).
//! Defining the IR here is what makes the JIT a second consumer of something that
//! exists from the first commit rather than a retrofit.
//!
//! **Invariant.** Running a complete `Program` against a reader leaves exactly
//! one value on the interpreter's stack — the decoded result. Each variant below
//! documents its own net effect; container ops consume their children's pushes
//! and net `+1`.
//!
//! This first cut is the *decode*, *dynamic-`Value`* path — the mirror of
//! `phon-engine`'s compatibility planner. Encode lowering and the typed
//! (descriptor-driven) path reuse this vocabulary and extend it.
//!
//! Spec: "The intermediate representation" (`r[ir.*]`).

use std::collections::BTreeMap;

use phon_schema::bytes::{Reader, skip_pad};
use phon_schema::{DecodeError, Primitive, SchemaId, SchemaRef};
pub use weavy::ir::{
    ControlOp, EffectContract, EffectOrdering, EffectResource, EffectStats, IntrinsicDescriptor,
    IntrinsicOp, LoweredEffectStats, LoweredProgramStats, MemoryRegion, ProgramStats,
    ResourceAccess, ResourceEffect, TypedMemoryAccess, TypedMemoryEffect, WeavyOp,
};
pub use weavy::mem::{
    BorrowOp, BorrowThunks, ByteValidator, BytesOp, CanonicalEnumOp, CanonicalEnumVariantOp,
    CanonicalMapOp, CanonicalMemError, CanonicalMemLowered, CanonicalMemProgram, CanonicalOptionOp,
    CanonicalPointerOp, CanonicalResultOp, CanonicalSeqOp, CanonicalSetOp, DefaultOp, DefaultThunk,
    LoweredMemProgramStats, LoweringError, MapThunks, MemIntrinsic, MemProgramStats, OpaqueOp,
    OpaqueThunks, OptionThunks, PointerThunks, ResultThunks, ScalarRunOp, ScalarSegment, SeqThunks,
    SetThunks, SkipOp, canonical_mem_intrinsic_counts, canonical_mem_lowered,
    canonical_mem_lowered_effect_stats, canonical_mem_lowered_intrinsic_counts,
    canonical_mem_lowered_stats, canonical_mem_program, canonical_mem_program_effect_stats,
    canonical_mem_program_stats, element_min_wire, group_record_scalars, lower_fixed_array,
    lower_record_fields, lowered_mem_program_stats, mem_lowered_from_canonical,
    mem_program_from_canonical, mem_program_stats, owned_sequence_op, set_op,
};

/// A lowered decode program: a straight run of [`Op`]s executed start to finish.
/// Container bodies (sequence element, map key/value, option payload, enum arm,
/// fixed-array element) are themselves `Program`s — recursion appears only at
/// genuine data-directed control flow, never within a fixed-shape run. A struct
/// of structs of scalars lowers to a single branch-free `Program`.
pub type Program = weavy::Program<Op>;

/// A lowered dynamic-value decode program plus callable blocks for recursive
/// reader schemas. Non-recursive plans have an empty `blocks` map.
pub type ValueProgram = weavy::Lowered<SchemaId, Op>;

/// One lowered decode step. Each reads from the wire and adjusts the
/// interpreter's value stack; the documented net stack effect of a *complete*
/// lowered subtree is always `+1`.
#[derive(Clone, Debug)]
pub enum Op {
    /// Decode a primitive from the wire and push its value. Net `+1`.
    Scalar(Primitive),
    /// Decode a self-describing dynamic value and push it. Net `+1`.
    Dynamic,
    /// Decode using the block program for a recursive reader schema. Net `+1`.
    CallBlock { schema: SchemaId },
    /// Push a null — a reader-only field's default, or a unit variant payload.
    /// Net `+1`.
    Null,
    /// Decode a value by this writer schema reference and discard it: a
    /// writer-only field the reader does not have (`r[compat.skip-writer-only]`).
    /// Net `0`.
    Skip(SchemaRef),
    /// Pop `keys.len()` values (the top of the stack, in order) and assemble an
    /// object pairing each key with its value; push it. The values were pushed by
    /// the immediately preceding ops, in `keys` order. Net `+1`.
    Object { keys: Vec<String> },
    /// Pop `count` values (the top of the stack, in order) into an array; push it.
    /// Used for tuples and tuple variant payloads, whose heterogeneous elements
    /// were lowered inline. Net `+1`.
    Array { count: usize },
    /// Read a `u32` length `n`; run `body` `n` times (each leaves one element on
    /// the stack); collect the `n` elements into an array, rejecting duplicates
    /// when `set`. Push the array. Net `+1`.
    ///
    /// `min_wire` is the element's minimum wire size for the `r[validate.lengths]`
    /// count guard: `0` for a zero-sized element (an empty struct, `unit`, …),
    /// else `1`. A `0` switches the guard to a fixed cap, since the buffer cannot
    /// bound a count of zero-byte elements.
    Seq {
        set: bool,
        min_wire: usize,
        body: Program,
    },
    /// Read a `u32` length `n`; run `key` then `value` `n` times; assemble an
    /// object (string keys), rejecting duplicate keys. Push it. Net `+1`.
    Map { key: Program, value: Program },
    /// Run `body` `product(dimensions)` times (a fixed-shape array); collect into
    /// an array; push it. The product is computed at run time so lowering stays
    /// infallible. `min_wire` bounds the product exactly as in [`Op::Seq`]. Net `+1`.
    FixedArray {
        dimensions: Vec<u64>,
        min_wire: usize,
        body: Program,
    },
    /// Read a presence byte; on `1` run `some` (leaving its value), on `0` push
    /// null. Net `+1`.
    Option { some: Program },
    /// Read a `u32` writer variant index; dispatch to the matching arm, run its
    /// payload, and wrap the result as a single-key object under the reader's
    /// variant name. An index with no arm is a writer-only variant: a decode
    /// error (`r[compat.enum]`). Net `+1`.
    Enum { arms: Vec<EnumArm> },
}

/// One enum arm: the writer's variant index it matches, the reader's name for
/// that variant, and the lowered payload program.
#[derive(Clone, Debug)]
pub struct EnumArm {
    pub writer_index: u32,
    pub reader_name: String,
    pub payload: Program,
}

/// A canonical dynamic-value program whose control and domain work are split
/// through [`weavy::ir::WeavyOp`].
pub type CanonicalProgram = weavy::ir::WeavyProgram<SchemaId, ValueIntrinsic>;

/// A canonical dynamic-value lowered program with recursive schema blocks.
pub type CanonicalValueProgram = weavy::ir::WeavyLowered<SchemaId, ValueIntrinsic>;

/// One executable canonical dynamic-value op.
pub type ValueOp = WeavyOp<SchemaId, ValueIntrinsic>;

/// PHON dynamic-value work that remains domain-specific inside canonical Weavy IR.
#[derive(Clone, Debug)]
pub enum ValueIntrinsic {
    Scalar(Primitive),
    Dynamic,
    Null,
    Skip(SchemaRef),
    Object {
        keys: Vec<String>,
    },
    Array {
        count: usize,
    },
    Seq {
        set: bool,
        min_wire: usize,
        body: CanonicalProgram,
    },
    Map {
        key: CanonicalProgram,
        value: CanonicalProgram,
    },
    FixedArray {
        dimensions: Vec<u64>,
        min_wire: usize,
        body: CanonicalProgram,
    },
    Option {
        some: CanonicalProgram,
    },
    Enum {
        arms: Vec<CanonicalEnumArm>,
    },
}

/// One canonical dynamic-value enum arm.
#[derive(Clone, Debug)]
pub struct CanonicalEnumArm {
    pub writer_index: u32,
    pub reader_name: String,
    pub payload: CanonicalProgram,
}

impl IntrinsicOp for ValueIntrinsic {
    fn descriptor(&self) -> IntrinsicDescriptor {
        IntrinsicDescriptor {
            dialect: "phon.value",
            name: match self {
                Self::Scalar(_) => "scalar",
                Self::Dynamic => "dynamic",
                Self::Null => "null",
                Self::Skip(_) => "skip",
                Self::Object { .. } => "object",
                Self::Array { .. } => "array",
                Self::Seq { .. } => "seq",
                Self::Map { .. } => "map",
                Self::FixedArray { .. } => "fixed_array",
                Self::Option { .. } => "option",
                Self::Enum { .. } => "enum",
            },
        }
    }

    fn effect(&self) -> EffectContract {
        match self {
            Self::Null | Self::Object { .. } | Self::Array { .. } => value_stack_effect(),
            Self::Scalar(_)
            | Self::Dynamic
            | Self::Skip(_)
            | Self::Seq { .. }
            | Self::Map { .. }
            | Self::FixedArray { .. }
            | Self::Option { .. }
            | Self::Enum { .. } => wire_value_stack_effect(),
        }
    }
}

fn value_stack_effect() -> EffectContract {
    EffectContract::new()
        .write_resource(EffectResource::SideChannel("phon.value_stack"))
        .may_fail()
        .may_allocate()
}

fn wire_value_stack_effect() -> EffectContract {
    value_stack_effect().advance_resource(EffectResource::Input("phon.wire"))
}

/// Convert the legacy public dynamic-value program form into canonical Weavy IR.
#[must_use]
pub fn canonical_program(program: &Program) -> CanonicalProgram {
    program.iter().map(canonical_op).collect()
}

/// Convert the legacy public lowered dynamic-value program form into canonical
/// Weavy IR.
#[must_use]
pub fn canonical_value_program(lowered: &ValueProgram) -> CanonicalValueProgram {
    CanonicalValueProgram {
        program: canonical_program(&lowered.program),
        blocks: lowered
            .blocks
            .iter()
            .map(|(id, program)| (*id, canonical_program(program)))
            .collect(),
    }
}

/// Count a canonical PHON dynamic-value program, recursively entering value
/// intrinsics with nested child programs.
#[must_use]
pub fn canonical_value_program_stats(program: &[ValueOp]) -> ProgramStats {
    let mut stats = weavy::ir::program_stats(program);
    for op in program {
        if let WeavyOp::Intrinsic(intrinsic) = op {
            add_canonical_value_intrinsic_stats(intrinsic, &mut stats);
        }
    }
    stats
}

/// Count a canonical PHON dynamic-value lowered program and its block table.
#[must_use]
pub fn canonical_value_lowered_stats(lowered: &CanonicalValueProgram) -> LoweredProgramStats {
    let root = canonical_value_program_stats(&lowered.program);
    let mut blocks = ProgramStats::default();
    for block in lowered.blocks.values() {
        blocks.accumulate(canonical_value_program_stats(block));
    }
    let mut total = root;
    total.accumulate(blocks);

    let mut stats = LoweredProgramStats::default();
    stats.root = root;
    stats.blocks = blocks;
    stats.total = total;
    stats.block_count = lowered.blocks.len();
    stats
}

/// Count PHON dynamic-value intrinsic descriptors in a canonical program,
/// including nested child programs.
#[must_use]
pub fn canonical_value_intrinsic_counts(
    program: &[ValueOp],
) -> BTreeMap<IntrinsicDescriptor, usize> {
    let mut counts = weavy::ir::intrinsic_counts(program);
    for op in program {
        if let WeavyOp::Intrinsic(intrinsic) = op {
            add_canonical_value_intrinsic_counts(intrinsic, &mut counts);
        }
    }
    counts
}

/// Count PHON dynamic-value intrinsic descriptors in a canonical lowered
/// program and its block table.
#[must_use]
pub fn canonical_value_lowered_intrinsic_counts(
    lowered: &CanonicalValueProgram,
) -> BTreeMap<IntrinsicDescriptor, usize> {
    let mut counts = canonical_value_intrinsic_counts(&lowered.program);
    for block in lowered.blocks.values() {
        for (descriptor, count) in canonical_value_intrinsic_counts(block) {
            *counts.entry(descriptor).or_default() += count;
        }
    }
    counts
}

/// Count canonical PHON dynamic-value effects, recursively entering value
/// intrinsics with nested child programs.
#[must_use]
pub fn canonical_value_program_effect_stats(program: &[ValueOp]) -> EffectStats {
    let mut stats = weavy::ir::effect_stats(program);
    for op in program {
        if let WeavyOp::Intrinsic(intrinsic) = op {
            add_canonical_value_intrinsic_effect_stats(intrinsic, &mut stats);
        }
    }
    stats
}

/// Count canonical PHON dynamic-value effects in a lowered program and its
/// block table.
#[must_use]
pub fn canonical_value_lowered_effect_stats(lowered: &CanonicalValueProgram) -> LoweredEffectStats {
    let root = canonical_value_program_effect_stats(&lowered.program);
    let mut blocks = EffectStats::default();
    for block in lowered.blocks.values() {
        blocks.accumulate(canonical_value_program_effect_stats(block));
    }
    LoweredEffectStats::new(root, blocks, lowered.blocks.len())
}

/// Canonical-to-legacy dynamic-value conversion failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanonicalValueError {
    /// Canonical `return` has no legacy [`Op`] equivalent.
    Return,
    /// Legacy [`Op::CallBlock`] cannot represent a base-relative block call.
    NonzeroBaseOffset,
    /// Canonical control operations other than block calls are not legacy value ops.
    Control,
    /// Canonical typed-memory operations are not dynamic-value ops.
    Memory,
    /// Canonical initialization operations are not dynamic-value ops.
    Init,
    /// Canonical aggregate operations are not dynamic-value ops.
    Aggregate,
    /// A future canonical op variant is not representable in legacy value IR.
    Unknown,
}

impl core::fmt::Display for CanonicalValueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Return => write!(f, "canonical return has no Op equivalent"),
            Self::NonzeroBaseOffset => {
                write!(f, "canonical block call base offset has no Op equivalent")
            }
            Self::Control => write!(f, "canonical control op has no Op equivalent"),
            Self::Memory => write!(f, "canonical memory op has no Op equivalent"),
            Self::Init => write!(f, "canonical init op has no Op equivalent"),
            Self::Aggregate => write!(f, "canonical aggregate op has no Op equivalent"),
            Self::Unknown => write!(f, "unknown canonical op has no Op equivalent"),
        }
    }
}

impl std::error::Error for CanonicalValueError {}

/// Convert canonical PHON dynamic-value IR back into the legacy public [`Op`]
/// form.
///
/// This is a compatibility projection for callers still inspecting the old
/// value IR. New execution and analysis should consume [`CanonicalProgram`]
/// directly.
pub fn value_program_from_canonical(
    program: CanonicalProgram,
) -> Result<Program, CanonicalValueError> {
    program.into_iter().map(value_op_from_canonical).collect()
}

/// Convert canonical lowered PHON dynamic-value IR back into the legacy public
/// [`ValueProgram`] form.
pub fn value_lowered_from_canonical(
    lowered: CanonicalValueProgram,
) -> Result<ValueProgram, CanonicalValueError> {
    let program = value_program_from_canonical(lowered.program)?;
    let blocks = lowered
        .blocks
        .into_iter()
        .map(|(id, block)| Ok((id, value_program_from_canonical(block)?)))
        .collect::<Result<_, CanonicalValueError>>()?;
    Ok(ValueProgram { program, blocks })
}

fn canonical_op(op: &Op) -> ValueOp {
    match op {
        Op::CallBlock { schema } => WeavyOp::Control(ControlOp::CallBlock {
            block: *schema,
            base_offset: 0,
        }),
        _ => WeavyOp::Intrinsic(canonical_intrinsic(op)),
    }
}

fn canonical_intrinsic(op: &Op) -> ValueIntrinsic {
    match op {
        Op::Scalar(p) => ValueIntrinsic::Scalar(*p),
        Op::Dynamic => ValueIntrinsic::Dynamic,
        Op::CallBlock { .. } => unreachable!("block calls lower to canonical control ops"),
        Op::Null => ValueIntrinsic::Null,
        Op::Skip(writer_ref) => ValueIntrinsic::Skip(writer_ref.clone()),
        Op::Object { keys } => ValueIntrinsic::Object { keys: keys.clone() },
        Op::Array { count } => ValueIntrinsic::Array { count: *count },
        Op::Seq {
            set,
            min_wire,
            body,
        } => ValueIntrinsic::Seq {
            set: *set,
            min_wire: *min_wire,
            body: canonical_program(body),
        },
        Op::Map { key, value } => ValueIntrinsic::Map {
            key: canonical_program(key),
            value: canonical_program(value),
        },
        Op::FixedArray {
            dimensions,
            min_wire,
            body,
        } => ValueIntrinsic::FixedArray {
            dimensions: dimensions.clone(),
            min_wire: *min_wire,
            body: canonical_program(body),
        },
        Op::Option { some } => ValueIntrinsic::Option {
            some: canonical_program(some),
        },
        Op::Enum { arms } => ValueIntrinsic::Enum {
            arms: arms
                .iter()
                .map(|arm| CanonicalEnumArm {
                    writer_index: arm.writer_index,
                    reader_name: arm.reader_name.clone(),
                    payload: canonical_program(&arm.payload),
                })
                .collect(),
        },
    }
}

fn value_op_from_canonical(op: ValueOp) -> Result<Op, CanonicalValueError> {
    Ok(match op {
        WeavyOp::Control(ControlOp::CallBlock { block, base_offset }) => {
            if base_offset != 0 {
                return Err(CanonicalValueError::NonzeroBaseOffset);
            }
            Op::CallBlock { schema: block }
        }
        WeavyOp::Control(ControlOp::Return) => return Err(CanonicalValueError::Return),
        WeavyOp::Control(_) => return Err(CanonicalValueError::Control),
        WeavyOp::Memory(_) => return Err(CanonicalValueError::Memory),
        WeavyOp::Init(_) => return Err(CanonicalValueError::Init),
        WeavyOp::Aggregate(_) => return Err(CanonicalValueError::Aggregate),
        WeavyOp::Intrinsic(intrinsic) => return value_op_from_intrinsic(intrinsic),
        _ => return Err(CanonicalValueError::Unknown),
    })
}

fn value_op_from_intrinsic(intrinsic: ValueIntrinsic) -> Result<Op, CanonicalValueError> {
    Ok(match intrinsic {
        ValueIntrinsic::Scalar(p) => Op::Scalar(p),
        ValueIntrinsic::Dynamic => Op::Dynamic,
        ValueIntrinsic::Null => Op::Null,
        ValueIntrinsic::Skip(writer_ref) => Op::Skip(writer_ref),
        ValueIntrinsic::Object { keys } => Op::Object { keys },
        ValueIntrinsic::Array { count } => Op::Array { count },
        ValueIntrinsic::Seq {
            set,
            min_wire,
            body,
        } => Op::Seq {
            set,
            min_wire,
            body: value_program_from_canonical(body)?,
        },
        ValueIntrinsic::Map { key, value } => Op::Map {
            key: value_program_from_canonical(key)?,
            value: value_program_from_canonical(value)?,
        },
        ValueIntrinsic::FixedArray {
            dimensions,
            min_wire,
            body,
        } => Op::FixedArray {
            dimensions,
            min_wire,
            body: value_program_from_canonical(body)?,
        },
        ValueIntrinsic::Option { some } => Op::Option {
            some: value_program_from_canonical(some)?,
        },
        ValueIntrinsic::Enum { arms } => Op::Enum {
            arms: arms
                .into_iter()
                .map(|arm| {
                    Ok(EnumArm {
                        writer_index: arm.writer_index,
                        reader_name: arm.reader_name,
                        payload: value_program_from_canonical(arm.payload)?,
                    })
                })
                .collect::<Result<_, CanonicalValueError>>()?,
        },
    })
}

fn add_canonical_value_intrinsic_stats(intrinsic: &ValueIntrinsic, stats: &mut ProgramStats) {
    match intrinsic {
        ValueIntrinsic::Seq { body, .. } | ValueIntrinsic::FixedArray { body, .. } => {
            stats.accumulate(canonical_value_program_stats(body));
        }
        ValueIntrinsic::Map { key, value } => {
            stats.accumulate(canonical_value_program_stats(key));
            stats.accumulate(canonical_value_program_stats(value));
        }
        ValueIntrinsic::Option { some } => {
            stats.accumulate(canonical_value_program_stats(some));
        }
        ValueIntrinsic::Enum { arms } => {
            for arm in arms {
                stats.accumulate(canonical_value_program_stats(&arm.payload));
            }
        }
        ValueIntrinsic::Scalar(_)
        | ValueIntrinsic::Dynamic
        | ValueIntrinsic::Null
        | ValueIntrinsic::Skip(_)
        | ValueIntrinsic::Object { .. }
        | ValueIntrinsic::Array { .. } => {}
    }
}

fn add_canonical_value_intrinsic_counts(
    intrinsic: &ValueIntrinsic,
    counts: &mut BTreeMap<IntrinsicDescriptor, usize>,
) {
    match intrinsic {
        ValueIntrinsic::Seq { body, .. } | ValueIntrinsic::FixedArray { body, .. } => {
            add_canonical_value_program_intrinsic_counts(body, counts);
        }
        ValueIntrinsic::Map { key, value } => {
            add_canonical_value_program_intrinsic_counts(key, counts);
            add_canonical_value_program_intrinsic_counts(value, counts);
        }
        ValueIntrinsic::Option { some } => {
            add_canonical_value_program_intrinsic_counts(some, counts);
        }
        ValueIntrinsic::Enum { arms } => {
            for arm in arms {
                add_canonical_value_program_intrinsic_counts(&arm.payload, counts);
            }
        }
        ValueIntrinsic::Scalar(_)
        | ValueIntrinsic::Dynamic
        | ValueIntrinsic::Null
        | ValueIntrinsic::Skip(_)
        | ValueIntrinsic::Object { .. }
        | ValueIntrinsic::Array { .. } => {}
    }
}

fn add_canonical_value_intrinsic_effect_stats(intrinsic: &ValueIntrinsic, stats: &mut EffectStats) {
    match intrinsic {
        ValueIntrinsic::Seq { body, .. } | ValueIntrinsic::FixedArray { body, .. } => {
            stats.accumulate(canonical_value_program_effect_stats(body));
        }
        ValueIntrinsic::Map { key, value } => {
            stats.accumulate(canonical_value_program_effect_stats(key));
            stats.accumulate(canonical_value_program_effect_stats(value));
        }
        ValueIntrinsic::Option { some } => {
            stats.accumulate(canonical_value_program_effect_stats(some));
        }
        ValueIntrinsic::Enum { arms } => {
            for arm in arms {
                stats.accumulate(canonical_value_program_effect_stats(&arm.payload));
            }
        }
        ValueIntrinsic::Scalar(_)
        | ValueIntrinsic::Dynamic
        | ValueIntrinsic::Null
        | ValueIntrinsic::Skip(_)
        | ValueIntrinsic::Object { .. }
        | ValueIntrinsic::Array { .. } => {}
    }
}

fn add_canonical_value_program_intrinsic_counts(
    program: &[ValueOp],
    counts: &mut BTreeMap<IntrinsicDescriptor, usize>,
) {
    for (descriptor, count) in canonical_value_intrinsic_counts(program) {
        *counts.entry(descriptor).or_default() += count;
    }
}

/// A lowered *typed* program: the memory side of the IR. Where [`Program`] builds
/// a dynamic `facet_value::Value` on a stack, a `MemProgram` moves bytes between
/// the wire and a value's in-memory layout, at offsets the descriptor supplies
/// (`r[ir.memory]`).
// r[impl ir.one-vocabulary]
pub type MemProgram = weavy::mem::MemProgram<SchemaId>;

/// One typed memory step for Phon schemas.
pub type MemOp = weavy::mem::MemOp<SchemaId>;

/// A lowered typed program: the root op stream plus the per-schema block
/// programs that [`MemOp::CallBlock`] calls into.
pub type Lowered = weavy::Lowered<SchemaId, MemOp>;

pub type SeqOp = weavy::mem::SeqOp<SchemaId>;
pub type SetOp = weavy::mem::SetOp<SchemaId>;
pub type OptionOp = weavy::mem::OptionOp<SchemaId>;
pub type EnumOp = weavy::mem::EnumOp<SchemaId>;
pub type EnumVariantOp = weavy::mem::EnumVariantOp<SchemaId>;
pub type MapOp = weavy::mem::MapOp<SchemaId>;
pub type ResultOp = weavy::mem::ResultOp<SchemaId>;
pub type PointerOp = weavy::mem::PointerOp<SchemaId>;

/// Advance the reader past one writer value described by `op`, writing nothing to
/// memory. The wire-shape mirror of the decode cursor moves, sharing the
/// `read_len`/`skip_pad`/`read_u8`/`read_u32` and bounds checks the decoders use.
///
/// One implementation, two consumers: the interpreter's `MemOp::SkipWire` arm and
/// the JIT's `phon_stencil_skipwire` wrapper both call this, so writer-only fields
/// are consumed identically regardless of decode engine.
///
/// An enum wire index matching no arm is hostile input; here it becomes
/// [`DecodeError::Malformed`] (the JIT maps its skip-failure status the same way).
///
/// Spec: `r[compat.skip-writer-only]`, `r[compact.alignment]`.
///
/// # Errors
/// [`DecodeError`] for truncated input, a bad `Option` presence byte, or an enum
/// wire index with no matching arm.
// r[impl compat.skip-writer-only]
// r[impl compact.alignment]
pub fn skip(r: &mut Reader, op: &SkipOp) -> Result<(), DecodeError> {
    match op {
        SkipOp::Scalar { size, align } => {
            skip_pad(r, *align)?;
            r.read_slice(*size)?;
            Ok(())
        }
        SkipOp::Bytes { stride, elem_align } => {
            let count = r.read_len((*stride).max(1))?;
            if count > 0 {
                skip_pad(r, *elem_align)?;
            }
            r.read_slice(count * stride)?;
            Ok(())
        }
        SkipOp::Seq(element) => {
            let count = r.read_len(1)?;
            for _ in 0..count {
                skip(r, element)?;
            }
            Ok(())
        }
        SkipOp::Option(inner) => match r.read_u8()? {
            0 => Ok(()),
            1 => skip(r, inner),
            b => Err(DecodeError::InvalidBool(b)),
        },
        SkipOp::Enum(arms) => {
            let wire_index = r.read_u32()?;
            let (_, fields) = arms
                .iter()
                .find(|(idx, _)| *idx == wire_index)
                .ok_or(DecodeError::Malformed("enum variant index out of range"))?;
            for f in fields {
                skip(r, f)?;
            }
            Ok(())
        }
        SkipOp::Map(key, value) => {
            let count = r.read_len(1)?;
            for _ in 0..count {
                skip(r, key)?;
                skip(r, value)?;
            }
            Ok(())
        }
        SkipOp::Struct(fields) => {
            for f in fields {
                skip(r, f)?;
            }
            Ok(())
        }
        // The self-describing codec is self-delimiting: decode one value
        // (consuming exactly its bytes) and discard it.
        SkipOp::Dynamic => phon_schema::read_value(r).map(|_| ()),
    }
}

/// Coalesce adjacent scalar copies that are contiguous in both wire and memory.
// r[impl ir.inlining]
#[must_use]
pub fn fuse(program: MemProgram) -> MemProgram {
    weavy::mem::fuse(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn canonical_value_projection_preserves_nested_legacy_ops() {
        let legacy = ValueProgram {
            program: vec![
                Op::Seq {
                    set: true,
                    min_wire: 1,
                    body: vec![Op::Option {
                        some: vec![Op::Scalar(Primitive::U8)],
                    }],
                },
                Op::CallBlock {
                    schema: SchemaId(9),
                },
            ],
            blocks: BTreeMap::from([(SchemaId(9), vec![Op::Null])]),
        };

        let projected = value_lowered_from_canonical(canonical_value_program(&legacy)).unwrap();

        match projected.program.as_slice() {
            [
                Op::Seq {
                    set,
                    min_wire,
                    body,
                },
                Op::CallBlock { schema },
            ] => {
                assert!(*set);
                assert_eq!(*min_wire, 1);
                assert_eq!(*schema, SchemaId(9));
                match body.as_slice() {
                    [Op::Option { some }] => match some.as_slice() {
                        [Op::Scalar(Primitive::U8)] => {}
                        other => panic!("unexpected option payload: {other:?}"),
                    },
                    other => panic!("unexpected sequence body: {other:?}"),
                }
            }
            other => panic!("unexpected projected program: {other:?}"),
        }
        match projected.blocks.get(&SchemaId(9)).map(Vec::as_slice) {
            Some([Op::Null]) => {}
            other => panic!("unexpected projected block: {other:?}"),
        }
    }

    #[test]
    fn canonical_value_projection_rejects_non_legacy_control() {
        let err = value_program_from_canonical(vec![WeavyOp::Control(ControlOp::Return)])
            .expect_err("return should not project to legacy value IR");
        assert_eq!(err, CanonicalValueError::Return);

        let err = value_program_from_canonical(vec![WeavyOp::Control(ControlOp::CallBlock {
            block: SchemaId(1),
            base_offset: 4,
        })])
        .expect_err("offset block call should not project to legacy value IR");
        assert_eq!(err, CanonicalValueError::NonzeroBaseOffset);
    }

    #[test]
    fn canonical_value_analysis_enters_nested_intrinsic_programs() {
        let program = vec![
            WeavyOp::Intrinsic(ValueIntrinsic::Seq {
                set: false,
                min_wire: 1,
                body: vec![WeavyOp::Intrinsic(ValueIntrinsic::Scalar(Primitive::U8))],
            }),
            WeavyOp::Intrinsic(ValueIntrinsic::Map {
                key: vec![WeavyOp::Intrinsic(ValueIntrinsic::Scalar(
                    Primitive::String,
                ))],
                value: vec![WeavyOp::Intrinsic(ValueIntrinsic::Option {
                    some: vec![WeavyOp::Intrinsic(ValueIntrinsic::Dynamic)],
                })],
            }),
            WeavyOp::Intrinsic(ValueIntrinsic::Enum {
                arms: vec![CanonicalEnumArm {
                    writer_index: 7,
                    reader_name: "Ready".to_string(),
                    payload: vec![WeavyOp::Intrinsic(ValueIntrinsic::Null)],
                }],
            }),
        ];

        let stats = canonical_value_program_stats(&program);
        assert_eq!(stats.op_count, 8);
        assert_eq!(stats.intrinsic_op_count, 8);

        let counts = canonical_value_intrinsic_counts(&program);
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "phon.value",
                name: "scalar",
            }],
            2
        );
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "phon.value",
                name: "option",
            }],
            1
        );
        assert_eq!(
            counts[&IntrinsicDescriptor {
                dialect: "phon.value",
                name: "null",
            }],
            1
        );

        let effects = canonical_value_program_effect_stats(&program);
        assert_eq!(effects.op_count, 8);
        assert_eq!(effects.intrinsic_op_count, 8);
        assert_eq!(effects.side_channel_count, 8);
        assert_eq!(effects.input_advance_count, 7);

        let lowered = CanonicalValueProgram {
            program: vec![WeavyOp::Control(ControlOp::CallBlock {
                block: SchemaId(99),
                base_offset: 0,
            })],
            blocks: BTreeMap::from([(SchemaId(99), program)]),
        };
        let lowered_stats = canonical_value_lowered_stats(&lowered);
        assert_eq!(lowered_stats.root.op_count, 1);
        assert_eq!(lowered_stats.blocks.op_count, 8);
        assert_eq!(lowered_stats.total.op_count, 9);
        assert_eq!(lowered_stats.block_count, 1);

        let lowered_effects = canonical_value_lowered_effect_stats(&lowered);
        assert_eq!(lowered_effects.total.op_count, 9);
        assert_eq!(lowered_effects.total.side_channel_count, 8);

        let lowered_counts = canonical_value_lowered_intrinsic_counts(&lowered);
        assert_eq!(
            lowered_counts[&IntrinsicDescriptor {
                dialect: "phon.value",
                name: "scalar",
            }],
            2
        );
    }
}
