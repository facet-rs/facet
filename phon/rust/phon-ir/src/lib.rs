//! The execution vocabulary shared by every Rust phon backend.
//!
//! Compatibility planning produces an IR; the interpreter (in `phon-engine`)
//! and the JIT (in `phon-jit`) both consume it. Defining the IR here, up front,
//! is what makes the JIT a second consumer of something that exists from the
//! first commit rather than a retrofit. This crate is binding-free: it never
//! touches facet or any reflection (`r[crates.engine-is-binding-free]`). Its
//! op-agnostic program/block carrier lives in `weavy`, shared with other
//! lowered-plan consumers.
//!
//! Spec: `docs/content/spec.md` — "The descriptor model" and "The intermediate
//! representation".

// r[impl crates.engine-is-binding-free]
pub mod descriptor;

pub use descriptor::{
    Access, ByteOwner, ByteRange, Construct, Descriptor, EnumAccess, FieldAccess, FieldDefault,
    Layout, MapAccess, MapStorage, OptionAccess, PointerAccess, Presence, RecordAccess,
    RecordByteOwnership, ResultAccess, SequenceAccess, SequenceStorage, SetAccess, SetStorage, Tag,
    TensorAccess, Thunk, VariantAccess,
};

pub mod ir;

pub use ir::{
    BorrowOp, BorrowThunks, ByteValidator, BytesOp, CanonicalEnumArm, CanonicalEnumOp,
    CanonicalEnumVariantOp, CanonicalMapOp, CanonicalMemError, CanonicalMemLowered,
    CanonicalMemProgram, CanonicalOptionOp, CanonicalPointerOp, CanonicalProgram,
    CanonicalResultOp, CanonicalSeqOp, CanonicalSetOp, CanonicalValueError, CanonicalValueProgram,
    DefaultOp, DefaultThunk, EffectContract, EffectOrdering, EffectResource, EffectStats, EnumArm,
    EnumOp, EnumVariantOp, Lowered, LoweredEffectStats, LoweredMemProgramStats, MapOp, MapThunks,
    MemIntrinsic, MemOp, MemProgram, MemProgramStats, MemoryRegion, Op, OpaqueOp, OpaqueThunks,
    OptionOp, OptionThunks, PointerOp, PointerThunks, Program, ResourceAccess, ResourceEffect,
    ResultOp, ResultThunks, ScalarRunOp, ScalarSegment, SeqOp, SeqThunks, SetOp, SetThunks, SkipOp,
    TypedMemoryAccess, TypedMemoryEffect, ValueIntrinsic, ValueOp, ValueProgram,
    canonical_mem_intrinsic_counts, canonical_mem_lowered, canonical_mem_lowered_effect_stats,
    canonical_mem_lowered_intrinsic_counts, canonical_mem_lowered_stats, canonical_mem_program,
    canonical_mem_program_effect_stats, canonical_mem_program_stats, canonical_program,
    canonical_value_program, lowered_mem_program_stats, mem_lowered_from_canonical,
    mem_program_from_canonical, mem_program_stats, value_lowered_from_canonical,
    value_program_from_canonical,
};

/// Thunk bindings: resolving thunk names to process-local function pointers
/// before an encoder or decoder is built. An unbound name is a build-time error.
///
/// Spec: `r[descriptors.thunk-binding]`.
pub mod thunk {}
