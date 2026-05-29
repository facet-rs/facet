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

/// The descriptor model: how a Rust implementation reads and constructs its own
/// in-memory values for a given schema. Direct facts (offsets, strides, tags,
/// niches) and thunks (named same-language helpers). Process-local, never
/// transmitted, never hashed.
///
/// Spec: "The descriptor model" (`r[descriptors.*]`).
pub mod descriptor {}

/// The intermediate representation in its two forms: the value-shaped tree
/// produced by planning, and the linear op stream it lowers to. One op
/// vocabulary serves both encode and decode.
///
/// Spec: "The intermediate representation" (`r[ir.*]`).
pub mod ir {}

/// Thunk bindings: resolving thunk names to process-local function pointers
/// before an encoder or decoder is built. An unbound name is a build-time error.
///
/// Spec: `r[descriptors.thunk-binding]`.
pub mod thunk {}
