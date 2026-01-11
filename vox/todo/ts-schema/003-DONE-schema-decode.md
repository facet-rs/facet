# Phase 003: Schema-Driven Decode Function

**Status**: TODO

## Objective

Implement `decodeWithSchema<T>(schema: Schema, buf: Uint8Array, offset: number, registry?: SchemaRegistry): DecodeResult<T>`
in `roam-postcard` that decodes bytes according to a schema, producing values compatible with
the TypeScript type representations.

The optional `registry` parameter is used to resolve `RefSchema` references to named types.

## Background

Currently, decoding is done with type-specific functions:

```typescript
// Current approach - separate functions per type
decodeU32(buf, offset)    // → DecodeResult<number>
decodeString(buf, offset) // → DecodeResult<string>
decodeVec(buf, offset, decodeItem) // → DecodeResult<T[]>
decodeOption(buf, offset, decodeInner) // → DecodeResult<T | null>
```

This works for primitives but requires hand-coded logic for complex types.
We need a single function that walks a schema and decodes accordingly.

## Design

### Core Function

```typescript
/**
 * Decode a value according to its schema.
 * 
 * @param schema - The schema describing the expected type
 * @param buf - The buffer to decode from
 * @param offset - Starting offset in the buffer
 * @param registry - Optional registry for resolving RefSchema references
 * @returns Decoded value and next offset
 * @throws Error if buffer doesn't match schema, is truncated, or ref cannot be resolved
 */
export function decodeWithSchema<T = unknown>(
  schema: Schema,
  buf: Uint8Array,
  offset: number,
  registry?: SchemaRegistry
): DecodeResult<T>;
```

### DecodeResult Type

Already exists in `roam-postcard`:

```typescript
export interface DecodeResult<T> {
  value: T;
  next: number; // offset after this value
}
```

### Decoding Rules (Postcard Format)

| Schema Kind | Decoding | Result Type |
|-------------|----------|-------------|
| `bool` | 1 byte: 0x00 → false, 0x01 → true | `boolean` |
| `u8` | 1 byte | `number` |
| `i8` | 1 byte (two's complement) | `number` |
| `u16`, `u32` | Varint | `number` |
| `u64` | Varint | `bigint` |
| `i16`, `i32` | Zigzag varint | `number` |
| `i64` | Zigzag varint | `bigint` |
| `f32` | 4 bytes little-endian IEEE 754 | `number` |
| `f64` | 8 bytes little-endian IEEE 754 | `number` |
| `string` | Varint length + UTF-8 bytes | `string` |
| `bytes` | Varint length + raw bytes | `Uint8Array` |
| `vec` | Varint count + decoded elements | `T[]` |
| `option` | 0x00 → null, 0x01 + value → T | `T \| null` |
| `map` | Varint count + key-value pairs | `Map<K, V>` |
| `struct` | Decoded fields in order | `{ [field]: value }` |
| `tagged-enum` | Varint discriminant + variant fields | `{ tag: string; ... }` |
| `tx`, `rx` | Varint channel ID (u64) | `{ channelId: bigint }` |
| `ref` | Resolve to actual schema and decode that | (depends on resolved schema) |

### Implementation Strategy

```typescript
function decodeWithSchema<T = unknown>(
  schema: Schema,
  buf: Uint8Array,
  offset: number,
  registry?: SchemaRegistry
): DecodeResult<T> {
  switch (schema.kind) {
    case "ref": {
      if (!registry) {
        throw new Error(`Cannot resolve ref "${schema.name}" without a registry`);
      }
      const resolved = registry.get(schema.name);
      if (!resolved) {
        throw new Error(`Unknown type ref: ${schema.name}`);
      }
      return decodeWithSchema<T>(resolved, buf, offset, registry);
    }
    
    case "bool":
      return decodeBool(buf, offset) as DecodeResult<T>;
    
    case "u8":
      return decodeU8(buf, offset) as DecodeResult<T>;
    
    // ... other primitives delegate to existing functions
    
    case "vec": {
      const len = decodeVarintNumber(buf, offset);
      let pos = len.next;
      const items: unknown[] = [];
      for (let i = 0; i < len.value; i++) {
        const item = decodeWithSchema(schema.element, buf, pos, registry);
        items.push(item.value);
        pos = item.next;
      }
      return { value: items as T, next: pos };
    }
    
    case "option": {
      if (offset >= buf.length) throw new Error("option: eof");
      const variant = buf[offset];
      if (variant === 0) {
        return { value: null as T, next: offset + 1 };
      } else if (variant === 1) {
        const inner = decodeWithSchema(schema.inner, buf, offset + 1, registry);
        return { value: inner.value as T, next: inner.next };
      } else {
        throw new Error(`option: invalid variant ${variant}`);
      }
    }
    
    case "struct": {
      const result: Record<string, unknown> = {};
      let pos = offset;
      for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
        const field = decodeWithSchema(fieldSchema, buf, pos, registry);
        result[fieldName] = field.value;
        pos = field.next;
      }
      return { value: result as T, next: pos };
    }
    
    case "tuple": {
      const result: unknown[] = [];
      let pos = offset;
      for (const elementSchema of schema.elements) {
        const element = decodeWithSchema(elementSchema, buf, pos, registry);
        result.push(element.value);
        pos = element.next;
      }
      return { value: result as T, next: pos };
    }
    
    case "enum": {
      const discrim = decodeVarintNumber(buf, offset);
      const variant = findVariantByDiscriminant(schema, discrim.value);
      if (!variant) {
        throw new Error(`Unknown discriminant: ${discrim.value}`);
      }
      
      let pos = discrim.next;
      const result: Record<string, unknown> = { tag: variant.name };
      
      if (variant.fields === null || variant.fields === undefined) {
        // Unit variant - nothing more to decode
      } else if ("kind" in variant.fields) {
        // Newtype variant - single schema
        const inner = decodeWithSchema(variant.fields as Schema, buf, pos, registry);
        // Store with lowercase variant name as key
        result[variant.name.toLowerCase()] = inner.value;
        pos = inner.next;
      } else if (Array.isArray(variant.fields)) {
        // Tuple variant - array of schemas
        const values: unknown[] = [];
        for (const fieldSchema of variant.fields) {
          const field = decodeWithSchema(fieldSchema, buf, pos, registry);
          values.push(field.value);
          pos = field.next;
        }
        result.values = values;
      } else {
        // Struct variant - named fields
        for (const [fieldName, fieldSchema] of Object.entries(variant.fields)) {
          const field = decodeWithSchema(fieldSchema as Schema, buf, pos, registry);
          result[fieldName] = field.value;
          pos = field.next;
        }
      }
      
      return { value: result as T, next: pos };
    }
    
    case "tx":
    case "rx": {
      const channelId = decodeU64(buf, offset);
      return { value: { channelId: channelId.value } as T, next: channelId.next };
    }
    
    case "map": {
      const len = decodeVarintNumber(buf, offset);
      let pos = len.next;
      const map = new Map<unknown, unknown>();
      for (let i = 0; i < len.value; i++) {
        const key = decodeWithSchema(schema.key, buf, pos, registry);
        const val = decodeWithSchema(schema.value, buf, key.next, registry);
        map.set(key.value, val.value);
        pos = val.next;
      }
      return { value: map as T, next: pos };
    }
    
    default:
      throw new Error(`Unsupported schema kind: ${(schema as Schema).kind}`);
  }
}
```

## Implementation Steps

1. Add `decodeWithSchema` to `roam-postcard/src/schema.ts`
2. Handle all primitive kinds (delegate to existing decode functions)
3. Handle container kinds (vec, option, map)
4. Handle struct (iterate fields in order, build object)
5. Handle tuple (decode elements in order, build array)
6. Handle enum (lookup variant by discriminant, decode fields)
7. Handle ref (resolve from registry and recurse)
8. Add comprehensive tests

## Files to Create/Modify

| File | Action |
|------|--------|
| `typescript/packages/roam-postcard/src/schema.ts` | MODIFY (add decodeWithSchema) |
| `typescript/packages/roam-postcard/src/schema.test.ts` | MODIFY (add decode tests) |

## Dependencies

- Phase 001 (Schema types) must be complete
- Phase 002 (Schema encode) should be complete (for roundtrip tests)

## Success Criteria

1. ✅ `decodeWithSchema` compiles with correct type signature (including optional registry)
2. ✅ Primitives decode identically to existing functions:
   - `decodeWithSchema({ kind: "u32" }, buf, 0)` equals `decodeU32(buf, 0)`
   - `decodeWithSchema({ kind: "string" }, buf, 0)` equals `decodeString(buf, 0)`
3. ✅ Containers decode correctly:
   - Vec with various element types
   - Option with Some and None
   - Nested containers (Vec<Option<String>>)
4. ✅ Structs decode into objects with correct field names
5. ✅ Tuples decode into arrays with correct element order
6. ✅ Enums decode correctly:
   - Unit variants → `{ tag: "VariantName" }`
   - Newtype variants → `{ tag: "VariantName", variantname: value }`
   - Struct variants → `{ tag: "VariantName", field1: v1, field2: v2, ... }`
7. ✅ Refs resolve correctly:
   - Resolves to actual schema and decodes
   - Throws if registry not provided
   - Throws if ref name not found
8. ✅ Errors thrown for truncated buffers or invalid discriminants
9. ✅ Roundtrip tests pass: `decode(encode(value)) === value`

## Test Cases

```typescript
// Primitives
const u32Buf = encodeU32(42);
expect(decodeWithSchema({ kind: "u32" }, u32Buf, 0).value).toBe(42);

const strBuf = encodeString("hello");
expect(decodeWithSchema({ kind: "string" }, strBuf, 0).value).toBe("hello");

// Vec
const vecBuf = encodeVec([1, 2, 3], encodeU32);
expect(decodeWithSchema(
  { kind: "vec", element: { kind: "u32" } },
  vecBuf,
  0
).value).toEqual([1, 2, 3]);

// Option
const someBuf = encodeOption("hello", encodeString);
expect(decodeWithSchema(
  { kind: "option", inner: { kind: "string" } },
  someBuf,
  0
).value).toBe("hello");

const noneBuf = encodeOption(null, encodeString);
expect(decodeWithSchema(
  { kind: "option", inner: { kind: "string" } },
  noneBuf,
  0
).value).toBe(null);

// Struct
const PointSchema = {
  kind: "struct" as const,
  fields: { x: { kind: "i32" as const }, y: { kind: "i32" as const } }
};
const pointBuf = concat(encodeI32(10), encodeI32(20));
expect(decodeWithSchema(PointSchema, pointBuf, 0).value)
  .toEqual({ x: 10, y: 20 });

// Enum - unit variant
const StatusSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Pending", discriminant: 0, fields: null },
    { name: "Complete", discriminant: 1, fields: { result: { kind: "string" } } },
  ]
};
const pendingBuf = encodeVarint(0);
expect(decodeWithSchema(StatusSchema, pendingBuf, 0).value)
  .toEqual({ tag: "Pending" });

// Enum - struct variant
const completeBuf = concat(encodeVarint(1), encodeString("done"));
expect(decodeWithSchema(StatusSchema, completeBuf, 0).value)
  .toEqual({ tag: "Complete", result: "done" });

// Roundtrip test
const value = { tag: "Complete", result: "success" };
const encoded = encodeWithSchema(StatusSchema, value);
const decoded = decodeWithSchema(StatusSchema, encoded, 0);
expect(decoded.value).toEqual(value);

// Tuple
const PairSchema = { kind: "tuple" as const, elements: [{ kind: "i32" as const }, { kind: "string" as const }] };
const pairBuf = concat(encodeI32(42), encodeString("hello"));
expect(decodeWithSchema(PairSchema, pairBuf, 0).value).toEqual([42, "hello"]);

// Ref with registry
const PointSchema = { kind: "struct" as const, fields: { x: { kind: "i32" as const }, y: { kind: "i32" as const } } };
const registry = new Map([["Point", PointSchema]]);
const pointBufForRef = concat(encodeI32(10), encodeI32(20));

expect(decodeWithSchema({ kind: "ref", name: "Point" }, pointBufForRef, 0, registry).value)
  .toEqual({ x: 10, y: 20 });

// Ref without registry throws
expect(() => decodeWithSchema({ kind: "ref", name: "Point" }, pointBufForRef, 0))
  .toThrow(/without a registry/);

// Unknown ref throws
expect(() => decodeWithSchema({ kind: "ref", name: "Unknown" }, pointBufForRef, 0, registry))
  .toThrow(/Unknown type ref/);

// Error cases
expect(() => decodeWithSchema({ kind: "u32" }, new Uint8Array([]), 0))
  .toThrow();
expect(() => decodeWithSchema(StatusSchema, encodeVarint(99), 0))
  .toThrow(/Unknown discriminant/);
```

## Notes

- The `next` field in `DecodeResult` is critical for nested decoding
- We must track position through all recursive calls
- Type parameter `T` is for caller convenience; runtime doesn't check
- For tagged enums, the convention for newtype variants is to use lowercase variant name as the field key
  (e.g., `{ tag: "Hello", hello: { ... } }`)
- Performance: Recursive approach is fine for wire types; hot paths use codegen'd code
- Error messages should be descriptive for debugging wire format issues