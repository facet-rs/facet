// phon's TypeScript engine: the compact (schema-driven) codec, compatibility
// planning, the interpreter baseline, and the JIT.
//
// The engine plans a writer schema against a reader schema once (`buildPlan`),
// baking in every writer<->reader discrepancy, then decodes either with the
// interpreter (`decodeWithPlan`) or with a `new Function`-compiled decoder
// (`compilePlan`). Both produce the identical reader-shaped Value; the
// conformance corpus asserts byte-for-byte agreement with the Rust reference.
//
// Spec: docs/content/spec.md — "Compact mode", "Compatibility",
// "Decoding untrusted input", "TypeScript".

export const PHON_ENGINE_PACKAGE = "@bearcove/phon-engine";

// Compact schema-driven codec.
export { encode, decode as decodeCompact, encodeRef, decodeRef, decodePrimitive, product, checkFixedCount } from "./compact.ts";

// Compatibility planning + interpreter.
export {
  buildPlan,
  decodeWithPlan,
  decode,
  IncompatibleError,
  WriterOnlyVariantError,
} from "./plan.ts";
export type { Plan, Node, Step, Payload, StructPlan, VariantPlan } from "./plan.ts";

// The new Function JIT (with transparent interpreter fallback under strict CSP).
export {
  compile,
  compilePlan,
  interpretPlan,
  compiledSource,
  jitAvailable,
  compileEncoder,
  compiledEncoderSource,
} from "./jit.ts";
export type { CompiledDecoder, CompiledEncoder } from "./jit.ts";

// The ergonomic typed front door (plain objects, numbers, {tag,value} enums).
export { decodeTyped, encodeTyped } from "./typed.ts";
export type { Typed, TypedEnum } from "./typed.ts";
