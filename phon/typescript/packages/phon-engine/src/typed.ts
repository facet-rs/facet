// The typed front door: ergonomic JavaScript values that match what phon's TS
// codegen emits.
//
// The coarse `Value` engine remains the interpreter/oracle and the implementation
// for actual `Dynamic` fields. The JIT-enabled typed path lowers the same
// writer->reader compatibility plan into generated JavaScript that constructs or
// consumes the public JS shape directly: plain objects for structs, codegen's
// discriminated-union objects for enums, arrays/sets/maps for containers, and
// `Value` only where the schema really says `Dynamic`.
//
// Representation (mirrors what codegen should emit):
//   struct            -> plain object { field: typed }
//   enum              -> { tag: "Variant", value: typed }   (value null for unit)
//   tuple/list/array -> typed[]
//   set               -> Set<typed>
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
  alignment,
  ByteSink,
  canonicalKey,
  DecodeError,
  EncodeError,
  formatDatetime,
  formatQName,
  parseDatetime,
  parseQName,
  parseUuid,
  readValue,
  Reader,
  Registry,
  writeValueInto,
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
  Variant,
  VariantPayload,
} from "@bearcove/phon-schema";
import { checkFixedCount, decodeRef, encode, product } from "./compact.ts";
import { buildPlan, WriterOnlyVariantError } from "./plan.ts";
import type { Node, Payload, Plan, StructPlan } from "./plan.ts";
import { compile, jitAvailable } from "./jit.ts";

/// A discriminated-union enum value: the variant name in `tag` (or `$tag` when
/// a struct variant has a real field named `tag`), plus its payload inlined per
/// the variant shape. Matches what phon-codegen emits.
export type TypedEnum = ({ readonly tag: string } | { readonly $tag: string }) & { readonly [field: string]: Typed };

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
  | Set<Typed>
  | Value; // dynamic passthrough

const SMALL_INTS = new Set<Primitive>(["u8", "u16", "u32", "i8", "i16", "i32"]);

// ============================================================================
// Public API
// ============================================================================

export type CompiledTypedDecoder = (bytes: Uint8Array) => Typed;
export type CompiledTypedEncoder = (typed: Typed) => Uint8Array;

/// Decode writer compact `bytes` into an ergonomic typed value shaped by the
/// reader schema, translating writer<->reader schema differences. Uses the JIT
/// when available (interpreter fallback under strict CSP); pass `{ jit }` to
/// force.
// r[impl crates.jit-opt-in]
// r[impl typed.no-dynamic-bounce]
export function decodeTyped(
  bytes: Uint8Array,
  writerRoot: bigint,
  readerRoot: bigint,
  reg: Registry,
  opts?: { jit?: boolean },
): Typed {
  return compileTypedDecoder(writerRoot, readerRoot, reg, opts)(bytes);
}

/// Encode an ergonomic typed value against the schema referenced by `root`.
// r[impl typed.no-dynamic-bounce]
export function encodeTyped(typed: Typed, root: bigint, reg: Registry, opts?: { jit?: boolean }): Uint8Array {
  return compileTypedEncoder(root, reg, opts)(typed);
}

const typedDecoderCache = new WeakMap<Registry, Map<string, CompiledTypedDecoder>>();
const typedEncoderCache = new WeakMap<Registry, Map<string, CompiledTypedEncoder>>();

export function compileTypedDecoder(
  writerRoot: bigint,
  readerRoot: bigint,
  reg: Registry,
  opts?: { jit?: boolean },
): CompiledTypedDecoder {
  const useJit = opts?.jit ?? jitAvailable();
  const key = `${writerRoot.toString(16)}:${readerRoot.toString(16)}:${useJit ? "j" : "i"}:typed`;
  let perReg = typedDecoderCache.get(reg);
  if (!perReg) {
    perReg = new Map();
    typedDecoderCache.set(reg, perReg);
  }
  const hit = perReg.get(key);
  if (hit) return hit;

  const fallback: CompiledTypedDecoder = (bytes) => {
    const value = compile(writerRoot, readerRoot, reg, { jit: false })(bytes);
    return valueToTypedRef(value, { kind: "concrete", id: readerRoot, args: [] }, reg);
  };
  let decoder = fallback;
  if (useJit) {
    try {
      decoder = compileTypedPlan(buildPlan(writerRoot, readerRoot, reg), readerRoot, reg);
    } catch (error) {
      if (opts?.jit === true) throw error;
      decoder = fallback;
    }
  }

  perReg.set(key, decoder);
  return decoder;
}

export function compileTypedEncoder(root: bigint, reg: Registry, opts?: { jit?: boolean }): CompiledTypedEncoder {
  const useJit = opts?.jit ?? jitAvailable();
  const key = `${root.toString(16)}:${useJit ? "j" : "i"}:typed`;
  let perReg = typedEncoderCache.get(reg);
  if (!perReg) {
    perReg = new Map();
    typedEncoderCache.set(reg, perReg);
  }
  const hit = perReg.get(key);
  if (hit) return hit;

  const fallback: CompiledTypedEncoder = (typed) => {
    const value = typedToValueRef(typed, { kind: "concrete", id: root, args: [] }, reg);
    return encode(value, root, reg);
  };
  let encoder = fallback;
  if (useJit) {
    try {
      encoder = compileTypedEncoderSourceFn(root, reg);
    } catch (error) {
      if (opts?.jit === true) throw error;
      encoder = fallback;
    }
  }

  perReg.set(key, encoder);
  return encoder;
}

export function compiledTypedDecoderSource(plan: Plan, readerRoot: bigint, reg: Registry): string {
  return new TypedDecodeCodegen(reg).genProgram(plan, readerRoot);
}

export function compiledTypedEncoderSource(root: bigint, reg: Registry): string {
  return new TypedEncodeCodegen(reg, recursiveBlockIds(root, reg)).genProgram(root);
}

function compileTypedPlan(plan: Plan, readerRoot: bigint, reg: Registry): CompiledTypedDecoder {
  const cg = new TypedDecodeCodegen(reg);
  const body = cg.genProgram(plan, readerRoot);
  const src =
    `"use strict";\n` +
    `const r = new H.Reader(bytes);\n` +
    `${body}` +
    `if (r.remaining() !== 0) throw new H.DecodeError(r.remaining() + " trailing bytes");\n` +
    `return __root;\n`;
  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  const fn = new Function("bytes", "reg", "skipRefs", "H", src) as (
    bytes: Uint8Array,
    reg: Registry,
    skipRefs: SchemaRef[],
    H: TypedDecodeHelpers,
  ) => Typed;
  const skipRefs = cg.skipRefs;
  return (bytes) => fn(bytes, reg, skipRefs, TYPED_DECODE_HELPERS);
}

function compileTypedEncoderSourceFn(root: bigint, reg: Registry): CompiledTypedEncoder {
  const body = compiledTypedEncoderSource(root, reg);
  const src = `"use strict";\nconst out = new H.ByteSink();\n${body}return out.finish();\n`;
  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  const fn = new Function("typed", "H", src) as (typed: Typed, H: TypedEncodeHelpers) => Uint8Array;
  return (typed) => fn(typed, TYPED_ENCODE_HELPERS);
}

interface TypedDecodeHelpers {
  Reader: typeof Reader;
  DecodeError: typeof DecodeError;
  WriterOnlyVariantError: typeof WriterOnlyVariantError;
  decodeRef: typeof decodeRef;
  typedCanonicalKey: typeof typedCanonicalKey;
  formatDatetime: typeof formatDatetime;
  formatQName: typeof formatQName;
  parseDatetime: typeof parseDatetime;
  parseUuid: typeof parseUuid;
  parseQName: typeof parseQName;
  readValue: typeof readValue;
  product: typeof product;
  checkFixedCount: typeof checkFixedCount;
}

interface TypedEncodeHelpers {
  ByteSink: typeof ByteSink;
  EncodeError: typeof EncodeError;
  writeValueInto: typeof writeValueInto;
  formatDatetime: typeof formatDatetime;
  formatQName: typeof formatQName;
  parseDatetime: typeof parseDatetime;
  parseUuid: typeof parseUuid;
  parseQName: typeof parseQName;
  charCode: typeof typedCharCode;
}

const TYPED_DECODE_HELPERS: TypedDecodeHelpers = {
  Reader,
  DecodeError,
  WriterOnlyVariantError,
  decodeRef,
  typedCanonicalKey,
  formatDatetime,
  formatQName,
  parseDatetime,
  parseUuid,
  parseQName,
  readValue,
  product,
  checkFixedCount,
};

const TYPED_ENCODE_HELPERS: TypedEncodeHelpers = {
  ByteSink,
  EncodeError,
  writeValueInto,
  formatDatetime,
  formatQName,
  parseDatetime,
  parseUuid,
  parseQName,
  charCode: typedCharCode,
};

const TYPED_RUNTIME_MAX_DEPTH = 128;

function typedScalarExpr(p: Primitive): string {
  switch (p) {
    case "bool": return "r.readBool()";
    case "u8": return "r.readU8()";
    case "u16": return "Number(r.readU16())";
    case "u32": return "Number(r.readU32())";
    case "u64": return "r.readU64()";
    case "u128": return "r.readU128()";
    case "i8": return "Number(r.readI8())";
    case "i16": return "Number(r.readI16())";
    case "i32": return "Number(r.readI32())";
    case "i64": return "r.readI64()";
    case "i128": return "r.readI128()";
    case "f32": return "r.readF32()";
    case "f64": return "r.readF64()";
    case "char": return "String.fromCodePoint(r.readCharCode())";
    case "string": return "r.readStr()";
    case "bytes": return "r.readBytes()";
    case "unit": return "null";
    case "never": return '(() => { throw new H.DecodeError("never is uninhabited"); })()';
    case "datetime": return "H.formatDatetime(H.parseDatetime(r.readStr()))";
    case "uuid": return "H.parseUuid(r.readStr()).text";
    case "qname": return "H.formatQName(H.parseQName(r.readStr()))";
  }
}

function typedScalarWrite(p: Primitive, v: string): string {
  const a = alignment(p);
  const pad = a > 1 ? `out.padTo(${a});\n` : "";
  switch (p) {
    case "bool": return `${pad}out.u8(${v} ? 1 : 0);\n`;
    case "u8": return `${pad}out.u8(Number(BigInt.asUintN(8, BigInt(${v}))));\n`;
    case "u16": return `${pad}out.u16(BigInt(${v}));\n`;
    case "u32": return `${pad}out.u32(Number(BigInt.asUintN(32, BigInt(${v}))));\n`;
    case "u64": return `${pad}out.u64(${v});\n`;
    case "u128": return `${pad}out.u128(${v});\n`;
    case "i8": return `${pad}out.u8(Number(BigInt.asUintN(8, BigInt(${v}))));\n`;
    case "i16": return `${pad}out.i16(BigInt(${v}));\n`;
    case "i32": return `${pad}out.i32(BigInt(${v}));\n`;
    case "i64": return `${pad}out.i64(${v});\n`;
    case "i128": return `${pad}out.i128(${v});\n`;
    case "f32": return `${pad}out.f32(Number(${v}));\n`;
    case "f64": return `${pad}out.f64(Number(${v}));\n`;
    case "char": return `${pad}out.u32(H.charCode(${v}));\n`;
    case "string": return `out.str(${v});\n`;
    case "bytes": return `out.bytes(${v});\n`;
    case "unit": return `if (${v} !== null) throw new H.EncodeError("expected unit (null)");\n`;
    case "never": return `throw new H.EncodeError("never is uninhabited");\n`;
    case "datetime": return `out.str(H.formatDatetime(H.parseDatetime(${v})));\n`;
    case "uuid": return `out.str(H.parseUuid(${v}).text);\n`;
    case "qname": return `out.str(H.formatQName(H.parseQName(${v})));\n`;
  }
}

class TypedDecodeCodegen {
  skipRefs: SchemaRef[] = [];
  private counter = 0;
  private readonly reg: Registry;

  constructor(reg: Registry) {
    this.reg = reg;
  }

  genProgram(plan: Plan, readerRoot: bigint): string {
    let out = "";
    for (const [schema, block] of plan.blocks) {
      out += this.genBlock(schema, block);
    }
    out += `let __root;\n`;
    out += this.genStmt(plan.root, this.reg.resolve({ kind: "concrete", id: readerRoot, args: [] }), "__root", "0");
    return out;
  }

  private genBlock(schema: bigint, block: Node): string {
    const fn = this.blockName(schema);
    const kind = this.reg.resolve({ kind: "concrete", id: schema, args: [] });
    let out = `function ${fn}(__depth) {\n`;
    out += `if (__depth > ${TYPED_RUNTIME_MAX_DEPTH}) throw new H.DecodeError("maximum nesting depth exceeded");\n`;
    out += `let __ret;\n`;
    out += this.genStmt(block, kind, "__ret", "__depth");
    out += `return __ret;\n`;
    out += `}\n`;
    return out;
  }

  private blockName(schema: bigint): string {
    return `__typed_block_${schema.toString(16).replace(/[^0-9a-f]/g, "_")}`;
  }

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  private childDepth(depth: string): string {
    return `(${depth} + 1)`;
  }

  private resolve(ref: SchemaRef): SchemaKind {
    return this.reg.resolve(ref);
  }

  genStmt(node: Node, readerKind: SchemaKind, target: string, depth: string): string {
    switch (node.kind) {
      case "scalar": {
        const a = alignment(node.primitive);
        const pad = a > 1 ? `r.skipPad(${a});\n` : "";
        return `${pad}${target} = ${typedScalarExpr(node.primitive)};\n`;
      }
      case "struct":
        if (readerKind.kind !== "struct") throw new Error("typed decode struct reader kind mismatch");
        return this.genStruct(node.plan, readerKind.fields, target, depth, null);
      case "tuple": {
        if (readerKind.kind !== "tuple") throw new Error("typed decode tuple reader kind mismatch");
        const a = this.fresh("a");
        let out = `const ${a} = [];\n`;
        node.nodes.forEach((child, i) => {
          const e = this.fresh("e");
          out += `let ${e};\n${this.genStmt(child, this.resolve(readerKind.elements[i]!), e, this.childDepth(depth))}${a}.push(${e});\n`;
        });
        return out + `${target} = ${a};\n`;
      }
      case "enum":
        if (readerKind.kind !== "enum") throw new Error("typed decode enum reader kind mismatch");
        return this.genEnum(node, readerKind, target, depth);
      case "seq":
        if (readerKind.kind !== "list" && readerKind.kind !== "set") throw new Error("typed decode sequence reader kind mismatch");
        return this.genSeq(node, readerKind, target, depth);
      case "map":
        if (readerKind.kind !== "map") throw new Error("typed decode map reader kind mismatch");
        return this.genMap(node, readerKind, target, depth);
      case "array":
        if (readerKind.kind !== "array") throw new Error("typed decode array reader kind mismatch");
        return this.genArray(node, readerKind, target, depth);
      case "option":
        if (readerKind.kind !== "option") throw new Error("typed decode option reader kind mismatch");
        return this.genOption(node, readerKind, target, depth);
      case "dynamic":
        return `${target} = H.readValue(r, ${depth});\n`;
      case "callBlock":
        return `${target} = ${this.blockName(node.schema)}(${this.childDepth(depth)});\n`;
    }
  }

  private genStruct(
    plan: StructPlan,
    fields: readonly { readonly name: string; readonly schema: SchemaRef }[],
    target: string,
    depth: string,
    initialField: { name: string; value: string } | null,
  ): string {
    const obj = this.fresh("o");
    const fieldByName = new Map(fields.map((field) => [field.name, field]));
    let out = initialField
      ? `const ${obj} = { ${JSON.stringify(initialField.name)}: ${initialField.value} };\n`
      : `const ${obj} = {};\n`;
    for (const step of plan.steps) {
      if (step.kind === "take") {
        const field = fieldByName.get(step.reader);
        if (!field) throw new Error(`typed decode missing reader field '${step.reader}'`);
        const f = this.fresh("f");
        out += `let ${f};\n${this.genStmt(step.node, this.resolve(field.schema), f, this.childDepth(depth))}`;
        out += `${obj}[${JSON.stringify(step.reader)}] = ${f};\n`;
      } else {
        const k = this.skipRefs.push(step.ref) - 1;
        out += `H.decodeRef(r, skipRefs[${k}], reg, ${this.childDepth(depth)});\n`;
      }
    }
    for (const name of plan.defaults) {
      out += `${obj}[${JSON.stringify(name)}] = null;\n`;
    }
    return out + `${target} = ${obj};\n`;
  }

  private genEnum(node: Extract<Node, { kind: "enum" }>, kind: Extract<SchemaKind, { kind: "enum" }>, target: string, depth: string): string {
    const idx = this.fresh("idx");
    let out = `const ${idx} = r.readU32raw();\n`;
    out += `switch (${idx}) {\n`;
    const discriminator = enumDiscriminatorField(kind.variants);
    for (const [index, vp] of node.byIndex) {
      const variant = kind.variants.find((candidate) => candidate.name === vp.reader);
      if (!variant) throw new Error(`typed decode missing reader variant '${vp.reader}'`);
      out += `case ${index}: {\n`;
      out += this.genPayload(vp.payload, variant.payload, target, depth, discriminator, vp.reader);
      out += `break;\n}\n`;
    }
    out += `default: throw new H.WriterOnlyVariantError(${idx});\n`;
    out += `}\n`;
    return out;
  }

  private genPayload(
    payload: Payload,
    readerPayload: VariantPayload,
    target: string,
    depth: string,
    discriminator: "tag" | "$tag",
    tag: string,
  ): string {
    switch (payload.kind) {
      case "unit":
        return `${target} = { ${JSON.stringify(discriminator)}: ${JSON.stringify(tag)} };\n`;
      case "newtype": {
        if (readerPayload.kind !== "newtype") throw new Error("typed decode newtype payload mismatch");
        const v = this.fresh("v");
        return `let ${v};\n${this.genStmt(payload.node, this.resolve(readerPayload.ref), v, this.childDepth(depth))}` +
          `${target} = { ${JSON.stringify(discriminator)}: ${JSON.stringify(tag)}, value: ${v} };\n`;
      }
      case "tuple": {
        if (readerPayload.kind !== "tuple") throw new Error("typed decode tuple payload mismatch");
        const a = this.fresh("a");
        let out = `const ${a} = [];\n`;
        payload.nodes.forEach((child, i) => {
          const e = this.fresh("e");
          out += `let ${e};\n${this.genStmt(child, this.resolve(readerPayload.refs[i]!), e, this.childDepth(depth))}${a}.push(${e});\n`;
        });
        return out + `${target} = { ${JSON.stringify(discriminator)}: ${JSON.stringify(tag)}, value: ${a} };\n`;
      }
      case "struct":
        if (readerPayload.kind !== "struct") throw new Error("typed decode struct payload mismatch");
        return this.genStruct(payload.plan, readerPayload.fields, target, depth, { name: discriminator, value: JSON.stringify(tag) });
    }
  }

  private genSeq(node: Extract<Node, { kind: "seq" }>, kind: Extract<SchemaKind, { kind: "list" | "set" }>, target: string, depth: string): string {
    const n = this.fresh("n");
    const a = this.fresh("a");
    const i = this.fresh("i");
    const e = this.fresh("e");
    let out = `const ${n} = r.readLen(${node.minWire});\n`;
    out += `const ${a} = [];\n`;
    let dup = "";
    if (node.set) {
      const seen = this.fresh("seen");
      const k = this.fresh("k");
      out += `const ${seen} = new Set();\n`;
      dup =
        `const ${k} = H.typedCanonicalKey(${e});\n` +
        `if (${seen}.has(${k})) throw new H.DecodeError("duplicate set element");\n` +
        `${seen}.add(${k});\n`;
    }
    const elementKind = this.resolve(kind.element);
    out += `for (let ${i} = 0; ${i} < ${n}; ${i}++) {\n`;
    out += `let ${e};\n${this.genStmt(node.element, elementKind, e, this.childDepth(depth))}${dup}${a}.push(${e});\n}\n`;
    if (kind.kind === "set") return out + `${target} = new Set(${a});\n`;
    if (isPrimitiveKind(elementKind, "u8")) return out + `${target} = Uint8Array.from(${a});\n`;
    return out + `${target} = ${a};\n`;
  }

  private genMap(node: Extract<Node, { kind: "map" }>, kind: Extract<SchemaKind, { kind: "map" }>, target: string, depth: string): string {
    const n = this.fresh("n");
    const m = this.fresh("m");
    const i = this.fresh("i");
    const k = this.fresh("k");
    const v = this.fresh("v");
    let out = `const ${n} = r.readLen(1);\n`;
    out += `const ${m} = new Map();\n`;
    out += `for (let ${i} = 0; ${i} < ${n}; ${i}++) {\n`;
    out += `let ${k};\n${this.genStmt(node.key, this.resolve(kind.key), k, this.childDepth(depth))}`;
    out += `let ${v};\n${this.genStmt(node.value, this.resolve(kind.value), v, this.childDepth(depth))}`;
    out += `if (typeof ${k} !== "string") throw new H.DecodeError("map with non-string keys");\n`;
    out += `if (${m}.has(${k})) throw new H.DecodeError("duplicate map key");\n`;
    out += `${m}.set(${k}, ${v});\n}\n`;
    return out + `${target} = ${m};\n`;
  }

  private genArray(node: Extract<Node, { kind: "array" }>, kind: Extract<SchemaKind, { kind: "array" }>, target: string, depth: string): string {
    const count = this.fresh("count");
    const a = this.fresh("a");
    const i = this.fresh("i");
    const e = this.fresh("e");
    const dims = `[${node.dims.map((dim) => `${dim}n`).join(", ")}]`;
    const elementKind = this.resolve(kind.element);
    let out = `const ${count} = H.product(${dims});\n`;
    out += `H.checkFixedCount(${count}, ${node.minWire}, r.remaining());\n`;
    out += `const ${a} = [];\n`;
    out += `for (let ${i} = 0n; ${i} < ${count}; ${i}++) {\n`;
    out += `let ${e};\n${this.genStmt(node.element, elementKind, e, this.childDepth(depth))}${a}.push(${e});\n}\n`;
    if (isPrimitiveKind(elementKind, "u8")) return out + `${target} = Uint8Array.from(${a});\n`;
    return out + `${target} = ${a};\n`;
  }

  private genOption(node: Extract<Node, { kind: "option" }>, kind: Extract<SchemaKind, { kind: "option" }>, target: string, depth: string): string {
    const b = this.fresh("b");
    const inner = this.fresh("inner");
    let out = `const ${b} = r.readU8();\n`;
    out += `if (${b} === 0) ${target} = null;\n`;
    out += `else if (${b} === 1) {\n`;
    out += `let ${inner};\n${this.genStmt(node.element, this.resolve(kind.element), inner, this.childDepth(depth))}${target} = ${inner};\n}\n`;
    out += `else throw new H.DecodeError("invalid bool byte 0x" + ${b}.toString(16));\n`;
    return out;
  }
}

class TypedEncodeCodegen {
  private counter = 0;
  private readonly reg: Registry;
  private readonly recursiveIds: Set<bigint>;

  constructor(reg: Registry, recursiveIds: Set<bigint>) {
    this.reg = reg;
    this.recursiveIds = recursiveIds;
  }

  genProgram(root: bigint): string {
    let out = "";
    for (const schema of this.recursiveIds) {
      out += this.genBlock(schema);
    }
    out += this.genEncRef({ kind: "concrete", id: root, args: [] }, "typed", "0");
    return out;
  }

  private genBlock(schema: bigint): string {
    const fn = this.blockName(schema);
    const kind = this.reg.resolve({ kind: "concrete", id: schema, args: [] });
    let out = `function ${fn}(__value, __depth) {\n`;
    out += `if (__depth > ${TYPED_RUNTIME_MAX_DEPTH}) throw new H.EncodeError("maximum nesting depth exceeded");\n`;
    out += this.genEnc(kind, "__value", "__depth");
    out += `}\n`;
    return out;
  }

  private blockName(schema: bigint): string {
    return `__typed_enc_block_${schema.toString(16).replace(/[^0-9a-f]/g, "_")}`;
  }

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  private childDepth(depth: string): string {
    return `(${depth} + 1)`;
  }

  private genEncRef(ref: SchemaRef, vexpr: string, depth: string): string {
    if (ref.kind === "concrete" && this.recursiveIds.has(ref.id)) {
      return `${this.blockName(ref.id)}(${vexpr}, ${this.childDepth(depth)});\n`;
    }
    return this.genEnc(this.reg.resolve(ref), vexpr, depth);
  }

  genEnc(kind: SchemaKind, vexpr: string, depth: string): string {
    switch (kind.kind) {
      case "primitive":
        return typedScalarWrite(kind.primitive, vexpr);
      case "struct": {
        let out = "";
        for (const field of kind.fields) {
          out += this.genEncRef(field.schema, `${vexpr}[${JSON.stringify(field.name)}]`, this.childDepth(depth));
        }
        return out;
      }
      case "tuple": {
        let out = "";
        kind.elements.forEach((element, i) => {
          out += this.genEncRef(element, `${vexpr}[${i}]`, this.childDepth(depth));
        });
        return out;
      }
      case "list":
      case "set": {
        const a = this.fresh("a");
        const e = this.fresh("e");
        const body = this.genEncRef(kind.element, e, this.childDepth(depth));
        return `const ${a} = ${vexpr};\nout.u32(${a} instanceof Set ? ${a}.size : ${a}.length);\nfor (const ${e} of ${a}) {\n${body}}\n`;
      }
      case "array": {
        const a = this.fresh("a");
        const e = this.fresh("e");
        const body = this.genEncRef(kind.element, e, this.childDepth(depth));
        return `const ${a} = ${vexpr};\nfor (const ${e} of ${a}) {\n${body}}\n`;
      }
      case "map": {
        const m = this.fresh("m");
        const k = this.fresh("k");
        const v = this.fresh("v");
        const kb = this.genEncRef(kind.key, k, this.childDepth(depth));
        const vb = this.genEncRef(kind.value, v, this.childDepth(depth));
        return `const ${m} = ${vexpr};\nout.u32(${m}.size);\nfor (const [${k}, ${v}] of ${m}) {\n${kb}${vb}}\n`;
      }
      case "option": {
        const o = this.fresh("o");
        const body = this.genEncRef(kind.element, o, this.childDepth(depth));
        return `const ${o} = ${vexpr};\nif (${o} === null) out.u8(0);\nelse {\nout.u8(1);\n${body}}\n`;
      }
      case "enum":
        return this.genEnum(kind, vexpr, depth);
      case "dynamic":
        return `H.writeValueInto(out, ${vexpr});\n`;
      case "tensor":
      case "channel":
      case "external":
        throw new Error(`typed encode unsupported for kind '${kind.kind}'`);
    }
  }

  private genEnum(kind: Extract<SchemaKind, { kind: "enum" }>, vexpr: string, depth: string): string {
    const discriminator = enumDiscriminatorField(kind.variants);
    const obj = this.fresh("enum");
    const tag = this.fresh("tag");
    let out = `const ${obj} = ${vexpr};\nconst ${tag} = ${obj}[${JSON.stringify(discriminator)}];\n`;
    out += `switch (${tag}) {\n`;
    for (const variant of kind.variants) {
      out += `case ${JSON.stringify(variant.name)}: {\nout.u32(${variant.index});\n`;
      out += this.genPayload(variant.payload, obj, this.childDepth(depth));
      out += `break;\n}\n`;
    }
    out += `default: throw new H.EncodeError("unknown variant " + ${tag});\n}\n`;
    return out;
  }

  private genPayload(payload: VariantPayload, vexpr: string, depth: string): string {
    switch (payload.kind) {
      case "unit":
        return "";
      case "newtype":
        return this.genEncRef(payload.ref, `${vexpr}.value`, this.childDepth(depth));
      case "tuple": {
        let out = "";
        payload.refs.forEach((ref, i) => {
          out += this.genEncRef(ref, `${vexpr}.value[${i}]`, this.childDepth(depth));
        });
        return out;
      }
      case "struct": {
        let out = "";
        for (const field of payload.fields) {
          out += this.genEncRef(field.schema, `${vexpr}[${JSON.stringify(field.name)}]`, this.childDepth(depth));
        }
        return out;
      }
    }
  }
}

function recursiveBlockIds(root: bigint, reg: Registry): Set<bigint> {
  return new Set(buildPlan(root, root, reg).blocks.keys());
}

function isPrimitiveKind(kind: SchemaKind, primitive: Primitive): boolean {
  return kind.kind === "primitive" && kind.primitive === primitive;
}

function typedCharCode(value: Typed): number {
  if (typeof value !== "string") throw new EncodeError("expected char");
  const code = value.codePointAt(0);
  if (code === undefined || String.fromCodePoint(code) !== value) throw new EncodeError("expected char");
  return code;
}

function typedCanonicalKey(value: Typed): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return `b:${value}`;
  if (typeof value === "bigint") return `n:${value}`;
  if (typeof value === "number") return `f:${value}`;
  if (typeof value === "string") return `s:${value}`;
  if (value instanceof Uint8Array) return `y:${Array.from(value).join(",")}`;
  if (Array.isArray(value)) return `a:[${value.map(typedCanonicalKey).join(",")}]`;
  if (value instanceof Set) return `set:{${[...value].map(typedCanonicalKey).join(",")}}`;
  if (value instanceof Map) {
    return `m:{${[...value.entries()].map(([key, val]) => `${key}=${typedCanonicalKey(val)}`).join(",")}}`;
  }
  if (typeof value === "object") {
    if ("kind" in value) return canonicalKey(value as Value);
    const obj = value as { readonly [field: string]: Typed };
    return `o:{${Object.keys(obj).toSorted().map((key) => `${key}=${typedCanonicalKey(obj[key]!)}`).join(",")}}`;
  }
  throw new EncodeError("unsupported typed value");
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
      return enumValueToTyped(enumDiscriminatorField(kind.variants), tag, payload, variant.payload, reg);
    }
    case "tuple":
      return (v as Value[]).map((e, i) => valueToTypedRef(e, kind.elements[i]!, reg));
    case "list":
    case "array": {
      const arr = (v as Value[]).map((e) => valueToTypedRef(e, kind.element, reg));
      // A list/array of `u8` surfaces as a `Uint8Array` (matching Rust
      // `Vec<u8>` ↔ TS `Uint8Array`), the inverse of the encode-side tolerance.
      const elem = reg.resolve(kind.element);
      if (elem.kind === "primitive" && elem.primitive === "u8") {
        return Uint8Array.from(arr as number[]);
      }
      return arr;
    }
    case "set":
      return new Set((v as Value[]).map((e) => valueToTypedRef(e, kind.element, reg)));
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

function enumDiscriminatorField(variants: readonly Variant[]): "tag" | "$tag" {
  return variants.some((variant) =>
    variant.payload.kind === "struct" && variant.payload.fields.some((field) => field.name === "tag")
  )
    ? "$tag"
    : "tag";
}

/// Build the inlined enum shape: `{ tag }`/`{ $tag }` for unit,
/// `{ tag, value }` for a newtype, `{ tag, value: [...] }` for a tuple variant,
/// or `{ tag, ...fields }` for a struct variant.
function enumValueToTyped(
  discriminator: "tag" | "$tag",
  tag: string,
  v: Value,
  payload: VariantPayload,
  reg: Registry,
): Typed {
  switch (payload.kind) {
    case "unit":
      return { [discriminator]: tag };
    case "newtype":
      return { [discriminator]: tag, value: valueToTypedRef(v, payload.ref, reg) };
    case "tuple":
      return { [discriminator]: tag, value: (v as Value[]).map((e, i) => valueToTypedRef(e, payload.refs[i]!, reg)) };
    case "struct": {
      const m = v as Map<string, Value>;
      const o: { [field: string]: Typed } = { [discriminator]: tag };
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
      const discriminator = enumDiscriminatorField(kind.variants);
      const e = t as { [field: string]: Typed };
      const tag = e[discriminator] as string;
      const variant = kind.variants.find((vt) => vt.name === tag);
      if (!variant) throw new Error(`enum tag '${tag}' not in schema`);
      return new Map<string, Value>([[tag, enumTypedToValue(e, variant.payload, reg)]]);
    }
    case "tuple":
      return (t as Typed[]).map((e, i) => typedToValueRef(e, kind.elements[i]!, reg));
    case "list":
    case "array": {
      // A list/array of `u8` is ergonomically a `Uint8Array` on the TS side
      // (matching Rust `Vec<u8>`); accept it transparently here.
      const items = t instanceof Uint8Array ? Array.from(t) : (t as Typed[]);
      return items.map((e) => typedToValueRef(e, kind.element, reg));
    }
    case "set": {
      const items = t instanceof Set ? Array.from(t) : (t as Typed[]);
      return items.map((e) => typedToValueRef(e, kind.element, reg));
    }
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

/// Reconstruct the payload Value from the inlined enum shape (the inverse of
/// `enumValueToTyped`): `value` for newtype/tuple, the named fields for a struct
/// variant, nothing for unit.
function enumTypedToValue(e: { [field: string]: Typed }, payload: VariantPayload, reg: Registry): Value {
  switch (payload.kind) {
    case "unit":
      return null;
    case "newtype":
      return typedToValueRef(e.value as Typed, payload.ref, reg);
    case "tuple":
      return (e.value as Typed[]).map((x, i) => typedToValueRef(x, payload.refs[i]!, reg));
    case "struct": {
      const m = new Map<string, Value>();
      for (const f of payload.fields) m.set(f.name, typedToValueRef(e[f.name] as Typed, f.schema, reg));
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
