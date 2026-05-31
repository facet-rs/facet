// The typed front door: ergonomic JavaScript values that match what phon's TS
// codegen emits, layered over the proven Value engine.
//
// The compact engine (plan.ts/jit.ts) decodes to phon's coarse dynamic `Value`
// (Maps, bigints) — faithful to Rust's dynamic plan path and the oracle. A vox
// TS peer wants ergonomic shapes instead: plain objects for structs, a
// `{ tag, value }` discriminated union for enums, `number` for integers that fit
// it, `bigint` only for 64/128-bit, and canonical strings for char/datetime/
// uuid/qname. `decodeTyped` runs the (JIT-accelerated) Value decode and remaps
// the result with a cheap schema-driven pass; `encodeTyped` is the inverse.
//
// The remap is O(value) — negligible against the parse — and keeps the Value
// engine the single proven source of truth. (Fusing the remap into the JIT's
// codegen is a possible future optimization; profiling hasn't demanded it.)
//
// Representation (mirrors what codegen should emit):
//   struct            -> plain object { field: typed }
//   enum              -> { tag: "Variant", value: typed }   (value null for unit)
//   tuple/list/set/array -> typed[]
//   map               -> Map<string, typed>   (ordered; objects would reorder
//                         integer-like keys and break round-trips)
//   option            -> typed | null
//   u8..u32 / i8..i32 -> number;  u64/u128/i64/i128 -> bigint
//   f32/f64           -> number;  bool -> boolean
//   char/datetime/uuid/qname -> canonical string
//   bytes             -> Uint8Array;  unit -> null
//   dynamic           -> the coarse Value (inherently untyped)
//
// Spec: docs/content/spec.md — "TypeScript", "Codegen".

import {
  formatDatetime,
  formatQName,
  parseDatetime,
  parseQName,
  parseUuid,
  Registry,
} from "@bearcove/phon-schema";
import type {
  PhonChar,
  PhonDateTime,
  PhonQName,
  PhonUuid,
  Primitive,
  SchemaKind,
  SchemaRef,
  Value,
  VariantPayload,
} from "@bearcove/phon-schema";
import { encode } from "./compact.ts";
import { compile } from "./jit.ts";

/// A discriminated-union enum value: the variant name plus its (typed) payload —
/// `null` for a unit variant, the inner value for a newtype, an array for a
/// tuple variant, an object for a struct variant.
export interface TypedEnum {
  readonly tag: string;
  readonly value: Typed;
}

/// An ergonomic decoded value (see the module header for the full mapping).
export type Typed =
  | null
  | boolean
  | number
  | bigint
  | string
  | Uint8Array
  | TypedEnum
  | Typed[]
  | { [field: string]: Typed }
  | Map<string, Typed>
  | Value; // dynamic passthrough

const SMALL_INTS = new Set<Primitive>(["u8", "u16", "u32", "i8", "i16", "i32"]);

// ============================================================================
// Public API
// ============================================================================

/// Decode writer compact `bytes` into an ergonomic typed value shaped by the
/// reader schema, reconciling writer<->reader drift. Uses the JIT when
/// available (interpreter fallback under strict CSP); pass `{ jit }` to force.
export function decodeTyped(
  bytes: Uint8Array,
  writerRoot: bigint,
  readerRoot: bigint,
  reg: Registry,
  opts?: { jit?: boolean },
): Typed {
  const value = compile(writerRoot, readerRoot, reg, opts)(bytes);
  return valueToTypedRef(value, { kind: "concrete", id: readerRoot, args: [] }, reg);
}

/// Encode an ergonomic typed value against the schema referenced by `root`.
export function encodeTyped(typed: Typed, root: bigint, reg: Registry): Uint8Array {
  const value = typedToValueRef(typed, { kind: "concrete", id: root, args: [] }, reg);
  return encode(value, root, reg);
}

// ============================================================================
// Value -> Typed (decode side)
// ============================================================================

function valueToTypedRef(v: Value, ref: SchemaRef, reg: Registry): Typed {
  return valueToTyped(v, reg.resolve(ref), reg);
}

function valueToTyped(v: Value, kind: SchemaKind, reg: Registry): Typed {
  switch (kind.kind) {
    case "primitive":
      return primitiveToTyped(v, kind.primitive);
    case "struct": {
      const m = v as Map<string, Value>;
      const o: { [field: string]: Typed } = {};
      for (const f of kind.fields) o[f.name] = valueToTypedRef(m.get(f.name) as Value, f.schema, reg);
      return o;
    }
    case "enum": {
      const m = v as Map<string, Value>;
      const [tag, payload] = m.entries().next().value as [string, Value];
      const variant = kind.variants.find((vt) => vt.name === tag);
      if (!variant) throw new Error(`enum value tag '${tag}' not in schema`);
      return { tag, value: payloadValueToTyped(payload, variant.payload, reg) };
    }
    case "tuple":
      return (v as Value[]).map((e, i) => valueToTypedRef(e, kind.elements[i]!, reg));
    case "list":
    case "set":
      return (v as Value[]).map((e) => valueToTypedRef(e, kind.element, reg));
    case "array":
      return (v as Value[]).map((e) => valueToTypedRef(e, kind.element, reg));
    case "map": {
      const m = v as Map<string, Value>;
      const out = new Map<string, Typed>();
      for (const [k, val] of m) out.set(k, valueToTypedRef(val, kind.value, reg));
      return out;
    }
    case "option":
      return v === null ? null : valueToTypedRef(v, kind.element, reg);
    case "dynamic":
      return v; // inherently untyped — keep the coarse Value
    case "tensor":
    case "channel":
    case "external":
      throw new Error(`typed decode unsupported for kind '${kind.kind}'`);
  }
}

function payloadValueToTyped(v: Value, payload: VariantPayload, reg: Registry): Typed {
  switch (payload.kind) {
    case "unit":
      return null;
    case "newtype":
      return valueToTypedRef(v, payload.ref, reg);
    case "tuple":
      return (v as Value[]).map((e, i) => valueToTypedRef(e, payload.refs[i]!, reg));
    case "struct": {
      const m = v as Map<string, Value>;
      const o: { [field: string]: Typed } = {};
      for (const f of payload.fields) o[f.name] = valueToTypedRef(m.get(f.name) as Value, f.schema, reg);
      return o;
    }
  }
}

function primitiveToTyped(v: Value, p: Primitive): Typed {
  if (SMALL_INTS.has(p)) return Number(v as bigint);
  switch (p) {
    case "u64":
    case "u128":
    case "i64":
    case "i128":
      return v as bigint;
    case "char":
      return (v as PhonChar).value;
    case "datetime":
      return formatDatetime(v as PhonDateTime);
    case "uuid":
      return (v as PhonUuid).text;
    case "qname":
      return formatQName(v as PhonQName);
    default:
      // bool, f32, f64, string, bytes, unit — already the ergonomic shape.
      return v as Typed;
  }
}

// ============================================================================
// Typed -> Value (encode side)
// ============================================================================

function typedToValueRef(t: Typed, ref: SchemaRef, reg: Registry): Value {
  return typedToValue(t, reg.resolve(ref), reg);
}

function typedToValue(t: Typed, kind: SchemaKind, reg: Registry): Value {
  switch (kind.kind) {
    case "primitive":
      return primitiveToValue(t, kind.primitive);
    case "struct": {
      const o = t as { [field: string]: Typed };
      const m = new Map<string, Value>();
      for (const f of kind.fields) m.set(f.name, typedToValueRef(o[f.name] as Typed, f.schema, reg));
      return m;
    }
    case "enum": {
      const e = t as TypedEnum;
      const variant = kind.variants.find((vt) => vt.name === e.tag);
      if (!variant) throw new Error(`enum tag '${e.tag}' not in schema`);
      return new Map<string, Value>([[e.tag, payloadTypedToValue(e.value, variant.payload, reg)]]);
    }
    case "tuple":
      return (t as Typed[]).map((e, i) => typedToValueRef(e, kind.elements[i]!, reg));
    case "list":
    case "set":
      return (t as Typed[]).map((e) => typedToValueRef(e, kind.element, reg));
    case "array":
      return (t as Typed[]).map((e) => typedToValueRef(e, kind.element, reg));
    case "map": {
      const m = t as Map<string, Typed>;
      const out = new Map<string, Value>();
      for (const [k, val] of m) out.set(k, typedToValueRef(val, kind.value, reg));
      return out;
    }
    case "option":
      return t === null ? null : typedToValueRef(t, kind.element, reg);
    case "dynamic":
      return t as Value;
    case "tensor":
    case "channel":
    case "external":
      throw new Error(`typed encode unsupported for kind '${kind.kind}'`);
  }
}

function payloadTypedToValue(t: Typed, payload: VariantPayload, reg: Registry): Value {
  switch (payload.kind) {
    case "unit":
      return null;
    case "newtype":
      return typedToValueRef(t, payload.ref, reg);
    case "tuple":
      return (t as Typed[]).map((e, i) => typedToValueRef(e, payload.refs[i]!, reg));
    case "struct": {
      const o = t as { [field: string]: Typed };
      const m = new Map<string, Value>();
      for (const f of payload.fields) m.set(f.name, typedToValueRef(o[f.name] as Typed, f.schema, reg));
      return m;
    }
  }
}

function primitiveToValue(t: Typed, p: Primitive): Value {
  if (SMALL_INTS.has(p)) return BigInt(t as number);
  switch (p) {
    case "u64":
    case "u128":
    case "i64":
    case "i128":
      return t as bigint;
    case "char":
      return { kind: "char", value: t as string };
    case "datetime":
      return parseDatetime(t as string);
    case "uuid":
      return parseUuid(t as string);
    case "qname":
      return parseQName(t as string);
    default:
      // bool, f32, f64, string, bytes, unit.
      return t as Value;
  }
}
