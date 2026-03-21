// Plan-driven postcard codec using the new Wire schema types.
//
// Decoding is plan-driven: the TranslationPlan tells us how to read remote
// postcard bytes into local types in a single pass, handling field reordering,
// skipping, and default-filling.

import type { DecodeResult } from "./index.ts";
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
import type {
  WireSchemaKind,
  WireSchemaRegistry,
  WireTypeRef,
  WireFieldSchema,
  WireVariantSchema,
} from "./schema.ts";
import { resolveWireTypeRef } from "./schema.ts";
import type { TranslationPlan, FieldOp } from "./plan.ts";

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

const TEXT_ENCODER = new TextEncoder();
const TEXT_DECODER = new TextDecoder();

function zigzagEncode(n: bigint): bigint {
  return (n << 1n) ^ (n >> 63n);
}

function encodeChar(value: string, writer: BufWriter): void {
  const bytes = TEXT_ENCODER.encode(value);
  if (bytes.length === 0 || bytes.length > 4 || [...value].length !== 1) {
    throw new Error(`expected single character, got ${JSON.stringify(value)}`);
  }
  writer.writeBytes(bytes);
}

function decodeChar(
  buf: Uint8Array,
  offset: number,
): DecodeResult<string> {
  const b = buf[offset];
  if (b === undefined) {
    throw new Error("unexpected end of buffer decoding char");
  }
  let len = 4;
  if (b < 0x80) {
    len = 1;
  } else if ((b & 0xe0) === 0xc0) {
    len = 2;
  } else if ((b & 0xf0) === 0xe0) {
    len = 3;
  }
  const end = offset + len;
  if (end > buf.length) {
    throw new Error("unexpected end of buffer decoding char");
  }
  return {
    value: TEXT_DECODER.decode(buf.subarray(offset, end)),
    next: end,
  };
}

export function encodeWithTypeRef(
  value: unknown,
  ref_: WireTypeRef,
  registry: WireSchemaRegistry,
): Uint8Array {
  return encodeWithKind(value, resolveTypeRefKind(ref_, registry), registry);
}

export function encodeWithKind(
  value: unknown,
  kind: WireSchemaKind,
  registry: WireSchemaRegistry,
): Uint8Array {
  const writer = new BufWriter();
  encodeByKind(value, kind, writer, registry);
  return writer.finish();
}

export function decodeWithTypeRef(
  buf: Uint8Array,
  offset: number,
  ref_: WireTypeRef,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  return decodeByKind(buf, offset, resolveTypeRefKind(ref_, registry), registry);
}

export function decodeWithKind(
  buf: Uint8Array,
  offset: number,
  kind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  return decodeByKind(buf, offset, kind, registry);
}

function encodeByKind(
  value: unknown,
  kind: WireSchemaKind,
  writer: BufWriter,
  registry: WireSchemaRegistry,
): void {
  switch (kind.tag) {
    case "primitive":
      encodePrimitive(value, kind.primitive_type, writer);
      return;
    case "struct": {
      const obj = value as Record<string, unknown>;
      for (const field of kind.fields) {
        encodeByKind(obj[field.name], resolveTypeRefKind(field.type_ref, registry), writer, registry);
      }
      return;
    }
    case "enum": {
      const enumValue = value as { tag: string; value?: unknown; [key: string]: unknown };
      const variant = kind.variants.find((candidate) => candidate.name === enumValue.tag);
      if (!variant) {
        throw new Error(`unknown variant: ${enumValue.tag}`);
      }
      writer.writeVarint(variant.index);
      switch (variant.payload.tag) {
        case "unit":
          return;
        case "newtype":
          encodeByKind(
            enumValue.value,
            resolveTypeRefKind(variant.payload.type_ref, registry),
            writer,
            registry,
          );
          return;
        case "tuple": {
          const tupleValues = Array.isArray(enumValue.value)
            ? enumValue.value
            : variant.payload.types.map((_, index) => enumValue[index.toString()]);
          if (tupleValues.length !== variant.payload.types.length) {
            throw new Error(
              `tuple variant length mismatch: got ${tupleValues.length}, expected ${variant.payload.types.length}`,
            );
          }
          for (let i = 0; i < variant.payload.types.length; i++) {
            encodeByKind(
              tupleValues[i],
              resolveTypeRefKind(variant.payload.types[i], registry),
              writer,
              registry,
            );
          }
          return;
        }
        case "struct":
          for (const field of variant.payload.fields) {
            encodeByKind(
              enumValue[field.name],
              resolveTypeRefKind(field.type_ref, registry),
              writer,
              registry,
            );
          }
          return;
      }
    }
    case "tuple": {
      const tupleValues = value as unknown[];
      if (tupleValues.length !== kind.elements.length) {
        throw new Error(
          `tuple length mismatch: got ${tupleValues.length}, expected ${kind.elements.length}`,
        );
      }
      for (let i = 0; i < kind.elements.length; i++) {
        encodeByKind(tupleValues[i], resolveTypeRefKind(kind.elements[i], registry), writer, registry);
      }
      return;
    }
    case "list": {
      const values = value as unknown[];
      writer.writeVarint(values.length);
      for (const item of values) {
        encodeByKind(item, resolveTypeRefKind(kind.element, registry), writer, registry);
      }
      return;
    }
    case "option":
      if (value === null || value === undefined) {
        writer.writeByte(0);
        return;
      }
      writer.writeByte(1);
      encodeByKind(value, resolveTypeRefKind(kind.element, registry), writer, registry);
      return;
    case "map": {
      const entries =
        value instanceof Map ? [...value.entries()] : Array.isArray(value) ? value : [];
      writer.writeVarint(entries.length);
      for (const [key, entryValue] of entries) {
        encodeByKind(key, resolveTypeRefKind(kind.key, registry), writer, registry);
        encodeByKind(entryValue, resolveTypeRefKind(kind.value, registry), writer, registry);
      }
      return;
    }
    case "array": {
      const values = value as unknown[];
      if (values.length !== kind.length) {
        throw new Error(`array length mismatch: got ${values.length}, expected ${kind.length}`);
      }
      for (const item of values) {
        encodeByKind(item, resolveTypeRefKind(kind.element, registry), writer, registry);
      }
      return;
    }
    case "channel":
      writer.writeVarint(extractChannelId(value));
      return;
  }
}

function extractChannelId(value: unknown): bigint {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isInteger(value) || value < 0) {
      throw new Error(`channel: expected non-negative integer, got ${value}`);
    }
    return BigInt(value);
  }
  if (value && typeof value === "object" && "channelId" in value) {
    const channelId = (value as { channelId: unknown }).channelId;
    if (typeof channelId === "bigint") {
      return channelId;
    }
    if (typeof channelId === "number" && Number.isInteger(channelId) && channelId >= 0) {
      return BigInt(channelId);
    }
  }
  throw new Error("channel: expected bigint, number, or bound channel handle");
}

function encodePrimitive(value: unknown, primitiveType: string, writer: BufWriter): void {
  switch (primitiveType) {
    case "bool":
      writer.writeByte(value ? 1 : 0);
      return;
    case "u8":
    case "i8":
      writer.writeByte(value as number);
      return;
    case "u16":
    case "u32":
      writer.writeVarint(value as number);
      return;
    case "u64":
    case "u128":
      writer.writeVarint(value as bigint);
      return;
    case "i16":
    case "i32":
      writer.writeVarint(zigzagEncode(BigInt(value as number)));
      return;
    case "i64":
    case "i128":
      writer.writeVarint(zigzagEncode(value as bigint));
      return;
    case "f32":
      writer.writeF32(value as number);
      return;
    case "f64":
      writer.writeF64(value as number);
      return;
    case "char":
      encodeChar(value as string, writer);
      return;
    case "string": {
      const bytes = TEXT_ENCODER.encode(value as string);
      writer.writeVarint(bytes.length);
      writer.writeBytes(bytes);
      return;
    }
    case "unit":
      return;
    case "bytes": {
      const bytes = value as Uint8Array;
      writer.writeVarint(bytes.length);
      writer.writeBytes(bytes);
      return;
    }
    case "payload": {
      const bytes = value as Uint8Array;
      const len = bytes.length;
      writer.writeByte(len & 0xff);
      writer.writeByte((len >> 8) & 0xff);
      writer.writeByte((len >> 16) & 0xff);
      writer.writeByte((len >> 24) & 0xff);
      writer.writeBytes(bytes);
      return;
    }
    default:
      throw new Error(`encodePrimitive: unknown type "${primitiveType}"`);
  }
}

// ============================================================================
// skipValue — advance past a postcard value without decoding it
// ============================================================================

/**
 * Skip past a postcard-encoded value described by `kind`.
 * Returns the new offset after the value.
 */
export function skipValue(
  buf: Uint8Array,
  offset: number,
  kind: WireSchemaKind,
  registry: WireSchemaRegistry,
): number {
  switch (kind.tag) {
    case "primitive":
      return skipPrimitive(buf, offset, kind.primitive_type);
    case "struct":
      return skipStruct(buf, offset, kind.fields, registry);
    case "enum":
      return skipEnum(buf, offset, kind.variants, registry);
    case "tuple":
      return skipTuple(buf, offset, kind.elements, registry);
    case "list": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      let off = next;
      const elemKind = resolveTypeRefKind(kind.element, registry);
      for (let i = 0; i < len; i++) {
        off = skipValue(buf, off, elemKind, registry);
      }
      return off;
    }
    case "option": {
      const flag = buf[offset];
      if (flag === 0) return offset + 1;
      const elemKind = resolveTypeRefKind(kind.element, registry);
      return skipValue(buf, offset + 1, elemKind, registry);
    }
    case "map": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      let off = next;
      const keyKind = resolveTypeRefKind(kind.key, registry);
      const valKind = resolveTypeRefKind(kind.value, registry);
      for (let i = 0; i < len; i++) {
        off = skipValue(buf, off, keyKind, registry);
        off = skipValue(buf, off, valKind, registry);
      }
      return off;
    }
    case "array": {
      const elemKind = resolveTypeRefKind(kind.element, registry);
      let off = offset;
      for (let i = 0; i < kind.length; i++) {
        off = skipValue(buf, off, elemKind, registry);
      }
      return off;
    }
    case "channel":
      return decodeU64(buf, offset).next;
  }
}

function skipPrimitive(buf: Uint8Array, offset: number, pt: string): number {
  switch (pt) {
    case "bool":
    case "u8":
    case "i8":
      return offset + 1;
    case "u16":
    case "u32":
    case "u64":
    case "u128":
      return decodeVarintNumber(buf, offset).next;
    case "i16":
    case "i32":
    case "i64":
    case "i128":
      return decodeVarintNumber(buf, offset).next;
    case "f32":
      return offset + 4;
    case "f64":
      return offset + 8;
    case "char": {
      // UTF-8 encoded char (1-4 bytes). First byte determines length.
      const b = buf[offset];
      if (b < 0x80) return offset + 1;
      if ((b & 0xe0) === 0xc0) return offset + 2;
      if ((b & 0xf0) === 0xe0) return offset + 3;
      return offset + 4;
    }
    case "string": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      return next + len;
    }
    case "unit":
      return offset;
    case "bytes": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      return next + len;
    }
    case "payload": {
      // 4-byte LE u32 length prefix
      const len =
        buf[offset] |
        (buf[offset + 1] << 8) |
        (buf[offset + 2] << 16) |
        (buf[offset + 3] << 24);
      return offset + 4 + len;
    }
    default:
      throw new Error(`skipPrimitive: unknown primitive type "${pt}"`);
  }
}

function skipStruct(
  buf: Uint8Array,
  offset: number,
  fields: WireFieldSchema[],
  registry: WireSchemaRegistry,
): number {
  let off = offset;
  for (const f of fields) {
    const fieldKind = resolveTypeRefKind(f.type_ref, registry);
    off = skipValue(buf, off, fieldKind, registry);
  }
  return off;
}

function skipEnum(
  buf: Uint8Array,
  offset: number,
  variants: WireVariantSchema[],
  registry: WireSchemaRegistry,
): number {
  const { value: discriminant, next } = decodeVarintNumber(buf, offset);
  const variant = variants.find((v) => v.index === discriminant);
  if (!variant) throw new Error(`skipEnum: unknown variant discriminant ${discriminant}`);

  switch (variant.payload.tag) {
    case "unit":
      return next;
    case "newtype": {
      const innerKind = resolveTypeRefKind(variant.payload.type_ref, registry);
      return skipValue(buf, next, innerKind, registry);
    }
    case "tuple": {
      let off = next;
      for (const tr of variant.payload.types) {
        const elemKind = resolveTypeRefKind(tr, registry);
        off = skipValue(buf, off, elemKind, registry);
      }
      return off;
    }
    case "struct":
      return skipStruct(buf, next, variant.payload.fields, registry);
  }
}

function skipTuple(
  buf: Uint8Array,
  offset: number,
  elements: WireTypeRef[],
  registry: WireSchemaRegistry,
): number {
  let off = offset;
  for (const elem of elements) {
    const elemKind = resolveTypeRefKind(elem, registry);
    off = skipValue(buf, off, elemKind, registry);
  }
  return off;
}

// ============================================================================
// decodeWithPlan — plan-driven single-pass decode
// ============================================================================

/**
 * Decode postcard bytes using a translation plan.
 *
 * @param buf - Postcard-encoded buffer
 * @param offset - Starting offset
 * @param plan - Translation plan from buildPlan()
 * @param localKind - Local schema kind (for decoding leaves and identity)
 * @param remoteKind - Remote schema kind (for skipping unknown fields)
 * @param registry - Schema registry for resolving type refs
 */
export function decodeWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: TranslationPlan,
  localKind: WireSchemaKind,
  remoteKind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  switch (plan.tag) {
    case "identity":
      return decodeByKind(buf, offset, localKind, registry);

    case "struct":
      return decodeStructWithPlan(buf, offset, plan, localKind, remoteKind, registry);

    case "enum":
      return decodeEnumWithPlan(buf, offset, plan, localKind, remoteKind, registry);

    case "tuple":
      return decodeTupleWithPlan(buf, offset, plan, localKind, remoteKind, registry);

    case "list": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      let off = next;
      const localList = localKind as Extract<WireSchemaKind, { tag: "list" }>;
      const remoteList = remoteKind as Extract<WireSchemaKind, { tag: "list" }>;
      const localElemKind = resolveTypeRefKind(localList.element, registry);
      const remoteElemKind = resolveTypeRefKind(remoteList.element, registry);
      const result: unknown[] = [];
      for (let i = 0; i < len; i++) {
        const decoded = decodeWithPlan(buf, off, plan.element, localElemKind, remoteElemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: result, next: off };
    }

    case "option": {
      const flag = buf[offset];
      if (flag === 0) return { value: null, next: offset + 1 };
      const localOpt = localKind as Extract<WireSchemaKind, { tag: "option" }>;
      const remoteOpt = remoteKind as Extract<WireSchemaKind, { tag: "option" }>;
      const localElemKind = resolveTypeRefKind(localOpt.element, registry);
      const remoteElemKind = resolveTypeRefKind(remoteOpt.element, registry);
      return decodeWithPlan(buf, offset + 1, plan.inner, localElemKind, remoteElemKind, registry);
    }

    case "map": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      let off = next;
      const localMap = localKind as Extract<WireSchemaKind, { tag: "map" }>;
      const remoteMap = remoteKind as Extract<WireSchemaKind, { tag: "map" }>;
      const localKeyKind = resolveTypeRefKind(localMap.key, registry);
      const localValKind = resolveTypeRefKind(localMap.value, registry);
      const remoteKeyKind = resolveTypeRefKind(remoteMap.key, registry);
      const remoteValKind = resolveTypeRefKind(remoteMap.value, registry);
      const result = new Map<unknown, unknown>();
      for (let i = 0; i < len; i++) {
        const k = decodeWithPlan(buf, off, plan.key, localKeyKind, remoteKeyKind, registry);
        off = k.next;
        const v = decodeWithPlan(buf, off, plan.value, localValKind, remoteValKind, registry);
        off = v.next;
        result.set(k.value, v.value);
      }
      return { value: result, next: off };
    }

    case "array": {
      const localArr = localKind as Extract<WireSchemaKind, { tag: "array" }>;
      const remoteArr = remoteKind as Extract<WireSchemaKind, { tag: "array" }>;
      const localElemKind = resolveTypeRefKind(localArr.element, registry);
      const remoteElemKind = resolveTypeRefKind(remoteArr.element, registry);
      let off = offset;
      const result: unknown[] = [];
      for (let i = 0; i < localArr.length; i++) {
        const decoded = decodeWithPlan(buf, off, plan.element, localElemKind, remoteElemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: result, next: off };
    }

    case "pointer": {
      // Pointer types (Box, Arc) — decode the inner value
      return decodeWithPlan(buf, offset, plan.pointee, localKind, remoteKind, registry);
    }
  }
}

function decodeStructWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: Extract<TranslationPlan, { tag: "struct" }>,
  localKind: WireSchemaKind,
  remoteKind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  const localStruct = localKind as Extract<WireSchemaKind, { tag: "struct" }>;
  const remoteStruct = remoteKind as Extract<WireSchemaKind, { tag: "struct" }>;

  // Pre-fill result with null for fields that can be defaulted.
  const result: Record<string, unknown> = {};
  for (const f of localStruct.fields) {
    const fieldKind = resolveTypeRefKind(f.type_ref, registry);
    if (!f.required || fieldKind.tag === "option") result[f.name] = null;
  }

  let off = offset;
  for (let remoteIdx = 0; remoteIdx < plan.field_ops.length; remoteIdx++) {
    const op = plan.field_ops[remoteIdx];
    if (op.tag === "skip") {
      const remoteField = remoteStruct.fields[remoteIdx];
      const fieldKind = resolveTypeRefKind(remoteField.type_ref, registry);
      off = skipValue(buf, off, fieldKind, registry);
    } else {
      const localField = localStruct.fields[op.local_index];
      const nestedPlan = plan.nested.get(op.local_index);
      if (nestedPlan) {
        const remoteField = remoteStruct.fields[remoteIdx];
        const remoteFieldKind = resolveTypeRefKind(remoteField.type_ref, registry);
        const localFieldKind = resolveTypeRefKind(localField.type_ref, registry);
        const decoded = decodeWithPlan(buf, off, nestedPlan, localFieldKind, remoteFieldKind, registry);
        result[localField.name] = decoded.value;
        off = decoded.next;
      } else {
        const localFieldKind = resolveTypeRefKind(localField.type_ref, registry);
        const decoded = decodeByKind(buf, off, localFieldKind, registry);
        result[localField.name] = decoded.value;
        off = decoded.next;
      }
    }
  }

  return { value: result, next: off };
}

function decodeEnumWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: Extract<TranslationPlan, { tag: "enum" }>,
  localKind: WireSchemaKind,
  remoteKind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  const localEnum = localKind as Extract<WireSchemaKind, { tag: "enum" }>;
  const remoteEnum = remoteKind as Extract<WireSchemaKind, { tag: "enum" }>;

  const { value: discriminant, next } = decodeVarintNumber(buf, offset);
  let off = next;

  // Find remote variant by index
  const remoteVariant = remoteEnum.variants.find((v) => v.index === discriminant);
  if (!remoteVariant) {
    throw new Error(`unknown remote variant discriminant: ${discriminant}`);
  }

  const remoteIdx = remoteEnum.variants.indexOf(remoteVariant);
  const localIdx = plan.variant_map[remoteIdx];
  if (localIdx == null) {
    throw new Error(`unknown remote variant "${remoteVariant.name}" has no local mapping`);
  }

  const localVariant = localEnum.variants[localIdx];
  const variantPlan = plan.variant_plans.get(remoteIdx);

  switch (remoteVariant.payload.tag) {
    case "unit":
      return { value: { tag: localVariant.name }, next: off };

    case "newtype": {
      const nestedPlan = plan.nested.get(localIdx);
      const localNewtype = localVariant.payload as Extract<
        typeof localVariant.payload,
        { tag: "newtype" }
      >;
      const remoteInnerKind = resolveTypeRefKind(remoteVariant.payload.type_ref, registry);
      const localInnerKind = resolveTypeRefKind(localNewtype.type_ref, registry);
      const decoded = nestedPlan
        ? decodeWithPlan(buf, off, nestedPlan, localInnerKind, remoteInnerKind, registry)
        : decodeByKind(buf, off, localInnerKind, registry);
      return { value: { tag: localVariant.name, value: decoded.value }, next: decoded.next };
    }

    case "tuple": {
      if (variantPlan) {
        const localTuple = localVariant.payload as Extract<
          typeof localVariant.payload,
          { tag: "tuple" }
        >;
        const remoteKindT: WireSchemaKind = {
          tag: "tuple",
          elements: remoteVariant.payload.types,
        };
        const localKindT: WireSchemaKind = { tag: "tuple", elements: localTuple.types };
        const decoded = decodeWithPlan(buf, off, variantPlan, localKindT, remoteKindT, registry);
        return {
          value: { tag: localVariant.name, value: decoded.value },
          next: decoded.next,
        };
      }
      // Identity — decode tuple elements directly
      const result: unknown[] = [];
      const localTuple = localVariant.payload as Extract<
        typeof localVariant.payload,
        { tag: "tuple" }
      >;
      for (const tr of localTuple.types) {
        const elemKind = resolveTypeRefKind(tr, registry);
        const decoded = decodeByKind(buf, off, elemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: { tag: localVariant.name, value: result }, next: off };
    }

    case "struct": {
      if (variantPlan && variantPlan.tag === "struct") {
        const localStructPayload = localVariant.payload as Extract<
          typeof localVariant.payload,
          { tag: "struct" }
        >;
        const remoteKindS: WireSchemaKind = {
          tag: "struct",
          name: remoteVariant.name,
          fields: remoteVariant.payload.fields,
        };
        const localKindS: WireSchemaKind = {
          tag: "struct",
          name: localVariant.name,
          fields: localStructPayload.fields,
        };
        const decoded = decodeStructWithPlan(
          buf,
          off,
          variantPlan,
          localKindS,
          remoteKindS,
          registry,
        );
        return {
          value: { tag: localVariant.name, ...decoded.value as Record<string, unknown> },
          next: decoded.next,
        };
      }
      // Identity — decode struct fields directly
      const localStructPayload = localVariant.payload as Extract<
        typeof localVariant.payload,
        { tag: "struct" }
      >;
      const obj: Record<string, unknown> = { tag: localVariant.name };
      for (const f of localStructPayload.fields) {
        const fieldKind = resolveTypeRefKind(f.type_ref, registry);
        const decoded = decodeByKind(buf, off, fieldKind, registry);
        obj[f.name] = decoded.value;
        off = decoded.next;
      }
      return { value: obj, next: off };
    }
  }
}

function decodeTupleWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: Extract<TranslationPlan, { tag: "tuple" }>,
  localKind: WireSchemaKind,
  remoteKind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  const localTuple = localKind as Extract<WireSchemaKind, { tag: "tuple" }>;
  const remoteTuple = remoteKind as Extract<WireSchemaKind, { tag: "tuple" }>;
  const result: unknown[] = new Array(localTuple.elements.length);

  let off = offset;
  for (let remoteIdx = 0; remoteIdx < plan.field_ops.length; remoteIdx++) {
    const op = plan.field_ops[remoteIdx];
    if (op.tag === "skip") {
      const remoteElemKind = resolveTypeRefKind(remoteTuple.elements[remoteIdx], registry);
      off = skipValue(buf, off, remoteElemKind, registry);
    } else {
      const nestedPlan = plan.nested.get(op.local_index);
      const localElemKind = resolveTypeRefKind(localTuple.elements[op.local_index], registry);
      if (nestedPlan) {
        const remoteElemKind = resolveTypeRefKind(remoteTuple.elements[remoteIdx], registry);
        const decoded = decodeWithPlan(buf, off, nestedPlan, localElemKind, remoteElemKind, registry);
        result[op.local_index] = decoded.value;
        off = decoded.next;
      } else {
        const decoded = decodeByKind(buf, off, localElemKind, registry);
        result[op.local_index] = decoded.value;
        off = decoded.next;
      }
    }
  }

  return { value: result, next: off };
}

// ============================================================================
// decodeByKind — identity decode using local schema kind
// ============================================================================

function decodeByKind(
  buf: Uint8Array,
  offset: number,
  kind: WireSchemaKind,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  switch (kind.tag) {
    case "primitive":
      return decodePrimitive(buf, offset, kind.primitive_type);
    case "struct": {
      const result: Record<string, unknown> = {};
      let off = offset;
      for (const f of kind.fields) {
        const fieldKind = resolveTypeRefKind(f.type_ref, registry);
        const decoded = decodeByKind(buf, off, fieldKind, registry);
        result[f.name] = decoded.value;
        off = decoded.next;
      }
      return { value: result, next: off };
    }
    case "enum": {
      const { value: discriminant, next } = decodeVarintNumber(buf, offset);
      const variant = kind.variants.find((v) => v.index === discriminant);
      if (!variant) throw new Error(`unknown variant discriminant: ${discriminant}`);
      return decodeVariant(buf, next, variant, registry);
    }
    case "tuple": {
      const result: unknown[] = [];
      let off = offset;
      for (const elem of kind.elements) {
        const elemKind = resolveTypeRefKind(elem, registry);
        const decoded = decodeByKind(buf, off, elemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: result, next: off };
    }
    case "list": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      const elemKind = resolveTypeRefKind(kind.element, registry);
      const result: unknown[] = [];
      let off = next;
      for (let i = 0; i < len; i++) {
        const decoded = decodeByKind(buf, off, elemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: result, next: off };
    }
    case "option": {
      const flag = buf[offset];
      if (flag === 0) return { value: null, next: offset + 1 };
      const elemKind = resolveTypeRefKind(kind.element, registry);
      return decodeByKind(buf, offset + 1, elemKind, registry);
    }
    case "map": {
      const { value: len, next } = decodeVarintNumber(buf, offset);
      const keyKind = resolveTypeRefKind(kind.key, registry);
      const valKind = resolveTypeRefKind(kind.value, registry);
      const result = new Map<unknown, unknown>();
      let off = next;
      for (let i = 0; i < len; i++) {
        const k = decodeByKind(buf, off, keyKind, registry);
        off = k.next;
        const v = decodeByKind(buf, off, valKind, registry);
        off = v.next;
        result.set(k.value, v.value);
      }
      return { value: result, next: off };
    }
    case "array": {
      const elemKind = resolveTypeRefKind(kind.element, registry);
      const result: unknown[] = [];
      let off = offset;
      for (let i = 0; i < kind.length; i++) {
        const decoded = decodeByKind(buf, off, elemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: result, next: off };
    }
    case "channel":
      return decodeU64(buf, offset);
  }
}

function decodeVariant(
  buf: Uint8Array,
  offset: number,
  variant: WireVariantSchema,
  registry: WireSchemaRegistry,
): DecodeResult<unknown> {
  switch (variant.payload.tag) {
    case "unit":
      return { value: { tag: variant.name }, next: offset };
    case "newtype": {
      const innerKind = resolveTypeRefKind(variant.payload.type_ref, registry);
      const decoded = decodeByKind(buf, offset, innerKind, registry);
      return { value: { tag: variant.name, value: decoded.value }, next: decoded.next };
    }
    case "tuple": {
      const result: unknown[] = [];
      let off = offset;
      for (const tr of variant.payload.types) {
        const elemKind = resolveTypeRefKind(tr, registry);
        const decoded = decodeByKind(buf, off, elemKind, registry);
        result.push(decoded.value);
        off = decoded.next;
      }
      return { value: { tag: variant.name, value: result }, next: off };
    }
    case "struct": {
      const obj: Record<string, unknown> = { tag: variant.name };
      let off = offset;
      for (const f of variant.payload.fields) {
        const fieldKind = resolveTypeRefKind(f.type_ref, registry);
        const decoded = decodeByKind(buf, off, fieldKind, registry);
        obj[f.name] = decoded.value;
        off = decoded.next;
      }
      return { value: obj, next: off };
    }
  }
}

function decodePrimitive(
  buf: Uint8Array,
  offset: number,
  pt: string,
): DecodeResult<unknown> {
  switch (pt) {
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
    case "unit":
      return { value: undefined, next: offset };
    case "payload": {
      // 4-byte LE u32 length prefix
      const len =
        buf[offset] |
        (buf[offset + 1] << 8) |
        (buf[offset + 2] << 16) |
        (buf[offset + 3] << 24);
      return {
        value: buf.subarray(offset + 4, offset + 4 + len),
        next: offset + 4 + len,
      };
    }
    default:
      throw new Error(`decodePrimitive: unknown type "${pt}"`);
  }
}

// ============================================================================
// Helpers
// ============================================================================

function resolveTypeRefKind(
  ref_: WireTypeRef,
  registry: WireSchemaRegistry,
): WireSchemaKind {
  const kind = resolveWireTypeRef(ref_, registry);
  if (!kind) {
    if (ref_.tag === "var") {
      throw new Error(`cannot resolve type variable "${ref_.name}"`);
    }
    throw new Error(`schema not found for type_id ${ref_.type_id}`);
  }
  return kind;
}
