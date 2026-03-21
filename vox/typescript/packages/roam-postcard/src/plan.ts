// Translation plan for schema evolution.
//
// Ported from Rust's roam-postcard/src/plan.rs. Compares remote and local
// schemas and produces a plan that drives single-pass postcard decoding with
// field reordering, skipping, and default-filling.

import type {
  WireSchema,
  WireSchemaKind,
  WireSchemaRegistry,
  WireTypeRef,
  WireFieldSchema,
  WireVariantSchema,
  WireVariantPayload,
  SchemaHash,
} from "./schema.ts";
import { resolveWireTypeRef } from "./schema.ts";

// ============================================================================
// TranslationPlan
// ============================================================================

export type TranslationPlan =
  | { tag: "identity" }
  | { tag: "struct"; field_ops: FieldOp[]; nested: Map<number, TranslationPlan> }
  | {
      tag: "enum";
      variant_map: (number | null)[];
      variant_plans: Map<number, TranslationPlan>;
      nested: Map<number, TranslationPlan>;
    }
  | { tag: "tuple"; field_ops: FieldOp[]; nested: Map<number, TranslationPlan> }
  | { tag: "list"; element: TranslationPlan }
  | { tag: "option"; inner: TranslationPlan }
  | { tag: "map"; key: TranslationPlan; value: TranslationPlan }
  | { tag: "array"; element: TranslationPlan }
  | { tag: "pointer"; pointee: TranslationPlan };

export type FieldOp =
  | { tag: "read"; local_index: number }
  | { tag: "skip"; type_ref: WireTypeRef };

export const IDENTITY: TranslationPlan = { tag: "identity" };

// ============================================================================
// SchemaSet
// ============================================================================

export interface SchemaSet {
  root: WireSchema;
  registry: WireSchemaRegistry;
}

/** Build a SchemaSet from a list of schemas (e.g. received from the wire). */
export function schemaSetFromSchemas(schemas: WireSchema[]): SchemaSet {
  const root = schemas[schemas.length - 1];
  if (!root) throw new Error("empty schema list");
  const registry: WireSchemaRegistry = new Map();
  for (const s of schemas) {
    registry.set(s.id, s);
  }
  return { root, registry };
}

// ============================================================================
// buildPlan
// ============================================================================

export class TranslationError extends Error {
  path: string[];

  constructor(message: string) {
    super(message);
    this.name = "TranslationError";
    this.path = [];
  }

  withPathPrefix(segment: string): TranslationError {
    this.path.unshift(segment);
    this.message = `${this.path.join(".")}: ${this.message}`;
    return this;
  }
}

/**
 * Build a translation plan by comparing remote and local schemas.
 *
 * Returns `null` if the schemas are identical (identity plan).
 */
export function buildPlan(remote: SchemaSet, local: SchemaSet): TranslationPlan {
  return buildPlanInner(remote.root, local.root, remote.registry, local.registry);
}

function buildPlanInner(
  remote: WireSchema,
  local: WireSchema,
  remoteReg: WireSchemaRegistry,
  localReg: WireSchemaRegistry,
): TranslationPlan {
  // Validate type names match for nominal types
  const remoteName = schemaName(remote.kind);
  const localName = schemaName(local.kind);
  if (remoteName && localName && remoteName !== localName) {
    throw new TranslationError(
      `type name mismatch: remote "${remoteName}" vs local "${localName}"`,
    );
  }

  const rk = remote.kind;
  const lk = local.kind;

  if (isByteBufferKind(rk, remoteReg) && isByteBufferKind(lk, localReg)) {
    return IDENTITY;
  }

  if (rk.tag !== lk.tag) {
    throw new TranslationError(
      `schema kind mismatch: remote "${rk.tag}" vs local "${lk.tag}"`,
    );
  }

  switch (rk.tag) {
    case "struct": {
      const lkStruct = lk as Extract<WireSchemaKind, { tag: "struct" }>;
      return buildStructPlan(rk.fields, lkStruct.fields, remote, remoteReg, localReg);
    }
    case "enum": {
      const lkEnum = lk as Extract<WireSchemaKind, { tag: "enum" }>;
      return buildEnumPlan(rk.variants, lkEnum.variants, remote, local, remoteReg, localReg);
    }
    case "tuple": {
      const lkTuple = lk as Extract<WireSchemaKind, { tag: "tuple" }>;
      return buildTuplePlan(rk.elements, lkTuple.elements, remote, local, remoteReg, localReg);
    }
    case "list": {
      const lkList = lk as Extract<WireSchemaKind, { tag: "list" }>;
      const element = nestedPlan(rk.element, lkList.element, remoteReg, localReg);
      return { tag: "list", element: element ?? IDENTITY };
    }
    case "option": {
      const lkOpt = lk as Extract<WireSchemaKind, { tag: "option" }>;
      const inner = nestedPlan(rk.element, lkOpt.element, remoteReg, localReg);
      return { tag: "option", inner: inner ?? IDENTITY };
    }
    case "map": {
      const lkMap = lk as Extract<WireSchemaKind, { tag: "map" }>;
      const key = nestedPlan(rk.key, lkMap.key, remoteReg, localReg);
      const value = nestedPlan(rk.value, lkMap.value, remoteReg, localReg);
      return { tag: "map", key: key ?? IDENTITY, value: value ?? IDENTITY };
    }
    case "array": {
      const lkArr = lk as Extract<WireSchemaKind, { tag: "array" }>;
      const element = nestedPlan(rk.element, lkArr.element, remoteReg, localReg);
      return { tag: "array", element: element ?? IDENTITY };
    }
    case "primitive":
      {
        const lkPrimitive = lk as Extract<WireSchemaKind, { tag: "primitive" }>;
        if (rk.primitive_type !== lkPrimitive.primitive_type) {
          throw new TranslationError(
            `primitive type mismatch: remote "${rk.primitive_type}" vs local "${lkPrimitive.primitive_type}"`,
          );
        }
      }
      return IDENTITY;
    case "channel":
      return IDENTITY;
  }
}

function nestedPlan(
  remoteRef: WireTypeRef,
  localRef: WireTypeRef,
  remoteReg: WireSchemaRegistry,
  localReg: WireSchemaRegistry,
): TranslationPlan | null {
  const resolveSchema = (
    ref_: WireTypeRef,
    registry: WireSchemaRegistry,
    side: string,
  ): WireSchema => {
    if (ref_.tag === "var") {
      throw new TranslationError(`unresolved type variable "${ref_.name}" on ${side} side`);
    }
    const kind = resolveWireTypeRef(ref_, registry);
    if (!kind) {
      throw new TranslationError(
        `schema not found for type_id ${ref_.type_id} on ${side} side`,
      );
    }
    const base = registry.get(ref_.type_id);
    if (!base) {
      throw new TranslationError(
        `schema not found for type_id ${ref_.type_id} on ${side} side`,
      );
    }
    return { id: base.id, type_params: [], kind };
  };

  const remoteSchema = resolveSchema(remoteRef, remoteReg, "remote");
  const localSchema = resolveSchema(localRef, localReg, "local");

  return buildPlanInner(remoteSchema, localSchema, remoteReg, localReg);
}

function buildStructPlan(
  remoteFields: WireFieldSchema[],
  localFields: WireFieldSchema[],
  remoteSchema: WireSchema,
  remoteReg: WireSchemaRegistry,
  localReg: WireSchemaRegistry,
): TranslationPlan {
  const fieldOps: FieldOp[] = [];
  const nested = new Map<number, TranslationPlan>();
  const matched = new Array(localFields.length).fill(false);

  for (const rf of remoteFields) {
    const localIdx = localFields.findIndex((f) => f.name === rf.name);
    if (localIdx >= 0) {
      matched[localIdx] = true;
      fieldOps.push({ tag: "read", local_index: localIdx });

      const np = nestedPlan(rf.type_ref, localFields[localIdx].type_ref, remoteReg, localReg);
      if (np) nested.set(localIdx, np);
    } else {
      fieldOps.push({ tag: "skip", type_ref: rf.type_ref });
    }
  }

  // Check for missing required fields
  for (let i = 0; i < localFields.length; i++) {
    if (!matched[i] && !fieldHasDefault(localFields[i], localReg)) {
      throw new TranslationError(
        `required field "${localFields[i].name}" missing from remote schema "${schemaName(remoteSchema.kind) ?? "?"}"`,
      );
    }
  }

  return { tag: "struct", field_ops: fieldOps, nested };
}

function fieldHasDefault(
  field: WireFieldSchema,
  registry: WireSchemaRegistry,
): boolean {
  if (!field.required) {
    return true;
  }
  const kind = resolveWireTypeRef(field.type_ref, registry);
  return kind?.tag === "option";
}

function buildTuplePlan(
  remoteElements: WireTypeRef[],
  localElements: WireTypeRef[],
  _remoteSchema: WireSchema,
  _localSchema: WireSchema,
  remoteReg: WireSchemaRegistry,
  localReg: WireSchemaRegistry,
): TranslationPlan {
  if (remoteElements.length !== localElements.length) {
    throw new TranslationError(
      `tuple length mismatch: remote ${remoteElements.length} vs local ${localElements.length}`,
    );
  }

  const fieldOps: FieldOp[] = [];
  const nested = new Map<number, TranslationPlan>();

  for (let i = 0; i < remoteElements.length; i++) {
    fieldOps.push({ tag: "read", local_index: i });
    const np = nestedPlan(remoteElements[i], localElements[i], remoteReg, localReg);
    if (np) nested.set(i, np);
  }

  return { tag: "tuple", field_ops: fieldOps, nested };
}

function buildEnumPlan(
  remoteVariants: WireVariantSchema[],
  localVariants: WireVariantSchema[],
  remoteSchema: WireSchema,
  localSchema: WireSchema,
  remoteReg: WireSchemaRegistry,
  localReg: WireSchemaRegistry,
): TranslationPlan {
  const variantMap: (number | null)[] = [];
  const variantPlans = new Map<number, TranslationPlan>();
  const nested = new Map<number, TranslationPlan>();

  for (let remoteIdx = 0; remoteIdx < remoteVariants.length; remoteIdx++) {
    const rv = remoteVariants[remoteIdx];
    const localIdx = localVariants.findIndex((v) => v.name === rv.name);

    if (localIdx < 0) {
      // Unknown remote variant
      variantMap.push(null);
      continue;
    }

    variantMap.push(localIdx);
    const lv = localVariants[localIdx];

    if (rv.payload.tag !== lv.payload.tag) {
      throw new TranslationError(
        `variant "${rv.name}": payload kind mismatch "${rv.payload.tag}" vs "${lv.payload.tag}"`,
      );
    }

    switch (rv.payload.tag) {
      case "struct": {
        const lvPayload = lv.payload as Extract<WireVariantPayload, { tag: "struct" }>;
        const varFieldOps: FieldOp[] = rv.payload.fields.map((rf) => {
          const idx = lvPayload.fields.findIndex((f) => f.name === rf.name);
          if (idx >= 0) {
            return { tag: "read" as const, local_index: idx };
          } else {
            return { tag: "skip" as const, type_ref: rf.type_ref };
          }
        });
        variantPlans.set(remoteIdx, {
          tag: "struct",
          field_ops: varFieldOps,
          nested: new Map(),
        });
        break;
      }
      case "newtype": {
        const lvPayload = lv.payload as Extract<WireVariantPayload, { tag: "newtype" }>;
        const np = nestedPlan(rv.payload.type_ref, lvPayload.type_ref, remoteReg, localReg);
        if (np) nested.set(localIdx, np);
        break;
      }
      case "tuple": {
        const lvPayload = lv.payload as Extract<WireVariantPayload, { tag: "tuple" }>;
        const tuplePlan = buildTuplePlan(
          rv.payload.types,
          lvPayload.types,
          remoteSchema,
          localSchema,
          remoteReg,
          localReg,
        );
        variantPlans.set(remoteIdx, tuplePlan);
        break;
      }
      case "unit":
        break;
    }
  }

  return { tag: "enum", variant_map: variantMap, variant_plans: variantPlans, nested };
}

function schemaName(kind: WireSchemaKind): string | null {
  if (kind.tag === "struct" || kind.tag === "enum") return kind.name;
  return null;
}

function isByteBufferKind(
  kind: WireSchemaKind,
  registry: WireSchemaRegistry,
): boolean {
  if (kind.tag === "primitive") {
    return kind.primitive_type === "bytes";
  }
  if (kind.tag !== "list") {
    return false;
  }
  const elementKind = resolveWireTypeRef(kind.element, registry);
  return elementKind?.tag === "primitive" && elementKind.primitive_type === "u8";
}
