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

use phon_schema::{Primitive, SchemaRef};

/// A lowered decode program: a straight run of [`Op`]s executed start to finish.
/// Container bodies (sequence element, map key/value, option payload, enum arm,
/// fixed-array element) are themselves `Program`s — recursion appears only at
/// genuine data-directed control flow, never within a fixed-shape run. A struct
/// of structs of scalars lowers to a single branch-free `Program`.
pub type Program = Vec<Op>;

/// One lowered decode step. Each reads from the wire and adjusts the
/// interpreter's value stack; the documented net stack effect of a *complete*
/// lowered subtree is always `+1`.
#[derive(Clone, Debug)]
pub enum Op {
    /// Decode a primitive from the wire and push its value. Net `+1`.
    Scalar(Primitive),
    /// Decode a self-describing dynamic value and push it. Net `+1`.
    Dynamic,
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
    Seq { set: bool, body: Program },
    /// Read a `u32` length `n`; run `key` then `value` `n` times; assemble an
    /// object (string keys), rejecting duplicate keys. Push it. Net `+1`.
    Map { key: Program, value: Program },
    /// Run `body` `product(dimensions)` times (a fixed-shape array); collect into
    /// an array; push it. The product is computed at run time so lowering stays
    /// infallible. Net `+1`.
    FixedArray { dimensions: Vec<u64>, body: Program },
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
