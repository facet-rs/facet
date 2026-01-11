# Phase 002: Schema-Driven Encode Function

**Status**: TODO

## Objective

Implement `encodeWithSchema(schema: Schema, value: unknown, registry?: SchemaRegistry): Uint8Array` 
in `roam-postcard` that encodes any value according to its schema, producing bytes compatible 
with Rust's postcard format.

The optional `registry` parameter is used to resolve `RefSchema` references to named types.

## Background

Currently, encoding is done with type-specific functions:

```typescript
// Current approach - separate functions per type
encodeU32(value)
encodeString(value)
encodeVec(values, encodeItem)
encodeOption(value, encodeInner)
```

This works for primitives but requires hand-coded logic for complex types.
We need a single function that walks a schema and encodes accordingly.

## Design

### Core Function

```typescript
/**
 * Encode a value according to its schema.
 * 
 * @param schema - The schema describing the value's type
 * @param value - The value to encode (must match schema structure)
 * @param registry - Optional registry for resolving RefSchema references
 * @returns Encoded bytes in postcard format
 * @throws Error if value doesn't match schema or ref cannot be resolved
 */
export function encodeWithSchema(
  schema: Schema, 
  value: unknown, 
  registry?: SchemaRegistry
): Uint8Array;
```

### Encoding Rules (Postcard Format)

| Schema Kind | Encoding |
|-------------|----------|
| `bool` | 1 byte: 0x00 or 0x01 |
| `u8` | 1 byte |
| `i8` | 1 byte (two's complement) |
| `u16`, `u32`, `u64` | Varint |
| `i16`, `i32`, `i64` | Zigzag varint |
| `f32` | 4 bytes little-endian IEEE 754 |
| `f64` | 8 bytes little-endian IEEE 754 |
| `string` | Varint length + UTF-8 bytes |
| `bytes` | Varint length + raw bytes |
| `vec` | Varint count + encoded elements |
| `option` | 0x00 for null, 0x01 + encoded value for Some |
| `map` | Varint count + encoded key-value pairs |
| `struct` | Encoded fields in declaration order (no framing) |
| `enum` | Varint discriminant + encoded variant fields |
| `tagged-enum` | Varint discriminant + encoded variant fields |
| `tx`, `rx` | Varint channel ID (u64) |
| `ref` | Resolve to actual schema and encode that |

### Implementation Strategy

```typescript
function encodeWithSchema(
  schema: Schema, 
  value: unknown, 
  registry?: SchemaRegistry
): Uint8Array {
  switch (schema.kind) {
    case "ref": {
      if (!registry) {
        throw new Error(`Cannot resolve ref "${schema.name}" without a registry`);
      }
      const resolved = registry.get(schema.name);
      if (!resolved) {
        throw new Error(`Unknown type ref: ${schema.name}`);
      }
      return encodeWithSchema(resolved, value, registry);
    }
    
    case "bool":
      return encodeBool(value as boolean);
    
    case "u8":
      return encodeU8(value as number);
    
    // ... other primitives use existing functions
    
    case "vec": {
      const arr = value as unknown[];
      const parts: Uint8Array[] = [encodeVarint(arr.length)];
      for (const item of arr) {
        parts.push(encodeWithSchema(schema.element, item, registry));
      }
      return concat(...parts);
    }
    
    case "option": {
      if (value === null || value === undefined) {
        return Uint8Array.of(0);
      }
      return concat(Uint8Array.of(1), encodeWithSchema(schema.inner, value, registry));
    }
    
    case "struct": {
      const obj = value as Record<string, unknown>;
      const parts: Uint8Array[] = [];
      // Fields MUST be encoded in schema declaration order
      for (const [fieldName, fieldSchema] of Object.entries(schema.fields)) {
        parts.push(encodeWithSchema(fieldSchema, obj[fieldName], registry));
      }
      return concat(...parts);
    }
    
    case "tuple": {
      const arr = value as unknown[];
      const parts: Uint8Array[] = [];
      for (let i = 0; i < schema.elements.length; i++) {
        parts.push(encodeWithSchema(schema.elements[i], arr[i], registry));
      }
      return concat(...parts);
    }
    
    case "enum": {
      const tagged = value as { tag: string; [key: string]: unknown };
      const variant = findVariantByName(schema, tagged.tag);
      if (!variant) {
        throw new Error(`Unknown variant: ${tagged.tag}`);
      }
      
      const discriminant = variant.discriminant ?? schema.variants.indexOf(variant);
      const parts: Uint8Array[] = [encodeVarint(discriminant)];
      
      // Encode variant fields
      if (variant.fields === null || variant.fields === undefined) {
        // Unit variant - nothing more to encode
      } else if ("kind" in variant.fields) {
        // Newtype variant - single schema, value is in a field matching variant name (lowercase)
        // e.g., { tag: "Hello", hello: { ... } }
        const fieldValue = tagged[variant.name.toLowerCase()] ?? tagged.value;
        parts.push(encodeWithSchema(variant.fields as Schema, fieldValue, registry));
      } else if (Array.isArray(variant.fields)) {
        // Tuple variant - array of schemas
        // Value should have numeric indices or be an array
        const tupleValue = tagged.values ?? tagged;
        for (let i = 0; i < variant.fields.length; i++) {
          parts.push(encodeWithSchema(variant.fields[i], (tupleValue as unknown[])[i], registry));
        }
      } else {
        // Struct variant - named fields
        for (const [fieldName, fieldSchema] of Object.entries(variant.fields)) {
          parts.push(encodeWithSchema(fieldSchema as Schema, tagged[fieldName], registry));
        }
      }
      
      return concat(...parts);
    }
    
    case "tx":
    case "rx": {
      // Streaming types encode as channel ID (u64)
      // r[impl streaming.type] - Tx/Rx serialize as channel_id on wire.
      return encodeU64((value as { channelId: bigint }).channelId);
    }
    
    case "map": {
      const map = value as Map<unknown, unknown>;
      const parts: Uint8Array[] = [encodeVarint(map.size)];
      for (const [k, v] of map.entries()) {
        parts.push(encodeWithSchema(schema.key, k, registry));
        parts.push(encodeWithSchema(schema.value, v, registry));
      }
      return concat(...parts);
    }
    
    default:
      throw new Error(`Unsupported schema kind: ${(schema as Schema).kind}`);
  }
}
```

## Implementation Steps

1. Add `encodeWithSchema` to `roam-postcard/src/schema.ts`
2. Handle all primitive kinds (delegate to existing encode functions)
3. Handle container kinds (vec, option, map)
4. Handle struct (iterate fields in order)
5. Handle enum (lookup variant, encode discriminant + fields)
6. Handle ref (resolve from registry and recurse)
7. Handle tuple (encode elements in order)
8. Add comprehensive tests

## Files to Create/Modify

| File | Action |
|------|--------|
| `typescript/packages/roam-postcard/src/schema.ts` | MODIFY (add encodeWithSchema) |
| `typescript/packages/roam-postcard/src/schema.test.ts` | MODIFY (add encode tests) |

## Dependencies

- Phase 001 (Schema types) must be complete

## Success Criteria

1. ✅ `encodeWithSchema` compiles with correct type signature (including optional registry)
2. ✅ Primitives encode identically to existing functions:
   - `encodeWithSchema({ kind: "u32" }, 42)` equals `encodeU32(42)`
   - `encodeWithSchema({ kind: "string" }, "hello")` equals `encodeString("hello")`
3. ✅ Containers encode correctly:
   - Vec with various element types
   - Option with Some and None
   - Nested containers (Vec<Option<String>>)
4. ✅ Structs encode fields in declaration order
5. ✅ Tuples encode elements in order
6. ✅ Enums encode correctly:
   - Unit variants (discriminant only)
   - Newtype variants (discriminant + inner value)
   - Struct variants (discriminant + fields in order)
7. ✅ Refs resolve correctly:
   - Resolves to actual schema and encodes
   - Throws if registry not provided
   - Throws if ref name not found
8. ✅ Errors thrown for mismatched schema/value

## Test Cases

```typescript
// Primitives
expect(encodeWithSchema({ kind: "bool" }, true)).toEqual(encodeBool(true));
expect(encodeWithSchema({ kind: "u32" }, 42)).toEqual(encodeU32(42));
expect(encodeWithSchema({ kind: "string" }, "hello")).toEqual(encodeString("hello"));

// Vec
expect(encodeWithSchema(
  { kind: "vec", element: { kind: "u32" } },
  [1, 2, 3]
)).toEqual(encodeVec([1, 2, 3], encodeU32));

// Option
expect(encodeWithSchema(
  { kind: "option", inner: { kind: "string" } },
  null
)).toEqual(encodeOption(null, encodeString));

expect(encodeWithSchema(
  { kind: "option", inner: { kind: "string" } },
  "hello"
)).toEqual(encodeOption("hello", encodeString));

// Struct
const PointSchema = {
  kind: "struct" as const,
  fields: { x: { kind: "i32" as const }, y: { kind: "i32" as const } }
};
expect(encodeWithSchema(PointSchema, { x: 10, y: 20 }))
  .toEqual(concat(encodeI32(10), encodeI32(20)));

// Enum - unit variant
const StatusSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Pending", discriminant: 0, fields: null },
    { name: "Complete", discriminant: 1, fields: { result: { kind: "string" } } },
  ]
};
expect(encodeWithSchema(StatusSchema, { tag: "Pending" }))
  .toEqual(encodeVarint(0));

// Enum - struct variant
expect(encodeWithSchema(StatusSchema, { tag: "Complete", result: "done" }))
  .toEqual(concat(encodeVarint(1), encodeString("done")));

// Nested: Vec<Option<u32>>
const NestedSchema = {
  kind: "vec" as const,
  element: { kind: "option" as const, inner: { kind: "u32" as const } }
};
expect(encodeWithSchema(NestedSchema, [42, null, 100])).toEqual(
  concat(
    encodeVarint(3),
    encodeOption(42, encodeU32),
    encodeOption(null, encodeU32),
    encodeOption(100, encodeU32)
  )
);

// Tuple
const PairSchema = { kind: "tuple" as const, elements: [{ kind: "i32" as const }, { kind: "string" as const }] };
expect(encodeWithSchema(PairSchema, [42, "hello"])).toEqual(
  concat(encodeI32(42), encodeString("hello"))
);

// Ref with registry
const PointSchema = { kind: "struct" as const, fields: { x: { kind: "i32" as const }, y: { kind: "i32" as const } } };
const registry = new Map([["Point", PointSchema]]);

expect(encodeWithSchema({ kind: "ref", name: "Point" }, { x: 1, y: 2 }, registry))
  .toEqual(concat(encodeI32(1), encodeI32(2)));

// Ref without registry throws
expect(() => encodeWithSchema({ kind: "ref", name: "Point" }, { x: 1, y: 2 }))
  .toThrow(/without a registry/);

// Unknown ref throws
expect(() => encodeWithSchema({ kind: "ref", name: "Unknown" }, {}, registry))
  .toThrow(/Unknown type ref/);
```

## Notes

- Field order in TypeScript objects is preserved for string keys (ES2015+)
- We rely on schema field order, not object key order
- For struct variants, we iterate `Object.entries(variant.fields)` which preserves insertion order
- Performance: This is recursive; for hot paths, codegen'd functions are faster
- The schema-driven approach is for wire types where correctness > performance