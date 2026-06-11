// Compatibility planning for TypeScript: translate a *writer* schema with a
// *reader* schema into a Plan, then decode the writer's compact
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
  readValue,
  Reader,
  Registry,
} from "@bearcove/phon-schema";
import type { Field, Primitive, SchemaKind, SchemaRef, Value, Variant, VariantPayload } from "@bearcove/phon-schema";
import { canonicalKey } from "@bearcove/phon-schema";
import { checkFixedCount, decodePrimitive, decodeRef, product } from "./compact.ts";
import { MESSAGE_MAX_DEPTH } from "./limits.ts";

const PLAN_MAX_DEPTH = 128;

// ============================================================================
// Errors
// ============================================================================

/// Two schemas cannot be translated — raised while building the plan, before any
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
  | { kind: "dynamic" }
  /// A back-edge into a recursive (cyclic) reader schema: decode this position with
  /// the schema's block plan from `Plan.blocks`, keyed by the reader schema id. Lets
  /// a recursive type's plan stay finite — the cyclic schema is planned once into a
  /// block and every reference to it is a `callBlock` (`r[ir.recursion]`).
  | { kind: "callBlock"; schema: bigint };

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
  /// Block plans for the recursive (cyclic) reader schemas, keyed by reader schema
  /// id; a `callBlock` node resolves into this. Empty for a non-recursive type.
  blocks: Map<bigint, Node>;
}

export type CompatDirection = "backward" | "forward" | "bidirectional" | "incompatible";

// ============================================================================
// Building the plan
// ============================================================================

/// Planning context: the registry, the reader schema ids on a cycle (so they lower
/// to callable blocks rather than inline, keeping a recursive plan finite), the
/// built block plans, and the reader ids whose block is in progress (to break a
/// back-edge). Mirrors Rust's `RecCtx` over the compat walk.
interface PlanCtx {
  reg: Registry;
  recIds: Set<bigint>;
  blocks: Map<bigint, Node>;
  building: Set<bigint>;
}

/// Build the translation plan from `writerRoot` to `readerRoot`. Throws
/// `IncompatibleError` if the schemas cannot be translated.
// r[impl compat.plan-first]
export function buildPlan(writerRoot: bigint, readerRoot: bigint, reg: Registry): Plan {
  const ctx: PlanCtx = {
    reg,
    recIds: recursiveSchemaIds(readerRoot, reg),
    blocks: new Map(),
    building: new Set(),
  };
  const root = planRef(
    { kind: "concrete", id: writerRoot, args: [] },
    { kind: "concrete", id: readerRoot, args: [] },
    ctx,
    0,
  );
  return { root, blocks: ctx.blocks };
}

/// Classify compatibility between an older and newer schema by planning both ways.
/// This is tooling over `buildPlan`, not a decode path.
// r[impl compat.direction]
export function compatDirection(olderRoot: bigint, newerRoot: bigint, reg: Registry): CompatDirection {
  const backward = canBuildPlan(olderRoot, newerRoot, reg);
  const forward = canBuildPlan(newerRoot, olderRoot, reg);
  if (backward && forward) return "bidirectional";
  if (backward) return "backward";
  if (forward) return "forward";
  return "incompatible";
}

function canBuildPlan(writerRoot: bigint, readerRoot: bigint, reg: Registry): boolean {
  try {
    buildPlan(writerRoot, readerRoot, reg);
    return true;
  } catch {
    return false;
  }
}

function planRef(w: SchemaRef, r: SchemaRef, ctx: PlanCtx, depth: number): Node {
  if (depth > PLAN_MAX_DEPTH) throw new IncompatibleError("schema nests too deep");
  // A recursive (cyclic) reader schema lowers to a callable block: emit a
  // `callBlock` back-edge and build the block once (its body translates the writer
  // against the reader at that position — the same-schema identity in the compat
  // matrix case). (`r[ir.recursion]`)
  const rId = r.kind === "concrete" ? r.id : null;
  if (rId !== null && ctx.recIds.has(rId)) {
    if (!ctx.blocks.has(rId) && !ctx.building.has(rId)) {
      ctx.building.add(rId);
      const body = planKind(ctx.reg.resolve(w), ctx.reg.resolve(r), ctx, depth);
      ctx.building.delete(rId);
      ctx.blocks.set(rId, body);
    }
    return { kind: "callBlock", schema: rId };
  }
  return planKind(ctx.reg.resolve(w), ctx.reg.resolve(r), ctx, depth);
}

// r[impl compat.type-match]
function planKind(wk: SchemaKind, rk: SchemaKind, ctx: PlanCtx, depth: number): Node {
  if (wk.kind === "primitive" && rk.kind === "primitive") {
    if (wk.primitive !== rk.primitive) {
      throw new IncompatibleError(`primitive ${wk.primitive} is not ${rk.primitive}`);
    }
    return { kind: "scalar", primitive: wk.primitive };
  }
  if (wk.kind === "struct" && rk.kind === "struct") {
    return { kind: "struct", plan: planStruct(wk.fields, rk.fields, ctx, depth) };
  }
  if (wk.kind === "enum" && rk.kind === "enum") {
    return planEnum(wk.variants, rk.variants, ctx, depth);
  }
  if (wk.kind === "tuple" && rk.kind === "tuple") {
    if (wk.elements.length !== rk.elements.length) throw new IncompatibleError("tuple arity differs");
    const nodes = wk.elements.map((we, i) => planRef(we, rk.elements[i]!, ctx, depth + 1));
    return { kind: "tuple", nodes };
  }
  if (wk.kind === "list" && rk.kind === "list") {
    return { kind: "seq", set: false, minWire: minWireSizeRef(ctx.reg, wk.element), element: planRef(wk.element, rk.element, ctx, depth + 1) };
  }
  if (wk.kind === "set" && rk.kind === "set") {
    return { kind: "seq", set: true, minWire: minWireSizeRef(ctx.reg, wk.element), element: planRef(wk.element, rk.element, ctx, depth + 1) };
  }
  if (wk.kind === "option" && rk.kind === "option") {
    return { kind: "option", element: planRef(wk.element, rk.element, ctx, depth + 1) };
  }
  if (wk.kind === "map" && rk.kind === "map") {
    return { kind: "map", key: planRef(wk.key, rk.key, ctx, depth + 1), value: planRef(wk.value, rk.value, ctx, depth + 1) };
  }
  if (wk.kind === "array" && rk.kind === "array") {
    if (!sameDims(wk.dimensions, rk.dimensions)) throw new IncompatibleError("array dimensions differ");
    return { kind: "array", minWire: minWireSizeRef(ctx.reg, wk.element), element: planRef(wk.element, rk.element, ctx, depth + 1), dims: wk.dimensions };
  }
  if (wk.kind === "dynamic" && rk.kind === "dynamic") {
    return { kind: "dynamic" };
  }
  if (wk.kind === rk.kind && (wk.kind === "tensor" || wk.kind === "channel" || wk.kind === "external")) {
    throw new Error(`compat plan unsupported for ${wk.kind}`);
  }
  throw new IncompatibleError(`schema kinds differ (${wk.kind} vs ${rk.kind})`);
}

// r[impl compat.field-matching]
// r[impl compat.skip-writer-only]
// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
function planStruct(wFields: Field[], rFields: Field[], ctx: PlanCtx, depth: number): StructPlan {
  const readerByName = new Map(rFields.map((f) => [f.name, f]));
  const steps: Step[] = [];
  const matched = new Set<string>();
  for (const wf of wFields) {
    const rf = readerByName.get(wf.name);
    if (rf) {
      steps.push({ kind: "take", reader: rf.name, node: planRef(wf.schema, rf.schema, ctx, depth + 1) });
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

// r[impl compat.enum]
function planEnum(wVariants: Variant[], rVariants: Variant[], ctx: PlanCtx, depth: number): Node {
  const readerByName = new Map(rVariants.map((v) => [v.name, v]));
  const byIndex = new Map<number, VariantPlan>();
  for (const wv of wVariants) {
    const rv = readerByName.get(wv.name);
    // A writer variant the reader lacks gets no entry: receiving it is a decode
    // error, but its absence here is fine.
    if (rv) {
      byIndex.set(wv.index, { reader: rv.name, payload: planPayload(wv.payload, rv.payload, ctx, depth) });
    }
  }
  return { kind: "enum", byIndex };
}

function planPayload(w: VariantPayload, r: VariantPayload, ctx: PlanCtx, depth: number): Payload {
  if (w.kind === "unit" && r.kind === "unit") return { kind: "unit" };
  if (w.kind === "newtype" && r.kind === "newtype") {
    return { kind: "newtype", node: planRef(w.ref, r.ref, ctx, depth + 1) };
  }
  if (w.kind === "tuple" && r.kind === "tuple") {
    if (w.refs.length !== r.refs.length) throw new IncompatibleError("variant tuple arity differs");
    return { kind: "tuple", nodes: w.refs.map((wr, i) => planRef(wr, r.refs[i]!, ctx, depth + 1)) };
  }
  if (w.kind === "struct" && r.kind === "struct") {
    return { kind: "struct", plan: planStruct(w.fields, r.fields, ctx, depth) };
  }
  throw new IncompatibleError("variant payload shapes differ");
}

function sameDims(a: bigint[], b: bigint[]): boolean {
  return a.length === b.length && a.every((d, i) => d === b[i]);
}

/// The reader schema ids on a reference cycle (a self-reference or a
/// mutual-recursion group), reachable from `root`. These lower to callable blocks
/// so a recursive type's plan stays finite. Mirrors Rust's `recursive_schema_ids`
/// (an SCC member): a DFS that, on a back-edge to a node still on the stack, marks
/// every node on the cycle between them.
function recursiveSchemaIds(root: bigint, reg: Registry): Set<bigint> {
  const recursive = new Set<bigint>();
  const visited = new Set<bigint>();
  const onStack = new Set<bigint>();
  const stack: bigint[] = [];

  const dfs = (id: bigint): void => {
    visited.add(id);
    onStack.add(id);
    stack.push(id);
    let kind: SchemaKind;
    try {
      kind = reg.resolve({ kind: "concrete", id, args: [] });
    } catch {
      stack.pop();
      onStack.delete(id);
      return;
    }
    for (const target of refTargets(kind)) {
      if (onStack.has(target)) {
        // Back-edge: every node from `target` up to the current top is on a cycle.
        for (let i = stack.length - 1; i >= 0; i--) {
          recursive.add(stack[i]!);
          if (stack[i] === target) break;
        }
      } else if (!visited.has(target)) {
        dfs(target);
      }
    }
    stack.pop();
    onStack.delete(id);
  };

  dfs(root);
  return recursive;
}

/// The concrete schema ids a kind references (its out-edges in the schema graph).
function refTargets(kind: SchemaKind): bigint[] {
  const out: bigint[] = [];
  const addRef = (ref: SchemaRef): void => {
    if (ref.kind === "concrete") {
      out.push(ref.id);
      for (const a of ref.args) addRef(a);
    }
  };
  switch (kind.kind) {
    case "struct":
      for (const f of kind.fields) addRef(f.schema);
      break;
    case "enum":
      for (const v of kind.variants) {
        switch (v.payload.kind) {
          case "newtype":
            addRef(v.payload.ref);
            break;
          case "tuple":
            for (const ref of v.payload.refs) addRef(ref);
            break;
          case "struct":
            for (const f of v.payload.fields) addRef(f.schema);
            break;
        }
      }
      break;
    case "tuple":
      for (const e of kind.elements) addRef(e);
      break;
    case "list":
    case "set":
    case "option":
    case "array":
    case "tensor":
    case "channel":
      addRef(kind.element);
      break;
    case "map":
      addRef(kind.key);
      addRef(kind.value);
      break;
    case "external":
      if (kind.metadata) addRef(kind.metadata);
      break;
  }
  return out;
}

// ============================================================================
// Interpreter (the baseline executor — mirror of Rust `exec`)
// ============================================================================

/// Decode writer compact `bytes` into a reader-shaped Value with a built plan,
/// rejecting trailing bytes.
export function decodeWithPlan(bytes: Uint8Array, plan: Plan, reg: Registry): Value {
  const r = new Reader(bytes);
  const v = exec(plan.root, r, reg, plan.blocks, 0);
  if (r.remaining() !== 0) throw new DecodeError(`${r.remaining()} trailing bytes`);
  return v;
}

/// Build a plan and decode in one step.
export function decode(bytes: Uint8Array, writerRoot: bigint, readerRoot: bigint, reg: Registry): Value {
  return decodeWithPlan(bytes, buildPlan(writerRoot, readerRoot, reg), reg);
}

function exec(node: Node, r: Reader, reg: Registry, blocks: Map<bigint, Node>, depth: number): Value {
  if (depth > MESSAGE_MAX_DEPTH) throw new DecodeError("maximum nesting depth exceeded");
  switch (node.kind) {
    case "callBlock": {
      // A recursive back-edge: decode with the cyclic schema's block plan. The
      // depth guard still bounds runaway recursion on hostile/over-deep input.
      const block = blocks.get(node.schema);
      if (!block) throw new DecodeError(`missing recursion block for schema ${node.schema.toString(16)}`);
      return exec(block, r, reg, blocks, depth + 1);
    }
    case "scalar":
      return decodePrimitive(r, node.primitive);
    case "struct":
      return execStruct(node.plan, r, reg, blocks, depth);
    case "enum": {
      const idx = r.readU32raw();
      const vp = node.byIndex.get(idx);
      if (!vp) throw new WriterOnlyVariantError(idx);
      const payload = execPayload(vp.payload, r, reg, blocks, depth);
      return new Map<string, Value>([[vp.reader, payload]]);
    }
    case "tuple": {
      const a: Value[] = [];
      for (const n of node.nodes) a.push(exec(n, r, reg, blocks, depth + 1));
      return a;
    }
    case "seq": {
      const n = r.readLen(node.minWire);
      const a: Value[] = [];
      const seen = node.set ? new Set<string>() : null;
      for (let i = 0; i < n; i++) {
        const v = exec(node.element, r, reg, blocks, depth + 1);
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
        const k = exec(node.key, r, reg, blocks, depth + 1);
        const v = exec(node.value, r, reg, blocks, depth + 1);
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
      for (let i = 0n; i < count; i++) a.push(exec(node.element, r, reg, blocks, depth + 1));
      return a;
    }
    case "option": {
      const b = r.readU8();
      if (b === 0) return null;
      if (b === 1) return exec(node.element, r, reg, blocks, depth + 1);
      throw new DecodeError(`invalid bool byte 0x${b.toString(16)}`);
    }
    case "dynamic":
      return readValue(r, depth);
  }
}

function execStruct(plan: StructPlan, r: Reader, reg: Registry, blocks: Map<bigint, Node>, depth: number): Value {
  const obj = new Map<string, Value>();
  for (const step of plan.steps) {
    if (step.kind === "take") {
      obj.set(step.reader, exec(step.node, r, reg, blocks, depth + 1));
    } else {
      // Walk the writer field by its own schema and discard it.
      decodeRef(r, step.ref, reg, depth + 1);
    }
  }
  for (const name of plan.defaults) obj.set(name, null);
  return obj;
}

function execPayload(p: Payload, r: Reader, reg: Registry, blocks: Map<bigint, Node>, depth: number): Value {
  switch (p.kind) {
    case "unit":
      return null;
    case "newtype":
      return exec(p.node, r, reg, blocks, depth + 1);
    case "tuple": {
      const a: Value[] = [];
      for (const n of p.nodes) a.push(exec(n, r, reg, blocks, depth + 1));
      return a;
    }
    case "struct":
      return execStruct(p.plan, r, reg, blocks, depth);
  }
}
