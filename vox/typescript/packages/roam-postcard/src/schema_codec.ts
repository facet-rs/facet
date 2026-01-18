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
// Decode Context - tracks path and buffer for error reporting
// ============================================================================

/**
 * Context for decode operations - tracks the path through the schema
 * and provides rich error messages when decoding fails.
 */
class DecodeContext {
  private path: string[] = [];

  constructor(
    public readonly buf: Uint8Array,
    public readonly rootSchema: Schema,
  ) {}

  /** Push a path segment (entering a field, element, or variant) */
  push(segment: string): void {
    this.path.push(segment);
  }

  /** Pop a path segment (leaving a field, element, or variant) */
  pop(): void {
    this.path.pop();
  }

  /** Get the current path as a string */
  currentPath(): string {
    return this.path.length === 0 ? "<root>" : this.path.join(".");
  }

  /** Create a rich error with context */
  error(message: string, offset: number, schema: Schema): Error {
    const hexDump = this.hexDumpAround(offset, 32);
    const schemaStr = this.schemaToString(schema);

    const details = [
      `Error: ${message}`,
      `Path: ${this.currentPath()}`,
      `Offset: ${offset} (0x${offset.toString(16)})`,
      `Buffer length: ${this.buf.length}`,
      `Schema: ${schemaStr}`,
      `Bytes around offset:`,
      hexDump,
    ].join("\n  ");

    return new Error(`Decode error:\n  ${details}`);
  }

  /** Hex dump of buffer around an offset */
  private hexDumpAround(offset: number, windowSize: number): string {
    const start = Math.max(0, offset - 8);
    const end = Math.min(this.buf.length, offset + windowSize - 8);

    const lines: string[] = [];
    for (let i = start; i < end; i += 16) {
      const lineEnd = Math.min(i + 16, end);
      const bytes: string[] = [];
      const chars: string[] = [];

      for (let j = i; j < lineEnd; j++) {
        const byte = this.buf[j];
        // Highlight the error offset
        if (j === offset) {
          bytes.push(`[${byte.toString(16).padStart(2, "0")}]`);
        } else {
          bytes.push(byte.toString(16).padStart(2, "0"));
        }
        chars.push(byte >= 32 && byte < 127 ? String.fromCharCode(byte) : ".");
      }

      const addr = i.toString(16).padStart(4, "0");
      lines.push(`    ${addr}: ${bytes.join(" ").padEnd(52)} ${chars.join("")}`);
    }

    return lines.join("\n");
  }

  /** Convert schema to a readable string (abbreviated) */
  private schemaToString(schema: Schema): string {
    switch (schema.kind) {
      case "enum": {
        const variants = schema.variants.map((v) => v.name).join(" | ");
        return `enum { ${variants} }`;
      }
      case "struct": {
        const fields = Object.keys(schema.fields).join(", ");
        return `struct { ${fields} }`;
      }
      case "vec":
        return `vec<${this.schemaToString(schema.element)}>`;
      case "option":
        return `option<${this.schemaToString(schema.inner)}>`;
      case "map":
        return `map<${this.schemaToString(schema.key)}, ${this.schemaToString(schema.value)}>`;
      case "tuple":
        return `tuple(${schema.elements.length} elements)`;
      default:
        return schema.kind;
    }
  }
}

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
  registry?: SchemaRegistry,
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
      return encodeEnumWithSchema(
        value as { tag: string; [key: string]: unknown },
        resolved,
        registry,
      );

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
  registry?: SchemaRegistry,
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
  registry?: SchemaRegistry,
): Uint8Array {
  if (value === null || value === undefined) {
    return Uint8Array.of(0);
  }
  return concat(Uint8Array.of(1), encodeWithSchema(value, schema.inner, registry));
}

function encodeMapWithSchema(
  map: Map<unknown, unknown>,
  schema: MapSchema,
  registry?: SchemaRegistry,
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
  registry?: SchemaRegistry,
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
  registry?: SchemaRegistry,
): Uint8Array {
  if (values.length !== schema.elements.length) {
    throw new Error(
      `Tuple length mismatch: got ${values.length}, expected ${schema.elements.length}`,
    );
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
  registry?: SchemaRegistry,
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
  registry?: SchemaRegistry,
): DecodeResult<unknown> {
  const ctx = new DecodeContext(buf, schema);
  return decodeWithSchemaImpl(buf, offset, schema, registry, ctx);
}

function decodeWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: Schema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<unknown> {
  // Resolve refs
  const resolved = registry ? resolveSchema(schema, registry) : schema;

  try {
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
        return decodeVecWithSchemaImpl(buf, offset, resolved, registry, ctx);
      case "option":
        return decodeOptionWithSchemaImpl(buf, offset, resolved, registry, ctx);
      case "map":
        return decodeMapWithSchemaImpl(buf, offset, resolved, registry, ctx);

      // Composites
      case "struct":
        return decodeStructWithSchemaImpl(buf, offset, resolved, registry, ctx);
      case "tuple":
        return decodeTupleWithSchemaImpl(buf, offset, resolved, registry, ctx);
      case "enum":
        return decodeEnumWithSchemaImpl(buf, offset, resolved, registry, ctx);

      // Streaming types decode as channel IDs (u64)
      case "tx":
      case "rx": {
        const result = decodeU64(buf, offset);
        return { value: { channelId: result.value }, next: result.next };
      }

      // Ref should have been resolved above
      case "ref":
        throw new Error(
          `Unresolved ref: ${(schema as { name: string }).name} - provide a registry`,
        );

      default:
        throw new Error(`Unknown schema kind: ${(resolved as { kind: string }).kind}`);
    }
  } catch (e) {
    // If it's already a decode error with context, re-throw
    if (e instanceof Error && e.message.startsWith("Decode error:")) {
      throw e;
    }
    // Otherwise wrap it with context
    throw ctx.error(e instanceof Error ? e.message : String(e), offset, resolved);
  }
}

function decodeVecWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: VecSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<unknown[]> {
  const len = decodeVarintNumber(buf, offset);
  let pos = len.next;
  const items: unknown[] = [];
  for (let i = 0; i < len.value; i++) {
    ctx.push(`[${i}]`);
    const item = decodeWithSchemaImpl(buf, pos, schema.element, registry, ctx);
    items.push(item.value);
    pos = item.next;
    ctx.pop();
  }
  return { value: items, next: pos };
}

function decodeOptionWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: OptionSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<unknown | null> {
  if (offset >= buf.length) {
    throw ctx.error("unexpected end of buffer reading option discriminant", offset, schema);
  }
  const variant = buf[offset];
  if (variant === 0) {
    return { value: null, next: offset + 1 };
  } else if (variant === 1) {
    ctx.push("Some");
    const inner = decodeWithSchemaImpl(buf, offset + 1, schema.inner, registry, ctx);
    ctx.pop();
    return { value: inner.value, next: inner.next };
  } else {
    throw ctx.error(`invalid option discriminant: ${variant} (expected 0 or 1)`, offset, schema);
  }
}

function decodeMapWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: MapSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<Map<unknown, unknown>> {
  const len = decodeVarintNumber(buf, offset);
  let pos = len.next;
  const map = new Map<unknown, unknown>();
  for (let i = 0; i < len.value; i++) {
    ctx.push(`{key ${i}}`);
    const k = decodeWithSchemaImpl(buf, pos, schema.key, registry, ctx);
    ctx.pop();
    ctx.push(`{value ${i}}`);
    const v = decodeWithSchemaImpl(buf, k.next, schema.value, registry, ctx);
    ctx.pop();
    map.set(k.value, v.value);
    pos = v.next;
  }
  return { value: map, next: pos };
}

function decodeStructWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: StructSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<Record<string, unknown>> {
  const obj: Record<string, unknown> = {};
  let pos = offset;
  for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
    ctx.push(fieldName);
    const field = decodeWithSchemaImpl(buf, pos, fieldSchema, registry, ctx);
    obj[fieldName] = field.value;
    pos = field.next;
    ctx.pop();
  }
  return { value: obj, next: pos };
}

function decodeTupleWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: TupleSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<unknown[]> {
  const values: unknown[] = [];
  let pos = offset;
  for (let i = 0; i < schema.elements.length; i++) {
    ctx.push(`${i}`);
    const element = decodeWithSchemaImpl(buf, pos, schema.elements[i], registry, ctx);
    values.push(element.value);
    pos = element.next;
    ctx.pop();
  }
  return { value: values, next: pos };
}

function decodeEnumWithSchemaImpl(
  buf: Uint8Array,
  offset: number,
  schema: EnumSchema,
  registry: SchemaRegistry | undefined,
  ctx: DecodeContext,
): DecodeResult<{ tag: string; [key: string]: unknown }> {
  const disc = decodeVarintNumber(buf, offset);
  const variant = findVariantByDiscriminant(schema, disc.value);
  if (!variant) {
    const validVariants = schema.variants.map((v, i) => `${i}=${v.name}`).join(", ");
    throw ctx.error(
      `unknown enum discriminant: ${disc.value} (valid: ${validVariants})`,
      offset,
      schema,
    );
  }

  ctx.push(variant.name);
  let pos = disc.next;
  const result: { tag: string; [key: string]: unknown } = { tag: variant.name };

  // Decode variant fields
  const fieldSchemas = getVariantFieldSchemas(variant);
  const fieldNames = getVariantFieldNames(variant);

  if (fieldSchemas.length === 0) {
    // Unit variant - no fields
  } else if (isNewtypeVariant(variant)) {
    // Newtype variant - store in `value` field
    ctx.push("value");
    const field = decodeWithSchemaImpl(buf, pos, fieldSchemas[0], registry, ctx);
    result.value = field.value;
    pos = field.next;
    ctx.pop();
  } else if (fieldNames === null) {
    // Tuple variant - store indexed (0, 1, 2, ...)
    for (let i = 0; i < fieldSchemas.length; i++) {
      ctx.push(`${i}`);
      const field = decodeWithSchemaImpl(buf, pos, fieldSchemas[i], registry, ctx);
      result[i.toString()] = field.value;
      pos = field.next;
      ctx.pop();
    }
  } else {
    // Struct variant - store by field name
    for (let i = 0; i < fieldNames.length; i++) {
      ctx.push(fieldNames[i]);
      const field = decodeWithSchemaImpl(buf, pos, fieldSchemas[i], registry, ctx);
      result[fieldNames[i]] = field.value;
      pos = field.next;
      ctx.pop();
    }
  }

  ctx.pop();
  return { value: result, next: pos };
}
