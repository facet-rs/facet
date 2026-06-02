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
export function compilePlan(plan: Plan, reg: Registry): CompiledDecoder {
  const cg = new Codegen();
  const body = cg.genStmt(plan.root, "__root", 0);
  const src =
    `"use strict";\n` +
    `const r = new H.Reader(bytes);\n` +
    `let __root;\n` +
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
export function interpretPlan(plan: Plan, reg: Registry): CompiledDecoder {
  return (bytes: Uint8Array) => decodeWithPlan(bytes, plan, reg);
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
  // A recursive plan (cyclic schema → `callBlock` blocks) runs on the interpreter;
  // the JIT's `callBlock` codegen is a follow-up, mirroring the Rust JIT (CallBlock
  // is interpreter-only there too).
  const decoder = useJit && plan.blocks.size === 0 ? compilePlan(plan, reg) : interpretPlan(plan, reg);
  perReg.set(key, decoder);
  return decoder;
}

/// The generated source of a plan's decoder, for inspection/debugging.
export function compiledSource(plan: Plan): string {
  return new Codegen().genStmt(plan.root, "__root", 0);
}

// ============================================================================
// Encode JIT — compile a schema to a specialized Value -> bytes encoder
// ============================================================================
//
// Encoding takes one schema (the local one) — there is no writer<->reader
// reconciliation — so the encoder compiles directly from the resolved schema,
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
/// JIT when `new Function` is available and the schema is non-recursive; falls
/// back to the recursive `encode` otherwise.
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
      const eg = new EncCodegen(reg);
      const body = eg.genEnc(reg.resolve({ kind: "concrete", id: root, args: [] }), "value", 0);
      const src = `"use strict";\nconst out = new H.ByteSink();\n${body}return out.finish();\n`;
      // eslint-disable-next-line @typescript-eslint/no-implied-eval
      const fn = new Function("value", "H", src) as (value: Value, H: EncHelpers) => Uint8Array;
      encoder = (value: Value) => fn(value, ENC_HELPERS);
    } catch {
      // Recursive schema (codegen depth guard) or no `new Function`: interpret.
      encoder = interp;
    }
  }
  perReg.set(key, encoder);
  return encoder;
}

/// The generated source of a schema's encoder, for inspection/debugging.
export function compiledEncoderSource(root: bigint, reg: Registry): string {
  return new EncCodegen(reg).genEnc(reg.resolve({ kind: "concrete", id: root, args: [] }), "value", 0);
}

const ENC_MAX_DEPTH = 128;

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
  // Explicit field assignment (not a constructor parameter property), so Node's
  // strip-only TypeScript loader can run this module without a compile step.
  constructor(reg: Registry) {
    this.reg = reg;
  }

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  /// Emit statements writing the value at `vexpr` against `kind`. Refs are
  /// resolved at compile time and inlined; a recursive schema trips the depth
  /// guard (caught by compileEncoder, which then interprets).
  genEnc(kind: SchemaKind, vexpr: string, depth: number): string {
    if (depth > ENC_MAX_DEPTH) throw new Error("schema too deep to compile (recursive?)");
    switch (kind.kind) {
      case "primitive":
        return scalarWrite(kind.primitive, vexpr);
      case "struct": {
        let out = "";
        for (const f of kind.fields) {
          out += this.genEnc(this.reg.resolve(f.schema), `${vexpr}.get(${JSON.stringify(f.name)})`, depth + 1);
        }
        return out;
      }
      case "tuple": {
        let out = "";
        kind.elements.forEach((e, i) => {
          out += this.genEnc(this.reg.resolve(e), `${vexpr}[${i}]`, depth + 1);
        });
        return out;
      }
      case "list":
      case "set": {
        const a = this.fresh("a");
        const e = this.fresh("e");
        const body = this.genEnc(this.reg.resolve(kind.element), e, depth + 1);
        return `const ${a} = ${vexpr};\nout.u32(${a}.length);\nfor (const ${e} of ${a}) {\n${body}}\n`;
      }
      case "array": {
        const a = this.fresh("a");
        const e = this.fresh("e");
        const body = this.genEnc(this.reg.resolve(kind.element), e, depth + 1);
        return `const ${a} = ${vexpr};\nfor (const ${e} of ${a}) {\n${body}}\n`;
      }
      case "map": {
        const m = this.fresh("m");
        const k = this.fresh("k");
        const v = this.fresh("v");
        const kb = this.genEnc(this.reg.resolve(kind.key), k, depth + 1);
        const vb = this.genEnc(this.reg.resolve(kind.value), v, depth + 1);
        return `const ${m} = ${vexpr};\nout.u32(${m}.size);\nfor (const [${k}, ${v}] of ${m}) {\n${kb}${vb}}\n`;
      }
      case "option": {
        const o = this.fresh("o");
        const body = this.genEnc(this.reg.resolve(kind.element), o, depth + 1);
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
          out += this.genPayload(variant.payload, pl, depth + 1);
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

  private genPayload(p: VariantPayload, vexpr: string, depth: number): string {
    switch (p.kind) {
      case "unit":
        return "";
      case "newtype":
        return this.genEnc(this.reg.resolve(p.ref), vexpr, depth + 1);
      case "tuple": {
        let out = "";
        p.refs.forEach((r, i) => {
          out += this.genEnc(this.reg.resolve(r), `${vexpr}[${i}]`, depth + 1);
        });
        return out;
      }
      case "struct": {
        let out = "";
        for (const f of p.fields) {
          out += this.genEnc(this.reg.resolve(f.schema), `${vexpr}.get(${JSON.stringify(f.name)})`, depth + 1);
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

  private fresh(prefix: string): string {
    return `_${prefix}${this.counter++}`;
  }

  /// Emit statements that decode `node` (reading from `r`) and assign the value
  /// to the already-declared variable `target`. `depth` is the compile-time
  /// nesting level — threaded so writer-only-field skips pass the SAME depth the
  /// interpreter would, keeping the hostile-input depth limit identical.
  genStmt(node: Node, target: string, depth: number): string {
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
          out += `let ${e};\n${this.genStmt(n, e, depth + 1)}${a}.push(${e});\n`;
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
        out += `for (let ${i} = 0; ${i} < ${n}; ${i}++) {\nlet ${e};\n${this.genStmt(node.element, e, depth + 1)}${dup}${a}.push(${e});\n}\n`;
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
        out += `let ${k};\n${this.genStmt(node.key, k, depth + 1)}`;
        out += `let ${v};\n${this.genStmt(node.value, v, depth + 1)}`;
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
        out += `for (let ${i} = 0n; ${i} < ${count}; ${i}++) {\nlet ${e};\n${this.genStmt(node.element, e, depth + 1)}${a}.push(${e});\n}\n`;
        return out + `${target} = ${a};\n`;
      }
      case "option": {
        const b = this.fresh("b");
        const inner = this.fresh("inner");
        let out = `const ${b} = r.readU8();\n`;
        out += `if (${b} === 0) ${target} = null;\n`;
        out += `else if (${b} === 1) {\nlet ${inner};\n${this.genStmt(node.element, inner, depth + 1)}${target} = ${inner};\n}\n`;
        out += `else throw new H.DecodeError("invalid bool byte 0x" + ${b}.toString(16));\n`;
        return out;
      }
      case "dynamic":
        return `${target} = H.readValue(r, ${depth});\n`;
      case "callBlock":
        // A recursive plan is decoded by the interpreter (`compile` falls back when
        // `plan.blocks` is non-empty); the JIT for `callBlock` is a follow-up, as in
        // the Rust copy-and-patch JIT. Reaching here means that guard was bypassed.
        throw new Error("JIT codegen does not support recursive (callBlock) plans");
    }
  }

  private genStruct(plan: StructPlan, target: string, depth: number): string {
    const m = this.fresh("m");
    let out = `const ${m} = new Map();\n`;
    for (const step of plan.steps) {
      if (step.kind === "take") {
        const f = this.fresh("f");
        out += `let ${f};\n${this.genStmt(step.node, f, depth + 1)}${m}.set(${JSON.stringify(step.reader)}, ${f});\n`;
      } else {
        const k = this.skipRefs.push(step.ref) - 1;
        // Same depth the interpreter passes (`depth + 1`): the writer-only field
        // sits one level below this struct.
        out += `H.decodeRef(r, skipRefs[${k}], reg, ${depth + 1});\n`;
      }
    }
    for (const name of plan.defaults) {
      out += `${m}.set(${JSON.stringify(name)}, null);\n`;
    }
    return out + `${target} = ${m};\n`;
  }

  private genPayload(p: Payload, target: string, depth: number): string {
    switch (p.kind) {
      case "unit":
        return `${target} = null;\n`;
      case "newtype":
        return this.genStmt(p.node, target, depth + 1);
      case "tuple": {
        const a = this.fresh("a");
        let out = `const ${a} = [];\n`;
        for (const n of p.nodes) {
          const e = this.fresh("e");
          out += `let ${e};\n${this.genStmt(n, e, depth + 1)}${a}.push(${e});\n`;
        }
        return out + `${target} = ${a};\n`;
      }
      case "struct":
        return this.genStruct(p.plan, target, depth);
    }
  }
}
