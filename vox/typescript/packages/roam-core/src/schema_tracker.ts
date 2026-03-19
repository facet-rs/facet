// Schema tracker for schema exchange.
//
// Receives CBOR-encoded SchemaMessage payloads from the remote peer,
// parses them into the TypeScript Schema format used by roam-postcard,
// and provides per-method translation-aware schemas for decoding.

// r[impl schema.tracking.received]
// r[impl schema.translation.field-matching]
// r[impl schema.translation.skip-unknown]
// r[impl schema.translation.fill-defaults]
// r[impl schema.translation.reorder]
// r[impl schema.errors.early-detection]
// r[impl schema.errors.type-mismatch]
// r[impl schema.errors.missing-required]

import type { Schema, StructSchema, EnumSchema, TupleSchema, SchemaRegistry } from "@bearcove/roam-postcard";
import {
  decodeCbor,
  cborUint,
  cborUint64,
  cborNull,
  cborBool,
  cborText,
  cborTupleStruct1,
  cborArray,
  cborMap,
  cborEmptyMap,
  cborEnum,
  type CborMap,
  type CborValue,
} from "./cbor.ts";
import { roamLogger } from "./logger.ts";

// A 16-byte type identifier, stored as a hex string for Map keying.
type TypeIdHex = string;

// Parsed remote schema in an intermediate representation (close to the Rust Schema).
interface RemoteSchema {
  typeIdHex: TypeIdHex;
  kind: RemoteSchemaKind;
}

type RemoteSchemaKind =
  | { tag: "Primitive"; primitiveType: string }
  | { tag: "Struct"; fields: RemoteFieldSchema[] }
  | { tag: "Enum"; variants: RemoteVariantSchema[] }
  | { tag: "Tuple"; elements: TypeIdHex[] }
  | { tag: "List"; element: TypeIdHex }
  | { tag: "Map"; key: TypeIdHex; value: TypeIdHex }
  | { tag: "Set"; element: TypeIdHex }
  | { tag: "Array"; element: TypeIdHex; length: number }
  | { tag: "Option"; element: TypeIdHex };

interface RemoteFieldSchema {
  name: string;
  typeIdHex: TypeIdHex;
  required: boolean;
}

interface RemoteVariantSchema {
  name: string;
  index: number;
  payload: RemoteVariantPayload;
}

type RemoteVariantPayload =
  | { tag: "Unit" }
  | { tag: "Newtype"; typeIdHex: TypeIdHex }
  | { tag: "Struct"; fields: RemoteFieldSchema[] };

interface RemoteMethodBinding {
  methodId: number;
  rootTypeIdHex: TypeIdHex;
}

/**
 * Translation plan for decoding postcard bytes with schema differences.
 *
 * Used when the remote's schema differs from our local schema. Tells the
 * decoder how to map remote fields to local fields.
 */
export interface TranslationPlan {
  kind: "struct" | "enum" | "tuple" | "identity";
  // For structs: instructions for reading remote fields in wire order.
  fieldOps?: FieldOp[];
  // For structs: local field names and their default values for unmatched fields.
  localDefaults?: Map<string, unknown>;
  // For enums: maps remote variant index → local variant name (null = unknown).
  variantMap?: (string | null)[];
}

export type FieldOp =
  | { kind: "read"; localFieldName: string }
  | { kind: "skip"; remoteSchema: Schema };

/**
 * Tracks schemas received from the remote peer and provides
 * translation-aware schemas for decoding.
 */
export class SchemaTracker {
  private remoteSchemas = new Map<TypeIdHex, RemoteSchema>();
  private methodBindings = new Map<number, TypeIdHex>();

  /**
   * Record a CBOR-encoded SchemaMessage payload from the remote peer.
   */
  recordReceived(cborBytes: Uint8Array): void {
    const parsed = decodeCbor(cborBytes);
    const payload = parsed.value as CborMap;

    const schemas = payload["schemas"] as CborValue[];
    if (schemas) {
      for (const raw of schemas) {
        const schema = parseRemoteSchema(raw as CborMap);
        this.remoteSchemas.set(schema.typeIdHex, schema);
      }
    }

    const bindings = payload["method_bindings"] as CborValue[];
    if (bindings) {
      for (const raw of bindings) {
        const binding = parseMethodBinding(raw as CborMap);
        this.methodBindings.set(binding.methodId, binding.rootTypeIdHex);
      }
    }

    roamLogger()?.debug(
      `[roam:schema] recorded ${schemas?.length ?? 0} schemas, ${bindings?.length ?? 0} bindings`,
    );
  }

  /**
   * Build a translation-aware args schema for a method.
   *
   * Given the method's local args schema (a TupleSchema from the ServiceDescriptor),
   * returns a modified schema that reads bytes in the remote's wire order
   * but produces values matching the local schema. Also returns a post-decode
   * transform function that fills defaults and reorders fields.
   *
   * Returns null if no remote schema is available for this method (identity decode).
   */
  buildArgsTranslation(
    methodId: bigint,
    localArgs: TupleSchema,
    localRegistry?: SchemaRegistry,
  ): { remoteArgsSchema: TupleSchema; transforms: ArgTransform[] } | null {
    const rootHex = this.methodBindings.get(Number(methodId));
    if (!rootHex) {
      return null;
    }

    const rootSchema = this.remoteSchemas.get(rootHex);
    if (!rootSchema) {
      return null;
    }

    // The root type for args is always a Tuple.
    if (rootSchema.kind.tag !== "Tuple") {
      roamLogger()?.error(`[roam:schema] expected Tuple root for method args, got ${rootSchema.kind.tag}`);
      return null;
    }

    const remoteElements = rootSchema.kind.elements;
    if (remoteElements.length !== localArgs.elements.length) {
      throw new SchemaTranslationError(
        `args tuple length mismatch: remote has ${remoteElements.length}, local has ${localArgs.elements.length}`,
      );
    }

    const remoteArgSchemas: Schema[] = [];
    const transforms: ArgTransform[] = [];

    for (let i = 0; i < remoteElements.length; i++) {
      const remoteArgHex = remoteElements[i];
      const localArgSchema = localArgs.elements[i];
      const resolvedLocal = resolveLocal(localArgSchema, localRegistry);

      const result = this.buildSchemaTranslation(remoteArgHex, resolvedLocal, localRegistry);
      remoteArgSchemas.push(result.remoteSchema);
      transforms.push(result.transform);
    }

    return {
      remoteArgsSchema: { kind: "tuple", elements: remoteArgSchemas },
      transforms,
    };
  }

  /**
   * Build a translation for a single type: the remote schema to use for decoding
   * and a transform to apply after decoding to match local expectations.
   */
  private buildSchemaTranslation(
    remoteTypeHex: TypeIdHex,
    localSchema: Schema,
    localRegistry?: SchemaRegistry,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    const remoteSchema = this.remoteSchemas.get(remoteTypeHex);
    if (!remoteSchema) {
      // No remote schema for this type — assume identity (types match).
      return { remoteSchema: localSchema, transform: identityTransform };
    }

    return this.buildTranslation(remoteSchema, localSchema, localRegistry);
  }

  private buildTranslation(
    remote: RemoteSchema,
    localSchema: Schema,
    localRegistry?: SchemaRegistry,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    const resolved = resolveLocal(localSchema, localRegistry);

    switch (remote.kind.tag) {
      case "Struct":
        return this.buildStructTranslation(remote.kind.fields, resolved, localRegistry);
      case "Enum":
        return this.buildEnumTranslation(remote.kind.variants, resolved);
      case "Tuple":
        return this.buildTupleTranslation(remote.kind.elements, resolved, localRegistry);
      case "Option": {
        const localOption = resolved as { kind: "option"; inner: Schema };
        if (localOption.kind !== "option") {
          throw new SchemaTranslationError(
            `kind mismatch: remote is Option, local is ${localOption.kind}`,
          );
        }
        const inner = this.buildSchemaTranslation(remote.kind.element, localOption.inner, localRegistry);
        return {
          remoteSchema: { kind: "option", inner: inner.remoteSchema },
          transform: (value: unknown) => {
            if (value === null || value === undefined) return value;
            return inner.transform(value);
          },
        };
      }
      case "List": {
        const localVec = resolved as { kind: "vec"; element: Schema };
        if (localVec.kind !== "vec") {
          throw new SchemaTranslationError(
            `kind mismatch: remote is List, local is ${localVec.kind}`,
          );
        }
        const elemResult = this.buildSchemaTranslation(remote.kind.element, localVec.element, localRegistry);
        return {
          remoteSchema: { kind: "vec", element: elemResult.remoteSchema },
          transform: (value: unknown) => {
            const arr = value as unknown[];
            return arr.map(elemResult.transform);
          },
        };
      }
      case "Primitive":
        return this.buildPrimitiveTranslation(remote.kind.primitiveType, resolved);
      default:
        // For Map, Set, Array, etc. — just use local schema (assume compatible).
        return { remoteSchema: localSchema, transform: identityTransform };
    }
  }

  private buildPrimitiveTranslation(
    remotePrimitive: string,
    localSchema: Schema,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    const localKind = localSchema.kind;
    const remoteKind = primitiveToSchemaKind(remotePrimitive);

    if (remoteKind !== localKind) {
      throw new SchemaTranslationError(
        `type mismatch: remote field is ${remoteKind}, local is ${localKind}`,
      );
    }

    return { remoteSchema: localSchema, transform: identityTransform };
  }

  // r[impl schema.translation.field-matching]
  // r[impl schema.translation.reorder]
  // r[impl schema.translation.skip-unknown]
  // r[impl schema.translation.fill-defaults]
  private buildStructTranslation(
    remoteFields: RemoteFieldSchema[],
    localSchema: Schema,
    localRegistry?: SchemaRegistry,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    if (localSchema.kind !== "struct") {
      throw new SchemaTranslationError(
        `kind mismatch: remote is Struct, local is ${localSchema.kind}`,
      );
    }

    const localStruct = localSchema as StructSchema;
    const localFieldNames = Object.keys(localStruct.fields);
    const localFieldSchemas = Object.entries(localStruct.fields);

    // Build the remote struct schema in remote field order for decoding.
    // Also track which local fields are matched.
    const remoteDecodingFields: Record<string, Schema> = {};
    const matchedLocalFields = new Set<string>();
    const fieldOps: FieldOp[] = [];

    for (const remoteField of remoteFields) {
      const localEntry = localFieldSchemas.find(([name]) => name === remoteField.name);

      if (localEntry) {
        const [localName, localFieldSchema] = localEntry;
        matchedLocalFields.add(localName);

        // Build translation for this field's type.
        const resolvedLocalField = resolveLocal(localFieldSchema, localRegistry);
        const fieldTranslation = this.buildSchemaTranslation(
          remoteField.typeIdHex,
          resolvedLocalField,
          localRegistry,
        );
        remoteDecodingFields[remoteField.name] = fieldTranslation.remoteSchema;
        fieldOps.push({ kind: "read", localFieldName: localName });
      } else {
        // Remote field not in local type — skip it during decode.
        const skipSchema = this.remoteTypeToSchema(remoteField.typeIdHex);
        remoteDecodingFields[remoteField.name] = skipSchema;
        fieldOps.push({ kind: "skip", remoteSchema: skipSchema });
      }
    }

    // r[impl schema.errors.missing-required]
    // Check for local fields not present in remote schema.
    const defaults = new Map<string, unknown>();
    for (const localName of localFieldNames) {
      if (!matchedLocalFields.has(localName)) {
        const localFieldSchema = resolveLocal(localStruct.fields[localName], localRegistry);
        if (localFieldSchema.kind === "option") {
          defaults.set(localName, null);
        } else {
          throw new SchemaTranslationError(
            `missing required field "${localName}" (type: ${localFieldSchema.kind}): remote does not provide it and it has no default`,
          );
        }
      }
    }

    const remoteSchema: StructSchema = {
      kind: "struct",
      fields: remoteDecodingFields,
    };

    // Build the transform: takes the decoded remote object (with remote field order),
    // produces the local object (with local field order, defaults filled).
    const transform: ArgTransform = (value: unknown) => {
      const remoteObj = value as Record<string, unknown>;
      const localObj: Record<string, unknown> = {};

      // Copy matched fields.
      for (const op of fieldOps) {
        if (op.kind === "read") {
          localObj[op.localFieldName] = remoteObj[op.localFieldName];
        }
        // Skip ops are just ignored — the value was decoded but we don't need it.
      }

      // Fill defaults.
      for (const [name, defaultValue] of defaults) {
        localObj[name] = defaultValue;
      }

      return localObj;
    };

    return { remoteSchema, transform };
  }

  // r[impl schema.translation.enum]
  // r[impl schema.translation.enum.unknown-variant]
  private buildEnumTranslation(
    remoteVariants: RemoteVariantSchema[],
    localSchema: Schema,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    if (localSchema.kind !== "enum") {
      throw new SchemaTranslationError(
        `kind mismatch: remote is Enum, local is ${localSchema.kind}`,
      );
    }

    const localEnum = localSchema as EnumSchema;

    // Build remote enum schema for decoding.
    // The remote schema uses the remote variant order and discriminants.
    // We decode with this, then map variant names (which should match).
    const remoteEnumSchema: EnumSchema = {
      kind: "enum",
      variants: remoteVariants.map((rv) => ({
        name: rv.name,
        discriminant: rv.index,
        fields: this.remoteVariantPayloadToFields(rv.payload),
      })),
    };

    // The postcard enum decoder already uses variant names in the output
    // (the `tag` field), so we just need to verify compatibility.
    // Unknown remote variants (not in local) will error at decode time
    // only if that variant is actually received.
    return { remoteSchema: remoteEnumSchema, transform: identityTransform };
  }

  private buildTupleTranslation(
    remoteElements: TypeIdHex[],
    localSchema: Schema,
    localRegistry?: SchemaRegistry,
  ): { remoteSchema: Schema; transform: ArgTransform } {
    if (localSchema.kind !== "tuple") {
      throw new SchemaTranslationError(
        `kind mismatch: remote is Tuple, local is ${localSchema.kind}`,
      );
    }

    const localTuple = localSchema as TupleSchema;
    if (remoteElements.length !== localTuple.elements.length) {
      throw new SchemaTranslationError(
        `tuple length mismatch: remote ${remoteElements.length}, local ${localTuple.elements.length}`,
      );
    }

    const remoteSchemas: Schema[] = [];
    const elementTransforms: ArgTransform[] = [];

    for (let i = 0; i < remoteElements.length; i++) {
      const result = this.buildSchemaTranslation(
        remoteElements[i],
        localTuple.elements[i],
        localRegistry,
      );
      remoteSchemas.push(result.remoteSchema);
      elementTransforms.push(result.transform);
    }

    const hasTransforms = elementTransforms.some((t) => t !== identityTransform);
    return {
      remoteSchema: { kind: "tuple", elements: remoteSchemas },
      transform: hasTransforms
        ? (value: unknown) => {
            const arr = value as unknown[];
            return arr.map((v, i) => elementTransforms[i](v));
          }
        : identityTransform,
    };
  }

  /**
   * Convert a remote TypeSchemaId to a TypeScript Schema for decoding.
   * Used for skip fields — we need to know the schema to skip the bytes.
   */
  private remoteTypeToSchema(typeIdHex: TypeIdHex): Schema {
    const remote = this.remoteSchemas.get(typeIdHex);
    if (!remote) {
      throw new SchemaTranslationError(
        `unknown remote type ${typeIdHex} — cannot build skip schema`,
      );
    }

    switch (remote.kind.tag) {
      case "Primitive":
        return { kind: primitiveToSchemaKind(remote.kind.primitiveType) as Schema["kind"] } as Schema;
      case "Struct": {
        const fields: Record<string, Schema> = {};
        for (const f of remote.kind.fields) {
          fields[f.name] = this.remoteTypeToSchema(f.typeIdHex);
        }
        return { kind: "struct", fields };
      }
      case "Enum":
        return {
          kind: "enum",
          variants: remote.kind.variants.map((v) => ({
            name: v.name,
            discriminant: v.index,
            fields: this.remoteVariantPayloadToFields(v.payload),
          })),
        };
      case "Tuple":
        return {
          kind: "tuple",
          elements: remote.kind.elements.map((e) => this.remoteTypeToSchema(e)),
        };
      case "Option":
        return { kind: "option", inner: this.remoteTypeToSchema(remote.kind.element) };
      case "List":
        return { kind: "vec", element: this.remoteTypeToSchema(remote.kind.element) };
      case "Map":
        return {
          kind: "map",
          key: this.remoteTypeToSchema(remote.kind.key),
          value: this.remoteTypeToSchema(remote.kind.value),
        };
      default:
        throw new SchemaTranslationError(
          `cannot convert remote schema kind ${remote.kind.tag} to TypeScript schema`,
        );
    }
  }

  private remoteVariantPayloadToFields(
    payload: RemoteVariantPayload,
  ): null | Schema | Record<string, Schema> {
    switch (payload.tag) {
      case "Unit":
        return null;
      case "Newtype":
        return this.remoteTypeToSchema(payload.typeIdHex);
      case "Struct": {
        const fields: Record<string, Schema> = {};
        for (const f of payload.fields) {
          fields[f.name] = this.remoteTypeToSchema(f.typeIdHex);
        }
        return fields;
      }
    }
  }
}

/** A transform applied to decoded values to match local schema. */
export type ArgTransform = (value: unknown) => unknown;

const identityTransform: ArgTransform = (value: unknown) => value;

export class SchemaTranslationError extends Error {
  constructor(message: string) {
    super(`Schema translation error: ${message}`);
    this.name = "SchemaTranslationError";
  }
}

// ============================================================================
// CBOR Parsing Helpers
// ============================================================================

function parseTypeIdHex(raw: CborValue): TypeIdHex {
  // TypeSchemaId is currently a tuple struct wrapping a u32.
  // In CBOR that arrives as array(1) containing the numeric ID.
  //
  // Older experimental code paths expected a [u8; 16] payload, so we
  // continue to accept that shape as a fallback for compatibility.
  if (Array.isArray(raw)) {
    const first = raw[0];

    if (typeof first === "number") {
      return String(first);
    }

    if (Array.isArray(first)) {
      const bytes = new Uint8Array(first as number[]);
      return hexFromBytes(bytes);
    }
  }

  if (typeof raw === "number") {
    return String(raw);
  }

  throw new Error(`unsupported TypeSchemaId encoding: ${JSON.stringify(raw)}`);
}

function hexFromBytes(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) {
    hex += b.toString(16).padStart(2, "0");
  }
  return hex;
}

function parseRemoteSchema(raw: CborMap): RemoteSchema {
  const typeIdHex = parseTypeIdHex(raw["type_id"]);
  const kindMap = raw["kind"] as CborMap;
  const kind = parseSchemaKind(kindMap);
  return { typeIdHex, kind };
}

function parseSchemaKind(kindMap: CborMap): RemoteSchemaKind {
  // facet-cbor encodes enums as maps with one entry: variant_name → payload
  const keys = Object.keys(kindMap);
  if (keys.length !== 1) {
    throw new Error(`expected enum map with 1 key, got ${keys.length}: ${keys.join(", ")}`);
  }
  const variant = keys[0];
  const payload = kindMap[variant] as CborMap;

  switch (variant) {
    case "Primitive": {
      const ptMap = payload["primitive_type"] as CborMap;
      // PrimitiveType is also an enum: {"String": null}, {"U32": null}, etc.
      const ptKeys = Object.keys(ptMap);
      return { tag: "Primitive", primitiveType: ptKeys[0] };
    }
    case "Struct": {
      const fieldsRaw = payload["fields"] as CborValue[];
      const fields = fieldsRaw.map(parseFieldSchema);
      return { tag: "Struct", fields };
    }
    case "Enum": {
      const variantsRaw = payload["variants"] as CborValue[];
      const variants = variantsRaw.map(parseVariantSchema);
      return { tag: "Enum", variants };
    }
    case "Tuple": {
      const elementsRaw = payload["elements"] as CborValue[];
      const elements = elementsRaw.map(parseTypeIdHex);
      return { tag: "Tuple", elements };
    }
    case "List":
      return { tag: "List", element: parseTypeIdHex(payload["element"]) };
    case "Map":
      return {
        tag: "Map",
        key: parseTypeIdHex(payload["key"]),
        value: parseTypeIdHex(payload["value"]),
      };
    case "Set":
      return { tag: "Set", element: parseTypeIdHex(payload["element"]) };
    case "Array":
      return {
        tag: "Array",
        element: parseTypeIdHex(payload["element"]),
        length: payload["length"] as number,
      };
    case "Option":
      return { tag: "Option", element: parseTypeIdHex(payload["element"]) };
    default:
      throw new Error(`unknown SchemaKind variant: ${variant}`);
  }
}

function parseMethodBinding(raw: CborValue): RemoteMethodBinding {
  const map = raw as CborMap;
  return {
    methodId: map["method_id"] as number,
    rootTypeIdHex: parseTypeIdHex(map["root_type_schema_id"]),
  };
}

function parseFieldSchema(raw: CborValue): RemoteFieldSchema {
  const map = raw as CborMap;
  return {
    name: map["name"] as string,
    typeIdHex: parseTypeIdHex(map["type_id"]),
    required: map["required"] as boolean,
  };
}

function parseVariantSchema(raw: CborValue): RemoteVariantSchema {
  const map = raw as CborMap;
  return {
    name: map["name"] as string,
    index: map["index"] as number,
    payload: parseVariantPayload(map["payload"] as CborMap),
  };
}

function parseVariantPayload(raw: CborValue): RemoteVariantPayload {
  // Unit variant is encoded as {"Unit": null}
  // Newtype: {"Newtype": {"type_id": ...}}
  // Struct: {"Struct": {"fields": [...]}}
  const map = raw as CborMap;
  const keys = Object.keys(map);
  const variant = keys[0];

  switch (variant) {
    case "Unit":
      return { tag: "Unit" };
    case "Newtype": {
      const inner = map["Newtype"] as CborMap;
      return { tag: "Newtype", typeIdHex: parseTypeIdHex(inner["type_id"]) };
    }
    case "Struct": {
      const inner = map["Struct"] as CborMap;
      const fieldsRaw = inner["fields"] as CborValue[];
      return { tag: "Struct", fields: fieldsRaw.map(parseFieldSchema) };
    }
    default:
      throw new Error(`unknown VariantPayload variant: ${variant}`);
  }
}

function primitiveToSchemaKind(primitiveType: string): string {
  switch (primitiveType) {
    case "Bool": return "bool";
    case "U8": return "u8";
    case "U16": return "u16";
    case "U32": return "u32";
    case "U64": return "u64";
    case "I8": return "i8";
    case "I16": return "i16";
    case "I32": return "i32";
    case "I64": return "i64";
    case "F32": return "f32";
    case "F64": return "f64";
    case "String": return "string";
    case "Bytes": return "bytes";
    case "Unit": return "bool"; // Unit is zero-sized, but we won't encounter it
    default: return primitiveType.toLowerCase();
  }
}

function resolveLocal(schema: Schema, registry?: SchemaRegistry): Schema {
  if (schema.kind === "ref" && registry) {
    const resolved = registry.get((schema as { kind: "ref"; name: string }).name);
    if (resolved) return resolved;
  }
  return schema;
}

// ============================================================================
// SchemaSendTracker — outbound schema exchange (TypeScript → Rust CBOR format)
// ============================================================================

// One Rust-format Schema entry ready for CBOR encoding.
interface RustSchema {
  typeId: number;
  name: string;
  kindBytes: Uint8Array; // pre-encoded CBOR for the SchemaKind enum value
}

/**
 * Tracks which method+direction schemas have been sent on a connection.
 *
 * Mirrors Rust's `SchemaSendTracker`. Call `reset()` on reconnect.
 * Call `prepareSchemas()` to get the CBOR bytes to embed in
 * RequestCall.schemas / RequestResponse.schemas.
 */
export class SchemaSendTracker {
  private sentMethods = new Set<string>();
  private nextTypeId = 1;
  // fingerprint → assigned TypeSchemaId
  private schemaIds = new Map<string, number>();
  // All schemas already sent (typeId → schema), for dedup
  private sentTypeIds = new Set<number>();

  reset(): void {
    this.sentMethods.clear();
    this.nextTypeId = 1;
    this.schemaIds.clear();
    this.sentTypeIds.clear();
  }

  /**
   * Returns CBOR bytes for the SchemaMessagePayload to embed in a
   * request/response for method `methodId` in `direction`.
   * Returns empty Uint8Array if schemas for this method+direction were already sent.
   */
  prepareSchemas(
    methodId: bigint,
    direction: "args" | "response",
    schema: Schema,
    registry: SchemaRegistry | undefined,
  ): Uint8Array {
    const key = `${methodId}:${direction}`;
    if (this.sentMethods.has(key)) return new Uint8Array(0);
    this.sentMethods.add(key);

    const collected: RustSchema[] = [];
    const rootId = this._collectSchema(schema, registry, collected);

    // Filter to only unsent schemas
    const newSchemas = collected.filter((s) => !this.sentTypeIds.has(s.typeId));
    for (const s of newSchemas) this.sentTypeIds.add(s.typeId);

    if (newSchemas.length === 0 && rootId === 0) return new Uint8Array(0);

    const schemasCbor = cborArray(newSchemas.map(encodeSingleSchema));

    // MethodSchemaBinding: struct map
    const bindingCbor = cborMap([
      ["method_id", cborUint64(methodId)],
      ["root_type_schema_id", cborTupleStruct1(cborUint(rootId))],
      ["direction", cborEnum(direction === "args" ? "Args" : "Response", cborNull())],
    ]);
    const bindingsCbor = cborArray([bindingCbor]);

    // SchemaMessagePayload: struct map
    return cborMap([
      ["schemas", schemasCbor],
      ["method_bindings", bindingsCbor],
    ]);
  }

  private _fingerprintSchema(schema: Schema, registry: SchemaRegistry | undefined): string {
    const resolved = resolveLocal(schema, registry);
    if (resolved.kind === "ref") {
      return `ref:${(resolved as { kind: "ref"; name: string }).name}`;
    }
    return JSON.stringify(resolved);
  }

  private _idForSchema(schema: Schema, registry: SchemaRegistry | undefined): number {
    const fp = this._fingerprintSchema(schema, registry);
    const existing = this.schemaIds.get(fp);
    if (existing !== undefined) return existing;
    const id = this.nextTypeId++;
    this.schemaIds.set(fp, id);
    return id;
  }

  /**
   * Recursively collect all schemas reachable from `schema`.
   * Returns the TypeSchemaId for the root schema.
   */
  private _collectSchema(
    schema: Schema,
    registry: SchemaRegistry | undefined,
    out: RustSchema[],
  ): number {
    const resolved = resolveLocal(schema, registry);

    // If already assigned an ID and already collected, just return the ID.
    const fp = this._fingerprintSchema(resolved, registry);
    const existing = this.schemaIds.get(fp);
    if (existing !== undefined) return existing;

    const id = this._idForSchema(resolved, registry);
    // Send empty name — TypeScript doesn't know Rust type names.
    // Rust skips the name-mismatch check when remote name is empty.
    const typeName = "";

    // Build the SchemaKind CBOR bytes
    const kindBytes = this._encodeSchemaKind(resolved, registry, out);
    out.push({ typeId: id, name: typeName, kindBytes });
    return id;
  }

  private _encodeSchemaKind(
    schema: Schema,
    registry: SchemaRegistry | undefined,
    out: RustSchema[],
  ): Uint8Array {
    switch (schema.kind) {
      case "struct": {
        const s = schema as import("@bearcove/roam-postcard").StructSchema;
        const fields = Object.entries(s.fields).map(([name, fieldSchema]) => {
          const fieldId = this._collectSchema(fieldSchema as Schema, registry, out);
          const required = !(fieldSchema as Schema & { optional?: boolean }).optional;
          return cborMap([
            ["name", cborText(name)],
            ["type_id", cborTupleStruct1(cborUint(fieldId))],
            ["required", cborBool(required)],
          ]);
        });
        return cborEnum("Struct", cborMap([["fields", cborArray(fields)]]));
      }
      case "enum": {
        const e = schema as import("@bearcove/roam-postcard").EnumSchema;
        const variants = e.variants.map((v, idx) => {
          const discriminant = v.discriminant ?? idx;
          const payload = this._encodeVariantPayload(v.fields, registry, out);
          return cborMap([
            ["name", cborText(v.name)],
            ["index", cborUint(discriminant)],
            ["payload", payload],
          ]);
        });
        return cborEnum("Enum", cborMap([["variants", cborArray(variants)]]));
      }
      case "tuple": {
        const t = schema as import("@bearcove/roam-postcard").TupleSchema;
        const elemIds = t.elements.map((e) => {
          const id = this._collectSchema(e as Schema, registry, out);
          return cborTupleStruct1(cborUint(id));
        });
        return cborEnum("Tuple", cborMap([["elements", cborArray(elemIds)]]));
      }
      case "vec": {
        const v = schema as import("@bearcove/roam-postcard").VecSchema;
        const elemId = this._collectSchema(v.element as Schema, registry, out);
        return cborEnum("List", cborMap([["element", cborTupleStruct1(cborUint(elemId))]]));
      }
      case "option": {
        const o = schema as import("@bearcove/roam-postcard").OptionSchema;
        const innerId = this._collectSchema(o.inner as Schema, registry, out);
        return cborEnum("Option", cborMap([["element", cborTupleStruct1(cborUint(innerId))]]));
      }
      case "map": {
        const m = schema as import("@bearcove/roam-postcard").MapSchema;
        const keyId = this._collectSchema(m.key as Schema, registry, out);
        const valId = this._collectSchema(m.value as Schema, registry, out);
        return cborEnum("Map", cborMap([
          ["key", cborTupleStruct1(cborUint(keyId))],
          ["value", cborTupleStruct1(cborUint(valId))],
        ]));
      }

      case "bytes":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("Bytes", cborNull())]]));
      case "string":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("String", cborNull())]]));
      case "bool":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("Bool", cborNull())]]));
      case "u8":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("U8", cborNull())]]));
      case "u16":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("U16", cborNull())]]));
      case "u32":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("U32", cborNull())]]));
      case "u64":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("U64", cborNull())]]));
      case "i8":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("I8", cborNull())]]));
      case "i16":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("I16", cborNull())]]));
      case "i32":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("I32", cborNull())]]));
      case "i64":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("I64", cborNull())]]));
      case "f32":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("F32", cborNull())]]));
      case "f64":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("F64", cborNull())]]));
      case "ref": {
        const name = (schema as { kind: "ref"; name: string }).name;
        const resolved = registry?.get(name);
        if (resolved) {
          return this._encodeSchemaKind(resolved, registry, out);
        }
        // Fallback: treat as unknown struct
        return cborEnum("Struct", cborMap([["fields", cborArray([])]]));
      }
      // tx/rx are channel types — encode as unit (no schema needed)
      case "tx":
      case "rx":
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("Unit", cborNull())]]));
      default:
        return cborEnum("Primitive", cborMap([["primitive_type", cborEnum("Unit", cborNull())]]));
    }
  }

  private _encodeVariantPayload(
    fields: Schema | Schema[] | { [key: string]: Schema } | null | undefined,
    registry: SchemaRegistry | undefined,
    out: RustSchema[],
  ): Uint8Array {
    if (fields === null || fields === undefined) {
      return cborEnum("Unit", cborNull());
    }
    // Tuple variant (Schema[]) — treat as Newtype if 1 element, else Struct with "0","1",... names
    if (Array.isArray(fields)) {
      if (fields.length === 1) {
        const innerId = this._collectSchema(fields[0] as Schema, registry, out);
        return cborEnum("Newtype", cborMap([["type_id", cborTupleStruct1(cborUint(innerId))]]));
      }
      const fieldEntries = fields.map((fieldSchema, idx) => {
        const fieldId = this._collectSchema(fieldSchema as Schema, registry, out);
        return cborMap([
          ["name", cborText(String(idx))],
          ["type_id", cborTupleStruct1(cborUint(fieldId))],
          ["required", cborBool(true)],
        ]);
      });
      return cborEnum("Struct", cborMap([["fields", cborArray(fieldEntries)]]));
    }
    if (typeof fields === "object" && "kind" in fields) {
      // Newtype variant — single schema
      const innerId = this._collectSchema(fields as Schema, registry, out);
      return cborEnum("Newtype", cborMap([["type_id", cborTupleStruct1(cborUint(innerId))]]));
    }
    // Struct variant
    const fieldMap = fields as { [key: string]: Schema };
    const fieldEntries = Object.entries(fieldMap).map(([name, fieldSchema]) => {
      const fieldId = this._collectSchema(fieldSchema as Schema, registry, out);
      return cborMap([
        ["name", cborText(name)],
        ["type_id", cborTupleStruct1(cborUint(fieldId))],
        ["required", cborBool(true)],
      ]);
    });
    return cborEnum("Struct", cborMap([["fields", cborArray(fieldEntries)]]));
  }
}

function encodeSingleSchema(s: RustSchema): Uint8Array {
  return cborMap([
    ["type_id", cborTupleStruct1(cborUint(s.typeId))],
    ["name", cborText(s.name)],
    ["kind", s.kindBytes],
  ]);
}
