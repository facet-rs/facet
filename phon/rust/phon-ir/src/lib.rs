//! The execution vocabulary shared by every Rust phon backend.
//!
//! Compatibility planning produces an IR; the interpreter (in `phon-engine`)
//! and the JIT (in `phon-jit`) both consume it. Defining the IR here, up front,
//! is what makes the JIT a second consumer of something that exists from the
//! first commit rather than a retrofit. This crate is binding-free: it never
//! touches facet or any reflection (`r[crates.engine-is-binding-free]`).
//!
//! Spec: `docs/content/spec.md` — "The descriptor model" and "The intermediate
//! representation".

pub mod descriptor;

pub use descriptor::{
    Access, Construct, Descriptor, EnumAccess, FieldAccess, Layout, MapAccess, OptionAccess,
    Presence, RecordAccess, SequenceAccess, SequenceStorage, Tag, TensorAccess, Thunk,
    VariantAccess,
};

pub mod ir;

pub use ir::{BytesOp, ByteValidator, EnumArm, MemOp, MemProgram, Op, Program, SeqOp, SeqThunks};

/// Thunk bindings: resolving thunk names to process-local function pointers
/// before an encoder or decoder is built. An unbound name is a build-time error.
///
/// Spec: `r[descriptors.thunk-binding]`.
pub mod thunk {}
