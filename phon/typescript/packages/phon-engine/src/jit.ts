// The TypeScript "JIT": compile a compat Plan to specialized JavaScript and hand
// it to `new Function`. This is the analogue of the Rust copy-and-patch JIT —
// same idea, different substrate. We compile once, baking every writer<->reader
// discrepancy (skipped fields, reordered fields, defaulted reader-only fields,
// remapped or rejected enum variants) into straight-line generated code. After
// that, decoding runs at full speed no matter how different the remote schema is
// from the local one: there is no fast path and slow path, there is one path.
//
// The generated function INLINES the entire structural walk — struct field
// sequences, enum index dispatch, scalar reads, alignment padding — so at decode
// time there is no Plan/Node object access and no per-step dispatch. Only
// data-driven control flow (sequence/map/array loops, the option presence byte,
// the enum switch) survives, exactly as in the interpreter but unrolled.
//
// `new Function` is unavailable under a strict Content-Security-Policy (no
// `'unsafe-eval'`), so `compile` transparently falls back to the interpreter;
// `compilePlan` is the strict primitive that throws if codegen is blocked.
//
// The compiled function is verified against the interpreter (plan.ts) on every
// conformance vector: same bytes -> same Value, same errors.
//
// Spec: docs/content/spec.md — "Compact mode", "Compatibility", "TypeScript".

import { ByteSink, DecodeError, EncodeError, Reader, Registry, alignment } from "@bearcove/phon-schema";
import {
  canonicalKey,
  formatDatetime,
  formatQName,
  parseDatetime,
  parseQName,
  parseUuid,
  readValue,
  writeValueInto,
} from "@bearcove/phon-schema";
import type { Primitive, SchemaKind, SchemaRef, Value, VariantPayload } from "@bearcove/phon-schema";
import { checkFixedCount, decodeRef, encode, product } from "./compact.ts";
import { MESSAGE_MAX_DEPTH } from "./limits.ts";
import type { Node, Payload, Plan, StructPlan } from "./plan.ts";
import { buildPlan, decodeWithPlan, WriterOnlyVariantError } from "./plan.ts";

/// The fixed helper surface the generated code closes over. `new Function` has no
/// access to module scope, so everything it needs is passed in as an argument.
interface Helpers {
  Reader: typeof Reader;
  DecodeError: typeof DecodeError;
  WriterOnlyVariantError: typeof WriterOnlyVariantError;
  decodeRef: typeof decodeRef;
  canonicalKey: typeof canonicalKey;
  parseDatetime: typeof parseDatetime;
  parseUuid: typeof parseUuid;
  parseQName: typeof parseQName;
  readValue: typeof readValue;
  product: typeof product;
  checkFixedCount: typeof checkFixedCount;
}

const HELPERS: Helpers = {
  Reader,
  DecodeError,
  WriterOnlyVariantError,
  decodeRef,
  canonicalKey,
  parseDatetime,
  parseUuid,
  parseQName,
  readValue,
  product,
  checkFixedCount,
};

/// A compiled decoder: writer bytes -> reader-shaped Value, rejecting trailing
/// bytes. Throws the same errors the interpreter would.
export type CompiledDecoder = (bytes: Uint8Array) => Value;

export interface JitFallbackRecord {
  path: string;
  reason: string;
}

/// Whether `new Function` is usable here. Memoized: a strict CSP disables it
/// process-wide, so probe once. When false, `compile` uses the interpreter.
let jitCapable: boolean | undefined;
export function jitAvailable(): boolean {
  if (jitCapable === undefined) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-implied-eval
      new Function("return 1")();
      jitCapable = true;
    } catch {
      jitCapable = false;
    }
  }
  return jitCapable;
}

/// Compile a built plan to a specialized decoder via `new Function`. Throws if
/// codegen is unavailable (strict CSP) — use `compile` for transparent fallback.
// r[impl exec.jit-optional]
// r[impl crates.jit-opt-in]
export function compilePlan(plan: Plan, reg: Registry): CompiledDecoder {
  const cg = new Codegen();
  const body = cg.genProgram(plan);
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
    H: Helpers,
  ) => Value;
  const skipRefs = cg.skipRefs;
  return (bytes: Uint8Array) => fn(bytes, reg, skipRefs, HELPERS);
}

/// An interpreter-backed decoder over a built plan — the CSP fallback, and what
/// `compile(..., { jit: false })` returns.
// r[impl exec.interpreter-baseline]
export function interpretPlan(plan: Plan, reg: Registry): CompiledDecoder {
  return (bytes: Uint8Array) => decodeWithPlan(bytes, plan, reg);
}

// r[impl exec.strict-recording]
export function recordJitFallbacks(plan: Plan): JitFallbackRecord[] {
  const records: JitFallbackRecord[] = [];
  recordNodeFallbacks(plan.root, "$", records);
  for (const [schema, block] of plan.blocks) {
    recordNodeFallbacks(block, `$block[0x${schema.toString(16)}]`, records);
  }
  return records;
}

function recordNodeFallbacks(node: Node, path: string, out: JitFallbackRecord[]): void {
  switch (node.kind) {
    case "callBlock":
      return;
    case "struct":
      node.plan.steps.forEach((step, i) => {
        if (step.kind === "take") recordNodeFallbacks(step.node, `${path}.field[${i}]`, out);
      });
      return;
    case "enum":
      for (const [index, variant] of node.byIndex) {
        recordPayloadFallbacks(variant.payload, `${path}.variant[${index}]`, out);
      }
      return;
    case "tuple":
      node.nodes.forEach((child, i) => recordNodeFallbacks(child, `${path}.tuple[${i}]`, out));
      return;
    case "seq":
    case "array":
    case "option":
      recordNodeFallbacks(node.element, `${path}.element`, out);
      return;
    case "map":
      recordNodeFallbacks(node.key, `${path}.key`, out);
      recordNodeFallbacks(node.value, `${path}.value`, out);
      return;
    case "scalar":
    case "dynamic":
      return;
  }
}

function recordPayloadFallbacks(payload: Payload, path: string, out: JitFallbackRecord[]): void {
  switch (payload.kind) {
    case "unit":
      return;
    case "newtype":
      recordNodeFallbacks(payload.node, `${path}.value`, out);
      return;
    case "tuple":
      payload.nodes.forEach((child, i) => recordNodeFallbacks(child, `${path}.tuple[${i}]`, out));
      return;
    case "struct":
      payload.plan.steps.forEach((step, i) => {
        if (step.kind === "take") recordNodeFallbacks(step.node, `${path}.field[${i}]`, out);
      });
      return;
  }
}

// A per-registry cache of compiled decoders, keyed by writer:reader:engine. A
// Registry is immutable after construction, so a plan depends only on the
// schemas reachable from the roots — caching by registry identity is sound. The
// WeakMap lets a dropped registry's cache be collected.
const decoderCache = new WeakMap<Registry, Map<string, CompiledDecoder>>();

/// Build a plan and return a decoder for `writerRoot -> readerRoot`, cached per
/// registry. Uses the JIT when `new Function` is available, else the
/// interpreter; pass `{ jit: true }` to require the JIT (throwing under CSP) or
/// `{ jit: false }` to force the interpreter.
// r[impl exec.jit-optional]
// r[impl crates.jit-opt-in]
export function compile(
  writerRoot: bigint,
  readerRoot: bigint,
  reg: Registry,
  opts?: { jit?: boolean },
): CompiledDecoder {
  const useJit = opts?.jit ?? jitAvailable();
  const key = `${writerRoot.toString(16)}:${readerRoot.toString(16)}:${useJit ? "j" : "i"}`;
  let perReg = decoderCache.get(reg);
  if (!perReg) {
    perReg = new Map();
    decoderCache.set(reg, perReg);
  }
  const hit = perReg.get(key);
  if (hit) return hit;
  const plan = buildPlan(writerRoot, readerRoot, reg);
  const fallbacks = recordJitFallbacks(plan);
  const decoder = useJit && fallbacks.length === 0 ? compilePlan(plan, reg) : interpretPlan(plan, reg);
  perReg.set(key, decoder);
  return decoder;
}

/// The generated source of a plan's decoder, for inspection/debugging.
export function compiledSource(plan: Plan): string {
  return new Codegen().genProgram(plan);
}

// ============================================================================
// Encode JIT — compile a schema to a specialized Value -> bytes encoder
// ============================================================================
//
// Encoding takes one schema (the local one) — there is no writer<->reader
// compat translation — so the encoder compiles directly from the resolved schema,
// inlining the structural walk just as the decoder does. A coarse `Value` goes
// in; compact bytes come out, byte-identical to the recursive `encode`.

/// A compiled encoder: a coarse Value -> compact bytes for one schema.
export type CompiledEncoder = (value: Value) => Uint8Array;

interface EncHelpers {
  ByteSink: typeof ByteSink;
  writeValueInto: typeof writeValueInto;
  formatDatetime: typeof formatDatetime;
  formatQName: typeof formatQName;
  EncodeError: typeof EncodeError;
}

const ENC_HELPERS: EncHelpers = { ByteSink, writeValueInto, formatDatetime, formatQName, EncodeError };

const encoderCache = new WeakMap<Registry, Map<string, CompiledEncoder>>();

/// Compile (and cache per registry) a Value->bytes encoder for `root`. Uses the
/// JIT when `new Function` is available; falls back to the recursive `encode`
/// when source generation is unavailable or the schema contains an unsupported
/// compact kind.
// r[impl crates.jit-opt-in]
export function compileEncoder(root: bigint, reg: Registry, opts?: { jit?: boolean }): CompiledEncoder {
  const useJit = opts?.jit ?? jitAvailable();
  const key = `${root.toString(16)}:${useJit ? "j" : "i"}`;
  let perReg = encoderCache.get(reg);
  if (!perReg) {
    perReg = new Map();
    encoderCache.set(reg, perReg);
  }
  const hit = perReg.get(key);
  if (hit) return hit;

  const interp: CompiledEncoder = (value) => encode(value, root, reg);
  let encoder: CompiledEncoder = interp;
  if (useJit) {
    try {
      const eg = new EncCodegen(reg, recursiveBlockIds(root, reg));
      const body = eg.genProgram(root);
      const src = `"use strict";\nconst out = new H.ByteSink();\n${body}return out.finish();\n`;
      // eslint-disable-next-line @typescript-eslint/no-implied-eval
      const fn = new Function("value", "H", src) as (value: Value, H: EncHelpers) => Uint8Array;
      encoder = (value: Value) => fn(value, ENC_HELPERS);
    } catch {
      encoder = interp;
    }
  }
  perReg.set(key, encoder);
  return encoder;
}

/// The generated source of a schema's encoder, for inspection/debugging.
export function compiledEncoderSource(root: bigint, reg: Registry): string {
  return new EncCodegen(reg, recursiveBlockIds(root, reg)).genProgram(root);
}

function recursiveBlockIds(root: bigint, reg: Registry): Set<bigint> {
  return new Set(buildPlan(root, root, reg).blocks.keys());
}

/// The JS write statements for a primitive scalar (a ByteSink `out` is in scope;
/// `v` is the value expression). Mirrors compact.ts encodePrimitive.
function scalarWrite(p: Primitive, v: string): string {
  const a = alignment(p);
  const pad = a > 1 ? `out.padTo(${a});\n` : "";
  switch (p) {
    case "bool": return `${pad}out.u8(${v} ? 1 : 0);\n`;
    case "u8": return `${pad}out.u8(Number(BigInt.asUintN(8, ${v})));\n`;
    case "u16": return `${pad}out.u16(${v});\n`;
    case "u32": return `${pad}out.u32(Number(BigInt.asUintN(32, ${v})));\n`;
    case "u64": return `${pad}out.u64(${v});\n`;
    case "u128": return `${pad}out.u128(${v});\n`;
    case "i8": return `${pad}out.u8(Number(BigInt.asUintN(8, ${v})));\n`;
    case "i16": return `${pad}out.i16(${v});\n`;
    case "i32": return `${pad}out.i32(${v});\n`;
    case "i64": return `${pad}out.i64(${v});\n`;
    case "i128": return `${pad}out.i128(${v});\n`;
    case "f32": return `${pad}out.f32(Number(${v}));\n`;
    case "f64": return `${pad}out.f64(Number(${v}));\n`;
    case "char": return `${pad}out.u32(${v}.value.codePointAt(0));\n`;
    case "string": return `out.str(${v});\n`;
    case "bytes": return `out.bytes(${v});\n`;
    case "unit": return "";
    case "never": return `throw new H.EncodeError("never is uninhabited");\n`;
    case "datetime": return `out.str(H.formatDatetime(${v}));\n`;
    case "uuid": return `out.str(${v}.text);\n`;
    case "qname": return `out.str(H.formatQName(${v}));\n`;
  }
}

class EncCodegen {
  private counter = 0;
  private readonly reg: Registry;
  private readonly recursiveIds: Set<bigint>;
  // Explicit field assignment (not a constructor parameter property), so Node's
  // strip-only TypeScript loader can run this module without a compile step.
  constructor(reg: Registry, recursiveIds: Set<bigint>) {
    this.reg = reg;
    this.recursiveIds = recursiveIds;
  }

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  private childDepth(depth: string): string {
    return `(${depth} + 1)`;
  }

  private blockName(schema: bigint): string {
    return `__enc_block_${schema.toString(16).replace(/[^0-9a-f]/g, "_")}`;
  }

  genProgram(root: bigint): string {
    let out = "";
    for (const schema of this.recursiveIds) {
      out += this.genBlock(schema);
    }
    out += this.genEncRef({ kind: "concrete", id: root, args: [] }, "value", "0");
    return out;
  }

  private genBlock(schema: bigint): string {
    const fn = this.blockName(schema);
    const kind = this.reg.resolve({ kind: "concrete", id: schema, args: [] });
    let out = `function ${fn}(__value, __depth) {\n`;
    out += `if (__depth > ${MESSAGE_MAX_DEPTH}) throw new H.EncodeError("maximum nesting depth exceeded");\n`;
    out += this.genEnc(kind, "__value", "__depth");
    out += `}\n`;
    return out;
  }

  private genEncRef(ref: SchemaRef, vexpr: string, depth: string): string {
    if (ref.kind === "concrete" && this.recursiveIds.has(ref.id)) {
      return `${this.blockName(ref.id)}(${vexpr}, ${this.childDepth(depth)});\n`;
    }
    return this.genEnc(this.reg.resolve(ref), vexpr, depth);
  }

  /// Emit statements writing the value at `vexpr` against `kind`. Refs are
  /// resolved at compile time and inlined until a recursive block boundary.
  genEnc(kind: SchemaKind, vexpr: string, depth: string): string {
    switch (kind.kind) {
      case "primitive":
        return scalarWrite(kind.primitive, vexpr);
      case "struct": {
        let out = "";
        for (const f of kind.fields) {
          out += this.genEncRef(f.schema, `${vexpr}.get(${JSON.stringify(f.name)})`, this.childDepth(depth));
        }
        return out;
      }
      case "tuple": {
        let out = "";
        kind.elements.forEach((e, i) => {
          out += this.genEncRef(e, `${vexpr}[${i}]`, this.childDepth(depth));
        });
        return out;
      }
      case "list":
      case "set": {
        const a = this.fresh("a");
        const e = this.fresh("e");
        const body = this.genEncRef(kind.element, e, this.childDepth(depth));
        return `const ${a} = ${vexpr};\nout.u32(${a}.length);\nfor (const ${e} of ${a}) {\n${body}}\n`;
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
      case "enum": {
        const ent = this.fresh("ent");
        const name = this.fresh("name");
        const pl = this.fresh("pl");
        let out = `const ${ent} = ${vexpr}.entries().next().value;\n`;
        out += `const ${name} = ${ent}[0];\nconst ${pl} = ${ent}[1];\n`;
        out += `switch (${name}) {\n`;
        for (const variant of kind.variants) {
          out += `case ${JSON.stringify(variant.name)}: {\nout.u32(${variant.index});\n`;
          out += this.genPayload(variant.payload, pl, this.childDepth(depth));
          out += `break;\n}\n`;
        }
        out += `default: throw new H.EncodeError("unknown variant " + ${name});\n}\n`;
        return out;
      }
      case "dynamic":
        return `H.writeValueInto(out, ${vexpr});\n`;
      case "tensor":
      case "channel":
      case "external":
        throw new Error(`compact encode unsupported for kind '${kind.kind}'`);
    }
  }

  private genPayload(p: VariantPayload, vexpr: string, depth: string): string {
    switch (p.kind) {
      case "unit":
        return "";
      case "newtype":
        return this.genEncRef(p.ref, vexpr, this.childDepth(depth));
      case "tuple": {
        let out = "";
        p.refs.forEach((r, i) => {
          out += this.genEncRef(r, `${vexpr}[${i}]`, this.childDepth(depth));
        });
        return out;
      }
      case "struct": {
        let out = "";
        for (const f of p.fields) {
          out += this.genEncRef(f.schema, `${vexpr}.get(${JSON.stringify(f.name)})`, this.childDepth(depth));
        }
        return out;
      }
    }
  }
}

// ============================================================================
// Code generation
// ============================================================================

/// The JS read expression for a primitive scalar (a Reader `r` is in scope).
function scalarExpr(p: Primitive): string {
  switch (p) {
    case "bool": return "r.readBool()";
    case "u8": return "BigInt(r.readU8())";
    case "u16": return "r.readU16()";
    case "u32": return "r.readU32()";
    case "u64": return "r.readU64()";
    case "u128": return "r.readU128()";
    case "i8": return "r.readI8()";
    case "i16": return "r.readI16()";
    case "i32": return "r.readI32()";
    case "i64": return "r.readI64()";
    case "i128": return "r.readI128()";
    case "f32": return "r.readF32()";
    case "f64": return "r.readF64()";
    case "char": return '{ kind: "char", value: String.fromCodePoint(r.readCharCode()) }';
    case "string": return "r.readStr()";
    case "bytes": return "r.readBytes()";
    case "unit": return "null";
    case "never": return '(() => { throw new H.DecodeError("never is uninhabited"); })()';
    case "datetime": return "H.parseDatetime(r.readStr())";
    case "uuid": return "H.parseUuid(r.readStr())";
    case "qname": return "H.parseQName(r.readStr())";
  }
}

class Codegen {
  /// Writer-only-field schemas to skip, looked up by index at runtime via the
  /// `skipRefs` argument (a SchemaRef can't be embedded as a JS literal).
  skipRefs: SchemaRef[] = [];
  private counter = 0;

  genProgram(plan: Plan): string {
    let out = "";
    for (const [schema, block] of plan.blocks) {
      out += this.genBlock(schema, block);
    }
    out += `let __root;\n`;
    out += this.genStmt(plan.root, "__root", "0");
    return out;
  }

  private genBlock(schema: bigint, block: Node): string {
    const fn = this.blockName(schema);
    let out = `function ${fn}(__depth) {\n`;
    out += `if (__depth > ${MESSAGE_MAX_DEPTH}) throw new H.DecodeError("maximum nesting depth exceeded");\n`;
    out += `let __ret;\n`;
    out += this.genStmt(block, "__ret", "__depth");
    out += `return __ret;\n`;
    out += `}\n`;
    return out;
  }

  private blockName(schema: bigint): string {
    return `__block_${schema.toString(16).replace(/[^0-9a-f]/g, "_")}`;
  }

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  private childDepth(depth: string): string {
    return `(${depth} + 1)`;
  }

  // r[impl ir.inlining]
  /// Emit statements that decode `node` (reading from `r`) and assign the value
  /// to the already-declared variable `target`. `depth` is the compile-time
  /// nesting level — threaded so writer-only-field skips pass the SAME depth the
  /// interpreter would, keeping the hostile-input depth limit identical.
  genStmt(node: Node, target: string, depth: string): string {
    switch (node.kind) {
      case "scalar": {
        const a = alignment(node.primitive);
        const pad = a > 1 ? `r.skipPad(${a});\n` : "";
        return `${pad}${target} = ${scalarExpr(node.primitive)};\n`;
      }
      case "struct":
        return this.genStruct(node.plan, target, depth);
      case "tuple": {
        const a = this.fresh("a");
        let out = `const ${a} = [];\n`;
        for (const n of node.nodes) {
          const e = this.fresh("e");
          out += `let ${e};\n${this.genStmt(n, e, this.childDepth(depth))}${a}.push(${e});\n`;
        }
        return out + `${target} = ${a};\n`;
      }
      case "enum": {
        const idx = this.fresh("idx");
        const res = this.fresh("res");
        let out = `const ${idx} = r.readU32raw();\n`;
        out += `let ${res};\n`;
        out += `switch (${idx}) {\n`;
        for (const [index, vp] of node.byIndex) {
          const p = this.fresh("p");
          out += `case ${index}: {\nlet ${p};\n${this.genPayload(vp.payload, p, depth)}`;
          out += `${res} = new Map([[${JSON.stringify(vp.reader)}, ${p}]]);\nbreak;\n}\n`;
        }
        out += `default: throw new H.WriterOnlyVariantError(${idx});\n`;
        out += `}\n`;
        return out + `${target} = ${res};\n`;
      }
      case "seq": {
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
            `const ${k} = H.canonicalKey(${e});\n` +
            `if (${seen}.has(${k})) throw new H.DecodeError("duplicate set element");\n` +
            `${seen}.add(${k});\n`;
        }
        out += `for (let ${i} = 0; ${i} < ${n}; ${i}++) {\nlet ${e};\n${this.genStmt(node.element, e, this.childDepth(depth))}${dup}${a}.push(${e});\n}\n`;
        return out + `${target} = ${a};\n`;
      }
      case "map": {
        const n = this.fresh("n");
        const m = this.fresh("m");
        const i = this.fresh("i");
        const k = this.fresh("k");
        const v = this.fresh("v");
        let out = `const ${n} = r.readLen(1);\n`;
        out += `const ${m} = new Map();\n`;
        out += `for (let ${i} = 0; ${i} < ${n}; ${i}++) {\n`;
        out += `let ${k};\n${this.genStmt(node.key, k, this.childDepth(depth))}`;
        out += `let ${v};\n${this.genStmt(node.value, v, this.childDepth(depth))}`;
        out += `if (typeof ${k} !== "string") throw new H.DecodeError("map with non-string keys");\n`;
        out += `if (${m}.has(${k})) throw new H.DecodeError("duplicate map key");\n`;
        out += `${m}.set(${k}, ${v});\n}\n`;
        return out + `${target} = ${m};\n`;
      }
      case "array": {
        const count = this.fresh("count");
        const a = this.fresh("a");
        const i = this.fresh("i");
        const e = this.fresh("e");
        const dims = `[${node.dims.map((d) => `${d}n`).join(", ")}]`;
        let out = `const ${count} = H.product(${dims});\n`;
        out += `H.checkFixedCount(${count}, ${node.minWire}, r.remaining());\n`;
        out += `const ${a} = [];\n`;
        out += `for (let ${i} = 0n; ${i} < ${count}; ${i}++) {\nlet ${e};\n${this.genStmt(node.element, e, this.childDepth(depth))}${a}.push(${e});\n}\n`;
        return out + `${target} = ${a};\n`;
      }
      case "option": {
        const b = this.fresh("b");
        const inner = this.fresh("inner");
        let out = `const ${b} = r.readU8();\n`;
        out += `if (${b} === 0) ${target} = null;\n`;
        out += `else if (${b} === 1) {\nlet ${inner};\n${this.genStmt(node.element, inner, this.childDepth(depth))}${target} = ${inner};\n}\n`;
        out += `else throw new H.DecodeError("invalid bool byte 0x" + ${b}.toString(16));\n`;
        return out;
      }
      case "dynamic":
        return `${target} = H.readValue(r, ${depth});\n`;
      case "callBlock":
        return `${target} = ${this.blockName(node.schema)}(${this.childDepth(depth)});\n`;
    }
  }

  private genStruct(plan: StructPlan, target: string, depth: string): string {
    const m = this.fresh("m");
    let out = `const ${m} = new Map();\n`;
    for (const step of plan.steps) {
      if (step.kind === "take") {
        const f = this.fresh("f");
        out += `let ${f};\n${this.genStmt(step.node, f, this.childDepth(depth))}${m}.set(${JSON.stringify(step.reader)}, ${f});\n`;
      } else {
        const k = this.skipRefs.push(step.ref) - 1;
        // Same depth the interpreter passes (`depth + 1`): the writer-only field
        // sits one level below this struct.
        out += `H.decodeRef(r, skipRefs[${k}], reg, ${this.childDepth(depth)});\n`;
      }
    }
    for (const name of plan.defaults) {
      out += `${m}.set(${JSON.stringify(name)}, null);\n`;
    }
    return out + `${target} = ${m};\n`;
  }

  private genPayload(p: Payload, target: string, depth: string): string {
    switch (p.kind) {
      case "unit":
        return `${target} = null;\n`;
      case "newtype":
        return this.genStmt(p.node, target, this.childDepth(depth));
      case "tuple": {
        const a = this.fresh("a");
        let out = `const ${a} = [];\n`;
        for (const n of p.nodes) {
          const e = this.fresh("e");
          out += `let ${e};\n${this.genStmt(n, e, this.childDepth(depth))}${a}.push(${e});\n`;
        }
        return out + `${target} = ${a};\n`;
      }
      case "struct":
        return this.genStruct(p.plan, target, depth);
    }
  }
}
