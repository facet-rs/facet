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
  BytesSchema,
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
  decodeBool,
  decodeU8,
  decodeI8,
  decodeU16,
  decodeI16,
  decodeU32,
  decodeI32,
  decodeU64,
  decodeI64,
  decodeF32,
  decodeF64,
  decodeString,
  decodeBytes,
  decodeVarintNumber,
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
      case "bytes":
        return schema.trailing ? "bytes(trailing)" : "bytes";
      default:
        return schema.kind;
    }
  }
}

// ============================================================================
// Schema-driven Encoding
// ============================================================================

class BufWriter {
  private buf: Uint8Array;
  private pos = 0;

  constructor(initialCapacity = 128) {
    this.buf = new Uint8Array(initialCapacity);
  }

  private reserve(additional: number): void {
    const needed = this.pos + additional;
    if (needed <= this.buf.length) {
      return;
    }
    let capacity = this.buf.length;
    while (capacity < needed) {
      capacity *= 2;
    }
    const next = new Uint8Array(capacity);
    next.set(this.buf.subarray(0, this.pos), 0);
    this.buf = next;
  }

  writeByte(value: number): void {
    this.reserve(1);
    this.buf[this.pos++] = value & 0xff;
  }

  writeBytes(value: Uint8Array): void {
    this.reserve(value.length);
    this.buf.set(value, this.pos);
    this.pos += value.length;
  }

  writeVarint(value: number | bigint): void {
    let v: bigint;
    if (typeof value === "number") {
      if (!Number.isInteger(value) || value < 0) {
        throw new Error(`varint: expected non-negative integer, got ${value}`);
      }
      v = BigInt(value);
    } else {
      if (value < 0n) {
        throw new Error(`varint: expected non-negative integer, got ${value.toString()}`);
      }
      v = value;
    }

    while (v >= 0x80n) {
      this.writeByte(Number((v & 0x7fn) | 0x80n));
      v >>= 7n;
    }
    this.writeByte(Number(v));
  }

  writeF32(value: number): void {
    this.reserve(4);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.pos, 4).setFloat32(0, value, true);
    this.pos += 4;
  }

  writeF64(value: number): void {
    this.reserve(8);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.pos, 8).setFloat64(0, value, true);
    this.pos += 8;
  }

  finish(): Uint8Array {
    return this.buf.subarray(0, this.pos);
  }
}

function zigzagEncode(n: bigint): bigint {
  return (n << 1n) ^ (n >> 63n);
}

const TEXT_ENCODER = new TextEncoder();

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
  const writer = new BufWriter();
  encodeWithSchemaInto(value, schema, writer, registry);
  return writer.finish();
}

function encodeWithSchemaInto(
  value: unknown,
  schema: Schema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  // Resolve refs
  const resolved = registry ? resolveSchema(schema, registry) : schema;

  switch (resolved.kind) {
    // Primitives
    case "bool":
      writer.writeByte(value ? 1 : 0);
      return;
    case "u8":
      writer.writeByte(value as number);
      return;
    case "i8":
      writer.writeByte(value as number);
      return;
    case "u16":
      writer.writeVarint(value as number);
      return;
    case "i16":
      writer.writeVarint(zigzagEncode(BigInt(value as number)));
      return;
    case "u32":
      writer.writeVarint(value as number);
      return;
    case "i32":
      writer.writeVarint(zigzagEncode(BigInt(value as number)));
      return;
    case "u64":
      writer.writeVarint(value as bigint);
      return;
    case "i64":
      writer.writeVarint(zigzagEncode(value as bigint));
      return;
    case "f32":
      writer.writeF32(value as number);
      return;
    case "f64":
      writer.writeF64(value as number);
      return;
    case "string":
      encodeStringWithSchema(value as string, writer);
      return;
    case "bytes":
      encodeBytesWithSchema(value, resolved, writer);
      return;

    // Containers
    case "vec":
      encodeVecWithSchema(value as unknown[], resolved, writer, registry);
      return;
    case "option":
      encodeOptionWithSchema(value, resolved, writer, registry);
      return;
    case "map":
      encodeMapWithSchema(value as Map<unknown, unknown>, resolved, writer, registry);
      return;

    // Composites
    case "struct":
      encodeStructWithSchema(value as Record<string, unknown>, resolved, writer, registry);
      return;
    case "tuple":
      encodeTupleWithSchema(value as unknown[], resolved, writer, registry);
      return;
    case "enum":
      encodeEnumWithSchema(
        value as { tag: string; [key: string]: unknown },
        resolved,
        writer,
        registry,
      );
      return;

    // Streaming types encode as unit (zero bytes) - channel IDs are carried
    // in the Request/Response `channels` field, not in the args payload.
    case "tx":
    case "rx":
      return;

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
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  writer.writeVarint(values.length);
  for (const item of values) {
    encodeWithSchemaInto(item, schema.element, writer, registry);
  }
}

function encodeOptionWithSchema(
  value: unknown,
  schema: OptionSchema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  if (value === null || value === undefined) {
    writer.writeByte(0);
    return;
  }
  writer.writeByte(1);
  encodeWithSchemaInto(value, schema.inner, writer, registry);
}

function encodeMapWithSchema(
  map: Map<unknown, unknown>,
  schema: MapSchema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  writer.writeVarint(map.size);
  for (const [k, v] of map) {
    encodeWithSchemaInto(k, schema.key, writer, registry);
    encodeWithSchemaInto(v, schema.value, writer, registry);
  }
}

function encodeStructWithSchema(
  obj: Record<string, unknown>,
  schema: StructSchema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  // Encode fields in schema order (Object.keys preserves insertion order)
  for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
    encodeWithSchemaInto(obj[fieldName], fieldSchema, writer, registry);
  }
}

function encodeBytesWithSchema(value: unknown, schema: BytesSchema, writer: BufWriter): void {
  const bytes = value as Uint8Array;
  if (schema.trailing) {
    writer.writeBytes(bytes);
    return;
  }
  if (schema.opaque) {
    const len = bytes.length;
    writer.writeByte(len & 0xff);
    writer.writeByte((len >> 8) & 0xff);
    writer.writeByte((len >> 16) & 0xff);
    writer.writeByte((len >> 24) & 0xff);
    writer.writeBytes(bytes);
    return;
  }
  writer.writeVarint(bytes.length);
  writer.writeBytes(bytes);
}

function encodeStringWithSchema(value: string, writer: BufWriter): void {
  const bytes = TEXT_ENCODER.encode(value);
  writer.writeVarint(bytes.length);
  writer.writeBytes(bytes);
}

function encodeTupleWithSchema(
  values: unknown[],
  schema: TupleSchema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  if (values.length !== schema.elements.length) {
    throw new Error(
      `Tuple length mismatch: got ${values.length}, expected ${schema.elements.length}`,
    );
  }
  for (let i = 0; i < values.length; i++) {
    encodeWithSchemaInto(values[i], schema.elements[i], writer, registry);
  }
}

function encodeEnumWithSchema(
  value: { tag: string; [key: string]: unknown },
  schema: EnumSchema,
  writer: BufWriter,
  registry?: SchemaRegistry,
): void {
  const variant = findVariantByName(schema, value.tag);
  if (!variant) {
    throw new Error(`Unknown variant: ${value.tag}`);
  }

  const discriminant = getVariantDiscriminant(schema, variant);
  writer.writeVarint(discriminant);

  // Encode variant fields
  const fieldSchemas = getVariantFieldSchemas(variant);
  const fieldNames = getVariantFieldNames(variant);

  if (fieldSchemas.length === 0) {
    // Unit variant - no fields
  } else if (isNewtypeVariant(variant)) {
    // Newtype variant - value is in `value` field
    encodeWithSchemaInto(value.value, fieldSchemas[0], writer, registry);
  } else if (fieldNames === null) {
    // Tuple variant - values are indexed (0, 1, 2, ...)
    for (let i = 0; i < fieldSchemas.length; i++) {
      encodeWithSchemaInto(value[i.toString()], fieldSchemas[i], writer, registry);
    }
  } else {
    // Struct variant - values are by field name
    for (let i = 0; i < fieldNames.length; i++) {
      encodeWithSchemaInto(value[fieldNames[i]], fieldSchemas[i], writer, registry);
    }
  }
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
        return decodeBytesWithSchema(buf, offset, resolved);

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

      // Streaming types decode as unit (zero bytes) - channel IDs are carried
      // in the Request/Response `channels` field, not in the args payload.
      case "tx":
      case "rx":
        return { value: {}, next: offset };

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

function decodeBytesWithSchema(
  buf: Uint8Array,
  offset: number,
  schema: BytesSchema,
): DecodeResult<Uint8Array> {
  if (schema.trailing) {
    return { value: buf.subarray(offset), next: buf.length };
  }
  if (schema.opaque) {
    if (offset + 4 > buf.length) {
      throw new Error(`opaque u32le length prefix out of bounds at offset ${offset}`);
    }
    const len =
      buf[offset] |
      (buf[offset + 1] << 8) |
      (buf[offset + 2] << 16) |
      (buf[offset + 3] << 24);
    const start = offset + 4;
    const end = start + len;
    if (end > buf.length) {
      throw new Error(`opaque payload length ${len} exceeds buffer at offset ${offset}`);
    }
    return { value: buf.subarray(start, end), next: end };
  }
  return decodeBytes(buf, offset);
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
