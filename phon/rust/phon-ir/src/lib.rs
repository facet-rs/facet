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

// r[impl crates.engine-is-binding-free]
pub mod descriptor;

pub use descriptor::{
    Access, Construct, Descriptor, EnumAccess, FieldAccess, FieldDefault, Layout, MapAccess,
    MapStorage, OptionAccess, PointerAccess, Presence, RecordAccess, ResultAccess, SequenceAccess,
    SequenceStorage, SetAccess, SetStorage, Tag, TensorAccess, Thunk, VariantAccess,
};

pub mod ir;

pub use ir::{
    BorrowOp, BorrowThunks, ByteValidator, BytesOp, DefaultOp, DefaultThunk, EnumArm, EnumOp,
    EnumVariantOp, Lowered, MapOp, MapThunks, MemOp, MemProgram, Op, OpaqueOp, OpaqueThunks,
    OptionOp, OptionThunks, PointerOp, PointerThunks, Program, ResultOp, ResultThunks, SeqOp,
    SeqThunks, SetOp, SetThunks, SkipOp,
};

/// Thunk bindings: resolving thunk names to process-local function pointers
/// before an encoder or decoder is built. An unbound name is a build-time error.
///
/// Spec: `r[descriptors.thunk-binding]`.
pub mod thunk {}
