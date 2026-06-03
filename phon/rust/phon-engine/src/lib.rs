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

pub mod compact;

pub use compact::{CompactError, Registry};

mod compat;

pub mod plan;

pub use plan::Plan;

pub mod interp;

pub use interp::run;

pub mod typed;

/// The hostile-input validation discipline every decode path enforces: length
/// and dimension bounds, depth limits, tag/text checks, set/map uniqueness, and
/// schema-bundle verification.
///
/// Spec: "Decoding untrusted input" (`r[validate.*]`).
pub mod validate {}
