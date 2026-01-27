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
  | { kind: PrimitiveKind }
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
  if (variant.fields === null || variant.fields === undefined) {
    // Unit variant
    return [];
  }
  if ("kind" in variant.fields) {
    // Newtype variant - single schema
    return [variant.fields as Schema];
  }
  if (Array.isArray(variant.fields)) {
    // Tuple variant
    return variant.fields;
  }
  // Struct variant - return schemas in key order
  return Object.values(variant.fields);
}

/**
 * Get the field names for a struct variant (for decoding into object).
 *
 * @param variant - The variant
 * @returns Array of field names, or null if not a struct variant
 */
export function getVariantFieldNames(variant: EnumVariant): string[] | null {
  if (variant.fields === null || variant.fields === undefined) {
    // Unit variant
    return null;
  }
  if ("kind" in variant.fields) {
    // Newtype variant - no field names
    return null;
  }
  if (Array.isArray(variant.fields)) {
    // Tuple variant - no field names
    return null;
  }
  // Struct variant - return keys in order
  return Object.keys(variant.fields);
}

/**
 * Check if variant fields represent a newtype (single Schema, not array/record).
 *
 * @param variant - The variant
 * @returns True if this is a newtype variant
 */
export function isNewtypeVariant(variant: EnumVariant): boolean {
  return variant.fields !== null && variant.fields !== undefined && "kind" in variant.fields;
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
 * Schema for a method's arguments and return type.
 *
 * For methods returning `Result<T, E>`:
 * - `returns` is the schema for `T` (success type)
 * - `error` is the schema for `E` (user error type)
 *
 * For infallible methods returning `T`:
 * - `returns` is the schema for `T`
 * - `error` is null
 *
 * Note: The outer `Result<T, RoamError<E>>` wrapper is handled by `decodeRpcResult`.
 * After that call succeeds, you decode with `returns`. If it throws a USER error,
 * you decode the error payload with `error`.
 */
export interface MethodSchema {
  args: Schema[];
  returns: Schema;
  error: Schema | null;
}
