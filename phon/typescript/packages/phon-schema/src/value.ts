// The self-describing (tag-led) `Value` codec for TypeScript.
//
// This mirrors the Rust coarse `Value` codec in
// `rust/phon-schema/src/selfdescribing.rs` (`write_value` / `dec_value`): it
// reads a one-byte tag, then the body the tag describes, producing a JS value,
// and writes a JS value back out to byte-identical self-describing bytes.
//
// It is *coarse* on purpose (spec `r[value]`): the rich wire tag set folds onto
// one number, one array, one object — the exact integer width and precise
// container kind are recovered only on the typed path (@bearcove/phon-engine).
// What this codec guarantees is that the bytes round-trip: decode then
// re-encode reproduces the input byte for byte, the cross-language oracle for
// the `conformance/values/*.phon` corpus.
//
// Integers use `bigint` so 64-bit and 128-bit values survive without precision
// loss; floats use `number`; bytes use `Uint8Array`. char, datetime, uuid, and
// qname decode to small tagged objects so they re-encode under their own tag
// rather than collapsing into a plain string.
//
// The byte-level primitives (tags, Reader, ByteSink, errors) live in wire.ts so
// the compact/typed engine shares them.
//
// Spec: docs/content/spec.md — "Self-describing mode", `r[value]`,
// `r[value.extended-kinds]`.

import {
  ByteSink,
  DecodeError,
  EncodeError,
  I64_MAX,
  I64_MIN,
  I128_MAX,
  I128_MIN,
  MAX_DEPTH,
  Reader,
  Tag,
  U64_MAX,
  U128_MAX,
  hex,
} from "./wire.ts";

export { Tag, DecodeError, EncodeError } from "./wire.ts";

// ============================================================================
// The Value model
// ============================================================================

/// A Unicode scalar that decoded from a `char` tag (0x0E). Kept distinct from a
/// plain string so it re-encodes under the `char` tag.
export interface PhonChar {
  readonly kind: "char";
  /// The single Unicode scalar, as a JS string (may be one or two UTF-16 units).
  readonly value: string;
}

/// A UUID (tag 0x1C), carrying its canonical lowercase-hyphenated string.
export interface PhonUuid {
  readonly kind: "uuid";
  /// Canonical `8-4-4-4-12` lowercase hex.
  readonly text: string;
}

/// A qualified name (tag 0x1D), carrying its James Clark notation.
export interface PhonQName {
  readonly kind: "qname";
  /// `null` for a local name, else the namespace URI.
  readonly namespace: string | null;
  readonly local: string;
}

/// A date/time (tag 0x1B), retaining the structured fields parsed from its
/// canonical RFC 3339 / ISO 8601 string. The four shapes mirror Rust's
/// `DateTimeKind`.
export type PhonDateTime =
  | { readonly kind: "datetime"; readonly shape: "date"; readonly year: number; readonly month: number; readonly day: number }
  | { readonly kind: "datetime"; readonly shape: "time"; readonly hour: number; readonly minute: number; readonly second: number; readonly nanos: number }
  | {
      readonly kind: "datetime";
      readonly shape: "localDateTime";
      readonly year: number;
      readonly month: number;
      readonly day: number;
      readonly hour: number;
      readonly minute: number;
      readonly second: number;
      readonly nanos: number;
    }
  | {
      readonly kind: "datetime";
      readonly shape: "offset";
      readonly year: number;
      readonly month: number;
      readonly day: number;
      readonly hour: number;
      readonly minute: number;
      readonly second: number;
      readonly nanos: number;
      /// Offset from UTC in minutes (0 renders as `Z`).
      readonly offsetMinutes: number;
    };

/// phon's dynamic value (`r[value]`), coarser than the wire tag set:
///  - `null`               — null / option-none / unit
///  - `boolean`            — bool
///  - `bigint`             — any integer width (u8..u128, i8..i128)
///  - `number`             — float (f32 widened to f64, or f64)
///  - `string`             — string
///  - `Uint8Array`         — bytes
///  - `PhonChar`           — char
///  - `Value[]`            — list / set / tuple / array / tensor
///  - `Map<string, Value>` — map (string keys) / struct / enum
///  - `Value[]` pairs      — map with non-string keys (array of [k, v] pairs)
///  - `PhonDateTime` / `PhonUuid` / `PhonQName` — the extended kinds
export type Value =
  | null
  | boolean
  | bigint
  | number
  | string
  | Uint8Array
  | PhonChar
  | PhonUuid
  | PhonQName
  | PhonDateTime
  | Value[]
  | Map<string, Value>;

// ============================================================================
// Decode
// ============================================================================

/// Decode a self-describing `Value` from `bytes`, rejecting trailing bytes.
/// Throws `DecodeError` on any malformed input.
export function decodeValue(bytes: Uint8Array): Value {
  const r = new Reader(bytes);
  const v = decValue(r, 0);
  if (r.remaining() !== 0) {
    throw new DecodeError(`${r.remaining()} trailing bytes after value`);
  }
  return v;
}

function checkDepth(depth: number): void {
  if (depth > MAX_DEPTH) {
    throw new DecodeError("maximum nesting depth exceeded");
  }
}

/// Read one self-describing Value from an existing `Reader` (no trailing-byte
/// check) — what the compact engine calls for a `dynamic` field. Mirrors Rust
/// `read_value`.
export function readValue(r: Reader, depth = 0): Value {
  return decValue(r, depth);
}

/// Write one self-describing Value into an existing `ByteSink` — the compact
/// engine's `dynamic` field encoder. Mirrors Rust `write_value`.
export function writeValueInto(out: ByteSink, value: Value): void {
  writeValue(out, value);
}

// Mirror of Rust `dec_value`: read a tag, then fold its body onto a coarse Value.
function decValue(r: Reader, depth: number): Value {
  checkDepth(depth);
  const t = r.readU8();
  switch (t) {
    case Tag.UNIT:
    case Tag.OPTION_NONE:
      return null;
    case Tag.BOOL:
      return r.readBool();
    case Tag.U8:
      return BigInt(r.readU8());
    case Tag.U16:
      return r.readU16();
    case Tag.U32:
      return r.readU32();
    case Tag.U64:
      return r.readU64();
    case Tag.U128:
      return r.readU128();
    case Tag.I8:
      return r.readI8();
    case Tag.I16:
      return r.readI16();
    case Tag.I32:
      return r.readI32();
    case Tag.I64:
      return r.readI64();
    case Tag.I128:
      return r.readI128();
    case Tag.F32:
      return r.readF32();
    case Tag.F64:
      return r.readF64();
    case Tag.CHAR:
      return { kind: "char", value: String.fromCodePoint(r.readCharCode()) };
    case Tag.STRING:
      return r.readStr();
    case Tag.BYTES:
      return r.readBytes();
    // list and tuple both fold to a flat array.
    case Tag.LIST:
    case Tag.TUPLE: {
      const n = r.readLen(1);
      const a: Value[] = [];
      for (let i = 0; i < n; i++) a.push(decValue(r, depth + 1));
      return a;
    }
    // set: like a list but elements must be unique (`r[validate.uniqueness]`).
    case Tag.SET: {
      const n = r.readLen(1);
      const a: Value[] = [];
      const seen = new Set<string>();
      for (let i = 0; i < n; i++) {
        const elem = decValue(r, depth + 1);
        const key = canonicalKey(elem);
        if (seen.has(key)) throw new DecodeError("duplicate set element");
        seen.add(key);
        a.push(elem);
      }
      return a;
    }
    case Tag.MAP:
      return decMap(r, depth);
    case Tag.ARRAY:
    case Tag.TENSOR:
      return decDimensioned(r, depth);
    case Tag.STRUCT:
      return decStruct(r, depth);
    case Tag.ENUM:
      return decEnum(r, depth);
    case Tag.OPTION_SOME:
      return decValue(r, depth + 1);
    case Tag.DATETIME:
      return parseDatetime(r.readStr());
    case Tag.UUID:
      return parseUuid(r.readStr());
    case Tag.QNAME:
      return parseQName(r.readStr());
    default:
      throw new DecodeError(`unknown tag ${hex(t)}`);
  }
}

/// A `map` folds to an object (Map) when its keys are all strings, else to an
/// array of `[key, value]` pairs. Keys must be unique (`r[validate.uniqueness]`).
function decMap(r: Reader, depth: number): Value {
  const n = r.readLen(2);
  const entries: [Value, Value][] = [];
  const seen = new Set<string>();
  let allString = true;
  for (let i = 0; i < n; i++) {
    const key = decValue(r, depth + 1);
    const val = decValue(r, depth + 1);
    const k = canonicalKey(key);
    if (seen.has(k)) throw new DecodeError("duplicate map key");
    seen.add(k);
    if (typeof key !== "string") allString = false;
    entries.push([key, val]);
  }
  if (allString) {
    const o = new Map<string, Value>();
    for (const [key, val] of entries) o.set(key as string, val);
    return o;
  }
  return entries.map(([key, val]) => [key, val] as Value[]);
}

/// `array` and `tensor` fold to a flat array. Dimensions are validated
/// (`r[validate.dimensions]`): rank and the element product are bounded by the
/// buffer, and the product uses checked arithmetic.
function decDimensioned(r: Reader, depth: number): Value {
  const rank = r.readU32raw();
  if (rank * 8 > r.remaining()) {
    throw new DecodeError(`length ${rank} exceeds ${r.remaining()} bytes remaining`);
  }
  let product = 1n;
  for (let i = 0; i < rank; i++) {
    product *= r.readU64();
  }
  if (product > BigInt(r.remaining())) {
    throw new DecodeError(`length ${product} exceeds ${r.remaining()} bytes remaining`);
  }
  const a: Value[] = [];
  for (let i = 0n; i < product; i++) a.push(decValue(r, depth + 1));
  return a;
}

/// A `struct` folds to an object keyed by field name (names must be unique).
function decStruct(r: Reader, depth: number): Value {
  r.readStr(); // struct name, folded away
  const n = r.readLen(1);
  const o = new Map<string, Value>();
  for (let i = 0; i < n; i++) {
    const field = r.readStr();
    if (o.has(field)) throw new DecodeError("duplicate map key");
    o.set(field, decValue(r, depth + 1));
  }
  return o;
}

/// An `enum` folds to a one-entry object mapping the variant name to its single
/// payload value (`r[self-describing.enum-payload]`).
function decEnum(r: Reader, depth: number): Value {
  const variant = r.readStr();
  const payload = decValue(r, depth + 1);
  const o = new Map<string, Value>();
  o.set(variant, payload);
  return o;
}

/// A structural key for uniqueness checks, matching Rust's value equality:
/// numbers compare by mathematical value regardless of width, so a u64 `1` and
/// an i64 `1` collide as the Rust `HashSet<Value>` would.
export function canonicalKey(v: Value): string {
  if (v === null) return "null";
  if (typeof v === "boolean") return `b:${v}`;
  if (typeof v === "bigint") return `n:${v}`;
  if (typeof v === "number") return `f:${v}`;
  if (typeof v === "string") return `s:${v}`;
  if (v instanceof Uint8Array) return `y:${Array.from(v).join(",")}`;
  if (Array.isArray(v)) return `a:[${v.map(canonicalKey).join(",")}]`;
  if (v instanceof Map) {
    return `o:{${[...v.entries()].map(([k, val]) => `${k}=${canonicalKey(val)}`).join(",")}}`;
  }
  switch (v.kind) {
    case "char":
      return `c:${v.value}`;
    case "uuid":
      return `u:${v.text}`;
    case "qname":
      return `q:${v.namespace ?? ""}|${v.local}`;
    case "datetime":
      return `d:${formatDatetime(v)}`;
  }
}

// ============================================================================
// Encode
// ============================================================================

/// Encode a `Value` to self-describing bytes, matching Rust's `write_value`
/// byte for byte. Throws `EncodeError` for an integer no wire tag can hold.
export function encodeValue(value: Value): Uint8Array {
  const out = new ByteSink();
  writeValue(out, value);
  return out.finish();
}

// Mirror of Rust `write_value`: each coarse Value case has one fixed tag, so the
// bytes are canonical across implementations (`r[value]`).
function writeValue(out: ByteSink, value: Value): void {
  if (value === null) {
    out.u8(Tag.OPTION_NONE);
    return;
  }
  if (typeof value === "boolean") {
    out.u8(Tag.BOOL);
    out.u8(value ? 1 : 0);
    return;
  }
  if (typeof value === "bigint") {
    encNumber(out, value);
    return;
  }
  if (typeof value === "number") {
    // A plain JS number is a float: f32 widened to f64, or f64 (`r[value]`).
    out.u8(Tag.F64);
    out.f64(value);
    return;
  }
  if (typeof value === "string") {
    out.u8(Tag.STRING);
    out.str(value);
    return;
  }
  if (value instanceof Uint8Array) {
    out.u8(Tag.BYTES);
    out.bytes(value);
    return;
  }
  if (Array.isArray(value)) {
    out.u8(Tag.LIST);
    out.u32(value.length);
    for (const elem of value) writeValue(out, elem);
    return;
  }
  if (value instanceof Map) {
    out.u8(Tag.MAP);
    out.u32(value.size);
    for (const [key, val] of value) {
      out.u8(Tag.STRING);
      out.str(key);
      writeValue(out, val);
    }
    return;
  }
  switch (value.kind) {
    case "char":
      out.u8(Tag.CHAR);
      out.u32(value.value.codePointAt(0)!);
      return;
    case "uuid":
      out.u8(Tag.UUID);
      out.str(value.text);
      return;
    case "qname":
      out.u8(Tag.QNAME);
      out.str(formatQName(value));
      return;
    case "datetime":
      out.u8(Tag.DATETIME);
      out.str(formatDatetime(value));
      return;
  }
}

/// A number's wire tag follows the narrowest of i64/u64/i128/u128 that holds it
/// (mirrors Rust `enc_number` and `VNumber`'s magnitude canonicalization, so the
/// choice is deterministic and byte-identical).
function encNumber(out: ByteSink, n: bigint): void {
  if (n >= I64_MIN && n <= I64_MAX) {
    out.u8(Tag.I64);
    out.i64(n);
  } else if (n >= 0n && n <= U64_MAX) {
    out.u8(Tag.U64);
    out.u64(n);
  } else if (n >= I128_MIN && n <= I128_MAX) {
    out.u8(Tag.I128);
    out.i128(n);
  } else if (n >= 0n && n <= U128_MAX) {
    out.u8(Tag.U128);
    out.u128(n);
  } else {
    throw new EncodeError(`integer ${n} does not fit any phon integer width`);
  }
}

// ============================================================================
// Extended kinds — canonical string formats (`r[value.extended-kinds]`)
// Exported so the compact/typed engine reuses the exact same parse/format.
// ============================================================================

/// `550e8400-e29b-41d4-a716-446655440000` (lowercase, hyphenated).
export function parseUuid(s: string): PhonUuid {
  const hexStr = s.replace(/-/g, "");
  if (hexStr.length !== 32 || !/^[0-9a-fA-F]{32}$/.test(hexStr)) {
    throw new DecodeError("malformed value: uuid");
  }
  // Canonicalize to lowercase hyphenated, matching Rust's `uuid_string`.
  const h = hexStr.toLowerCase();
  const text = `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20, 32)}`;
  return { kind: "uuid", text };
}

/// James Clark notation: `{namespace}local`, or `local` with no namespace.
export function parseQName(s: string): PhonQName {
  if (s.startsWith("{")) {
    const close = s.indexOf("}");
    if (close < 0) throw new DecodeError("malformed value: qname");
    return { kind: "qname", namespace: s.slice(1, close), local: s.slice(close + 1) };
  }
  return { kind: "qname", namespace: null, local: s };
}

export function formatQName(q: PhonQName): string {
  return q.namespace === null ? q.local : `{${q.namespace}}${q.local}`;
}

function pad(n: number, width: number): string {
  return n.toString().padStart(width, "0");
}

/// RFC 3339 / ISO 8601 (`r[value.extended-kinds]`): `T` marks a datetime, `:` a
/// time, `-` a date; fractional seconds are `.` plus nine digits when nonzero;
/// the offset is `Z` or `±HH:MM`. Mirrors Rust `datetime_string`.
export function formatDatetime(d: PhonDateTime): string {
  const date = () => `${pad((d as { year: number }).year, 4)}-${pad((d as { month: number }).month, 2)}-${pad((d as { day: number }).day, 2)}`;
  const time = () => {
    const dt = d as { hour: number; minute: number; second: number; nanos: number };
    let t = `${pad(dt.hour, 2)}:${pad(dt.minute, 2)}:${pad(dt.second, 2)}`;
    if (dt.nanos !== 0) t += `.${pad(dt.nanos, 9)}`;
    return t;
  };
  switch (d.shape) {
    case "date":
      return date();
    case "time":
      return time();
    case "localDateTime":
      return `${date()}T${time()}`;
    case "offset": {
      let offset: string;
      if (d.offsetMinutes === 0) {
        offset = "Z";
      } else {
        const sign = d.offsetMinutes < 0 ? "-" : "+";
        const abs = Math.abs(d.offsetMinutes);
        offset = `${sign}${pad(Math.floor(abs / 60), 2)}:${pad(abs % 60, 2)}`;
      }
      return `${date()}T${time()}${offset}`;
    }
  }
}

export function parseDatetime(s: string): PhonDateTime {
  const bad = () => new DecodeError("malformed value: datetime");
  const tIdx = s.indexOf("T");
  if (tIdx >= 0) {
    const datePart = s.slice(0, tIdx);
    const rest = s.slice(tIdx + 1);
    const { year, month, day } = parseDate(datePart, bad);
    // The offset starts at a trailing `Z`, `+`, or `-`; the time has none.
    const offIdx = findOffset(rest);
    const timePart = offIdx >= 0 ? rest.slice(0, offIdx) : rest;
    const offPart = offIdx >= 0 ? rest.slice(offIdx) : null;
    const { hour, minute, second, nanos } = parseTime(timePart, bad);
    if (offPart === null) {
      return { kind: "datetime", shape: "localDateTime", year, month, day, hour, minute, second, nanos };
    }
    const offsetMinutes = parseOffset(offPart, bad);
    return { kind: "datetime", shape: "offset", year, month, day, hour, minute, second, nanos, offsetMinutes };
  }
  if (s.includes(":")) {
    const { hour, minute, second, nanos } = parseTime(s, bad);
    return { kind: "datetime", shape: "time", hour, minute, second, nanos };
  }
  if (s.includes("-")) {
    const { year, month, day } = parseDate(s, bad);
    return { kind: "datetime", shape: "date", year, month, day };
  }
  throw bad();
}

/// The offset starts at a trailing `Z`, `+`, or `-` in the time portion (which
/// itself never contains those characters). Mirrors Rust's `rest.find`.
function findOffset(rest: string): number {
  for (let i = 0; i < rest.length; i++) {
    const c = rest[i];
    if (c === "Z" || c === "+" || c === "-") return i;
  }
  return -1;
}

function parseInt10(s: string, bad: () => DecodeError): number {
  if (!/^-?\d+$/.test(s)) throw bad();
  return Number.parseInt(s, 10);
}

function parseDate(s: string, bad: () => DecodeError): { year: number; month: number; day: number } {
  // `[-]YYYY-MM-DD`: split day then month off the right so a negative year's
  // leading `-` stays with the year.
  const dayIdx = s.lastIndexOf("-");
  if (dayIdx <= 0) throw bad();
  const rest = s.slice(0, dayIdx);
  const day = parseInt10(s.slice(dayIdx + 1), bad);
  const monthIdx = rest.lastIndexOf("-");
  if (monthIdx <= 0) throw bad();
  const year = parseInt10(rest.slice(0, monthIdx), bad);
  const month = parseInt10(rest.slice(monthIdx + 1), bad);
  return { year, month, day };
}

function parseTime(s: string, bad: () => DecodeError): { hour: number; minute: number; second: number; nanos: number } {
  const dotIdx = s.indexOf(".");
  const hms = dotIdx >= 0 ? s.slice(0, dotIdx) : s;
  const frac = dotIdx >= 0 ? s.slice(dotIdx + 1) : null;
  const parts = hms.split(":");
  if (parts.length !== 3) throw bad();
  const hour = parseInt10(parts[0]!, bad);
  const minute = parseInt10(parts[1]!, bad);
  const second = parseInt10(parts[2]!, bad);
  let nanos = 0;
  if (frac !== null) {
    if (frac.length < 1 || frac.length > 9 || !/^\d+$/.test(frac)) throw bad();
    nanos = Number.parseInt(frac.padEnd(9, "0"), 10);
  }
  return { hour, minute, second, nanos };
}

function parseOffset(s: string, bad: () => DecodeError): number {
  if (s === "Z") return 0;
  const sign = s[0] === "+" ? 1 : s[0] === "-" ? -1 : null;
  if (sign === null) throw bad();
  const colon = s.indexOf(":", 1);
  if (colon < 0) throw bad();
  const h = parseInt10(s.slice(1, colon), bad);
  const m = parseInt10(s.slice(colon + 1), bad);
  const total = sign * (h * 60 + m);
  if (total < -0x8000 || total > 0x7fff) throw bad();
  return total;
}
