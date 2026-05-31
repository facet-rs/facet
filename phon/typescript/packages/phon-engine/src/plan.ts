// Compatibility planning for TypeScript: reconcile a *writer* schema with a
// *reader* schema into a translation Plan, then decode the writer's compact
// bytes into a reader-shaped Value. Mirrors Rust `phon-engine/src/plan.rs`.
//
// The plan is built from the two schemas alone, before any payload is touched
// (`r[compat.plan-first]`): if it cannot be built the schemas are incompatible
// and decoding never begins. Struct fields are matched by name
// (`r[compat.field-matching]`) — writer-only fields are skipped, reader-only
// non-required fields are defaulted to null. Enum variants are matched by name
// (`r[compat.enum]`); a writer-only variant arriving on the wire is a decode
// error. Scalars match only when identical — no implicit numeric widening
// (`r[compat.type-match]`).
//
// `decodeWithPlan` is the interpreter baseline; the JIT (jit.ts) compiles the
// same plan to specialized JavaScript via `new Function`. Both must produce the
// identical Value and identical errors — the conformance corpus asserts it.
//
// Spec: docs/content/spec.md — "Compatibility".

import {
  DecodeError,
  minWireSizeRef,
  Reader,
  Registry,
} from "@bearcove/phon-schema";
import type { Field, Primitive, SchemaKind, SchemaRef, Value, Variant, VariantPayload } from "@bearcove/phon-schema";
import { canonicalKey } from "@bearcove/phon-schema";
import { checkFixedCount, decodePrimitive, decodeRef, product } from "./compact.ts";

const MAX_DEPTH = 128;

// ============================================================================
// Errors
// ============================================================================

/// Two schemas cannot be reconciled — raised while building the plan, before any
/// bytes are read (mirror of Rust `CompactError::Incompatible`).
export class IncompatibleError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "Incompatible";
  }
}

/// A writer enum variant the reader lacks arrived on the wire — raised at decode
/// time (mirror of Rust `CompactError::WriterOnlyVariant`).
export class WriterOnlyVariantError extends DecodeError {
  readonly index: number;
  constructor(index: number) {
    super(`writer-only enum variant ${index}`);
    this.name = "WriterOnlyVariant";
    this.index = index;
  }
}

// ============================================================================
// The plan (Node tree)
// ============================================================================

export type Node =
  | { kind: "scalar"; primitive: Primitive }
  | { kind: "struct"; plan: StructPlan }
  | { kind: "enum"; byIndex: Map<number, VariantPlan> }
  | { kind: "tuple"; nodes: Node[] }
  | { kind: "seq"; set: boolean; element: Node; minWire: number }
  | { kind: "map"; key: Node; value: Node }
  | { kind: "array"; element: Node; dims: bigint[]; minWire: number }
  | { kind: "option"; element: Node }
  | { kind: "dynamic" };

export interface StructPlan {
  /// One step per writer field, in wire order.
  steps: Step[];
  /// Reader-only, non-required field names to fill with a default (null).
  defaults: string[];
}

export type Step =
  | { kind: "take"; reader: string; node: Node }
  | { kind: "skip"; ref: SchemaRef };

export interface VariantPlan {
  reader: string;
  payload: Payload;
}

export type Payload =
  | { kind: "unit" }
  | { kind: "newtype"; node: Node }
  | { kind: "tuple"; nodes: Node[] }
  | { kind: "struct"; plan: StructPlan };

export interface Plan {
  root: Node;
}

// ============================================================================
// Building the plan
// ============================================================================

/// Build the translation plan from `writerRoot` to `readerRoot`. Throws
/// `IncompatibleError` if the schemas cannot be reconciled.
export function buildPlan(writerRoot: bigint, readerRoot: bigint, reg: Registry): Plan {
  const root = planRef(
    { kind: "concrete", id: writerRoot, args: [] },
    { kind: "concrete", id: readerRoot, args: [] },
    reg,
    0,
  );
  return { root };
}

function planRef(w: SchemaRef, r: SchemaRef, reg: Registry, depth: number): Node {
  if (depth > MAX_DEPTH) throw new IncompatibleError("schema nests too deep");
  return planKind(reg.resolve(w), reg.resolve(r), reg, depth);
}

function planKind(wk: SchemaKind, rk: SchemaKind, reg: Registry, depth: number): Node {
  if (wk.kind === "primitive" && rk.kind === "primitive") {
    if (wk.primitive !== rk.primitive) {
      throw new IncompatibleError(`primitive ${wk.primitive} is not ${rk.primitive}`);
    }
    return { kind: "scalar", primitive: wk.primitive };
  }
  if (wk.kind === "struct" && rk.kind === "struct") {
    return { kind: "struct", plan: planStruct(wk.fields, rk.fields, reg, depth) };
  }
  if (wk.kind === "enum" && rk.kind === "enum") {
    return planEnum(wk.variants, rk.variants, reg, depth);
  }
  if (wk.kind === "tuple" && rk.kind === "tuple") {
    if (wk.elements.length !== rk.elements.length) throw new IncompatibleError("tuple arity differs");
    const nodes = wk.elements.map((we, i) => planRef(we, rk.elements[i]!, reg, depth + 1));
    return { kind: "tuple", nodes };
  }
  if (wk.kind === "list" && rk.kind === "list") {
    return { kind: "seq", set: false, minWire: minWireSizeRef(reg, wk.element), element: planRef(wk.element, rk.element, reg, depth + 1) };
  }
  if (wk.kind === "set" && rk.kind === "set") {
    return { kind: "seq", set: true, minWire: minWireSizeRef(reg, wk.element), element: planRef(wk.element, rk.element, reg, depth + 1) };
  }
  if (wk.kind === "option" && rk.kind === "option") {
    return { kind: "option", element: planRef(wk.element, rk.element, reg, depth + 1) };
  }
  if (wk.kind === "map" && rk.kind === "map") {
    return { kind: "map", key: planRef(wk.key, rk.key, reg, depth + 1), value: planRef(wk.value, rk.value, reg, depth + 1) };
  }
  if (wk.kind === "array" && rk.kind === "array") {
    if (!sameDims(wk.dimensions, rk.dimensions)) throw new IncompatibleError("array dimensions differ");
    return { kind: "array", minWire: minWireSizeRef(reg, wk.element), element: planRef(wk.element, rk.element, reg, depth + 1), dims: wk.dimensions };
  }
  if (wk.kind === "dynamic" && rk.kind === "dynamic") {
    return { kind: "dynamic" };
  }
  throw new IncompatibleError(`schema kinds differ (${wk.kind} vs ${rk.kind})`);
}

function planStruct(wFields: Field[], rFields: Field[], reg: Registry, depth: number): StructPlan {
  const readerByName = new Map(rFields.map((f) => [f.name, f]));
  const steps: Step[] = [];
  const matched = new Set<string>();
  for (const wf of wFields) {
    const rf = readerByName.get(wf.name);
    if (rf) {
      steps.push({ kind: "take", reader: rf.name, node: planRef(wf.schema, rf.schema, reg, depth + 1) });
      matched.add(rf.name);
    } else {
      steps.push({ kind: "skip", ref: wf.schema });
    }
  }
  const defaults: string[] = [];
  for (const rf of rFields) {
    if (!matched.has(rf.name)) {
      if (rf.required) {
        throw new IncompatibleError(`required reader field '${rf.name}' is absent from the writer`);
      }
      defaults.push(rf.name);
    }
  }
  return { steps, defaults };
}

function planEnum(wVariants: Variant[], rVariants: Variant[], reg: Registry, depth: number): Node {
  const readerByName = new Map(rVariants.map((v) => [v.name, v]));
  const byIndex = new Map<number, VariantPlan>();
  for (const wv of wVariants) {
    const rv = readerByName.get(wv.name);
    // A writer variant the reader lacks gets no entry: receiving it is a decode
    // error, but its absence here is fine.
    if (rv) {
      byIndex.set(wv.index, { reader: rv.name, payload: planPayload(wv.payload, rv.payload, reg, depth) });
    }
  }
  return { kind: "enum", byIndex };
}

function planPayload(w: VariantPayload, r: VariantPayload, reg: Registry, depth: number): Payload {
  if (w.kind === "unit" && r.kind === "unit") return { kind: "unit" };
  if (w.kind === "newtype" && r.kind === "newtype") {
    return { kind: "newtype", node: planRef(w.ref, r.ref, reg, depth + 1) };
  }
  if (w.kind === "tuple" && r.kind === "tuple") {
    if (w.refs.length !== r.refs.length) throw new IncompatibleError("variant tuple arity differs");
    return { kind: "tuple", nodes: w.refs.map((wr, i) => planRef(wr, r.refs[i]!, reg, depth + 1)) };
  }
  if (w.kind === "struct" && r.kind === "struct") {
    return { kind: "struct", plan: planStruct(w.fields, r.fields, reg, depth) };
  }
  throw new IncompatibleError("variant payload shapes differ");
}

function sameDims(a: bigint[], b: bigint[]): boolean {
  return a.length === b.length && a.every((d, i) => d === b[i]);
}

// ============================================================================
// Interpreter (the baseline executor — mirror of Rust `exec`)
// ============================================================================

/// Decode writer compact `bytes` into a reader-shaped Value with a built plan,
/// rejecting trailing bytes.
export function decodeWithPlan(bytes: Uint8Array, plan: Plan, reg: Registry): Value {
  const r = new Reader(bytes);
  const v = exec(plan.root, r, reg, 0);
  if (r.remaining() !== 0) throw new DecodeError(`${r.remaining()} trailing bytes`);
  return v;
}

/// Build a plan and decode in one step.
export function decode(bytes: Uint8Array, writerRoot: bigint, readerRoot: bigint, reg: Registry): Value {
  return decodeWithPlan(bytes, buildPlan(writerRoot, readerRoot, reg), reg);
}

function exec(node: Node, r: Reader, reg: Registry, depth: number): Value {
  if (depth > MAX_DEPTH) throw new DecodeError("maximum nesting depth exceeded");
  switch (node.kind) {
    case "scalar":
      return decodePrimitive(r, node.primitive);
    case "struct":
      return execStruct(node.plan, r, reg, depth);
    case "enum": {
      const idx = r.readU32raw();
      const vp = node.byIndex.get(idx);
      if (!vp) throw new WriterOnlyVariantError(idx);
      const payload = execPayload(vp.payload, r, reg, depth);
      return new Map<string, Value>([[vp.reader, payload]]);
    }
    case "tuple": {
      const a: Value[] = [];
      for (const n of node.nodes) a.push(exec(n, r, reg, depth + 1));
      return a;
    }
    case "seq": {
      const n = r.readLen(node.minWire);
      const a: Value[] = [];
      const seen = node.set ? new Set<string>() : null;
      for (let i = 0; i < n; i++) {
        const v = exec(node.element, r, reg, depth + 1);
        if (seen) {
          const k = canonicalKey(v);
          if (seen.has(k)) throw new DecodeError("duplicate set element");
          seen.add(k);
        }
        a.push(v);
      }
      return a;
    }
    case "map": {
      const n = r.readLen(1);
      const obj = new Map<string, Value>();
      for (let i = 0; i < n; i++) {
        const k = exec(node.key, r, reg, depth + 1);
        const v = exec(node.value, r, reg, depth + 1);
        if (typeof k !== "string") throw new DecodeError("map with non-string keys");
        if (obj.has(k)) throw new DecodeError("duplicate map key");
        obj.set(k, v);
      }
      return obj;
    }
    case "array": {
      const count = product(node.dims);
      checkFixedCount(count, node.minWire, r.remaining());
      const a: Value[] = [];
      for (let i = 0n; i < count; i++) a.push(exec(node.element, r, reg, depth + 1));
      return a;
    }
    case "option": {
      const b = r.readU8();
      if (b === 0) return null;
      if (b === 1) return exec(node.element, r, reg, depth + 1);
      throw new DecodeError(`invalid bool byte 0x${b.toString(16)}`);
    }
    case "dynamic":
      throw new DecodeError("dynamic kind is not yet supported in the TS engine");
  }
}

function execStruct(plan: StructPlan, r: Reader, reg: Registry, depth: number): Value {
  const obj = new Map<string, Value>();
  for (const step of plan.steps) {
    if (step.kind === "take") {
      obj.set(step.reader, exec(step.node, r, reg, depth + 1));
    } else {
      // Walk the writer field by its own schema and discard it.
      decodeRef(r, step.ref, reg, depth + 1);
    }
  }
  for (const name of plan.defaults) obj.set(name, null);
  return obj;
}

function execPayload(p: Payload, r: Reader, reg: Registry, depth: number): Value {
  switch (p.kind) {
    case "unit":
      return null;
    case "newtype":
      return exec(p.node, r, reg, depth + 1);
    case "tuple": {
      const a: Value[] = [];
      for (const n of p.nodes) a.push(exec(n, r, reg, depth + 1));
      return a;
    }
    case "struct":
      return execStruct(p.plan, r, reg, depth);
  }
}
