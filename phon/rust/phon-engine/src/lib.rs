//! The backend-blind baseline engine.
//!
//! Compact (schema-driven) encode/decode, compatibility planning that turns a
//! `(writer schema, reader schema, descriptor)` triple into IR, and the
//! interpreter that runs that IR. The interpreter always works, on every
//! platform, including those where a JIT cannot run (`r[exec.interpreter-baseline]`).
//! Like `phon-ir`, this crate is binding-free: it consumes descriptors and an
//! IR and reaches for no reflection (`r[crates.engine-is-binding-free]`).
//!
//! Spec: `docs/content/spec.md` — "Compact mode", "Compatibility", "Decoding",
//! "Decoding untrusted input".

/// The compact codec: tagless, schema-driven encode and decode, including
/// alignment padding for borrowable runs.
///
/// Spec: "Compact mode" (`r[compact.*]`).
pub mod compact {}

/// Compatibility planning: build a translation plan from writer schema to reader
/// schema (failing fast if impossible), fuse it with the descriptor, and emit
/// the IR tree the backends lower.
///
/// Spec: "Compatibility" (`r[compat.*]`) and `r[ir.two-forms]`.
pub mod plan {}

/// The interpreter: walk the lowered IR directly. The reference semantics every
/// JIT must match exactly.
///
/// Spec: `r[exec.interpreter-baseline]`, `r[ir.total]`.
pub mod interp {}

/// The hostile-input validation discipline every decode path enforces: length
/// and dimension bounds, depth limits, tag/text checks, set/map uniqueness, and
/// schema-bundle verification.
///
/// Spec: "Decoding untrusted input" (`r[validate.*]`).
pub mod validate {}
