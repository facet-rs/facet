// Schema-driven encoding/decoding for postcard format.
//
// This module provides generic encode/decode functions that use runtime
// schema information to serialize/deserialize values.

import type { DecodeResult } from "./index.ts";
import type {
  Schema,
  SchemaRegistry,
  EnumSchema,
  StructSchema,
  TupleSchema,
  VecSchema,
  OptionSchema,
  MapSchema,
  EnumVariant,
} from "./schema.ts";
import {
  resolveSchema,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
} from "./schema.ts";
import {
  encodeBool,
  decodeBool,
  encodeU8,
  decodeU8,
  encodeI8,
  decodeI8,
  encodeU16,
  decodeU16,
  encodeI16,
  decodeI16,
  encodeU32,
  decodeU32,
  encodeI32,
  decodeI32,
  encodeU64,
  decodeU64,
  encodeI64,
  decodeI64,
  encodeF32,
  decodeF32,
  encodeF64,
  decodeF64,
  encodeString,
  decodeString,
  encodeBytes,
  decodeBytes,
  encodeVarint,
  decodeVarintNumber,
  concat,
} from "./index.ts";

// ============================================================================
// Schema-driven Encoding
// ============================================================================

/**
 * Encode a value according to its schema.
 *
 * @param value - The value to encode
 * @param schema - Schema describing the value's type
 * @param registry - Optional registry for resolving type references
 * @returns Encoded bytes
 */
export function encodeWithSchema(
  value: unknown,
  schema: Schema,
  registry?: SchemaRegistry
): Uint8Array {
  // Resolve refs
  const resolved = registry ? resolveSchema(schema, registry) : schema;

  switch (resolved.kind) {
    // Primitives
    case "bool":
      return encodeBool(value as boolean);
    case "u8":
      return encodeU8(value as number);
    case "i8":
      return encodeI8(value as number);
    case "u16":
      return encodeU16(value as number);
    case "i16":
      return encodeI16(value as number);
    case "u32":
      return encodeU32(value as number);
    case "i32":
      return encodeI32(value as number);
    case "u64":
      return encodeU64(value as bigint);
    case "i64":
      return encodeI64(value as bigint);
    case "f32":
      return encodeF32(value as number);
    case "f64":
      return encodeF64(value as number);
    case "string":
      return encodeString(value as string);
    case "bytes":
      return encodeBytes(value as Uint8Array);

    // Containers
    case "vec":
      return encodeVecWithSchema(value as unknown[], resolved, registry);
    case "option":
      return encodeOptionWithSchema(value, resolved, registry);
    case "map":
      return encodeMapWithSchema(value as Map<unknown, unknown>, resolved, registry);

    // Composites
    case "struct":
      return encodeStructWithSchema(value as Record<string, unknown>, resolved, registry);
    case "tuple":
      return encodeTupleWithSchema(value as unknown[], resolved, registry);
    case "enum":
      return encodeEnumWithSchema(value as { tag: string; [key: string]: unknown }, resolved, registry);

    // Streaming types encode as channel IDs (u64)
    case "tx":
    case "rx":
      // The value should have a channelId property
      const channelId = (value as { channelId: bigint }).channelId;
      return encodeU64(channelId);

    // Ref should have been resolved above
    case "ref":
      throw new Error(`Unresolved ref: ${(schema as { name: string }).name} - provide a registry`);

    default:
      throw new Error(`Unknown schema kind: ${(resolved as { kind: string }).kind}`);
  }
}

function encodeVecWithSchema(
  values: unknown[],
  schema: VecSchema,
  registry?: SchemaRegistry
): Uint8Array {
  const parts: Uint8Array[] = [encodeVarint(values.length)];
  for (const item of values) {
    parts.push(encodeWithSchema(item, schema.element, registry));
  }
  return concat(...parts);
}

function encodeOptionWithSchema(
  value: unknown,
  schema: OptionSchema,
  registry?: SchemaRegistry
): Uint8Array {
  if (value === null || value === undefined) {
    return Uint8Array.of(0);
  }
  return concat(Uint8Array.of(1), encodeWithSchema(value, schema.inner, registry));
}

function encodeMapWithSchema(
  map: Map<unknown, unknown>,
  schema: MapSchema,
  registry?: SchemaRegistry
): Uint8Array {
  const parts: Uint8Array[] = [encodeVarint(map.size)];
  for (const [k, v] of map) {
    parts.push(encodeWithSchema(k, schema.key, registry));
    parts.push(encodeWithSchema(v, schema.value, registry));
  }
  return concat(...parts);
}

function encodeStructWithSchema(
  obj: Record<string, unknown>,
  schema: StructSchema,
  registry?: SchemaRegistry
): Uint8Array {
  const parts: Uint8Array[] = [];
  // Encode fields in schema order (Object.keys preserves insertion order)
  for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
    parts.push(encodeWithSchema(obj[fieldName], fieldSchema, registry));
  }
  return concat(...parts);
}

function encodeTupleWithSchema(
  values: unknown[],
  schema: TupleSchema,
  registry?: SchemaRegistry
): Uint8Array {
  if (values.length !== schema.elements.length) {
    throw new Error(`Tuple length mismatch: got ${values.length}, expected ${schema.elements.length}`);
  }
  const parts: Uint8Array[] = [];
  for (let i = 0; i < values.length; i++) {
    parts.push(encodeWithSchema(values[i], schema.elements[i], registry));
  }
  return concat(...parts);
}

function encodeEnumWithSchema(
  value: { tag: string; [key: string]: unknown },
  schema: EnumSchema,
  registry?: SchemaRegistry
): Uint8Array {
  const variant = findVariantByName(schema, value.tag);
  if (!variant) {
    throw new Error(`Unknown variant: ${value.tag}`);
  }

  const discriminant = getVariantDiscriminant(schema, variant);
  const parts: Uint8Array[] = [encodeVarint(discriminant)];

  // Encode variant fields
  const fieldSchemas = getVariantFieldSchemas(variant);
  const fieldNames = getVariantFieldNames(variant);

  if (fieldSchemas.length === 0) {
    // Unit variant - no fields
  } else if (isNewtypeVariant(variant)) {
    // Newtype variant - value is in `value` field
    parts.push(encodeWithSchema(value.value, fieldSchemas[0], registry));
  } else if (fieldNames === null) {
    // Tuple variant - values are indexed (0, 1, 2, ...)
    for (let i = 0; i < fieldSchemas.length; i++) {
      parts.push(encodeWithSchema(value[i.toString()], fieldSchemas[i], registry));
    }
  } else {
    // Struct variant - values are by field name
    for (let i = 0; i < fieldNames.length; i++) {
      parts.push(encodeWithSchema(value[fieldNames[i]], fieldSchemas[i], registry));
    }
  }

  return concat(...parts);
}

// ============================================================================
// Schema-driven Decoding
// ============================================================================

/**
 * Decode a value according to its schema.
 *
 * @param buf - Buffer to decode from
 * @param offset - Starting offset in buffer
 * @param schema - Schema describing the expected type
 * @param registry - Optional registry for resolving type references
 * @returns Decoded value and next offset
 */
export function decodeWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: Schema,
  registry?: SchemaRegistry
): DecodeResult<unknown> {
  // Resolve refs
  const resolved = registry ? resolveSchema(schema, registry) : schema;

  switch (resolved.kind) {
    // Primitives
    case "bool":
      return decodeBool(buf, offset);
    case "u8":
      return decodeU8(buf, offset);
    case "i8":
      return decodeI8(buf, offset);
    case "u16":
      return decodeU16(buf, offset);
    case "i16":
      return decodeI16(buf, offset);
    case "u32":
      return decodeU32(buf, offset);
    case "i32":
      return decodeI32(buf, offset);
    case "u64":
      return decodeU64(buf, offset);
    case "i64":
      return decodeI64(buf, offset);
    case "f32":
      return decodeF32(buf, offset);
    case "f64":
      return decodeF64(buf, offset);
    case "string":
      return decodeString(buf, offset);
    case "bytes":
      return decodeBytes(buf, offset);

    // Containers
    case "vec":
      return decodeVecWithSchema(buf, offset, resolved, registry);
    case "option":
      return decodeOptionWithSchema(buf, offset, resolved, registry);
    case "map":
      return decodeMapWithSchema(buf, offset, resolved, registry);

    // Composites
    case "struct":
      return decodeStructWithSchema(buf, offset, resolved, registry);
    case "tuple":
      return decodeTupleWithSchema(buf, offset, resolved, registry);
    case "enum":
      return decodeEnumWithSchema(buf, offset, resolved, registry);

    // Streaming types decode as channel IDs (u64)
    case "tx":
    case "rx": {
      const result = decodeU64(buf, offset);
      return { value: { channelId: result.value }, next: result.next };
    }

    // Ref should have been resolved above
    case "ref":
      throw new Error(`Unresolved ref: ${(schema as { name: string }).name} - provide a registry`);

    default:
      throw new Error(`Unknown schema kind: ${(resolved as { kind: string }).kind}`);
  }
}

function decodeVecWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: VecSchema,
  registry?: SchemaRegistry
): DecodeResult<unknown[]> {
  const len = decodeVarintNumber(buf, offset);
  let pos = len.next;
  const items: unknown[] = [];
  for (let i = 0; i < len.value; i++) {
    const item = decodeWithSchema(buf, pos, schema.element, registry);
    items.push(item.value);
    pos = item.next;
  }
  return { value: items, next: pos };
}

function decodeOptionWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: OptionSchema,
  registry?: SchemaRegistry
): DecodeResult<unknown | null> {
  if (offset >= buf.length) throw new Error("option: eof");
  const variant = buf[offset];
  if (variant === 0) {
    return { value: null, next: offset + 1 };
  } else if (variant === 1) {
    const inner = decodeWithSchema(buf, offset + 1, schema.inner, registry);
    return { value: inner.value, next: inner.next };
  } else {
    throw new Error(`option: invalid variant ${variant}`);
  }
}

function decodeMapWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: MapSchema,
  registry?: SchemaRegistry
): DecodeResult<Map<unknown, unknown>> {
  const len = decodeVarintNumber(buf, offset);
  let pos = len.next;
  const map = new Map<unknown, unknown>();
  for (let i = 0; i < len.value; i++) {
    const k = decodeWithSchema(buf, pos, schema.key, registry);
    const v = decodeWithSchema(buf, k.next, schema.value, registry);
    map.set(k.value, v.value);
    pos = v.next;
  }
  return { value: map, next: pos };
}

function decodeStructWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: StructSchema,
  registry?: SchemaRegistry
): DecodeResult<Record<string, unknown>> {
  const obj: Record<string, unknown> = {};
  let pos = offset;
  for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
    const field = decodeWithSchema(buf, pos, fieldSchema, registry);
    obj[fieldName] = field.value;
    pos = field.next;
  }
  return { value: obj, next: pos };
}

function decodeTupleWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: TupleSchema,
  registry?: SchemaRegistry
): DecodeResult<unknown[]> {
  const values: unknown[] = [];
  let pos = offset;
  for (const elementSchema of schema.elements) {
    const element = decodeWithSchema(buf, pos, elementSchema, registry);
    values.push(element.value);
    pos = element.next;
  }
  return { value: values, next: pos };
}

function decodeEnumWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: EnumSchema,
  registry?: SchemaRegistry
): DecodeResult<{ tag: string; [key: string]: unknown }> {
  const disc = decodeVarintNumber(buf, offset);
  const variant = findVariantByDiscriminant(schema, disc.value);
  if (!variant) {
    throw new Error(`Unknown discriminant: ${disc.value}`);
  }

  let pos = disc.next;
  const result: { tag: string; [key: string]: unknown } = { tag: variant.name };

  // Decode variant fields
  const fieldSchemas = getVariantFieldSchemas(variant);
  const fieldNames = getVariantFieldNames(variant);

  if (fieldSchemas.length === 0) {
    // Unit variant - no fields
  } else if (isNewtypeVariant(variant)) {
    // Newtype variant - store in `value` field
    const field = decodeWithSchema(buf, pos, fieldSchemas[0], registry);
    result.value = field.value;
    pos = field.next;
  } else if (fieldNames === null) {
    // Tuple variant - store indexed (0, 1, 2, ...)
    for (let i = 0; i < fieldSchemas.length; i++) {
      const field = decodeWithSchema(buf, pos, fieldSchemas[i], registry);
      result[i.toString()] = field.value;
      pos = field.next;
    }
  } else {
    // Struct variant - store by field name
    for (let i = 0; i < fieldNames.length; i++) {
      const field = decodeWithSchema(buf, pos, fieldSchemas[i], registry);
      result[fieldNames[i]] = field.value;
      pos = field.next;
    }
  }

  return { value: result, next: pos };
}
