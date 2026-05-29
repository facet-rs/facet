//! The phon front door — the only crate (besides `phon-codegen`) that depends on
//! facet.
//!
//! This is where Rust types become phon: facet metadata is turned into a
//! [`schema`] and a [`descriptor`], and the typed `encode::<T>` / `decode::<T>`
//! API wraps the engine. With the `jit` feature on, the typed API routes through
//! `phon-jit`; with it off, it runs the `phon-engine` interpreter — same results,
//! different speed (`r[crates.jit-opt-in]`).
//!
//! Spec: `docs/content/spec.md` — "Crates and packages" and "Rust".

pub use phon_schema as schema_contract;

/// Derive a phon [`Schema`](phon_schema::schema) and a `phon-ir` descriptor from
/// a facet `Shape`. The bridge from Rust's type metadata to phon's portable
/// schema and process-local descriptor.
///
/// Spec: "Rust" (language section), `r[descriptors.fact-driven]`.
pub mod derive {}

/// The ergonomic typed API: `encode::<T>` and `decode::<T>`, resolving thunk
/// bindings and selecting interpreter vs. JIT.
///
/// Spec: `r[exec.interpreter-baseline]`, `r[exec.jit-optional]`.
pub mod api {}

/// phon's dynamic value, re-exported for convenience at the front door. It *is*
/// `facet_value::Value` (see `phon_schema::value`); there is no separate phon
/// value type and no conversion between them.
///
/// Spec: "Value" (`r[value]`).
pub mod value {
    pub use phon_schema::value::Value;
}
