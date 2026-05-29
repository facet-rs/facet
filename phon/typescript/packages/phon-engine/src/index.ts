// phon's TypeScript engine: the compact (schema-driven) codec, compatibility
// planning, the interpreter baseline, and the JIT.
//
// TypeScript has no descriptor model — values are GC'd objects accessed by
// property — so the engine consumes codegen-emitted accessor functions rather
// than descriptor data. Its JIT is generated JavaScript handed to new Function(),
// light enough to live here rather than in a separate package.
//
// Spec: docs/content/spec.md — "Compact mode", "Compatibility",
// "Decoding untrusted input", "TypeScript".

export const PHON_ENGINE_PACKAGE = "@bearcove/phon-engine";
