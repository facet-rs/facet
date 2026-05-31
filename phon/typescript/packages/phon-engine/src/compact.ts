// The compact (schema-driven) codec for TypeScript: encode a Value through a
// schema, and decode writer bytes through a schema. Mirrors Rust
// `phon-engine/src/compact.rs` (`encode_kind`/`encode_primitive`,
// `decode_kind`/`decode_primitive`) byte for byte.
//
// The compact format has exactly one source of padding: a primitive scalar pads
// the *absolute* buffer offset up to its own alignment before its bytes
// (`r[impl compact.alignment]`). Length/count prefixes, the option presence
// byte, and the enum variant index are written raw, unaligned; containers add no
// padding of their own — a container's first element pads itself.
//
// `encode` is the round-trip oracle's other half (the compat planner decodes;
// this re-encodes through the reader schema). `decodeRef` is also what the
// planner calls to skip a writer-only field: decode it through its own writer
// schema and discard.
//
// Spec: docs/content/spec.md — "Compact mode".

import {
  alignment,
  ByteSink,
  canonicalKey,
  DecodeError,
  EncodeError,
  formatDatetime,
  formatQName,
  minWireSizeRef,
  parseDatetime,
  parseQName,
  parseUuid,
  Reader,
  Registry,
} from "@bearcove/phon-schema";
import type {
  PhonDateTime,
  PhonQName,
  Primitive,
  SchemaKind,
  SchemaRef,
  Value,
  VariantPayload,
} from "@bearcove/phon-schema";

// ============================================================================
// Public API
// ============================================================================

/// Encode `value` against the schema referenced by `root` in `reg`.
export function encode(value: Value, root: bigint, reg: Registry): Uint8Array {
  const out = new ByteSink();
  encodeRef(out, value, { kind: "concrete", id: root, args: [] }, reg);
  return out.finish();
}

/// Decode a value of schema `root` from `bytes`, rejecting trailing bytes. This
/// is the same-schema decoder (no reconciliation); compat decode lives in
/// plan.ts.
export function decode(bytes: Uint8Array, root: bigint, reg: Registry): Value {
  const r = new Reader(bytes);
  const v = decodeRef(r, { kind: "concrete", id: root, args: [] }, reg, 0);
  if (r.remaining() !== 0) throw new DecodeError(`${r.remaining()} trailing bytes`);
  return v;
}

const MAX_DEPTH = 128;

// ============================================================================
// Encode
// ============================================================================

export function encodeRef(out: ByteSink, value: Value, ref: SchemaRef, reg: Registry): void {
  encodeKind(out, value, reg.resolve(ref), reg);
}

function encodeKind(out: ByteSink, value: Value, kind: SchemaKind, reg: Registry): void {
  switch (kind.kind) {
    case "primitive":
      encodePrimitive(out, value, kind.primitive);
      return;
    case "struct": {
      const obj = asMap(value, "struct");
      for (const f of kind.fields) {
        if (!obj.has(f.name)) throw new EncodeError(`missing struct field '${f.name}'`);
        encodeRef(out, obj.get(f.name)!, f.schema, reg);
      }
      return;
    }
    case "tuple": {
      const arr = asArray(value, "tuple");
      if (arr.length !== kind.elements.length) throw new EncodeError("tuple arity");
      kind.elements.forEach((e, i) => encodeRef(out, arr[i]!, e, reg));
      return;
    }
    case "list":
    case "set": {
      const arr = asArray(value, "list");
      out.u32(arr.length);
      for (const el of arr) encodeRef(out, el, kind.element, reg);
      return;
    }
    case "array": {
      const arr = asArray(value, "array");
      const count = product(kind.dimensions);
      if (BigInt(arr.length) !== count) throw new EncodeError("array shape");
      for (const el of arr) encodeRef(out, el, kind.element, reg);
      return;
    }
    case "map": {
      const obj = asMap(value, "map");
      out.u32(obj.size);
      for (const [k, v] of obj) {
        encodeRef(out, k, kind.key, reg);
        encodeRef(out, v, kind.value, reg);
      }
      return;
    }
    case "option": {
      if (value === null) {
        out.u8(0);
      } else {
        out.u8(1);
        encodeRef(out, value, kind.element, reg);
      }
      return;
    }
    case "enum": {
      const obj = asMap(value, "enum");
      if (obj.size !== 1) throw new EncodeError("single-variant enum object");
      const [name, payload] = obj.entries().next().value as [string, Value];
      const variant = kind.variants.find((v) => v.name === name);
      if (!variant) throw new EncodeError(`unknown variant '${name}'`);
      out.u32(variant.index);
      encodePayload(out, payload, variant.payload, reg);
      return;
    }
    case "dynamic":
    case "tensor":
    case "channel":
    case "external":
      throw new EncodeError(`compact encode unsupported for kind '${kind.kind}'`);
  }
}

function encodePayload(out: ByteSink, value: Value, payload: VariantPayload, reg: Registry): void {
  switch (payload.kind) {
    case "unit":
      return;
    case "newtype":
      encodeRef(out, value, payload.ref, reg);
      return;
    case "tuple": {
      const arr = asArray(value, "variant tuple");
      if (arr.length !== payload.refs.length) throw new EncodeError("variant tuple arity");
      payload.refs.forEach((r, i) => encodeRef(out, arr[i]!, r, reg));
      return;
    }
    case "struct": {
      const obj = asMap(value, "variant struct");
      for (const f of payload.fields) {
        if (!obj.has(f.name)) throw new EncodeError(`missing variant field '${f.name}'`);
        encodeRef(out, obj.get(f.name)!, f.schema, reg);
      }
      return;
    }
  }
}

function encodePrimitive(out: ByteSink, value: Value, p: Primitive): void {
  out.padTo(alignment(p));
  switch (p) {
    case "bool":
      out.u8(asBool(value) ? 1 : 0);
      return;
    case "u8":
      out.u8(Number(BigInt.asUintN(8, asInt(value))));
      return;
    case "u16":
      out.u16(asInt(value));
      return;
    case "u32":
      out.u32(Number(BigInt.asUintN(32, asInt(value))));
      return;
    case "u64":
      out.u64(asInt(value));
      return;
    case "u128":
      out.u128(asInt(value));
      return;
    case "i8":
      out.u8(Number(BigInt.asUintN(8, asInt(value))));
      return;
    case "i16":
      out.i16(asInt(value));
      return;
    case "i32":
      out.i32(asInt(value));
      return;
    case "i64":
      out.i64(asInt(value));
      return;
    case "i128":
      out.i128(asInt(value));
      return;
    case "f32":
      out.f32(asFloat(value));
      return;
    case "f64":
      out.f64(asFloat(value));
      return;
    case "char":
      out.u32(charCode(value));
      return;
    case "string":
      out.str(asString(value));
      return;
    case "bytes":
      out.bytes(asBytes(value));
      return;
    case "unit":
      if (value !== null) throw new EncodeError("expected unit (null)");
      return;
    case "never":
      throw new EncodeError("never is uninhabited");
    case "datetime":
      out.str(formatDatetime(asDateTime(value)));
      return;
    case "uuid":
      out.str(asUuid(value));
      return;
    case "qname":
      out.str(formatQName(asQName(value)));
      return;
  }
}

// ============================================================================
// Decode (same-schema; used directly and for writer-only-field skips)
// ============================================================================

export function decodeRef(r: Reader, ref: SchemaRef, reg: Registry, depth: number): Value {
  if (depth > MAX_DEPTH) throw new DecodeError("maximum nesting depth exceeded");
  return decodeKind(r, reg.resolve(ref), reg, depth);
}

function decodeKind(r: Reader, kind: SchemaKind, reg: Registry, depth: number): Value {
  switch (kind.kind) {
    case "primitive":
      return decodePrimitive(r, kind.primitive);
    case "struct": {
      const obj = new Map<string, Value>();
      for (const f of kind.fields) obj.set(f.name, decodeRef(r, f.schema, reg, depth + 1));
      return obj;
    }
    case "tuple": {
      const a: Value[] = [];
      for (const e of kind.elements) a.push(decodeRef(r, e, reg, depth + 1));
      return a;
    }
    case "list": {
      const n = r.readLen(minWireSizeRef(reg, kind.element));
      const a: Value[] = [];
      for (let i = 0; i < n; i++) a.push(decodeRef(r, kind.element, reg, depth + 1));
      return a;
    }
    case "set": {
      const n = r.readLen(minWireSizeRef(reg, kind.element));
      const a: Value[] = [];
      const seen = new Set<string>();
      for (let i = 0; i < n; i++) {
        const v = decodeRef(r, kind.element, reg, depth + 1);
        if (!addUnique(seen, v)) throw new DecodeError("duplicate set element");
        a.push(v);
      }
      return a;
    }
    case "array": {
      const count = product(kind.dimensions);
      checkFixedCount(count, minWireSizeRef(reg, kind.element), r.remaining());
      const a: Value[] = [];
      for (let i = 0n; i < count; i++) a.push(decodeRef(r, kind.element, reg, depth + 1));
      return a;
    }
    case "map": {
      const n = r.readLen(1);
      const obj = new Map<string, Value>();
      for (let i = 0; i < n; i++) {
        const k = decodeRef(r, kind.key, reg, depth + 1);
        const v = decodeRef(r, kind.value, reg, depth + 1);
        if (typeof k !== "string") throw new DecodeError("map with non-string keys");
        if (obj.has(k)) throw new DecodeError("duplicate map key");
        obj.set(k, v);
      }
      return obj;
    }
    case "option": {
      const b = r.readU8();
      if (b === 0) return null;
      if (b === 1) return decodeRef(r, kind.element, reg, depth + 1);
      throw new DecodeError(`invalid bool byte 0x${b.toString(16)}`);
    }
    case "enum": {
      const index = r.readU32raw();
      const variant = kind.variants.find((v) => v.index === index);
      if (!variant) throw new DecodeError(`bad variant index ${index}`);
      const payload = decodePayloadKind(r, variant.payload, reg, depth);
      return new Map<string, Value>([[variant.name, payload]]);
    }
    case "dynamic":
    case "tensor":
    case "channel":
    case "external":
      throw new DecodeError(`compact decode unsupported for kind '${kind.kind}'`);
  }
}

function decodePayloadKind(r: Reader, payload: VariantPayload, reg: Registry, depth: number): Value {
  switch (payload.kind) {
    case "unit":
      return null;
    case "newtype":
      return decodeRef(r, payload.ref, reg, depth + 1);
    case "tuple": {
      const a: Value[] = [];
      for (const ref of payload.refs) a.push(decodeRef(r, ref, reg, depth + 1));
      return a;
    }
    case "struct": {
      const obj = new Map<string, Value>();
      for (const f of payload.fields) obj.set(f.name, decodeRef(r, f.schema, reg, depth + 1));
      return obj;
    }
  }
}

export function decodePrimitive(r: Reader, p: Primitive): Value {
  r.skipPad(alignment(p));
  switch (p) {
    case "bool":
      return r.readBool();
    case "u8":
      return BigInt(r.readU8());
    case "u16":
      return r.readU16();
    case "u32":
      return r.readU32();
    case "u64":
      return r.readU64();
    case "u128":
      return r.readU128();
    case "i8":
      return r.readI8();
    case "i16":
      return r.readI16();
    case "i32":
      return r.readI32();
    case "i64":
      return r.readI64();
    case "i128":
      return r.readI128();
    case "f32":
      return r.readF32();
    case "f64":
      return r.readF64();
    case "char":
      return { kind: "char", value: String.fromCodePoint(r.readCharCode()) };
    case "string":
      return r.readStr();
    case "bytes":
      return r.readBytes();
    case "unit":
      return null;
    case "never":
      throw new DecodeError("never is uninhabited");
    case "datetime":
      return parseDatetime(r.readStr());
    case "uuid":
      return parseUuid(r.readStr());
    case "qname":
      return parseQName(r.readStr());
  }
}

// ============================================================================
// Dimensions + uniqueness helpers
// ============================================================================

/// `product(dimensions)` with overflow as an error (`r[validate.dimensions]`).
export function product(dimensions: bigint[]): bigint {
  let p = 1n;
  for (const d of dimensions) {
    p *= d;
    if (p > (1n << 64n) - 1n) throw new DecodeError("array dimensions overflow");
  }
  return p;
}

/// Bound a fixed-array element count before the construction loop, mirroring
/// `compact::check_fixed_count` (ZST cap when `minWire == 0`).
export function checkFixedCount(count: bigint, minWire: number, remaining: number): void {
  const max = minWire === 0 ? BigInt(1 << 24) : BigInt(Math.floor(remaining / minWire));
  if (count > max) throw new DecodeError(`length ${count} exceeds ${remaining} bytes remaining`);
}

/// Insert `v`'s canonical key into `seen`, returning false if already present.
/// Reuses the same structural key the self-describing Value codec uses so set
/// uniqueness matches Rust's `HashSet<Value>` (numbers by value, etc.).
function addUnique(seen: Set<string>, v: Value): boolean {
  const key = canonicalKey(v);
  if (seen.has(key)) return false;
  seen.add(key);
  return true;
}

// ============================================================================
// Value coercions (a Value is loosely typed; these enforce the schema's shape)
// ============================================================================

function asInt(v: Value): bigint {
  if (typeof v === "bigint") return v;
  throw new EncodeError(`expected integer, got ${typeof v}`);
}
function asFloat(v: Value): number {
  if (typeof v === "number") return v;
  if (typeof v === "bigint") return Number(v);
  throw new EncodeError(`expected float, got ${typeof v}`);
}
function asBool(v: Value): boolean {
  if (typeof v === "boolean") return v;
  throw new EncodeError("expected bool");
}
function asString(v: Value): string {
  if (typeof v === "string") return v;
  throw new EncodeError("expected string");
}
function asBytes(v: Value): Uint8Array {
  if (v instanceof Uint8Array) return v;
  throw new EncodeError("expected bytes");
}
function asArray(v: Value, what: string): Value[] {
  if (Array.isArray(v)) return v;
  throw new EncodeError(`expected ${what} (array)`);
}
function asMap(v: Value, what: string): Map<string, Value> {
  if (v instanceof Map) return v;
  throw new EncodeError(`expected ${what} (object)`);
}
function charCode(v: Value): number {
  if (typeof v === "object" && v !== null && "kind" in v && v.kind === "char") {
    return v.value.codePointAt(0)!;
  }
  throw new EncodeError("expected char");
}
function asDateTime(v: Value): import("@bearcove/phon-schema").PhonDateTime {
  if (typeof v === "object" && v !== null && "kind" in v && v.kind === "datetime") return v;
  throw new EncodeError("expected datetime");
}
function asUuid(v: Value): string {
  if (typeof v === "object" && v !== null && "kind" in v && v.kind === "uuid") return v.text;
  throw new EncodeError("expected uuid");
}
function asQName(v: Value): import("@bearcove/phon-schema").PhonQName {
  if (typeof v === "object" && v !== null && "kind" in v && v.kind === "qname") return v;
  throw new EncodeError("expected qname");
}
