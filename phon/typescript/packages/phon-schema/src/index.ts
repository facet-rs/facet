// phon's wire contract for TypeScript: the schema model, content-derived schema
// identity, the dynamic Value type, and the self-describing codec.
//
// A phon Schema in TypeScript is a discriminated union with matching fields to
// the canonical Rust definitions, producing and consuming identical
// self-describing phon bytes (r[type-system.canonical-form]). blake3 (via
// @noble/hashes) computes the SchemaId so it matches Rust and Swift byte for
// byte.
//
// Spec: docs/content/spec.md — "Type system", "Schema identity",
// "Self-describing mode".

export const PHON_SCHEMA_PACKAGE = "@bearcove/phon-schema";
