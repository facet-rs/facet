//! The codegen tool.
//!
//! phon schemas originate from Rust types via facet; codegen turns those schemas
//! into source for the other languages a system speaks. For each target it emits
//! two things per schema: the type definitions a programmer writes against, and
//! the schema itself as a constant — the self-describing phon bytes of the
//! `Schema` value, which the peer parses at startup. A non-Rust peer never
//! re-derives a schema from its generated types; the emitted bytes are the source
//! of truth, so its `SchemaId` matches the Rust origin exactly
//! (`r[codegen.schema-is-source-of-truth]`).
//!
//! Spec: `docs/content/spec.md` — "Codegen".

/// Read schemas to generate from: a facet `Shape` (Rust types) or a received
/// schema bundle.
///
/// Spec: `r[codegen.emits]`.
pub mod source {}

/// Emit Swift: the type definitions plus the schema-bytes constant per schema.
///
/// Spec: "Swift" (language section).
pub mod swift {}

/// Emit TypeScript: discriminated-union types plus the schema-bytes constant,
/// and the property accessors the engine consumes in place of descriptors.
///
/// Spec: "TypeScript" (language section).
pub mod typescript {}
