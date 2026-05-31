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

import { DecodeError, Reader, Registry, alignment } from "@bearcove/phon-schema";
import { canonicalKey, parseDatetime, parseQName, parseUuid, readValue } from "@bearcove/phon-schema";
import type { Primitive, SchemaRef, Value } from "@bearcove/phon-schema";
import { checkFixedCount, decodeRef, product } from "./compact.ts";
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
  const decoder = useJit ? compilePlan(plan, reg) : interpretPlan(plan, reg);
  perReg.set(key, decoder);
  return decoder;
}

/// The generated source of a plan's decoder, for inspection/debugging.
export function compiledSource(plan: Plan): string {
  return new Codegen().genStmt(plan.root, "__root", 0);
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
