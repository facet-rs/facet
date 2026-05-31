// phon's schema model for TypeScript, plus the self-describing schema parser.
//
// A `Schema` is the structural description the compact/typed engine plans and
// codes against. The model mirrors the canonical Rust definitions in
// `rust/phon-schema/src/schema.rs`; `schemaFromBytes` is a byte-for-byte port of
// Rust `schema_from_bytes` (`selfdescribing.rs` `dec_schema`/`dec_kind`/
// `dec_ref`), so a TS peer reconstructs the exact same schema a Rust peer
// emitted — schema bytes are the source of truth (`r[codegen.schema-is-source-
// of-truth]`).
//
// Schemas reference each other by `SchemaId` (a content-derived u64); a
// `Registry` resolves those refs. Primitive ids are content-derived too and
// recognized intrinsically — the registry is seeded with a primitive id->tag
// table rather than recomputing the blake3 ids here.
//
// Spec: docs/content/spec.md — "Type system", "Schema identity",
// "Self-describing mode".

import { DecodeError, MAX_DEPTH, Reader, Tag } from "./wire.ts";

// ============================================================================
// The schema model (mirror of rust/phon-schema/src/schema.rs)
// ============================================================================

/// A leaf type. Represented by its tag string (the same strings Rust's
/// `Primitive::tag` produces), which doubles as the discriminant.
export type Primitive =
  | "bool"
  | "u8"
  | "u16"
  | "u32"
  | "u64"
  | "u128"
  | "i8"
  | "i16"
  | "i32"
  | "i64"
  | "i128"
  | "f32"
  | "f64"
  | "char"
  | "string"
  | "bytes"
  | "datetime"
  | "uuid"
  | "qname"
  | "unit"
  | "never";

const PRIMITIVE_TAGS = new Set<string>([
  "bool", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128",
  "f32", "f64", "char", "string", "bytes", "datetime", "uuid", "qname", "unit", "never",
]);

function asPrimitive(tag: string): Primitive {
  if (!PRIMITIVE_TAGS.has(tag)) throw new DecodeError(`unknown primitive '${tag}'`);
  return tag as Primitive;
}

/// A reference to a schema: concrete (by content-derived id, with type args) or
/// a type variable (parametric schemas). The TS engine supports concrete,
/// non-generic refs; variables and type args are carried but rejected at use.
export type SchemaRef =
  | { readonly kind: "concrete"; readonly id: bigint; readonly args: SchemaRef[] }
  | { readonly kind: "var"; readonly name: string };

export interface Field {
  readonly name: string;
  readonly schema: SchemaRef;
  /// A reader field that is *not* required may be absent from the writer and
  /// filled with its default (`r[compat.reader-only-fields]`).
  readonly required: boolean;
}

export type VariantPayload =
  | { readonly kind: "unit" }
  | { readonly kind: "newtype"; readonly ref: SchemaRef }
  | { readonly kind: "tuple"; readonly refs: SchemaRef[] }
  | { readonly kind: "struct"; readonly fields: Field[] };

export interface Variant {
  readonly name: string;
  /// The wire discriminant (a u32). Variants are matched across schemas by name;
  /// the index is what travels on the wire.
  readonly index: number;
  readonly payload: VariantPayload;
}

export type ChannelDirection = "tx" | "rx";

export type SchemaKind =
  | { readonly kind: "primitive"; readonly primitive: Primitive }
  | { readonly kind: "struct"; readonly name: string; readonly fields: Field[] }
  | { readonly kind: "enum"; readonly name: string; readonly variants: Variant[] }
  | { readonly kind: "tuple"; readonly elements: SchemaRef[] }
  | { readonly kind: "list"; readonly element: SchemaRef }
  | { readonly kind: "set"; readonly element: SchemaRef }
  | { readonly kind: "map"; readonly key: SchemaRef; readonly value: SchemaRef }
  | { readonly kind: "array"; readonly element: SchemaRef; readonly dimensions: bigint[] }
  | { readonly kind: "tensor"; readonly element: SchemaRef; readonly rank: number | null }
  | { readonly kind: "option"; readonly element: SchemaRef }
  | { readonly kind: "channel"; readonly direction: ChannelDirection; readonly element: SchemaRef }
  | { readonly kind: "dynamic" }
  | { readonly kind: "external"; readonly external: string; readonly metadata: SchemaRef | null };

export interface Schema {
  readonly id: bigint;
  readonly typeParams: string[];
  readonly kind: SchemaKind;
}

// ============================================================================
// Alignment & zero-sized analysis (mirror of compact.rs `alignment` /
// `min_wire_size_ref` / `is_zero_sized_*`)
// ============================================================================

/// The compact-mode alignment of a primitive scalar — the only source of wire
/// padding (`r[impl compact.alignment]`). Everything else is byte-aligned.
export function alignment(p: Primitive): number {
  switch (p) {
    case "u16":
    case "i16":
      return 2;
    case "u32":
    case "i32":
    case "f32":
    case "char":
      return 4;
    case "u64":
    case "i64":
    case "f64":
      return 8;
    case "u128":
    case "i128":
      return 16;
    default:
      return 1;
  }
}

const MIN_WIRE_DEPTH = 64;

/// `0` when a sequence element provably encodes to zero bytes (a `unit`, an
/// empty struct/tuple, an array of those), else `1` — the value to hand
/// `Reader.readLen` (`r[validate.lengths]`).
export function minWireSizeRef(reg: Registry, ref: SchemaRef): number {
  return isZeroSizedRef(reg, ref, 0) ? 0 : 1;
}

function isZeroSizedRef(reg: Registry, ref: SchemaRef, depth: number): boolean {
  if (depth > MIN_WIRE_DEPTH) return false;
  let kind: SchemaKind;
  try {
    kind = reg.resolve(ref);
  } catch {
    return false;
  }
  return isZeroSizedKind(reg, kind, depth);
}

function isZeroSizedKind(reg: Registry, kind: SchemaKind, depth: number): boolean {
  switch (kind.kind) {
    case "primitive":
      return kind.primitive === "unit";
    case "struct":
      return kind.fields.every((f) => isZeroSizedRef(reg, f.schema, depth + 1));
    case "tuple":
      return kind.elements.every((e) => isZeroSizedRef(reg, e, depth + 1));
    case "array":
      return isZeroSizedRef(reg, kind.element, depth + 1);
    default:
      return false;
  }
}

// ============================================================================
// Registry
// ============================================================================

/// Resolves `SchemaRef`s to `SchemaKind`s. Composite schemas are keyed by id;
/// primitive ids map to their tag. The engine plans and codes by walking
/// resolved kinds.
export class Registry {
  private readonly composites = new Map<bigint, Schema>();
  private readonly primitives = new Map<bigint, Primitive>();

  /// Build from parsed composite schemas plus the primitive id->tag table the
  /// corpus carries.
  constructor(schemas: Iterable<Schema>, primitiveTable: Iterable<{ id: bigint; tag: Primitive }>) {
    for (const s of schemas) this.composites.set(s.id, s);
    for (const { id, tag } of primitiveTable) this.primitives.set(id, tag);
  }

  schema(id: bigint): Schema | undefined {
    return this.composites.get(id);
  }

  /// A new registry with `extra` composite schemas merged in, sharing this
  /// registry's primitive table. Used to reconcile a peer's exchanged schemas
  /// (a writer closure) against the local reader registry for compat decode.
  /// Colliding ids are content-hashes, so an overwrite is identity.
  with(extra: Iterable<Schema>): Registry {
    const r = new Registry([], []);
    for (const [id, s] of this.composites) r.composites.set(id, s);
    for (const [id, t] of this.primitives) r.primitives.set(id, t);
    for (const s of extra) r.composites.set(s.id, s);
    return r;
  }

  /// Resolve a concrete ref to a Var-free kind. A parametric schema's type
  /// parameters are substituted by the ref's args, eagerly and per-reference, so
  /// the walker never meets a `Var` (`r[type-system.generic-resolution]`). Each
  /// arg carries its own binding forward, so no environment is threaded.
  resolve(ref: SchemaRef): SchemaKind {
    if (ref.kind === "var") {
      throw new DecodeError("unbound type variable");
    }
    const prim = this.primitives.get(ref.id);
    if (prim !== undefined) {
      if (ref.args.length !== 0) throw new DecodeError("primitive carrying type arguments");
      return { kind: "primitive", primitive: prim };
    }
    const schema = this.composites.get(ref.id);
    if (schema === undefined) {
      throw new DecodeError(`unknown schema id ${ref.id.toString(16)}`);
    }
    if (schema.typeParams.length !== ref.args.length) {
      throw new DecodeError(
        `generic expects ${schema.typeParams.length} type arguments, got ${ref.args.length}`,
      );
    }
    if (ref.args.length === 0) return schema.kind;
    return substituteKind(schema.kind, schema.typeParams, ref.args);
  }
}

// ============================================================================
// Generic substitution (mirror of compact.rs substitute_kind/substitute_ref)
// ============================================================================

function substituteRef(ref: SchemaRef, params: string[], args: SchemaRef[]): SchemaRef {
  if (ref.kind === "var") {
    const i = params.indexOf(ref.name);
    return i >= 0 ? args[i]! : ref;
  }
  // A concrete ref keeps its id; substitute within its own type args so nested
  // parametric refs (`Holder<T>` inside `Wrapper<T>`) carry the binding forward.
  return { kind: "concrete", id: ref.id, args: ref.args.map((a) => substituteRef(a, params, args)) };
}

function substituteField(f: Field, params: string[], args: SchemaRef[]): Field {
  return { name: f.name, schema: substituteRef(f.schema, params, args), required: f.required };
}

function substitutePayload(p: VariantPayload, params: string[], args: SchemaRef[]): VariantPayload {
  switch (p.kind) {
    case "unit":
      return p;
    case "newtype":
      return { kind: "newtype", ref: substituteRef(p.ref, params, args) };
    case "tuple":
      return { kind: "tuple", refs: p.refs.map((r) => substituteRef(r, params, args)) };
    case "struct":
      return { kind: "struct", fields: p.fields.map((f) => substituteField(f, params, args)) };
  }
}

function substituteKind(kind: SchemaKind, params: string[], args: SchemaRef[]): SchemaKind {
  switch (kind.kind) {
    case "primitive":
    case "dynamic":
      return kind;
    case "struct":
      return { kind: "struct", name: kind.name, fields: kind.fields.map((f) => substituteField(f, params, args)) };
    case "enum":
      return {
        kind: "enum",
        name: kind.name,
        variants: kind.variants.map((v) => ({ name: v.name, index: v.index, payload: substitutePayload(v.payload, params, args) })),
      };
    case "tuple":
      return { kind: "tuple", elements: kind.elements.map((e) => substituteRef(e, params, args)) };
    case "list":
      return { kind: "list", element: substituteRef(kind.element, params, args) };
    case "set":
      return { kind: "set", element: substituteRef(kind.element, params, args) };
    case "option":
      return { kind: "option", element: substituteRef(kind.element, params, args) };
    case "map":
      return { kind: "map", key: substituteRef(kind.key, params, args), value: substituteRef(kind.value, params, args) };
    case "array":
      return { kind: "array", element: substituteRef(kind.element, params, args), dimensions: kind.dimensions };
    case "tensor":
      return { kind: "tensor", element: substituteRef(kind.element, params, args), rank: kind.rank };
    case "channel":
      return { kind: "channel", direction: kind.direction, element: substituteRef(kind.element, params, args) };
    case "external":
      return {
        kind: "external",
        external: kind.external,
        metadata: kind.metadata ? substituteRef(kind.metadata, params, args) : null,
      };
  }
}

// ============================================================================
// Self-describing schema parser (port of selfdescribing.rs dec_schema/...)
// ============================================================================

/// Parse a `Schema` from self-describing bytes (the bytes Rust `schema_to_bytes`
/// produces). Rejects trailing bytes. Throws `DecodeError` on malformed input.
export function schemaFromBytes(bytes: Uint8Array): Schema {
  const r = new Reader(bytes);
  const s = decSchema(r, 0);
  if (r.remaining() !== 0) {
    throw new DecodeError(`${r.remaining()} trailing bytes after schema`);
  }
  return s;
}

function checkDepth(depth: number): void {
  if (depth > MAX_DEPTH) throw new DecodeError("schema nests too deep");
}

// The schema self-describing form is a tagged value tree: enums are
// `ENUM`-tag + variant-name string + payload; structs are `STRUCT`-tag + name +
// field count + (name string, value)*. The decoder reads that framing exactly.

function expect(r: Reader, t: number, what: string): void {
  const got = r.readU8();
  if (got !== t) throw new DecodeError(`expected ${what}, got tag 0x${got.toString(16)}`);
}

function dU32(r: Reader): number {
  expect(r, Tag.U32, "u32");
  return r.readU32raw();
}

function dU64(r: Reader): bigint {
  expect(r, Tag.U64, "u64");
  return r.readU64();
}

function dBool(r: Reader): boolean {
  expect(r, Tag.BOOL, "bool");
  return r.readBool();
}

function dStr(r: Reader): string {
  expect(r, Tag.STRING, "string");
  return r.readStr();
}

function dUnit(r: Reader): void {
  expect(r, Tag.UNIT, "unit");
}

/// Read a struct header (tag, name, field count), verifying the count.
function stBegin(r: Reader, fields: number): void {
  expect(r, Tag.STRUCT, "struct");
  r.readStr(); // struct name (informational)
  if (r.readU32raw() !== fields) throw new DecodeError("struct field count");
}

/// Read and discard a struct field's name (fields are positional here).
function fname(r: Reader): void {
  r.readStr();
}

function listLen(r: Reader): number {
  expect(r, Tag.LIST, "list");
  return r.readLen(1);
}

function decSchema(r: Reader, depth: number): Schema {
  checkDepth(depth);
  stBegin(r, 3);
  fname(r);
  const id = dU64(r);
  fname(r);
  const n = listLen(r);
  const typeParams: string[] = [];
  for (let i = 0; i < n; i++) typeParams.push(dStr(r));
  fname(r);
  const kind = decKind(r, depth + 1);
  return { id, typeParams, kind };
}

function decKind(r: Reader, depth: number): SchemaKind {
  checkDepth(depth);
  expect(r, Tag.ENUM, "enum");
  const variant = r.readStr();
  switch (variant) {
    case "Primitive":
      return { kind: "primitive", primitive: decPrimitive(r) };
    case "Struct": {
      stBegin(r, 2);
      fname(r);
      const name = dStr(r);
      fname(r);
      const fields = decFieldList(r, depth + 1);
      return { kind: "struct", name, fields };
    }
    case "Enum": {
      stBegin(r, 2);
      fname(r);
      const name = dStr(r);
      fname(r);
      const count = listLen(r);
      const variants: Variant[] = [];
      for (let i = 0; i < count; i++) variants.push(decVariant(r, depth + 1));
      return { kind: "enum", name, variants };
    }
    case "Tuple": {
      stBegin(r, 1);
      fname(r);
      return { kind: "tuple", elements: decRefList(r, depth + 1) };
    }
    case "List": {
      stBegin(r, 1);
      fname(r);
      return { kind: "list", element: decRef(r, depth + 1) };
    }
    case "Set": {
      stBegin(r, 1);
      fname(r);
      return { kind: "set", element: decRef(r, depth + 1) };
    }
    case "Option": {
      stBegin(r, 1);
      fname(r);
      return { kind: "option", element: decRef(r, depth + 1) };
    }
    case "Map": {
      stBegin(r, 2);
      fname(r);
      const key = decRef(r, depth + 1);
      fname(r);
      const value = decRef(r, depth + 1);
      return { kind: "map", key, value };
    }
    case "Array": {
      stBegin(r, 2);
      fname(r);
      const element = decRef(r, depth + 1);
      fname(r);
      const count = listLen(r);
      const dimensions: bigint[] = [];
      for (let i = 0; i < count; i++) dimensions.push(dU64(r));
      return { kind: "array", element, dimensions };
    }
    case "Tensor": {
      stBegin(r, 2);
      fname(r);
      const element = decRef(r, depth + 1);
      fname(r);
      const t = r.readU8();
      let rank: number | null;
      if (t === Tag.OPTION_NONE) rank = null;
      else if (t === Tag.OPTION_SOME) rank = dU32(r);
      else throw new DecodeError(`expected option, got tag 0x${t.toString(16)}`);
      return { kind: "tensor", element, rank };
    }
    case "Channel": {
      stBegin(r, 2);
      fname(r);
      const direction = decDirection(r);
      fname(r);
      const element = decRef(r, depth + 1);
      return { kind: "channel", direction, element };
    }
    case "Dynamic": {
      dUnit(r);
      return { kind: "dynamic" };
    }
    case "External": {
      stBegin(r, 2);
      fname(r);
      const external = dStr(r);
      fname(r);
      const t = r.readU8();
      let metadata: SchemaRef | null;
      if (t === Tag.OPTION_NONE) metadata = null;
      else if (t === Tag.OPTION_SOME) metadata = decRef(r, depth + 1);
      else throw new DecodeError(`expected option, got tag 0x${t.toString(16)}`);
      return { kind: "external", external, metadata };
    }
    default:
      throw new DecodeError(`unknown SchemaKind variant '${variant}'`);
  }
}

function decPrimitive(r: Reader): Primitive {
  expect(r, Tag.ENUM, "enum");
  const name = r.readStr();
  dUnit(r);
  return asPrimitive(name);
}

function decDirection(r: Reader): ChannelDirection {
  expect(r, Tag.ENUM, "enum");
  const name = r.readStr();
  dUnit(r);
  if (name === "tx" || name === "rx") return name;
  throw new DecodeError(`unknown channel direction '${name}'`);
}

function decRef(r: Reader, depth: number): SchemaRef {
  checkDepth(depth);
  expect(r, Tag.ENUM, "enum");
  const variant = r.readStr();
  switch (variant) {
    case "Concrete": {
      stBegin(r, 2);
      fname(r);
      const id = dU64(r);
      fname(r);
      const args = decRefList(r, depth + 1);
      return { kind: "concrete", id, args };
    }
    case "Var": {
      stBegin(r, 1);
      fname(r);
      return { kind: "var", name: dStr(r) };
    }
    default:
      throw new DecodeError(`unknown SchemaRef variant '${variant}'`);
  }
}

function decField(r: Reader, depth: number): Field {
  checkDepth(depth);
  stBegin(r, 3);
  fname(r);
  const name = dStr(r);
  fname(r);
  const schema = decRef(r, depth + 1);
  fname(r);
  const required = dBool(r);
  return { name, schema, required };
}

function decVariant(r: Reader, depth: number): Variant {
  checkDepth(depth);
  stBegin(r, 3);
  fname(r);
  const name = dStr(r);
  fname(r);
  const index = dU32(r);
  fname(r);
  const payload = decVariantPayload(r, depth + 1);
  return { name, index, payload };
}

function decVariantPayload(r: Reader, depth: number): VariantPayload {
  checkDepth(depth);
  expect(r, Tag.ENUM, "enum");
  const variant = r.readStr();
  switch (variant) {
    case "Unit":
      dUnit(r);
      return { kind: "unit" };
    case "Newtype":
      return { kind: "newtype", ref: decRef(r, depth + 1) };
    case "Tuple":
      return { kind: "tuple", refs: decRefList(r, depth + 1) };
    case "Struct":
      return { kind: "struct", fields: decFieldList(r, depth + 1) };
    default:
      throw new DecodeError(`unknown VariantPayload variant '${variant}'`);
  }
}

function decRefList(r: Reader, depth: number): SchemaRef[] {
  const n = listLen(r);
  const v: SchemaRef[] = [];
  for (let i = 0; i < n; i++) v.push(decRef(r, depth + 1));
  return v;
}

function decFieldList(r: Reader, depth: number): Field[] {
  const n = listLen(r);
  const v: Field[] = [];
  for (let i = 0; i < n; i++) v.push(decField(r, depth + 1));
  return v;
}
