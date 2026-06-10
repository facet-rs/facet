// The phon front door for TypeScript: the ergonomic typed encode/decode API.
//
// TypeScript peers consume phon codegen output — discriminated-union types plus
// the schema-bytes constant for each schema, which this package's engine parses
// at startup. A TS peer never re-derives a schema from its generated types; the
// emitted bytes are the source of truth, so its SchemaId matches the Rust origin
// exactly (r[codegen.schema-is-source-of-truth]).
//
// Spec: docs/content/spec.md — "TypeScript", "Codegen".

// r[impl crates.concern-separation]
export { PHON_SCHEMA_PACKAGE } from "@bearcove/phon-schema";
export { PHON_ENGINE_PACKAGE } from "@bearcove/phon-engine";
