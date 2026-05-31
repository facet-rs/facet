// phon's wire contract for TypeScript: the schema model, schema identity, the
// dynamic Value type, the self-describing codec, and the shared wire primitives
// (Reader/ByteSink/tags) the compact engine builds on.
//
// A phon Schema in TypeScript is a discriminated union with matching fields to
// the canonical Rust definitions, producing and consuming identical
// self-describing phon bytes (`r[type-system.canonical-form]`).
//
// Spec: docs/content/spec.md — "Type system", "Schema identity",
// "Self-describing mode".

export const PHON_SCHEMA_PACKAGE = "@bearcove/phon-schema";

// Shared wire primitives.
export {
  Tag,
  DecodeError,
  EncodeError,
  Reader,
  ByteSink,
  MAX_DEPTH,
  ZST_COUNT_CAP,
  hex,
  hexToBytes,
  bytesToHex,
} from "./wire.ts";

// The self-describing Value codec + model.
export {
  decodeValue,
  encodeValue,
  readValue,
  writeValueInto,
  canonicalKey,
  parseUuid,
  parseQName,
  formatQName,
  parseDatetime,
  formatDatetime,
} from "./value.ts";
export type { Value, PhonChar, PhonUuid, PhonQName, PhonDateTime } from "./value.ts";

// The schema model + self-describing schema parser + alignment analysis.
export { schemaFromBytes, Registry, alignment, minWireSizeRef } from "./schema.ts";
export type {
  Schema,
  SchemaKind,
  SchemaRef,
  Field,
  Variant,
  VariantPayload,
  Primitive,
  ChannelDirection,
} from "./schema.ts";
