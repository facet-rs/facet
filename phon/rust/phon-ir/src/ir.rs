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

use phon_schema::bytes::{Reader, skip_pad};
use phon_schema::{DecodeError, Primitive, SchemaId, SchemaRef};
pub use weavy::mem::{
    BorrowOp, BorrowThunks, ByteValidator, BytesOp, DefaultOp, DefaultThunk, MapThunks, OpaqueOp,
    OpaqueThunks, OptionThunks, PointerThunks, ResultThunks, SeqThunks, SetThunks, SkipOp,
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
