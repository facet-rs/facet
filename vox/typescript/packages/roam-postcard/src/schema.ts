// Schema types for runtime type description and encoding/decoding.
//
// This module provides schema types that describe the structure of values
// for schema-driven serialization. It supports:
// - Primitive types (bool, integers, floats, string, bytes)
// - Container types (vec, option, map)
// - Composite types (struct, enum, tuple)
// - Type references (ref) for deduplication and circular types

// ============================================================================
// Primitive Schema Kinds
// ============================================================================

/** Primitive types that map directly to postcard encoding. */
export type PrimitiveKind =
  | "bool"
  | "u8"
  | "u16"
  | "u32"
  | "u64"
  | "i8"
  | "i16"
  | "i32"
  | "i64"
  | "f32"
  | "f64"
  | "string"
  | "bytes";

/** Schema for bytes (`Vec<u8>`/`&[u8]` style payloads). */
export interface BytesSchema {
  kind: "bytes";
  /**
   * When true, bytes are encoded/decoded as "remaining input" without an outer
   * length prefix. This must only be used for structurally trailing fields.
   */
  trailing?: boolean;
  /**
   * When true, bytes are encoded/decoded with a 4-byte little-endian length
   * prefix (u32le). Used for opaque Payload fields (args, ret, item) that must
   * be framed so that subsequent fields (e.g. schemas) can be read.
   */
  opaque?: boolean;
}

// ============================================================================
// Container Schemas
// ============================================================================

/** Schema for Vec<T>. */
export interface VecSchema {
  kind: "vec";
  element: Schema;
}

/** Schema for Option<T>. */
export interface OptionSchema {
  kind: "option";
  inner: Schema;
}

/** Schema for HashMap<K, V> or BTreeMap<K, V>. */
export interface MapSchema {
  kind: "map";
  key: Schema;
  value: Schema;
}

// ============================================================================
// Composite Schemas
// ============================================================================

/** Schema for a struct with named fields. */
export interface StructSchema {
  kind: "struct";
  /** Fields in declaration order. Order is significant for encoding! */
  fields: Record<string, Schema>;
}

/**
 * Schema for fixed-size tuples like (String, MetadataValue).
 *
 * Postcard encodes tuples by concatenating elements in order (no length prefix).
 */
export interface TupleSchema {
  kind: "tuple";
  /** Element schemas in order. */
  elements: Schema[];
}

/**
 * A variant in an enum.
 */
export interface EnumVariant {
  /** Variant name (e.g., "Hello", "Goodbye"). */
  name: string;

  /**
   * Wire discriminant value (matches Rust #[repr(u8)] = N).
   * If omitted, defaults to the variant's index in the variants array.
   */
  discriminant?: number;

  /**
   * Variant fields. Can be:
   * - null/undefined: unit variant (no fields)
   * - Schema: newtype variant (single unnamed field)
   * - Schema[]: tuple variant (multiple unnamed fields)
   * - Record<string, Schema>: struct variant (named fields, encoded in key order)
   */
  fields?: null | Schema | Schema[] | Record<string, Schema>;
}

/**
 * Enum schema with variants.
 *
 * Supports both simple enums (discriminant = index) and
 * explicit discriminant enums (#[repr(u8)]).
 * Discriminant is encoded as a varint, followed by variant fields.
 */
export interface EnumSchema {
  kind: "enum";
  /** Variants in declaration order. */
  variants: EnumVariant[];
}

// ============================================================================
// Reference Schema
// ============================================================================

/**
 * Reference to a named type defined elsewhere.
 *
 * Used for:
 * - Deduplication (don't inline the same struct schema multiple times)
 * - Circular types (a type that references itself)
 * - Complex nested types (refer by name instead of inlining)
 *
 * The name is the fully qualified path: "module::TypeName" or just "TypeName".
 * In facet, this is `module_path::type_identifier`.
 */
export interface RefSchema {
  kind: "ref";
  /** Type name to look up in the schema registry. */
  name: string;
}

// ============================================================================
// Streaming Schemas (for Tx/Rx)
// ============================================================================

/** Schema for Tx<T> - data flowing from caller to callee. */
export interface TxSchema {
  kind: "tx";
  element: Schema;
}

/** Schema for Rx<T> - data flowing from callee to caller. */
export interface RxSchema {
  kind: "rx";
  element: Schema;
}

// ============================================================================
// Union Type
// ============================================================================

/** Union of all schema types. */
export type Schema =
  | { kind: Exclude<PrimitiveKind, "bytes"> }
  | BytesSchema
  | VecSchema
  | OptionSchema
  | MapSchema
  | StructSchema
  | TupleSchema
  | EnumSchema
  | RefSchema
  | TxSchema
  | RxSchema;

// ============================================================================
// Schema Registry
// ============================================================================

/**
 * Registry of named type schemas.
 *
 * Maps type names to their schemas. Used to resolve RefSchema references.
 */
export type SchemaRegistry = Map<string, Schema>;

/**
 * Resolve a schema, following refs to get the actual schema.
 *
 * @param schema - The schema (may be a ref)
 * @param registry - Registry of named schemas
 * @returns The resolved schema (never a ref)
 * @throws Error if ref points to unknown type
 */
export function resolveSchema(schema: Schema, registry: SchemaRegistry): Schema {
  if (schema.kind === "ref") {
    const resolved = registry.get(schema.name);
    if (!resolved) {
      throw new Error(`Unknown type ref: ${schema.name}`);
    }
    // Don't recursively resolve - the resolved schema may itself contain refs
    // that should only be resolved when actually encoding/decoding those fields
    return resolved;
  }
  return schema;
}

// ============================================================================
// Enum Helper Functions
// ============================================================================

/**
 * Find a variant by discriminant value (for decoding).
 *
 * @param schema - The enum schema
 * @param discriminant - The discriminant value from the wire
 * @returns The variant, or undefined if not found
 */
export function findVariantByDiscriminant(
  schema: EnumSchema,
  discriminant: number,
): EnumVariant | undefined {
  return schema.variants.find((v, index) => (v.discriminant ?? index) === discriminant);
}

/**
 * Find a variant by name (for encoding).
 *
 * @param schema - The enum schema
 * @param name - The variant name (from the `tag` field)
 * @returns The variant, or undefined if not found
 */
export function findVariantByName(schema: EnumSchema, name: string): EnumVariant | undefined {
  return schema.variants.find((v) => v.name === name);
}

/**
 * Get the discriminant for a variant.
 *
 * @param schema - The enum schema
 * @param variant - The variant
 * @returns The discriminant value (explicit or index-based)
 */
export function getVariantDiscriminant(schema: EnumSchema, variant: EnumVariant): number {
  if (variant.discriminant !== undefined) {
    return variant.discriminant;
  }
  const index = schema.variants.indexOf(variant);
  if (index === -1) {
    throw new Error(`Variant "${variant.name}" not found in schema`);
  }
  return index;
}

/**
 * Get the field schemas for a variant as an ordered array.
 *
 * @param variant - The variant
 * @returns Array of field schemas in encoding order
 */
export function getVariantFieldSchemas(variant: EnumVariant): Schema[] {
  return classifyVariantFields(variant).schemas;
}

/**
 * Get the field names for a struct variant (for decoding into object).
 *
 * @param variant - The variant
 * @returns Array of field names, or null if not a struct variant
 */
export function getVariantFieldNames(variant: EnumVariant): string[] | null {
  return classifyVariantFields(variant).names;
}

/**
 * Check if variant fields represent a newtype (single Schema, not array/record).
 *
 * @param variant - The variant
 * @returns True if this is a newtype variant
 */
export function isNewtypeVariant(variant: EnumVariant): boolean {
  return classifyVariantFields(variant).kind === "newtype";
}

type VariantFieldInfo =
  | { kind: "unit"; schemas: []; names: null }
  | { kind: "newtype"; schemas: [Schema]; names: null }
  | { kind: "tuple"; schemas: Schema[]; names: null }
  | { kind: "struct"; schemas: Schema[]; names: string[] };

function classifyVariantFields(variant: EnumVariant): VariantFieldInfo {
  const fields = variant.fields;
  if (fields === null || fields === undefined) {
    return { kind: "unit", schemas: [], names: null };
  }
  if (Array.isArray(fields)) {
    return { kind: "tuple", schemas: fields, names: null };
  }
  if (isSchemaNode(fields)) {
    return { kind: "newtype", schemas: [fields], names: null };
  }

  const names = Object.keys(fields);
  return {
    kind: "struct",
    schemas: names.map((name) => fields[name]),
    names,
  };
}

function isSchemaNode(fields: Schema | Record<string, Schema>): fields is Schema {
  return typeof (fields as { kind?: unknown }).kind === "string";
}

/**
 * Check if a schema is a reference.
 *
 * @param schema - The schema to check
 * @returns True if the schema is a RefSchema
 */
export function isRefSchema(schema: Schema): schema is RefSchema {
  return schema.kind === "ref";
}

// ============================================================================
// Method Schema (for service methods)
// ============================================================================

/**
 * Schema for a method's request/response wire format.
 *
 * - `args` is the schema list for request arguments.
 * - `wire` is the full response schema: `Result<T, RoamError<E>>`.
 *
 * `wire` is always an enum with two variants:
 * - `Ok` (index 0): method success payload `T`
 * - `Err` (index 1): `RoamError<E>`
 */
export interface MethodSchema {
  args: Schema[];
  wire: EnumSchema;
}

// ============================================================================
// New Wire Schema Types (matches Rust's facet-cbor internally-tagged format)
// ============================================================================
// These types match the CBOR wire format directly — decodeCbor() output is
// already this shape thanks to #[facet(tag = "tag", rename_all = "snake_case")].

/** Content hash uniquely identifying a type's postcard-level structure. */
export type SchemaHash = bigint;

/**
 * A reference to a type in a schema. Matches Rust's TypeRef enum.
 * Discriminated by `tag` field (internally-tagged CBOR encoding).
 */
export type WireTypeRef =
  | { tag: "concrete"; type_id: SchemaHash; args: WireTypeRef[] }
  | { tag: "var"; name: string };

/** A complete schema for a single type. */
export interface WireSchema {
  id: SchemaHash;
  type_params: string[];
  kind: WireSchemaKind;
}

/**
 * The structural kind of a type. Matches Rust's SchemaKind enum.
 * Discriminated by `tag` field.
 */
export type WireSchemaKind =
  | { tag: "struct"; name: string; fields: WireFieldSchema[] }
  | { tag: "enum"; name: string; variants: WireVariantSchema[] }
  | { tag: "tuple"; elements: WireTypeRef[] }
  | { tag: "list"; element: WireTypeRef }
  | { tag: "map"; key: WireTypeRef; value: WireTypeRef }
  | { tag: "array"; element: WireTypeRef; length: number }
  | { tag: "option"; element: WireTypeRef }
  | { tag: "channel"; direction: WireChannelDirection; element: WireTypeRef }
  | { tag: "primitive"; primitive_type: WirePrimitiveType };

/** Primitive types supported by the wire format. Just a string (unit variant). */
export type WirePrimitiveType =
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
  | "unit"
  | "bytes"
  | "payload";

/** Channel direction. Just a string (unit variant). */
export type WireChannelDirection = "tx" | "rx";

/** Describes a single field in a struct or struct variant. */
export interface WireFieldSchema {
  name: string;
  type_ref: WireTypeRef;
  required: boolean;
}

/** Describes a single variant in an enum. */
export interface WireVariantSchema {
  name: string;
  index: number;
  payload: WireVariantPayload;
}

/** The payload of an enum variant. Discriminated by `tag`. */
export type WireVariantPayload =
  | { tag: "unit" }
  | { tag: "newtype"; type_ref: WireTypeRef }
  | { tag: "tuple"; types: WireTypeRef[] }
  | { tag: "struct"; fields: WireFieldSchema[] };

/** Registry mapping SchemaHash → WireSchema. */
export type WireSchemaRegistry = Map<SchemaHash, WireSchema>;

// --- Wire exchange types ---

/** CBOR-encoded payload inside a schema wire message. */
export interface WireSchemaPayload {
  schemas: WireSchema[];
  method_bindings: WireMethodSchemaBinding[];
}

/** Binding direction for method schema bindings. Just a string (unit variant). */
export type WireBindingDirection = "args" | "response";

/** Associates a method ID with its root type ref for args or response. */
export interface WireMethodSchemaBinding {
  method_id: bigint;
  root_type_ref: WireTypeRef;
  direction: WireBindingDirection;
}

// --- Helper ---

/**
 * Look up the schema for a WireTypeRef in the registry and return
 * the schema's kind with all type variables substituted.
 */
export function resolveWireTypeRef(
  ref_: WireTypeRef,
  registry: WireSchemaRegistry,
): WireSchemaKind | undefined {
  if (ref_.tag === "var") return undefined;
  const schema = registry.get(ref_.type_id);
  if (!schema) return undefined;
  if (ref_.args.length === 0) return schema.kind;

  // Build substitution map: type param name → concrete TypeRef
  const subst = new Map<string, WireTypeRef>();
  for (let i = 0; i < schema.type_params.length && i < ref_.args.length; i++) {
    subst.set(schema.type_params[i], ref_.args[i]);
  }
  return substituteTypeRefs(schema.kind, subst);
}

function substituteTypeRef(
  ref_: WireTypeRef,
  subst: Map<string, WireTypeRef>,
): WireTypeRef {
  if (ref_.tag === "var") {
    return subst.get(ref_.name) ?? ref_;
  }
  return {
    tag: "concrete",
    type_id: ref_.type_id,
    args: ref_.args.map((a) => substituteTypeRef(a, subst)),
  };
}

function substituteTypeRefs(
  kind: WireSchemaKind,
  subst: Map<string, WireTypeRef>,
): WireSchemaKind {
  const sub = (r: WireTypeRef) => substituteTypeRef(r, subst);
  switch (kind.tag) {
    case "primitive":
      return kind;
    case "struct":
      return {
        ...kind,
        fields: kind.fields.map((f) => ({ ...f, type_ref: sub(f.type_ref) })),
      };
    case "enum":
      return {
        ...kind,
        variants: kind.variants.map((v) => ({
          ...v,
          payload: substitutePayload(v.payload, subst),
        })),
      };
    case "tuple":
      return { ...kind, elements: kind.elements.map(sub) };
    case "list":
      return { ...kind, element: sub(kind.element) };
    case "map":
      return { ...kind, key: sub(kind.key), value: sub(kind.value) };
    case "array":
      return { ...kind, element: sub(kind.element) };
    case "option":
      return { ...kind, element: sub(kind.element) };
    case "channel":
      return { ...kind, direction: kind.direction, element: sub(kind.element) };
  }
}

function substitutePayload(
  payload: WireVariantPayload,
  subst: Map<string, WireTypeRef>,
): WireVariantPayload {
  const sub = (r: WireTypeRef) => substituteTypeRef(r, subst);
  switch (payload.tag) {
    case "unit":
      return payload;
    case "newtype":
      return { ...payload, type_ref: sub(payload.type_ref) };
    case "tuple":
      return { ...payload, types: payload.types.map(sub) };
    case "struct":
      return {
        ...payload,
        fields: payload.fields.map((f) => ({ ...f, type_ref: sub(f.type_ref) })),
      };
  }
}
