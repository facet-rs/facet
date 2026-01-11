# Phase 001: Extended Schema Types

**Status**: DONE

## Objective

Extend the TypeScript `Schema` type system in `roam-postcard` to support:
1. Enums with explicit discriminants (for `#[repr(u8)]` wire types)
2. Tuples (for `Vec<(String, MetadataValue)>` in metadata)
3. Type references (for complex/nested types and circular structures)

This replaces the existing `EnumSchema` with a more capable version.

## Background

The current `Schema` type in `roam-core/src/channeling/schema.ts` has:

```typescript
export interface EnumSchema {
  kind: "enum";
  variants: Record<string, Schema[]>; // variant name -> tuple of field schemas
}
```

This is insufficient for wire types because:

1. **No discriminant mapping**: Rust `#[repr(u8)]` enums have explicit discriminant values
   (e.g., `Hello = 0`, `Goodbye = 1`). We need to know which byte value maps to which variant.

2. **No struct variant support**: The current `Schema[]` represents tuple fields, but
   Rust variants can have named fields: `Request { request_id: u64, method_id: u64, ... }`.

3. **No tuple type**: We need `(String, MetadataValue)` for metadata entries.

4. **No type references**: Complex types are inlined, causing duplication. Circular
   types would cause infinite recursion. We need `{ kind: "ref", name: "Point" }`.

5. **Location**: Schema types should live in `roam-postcard` (the serialization layer),
   not `roam-core`. The channeling code in `roam-core` can import from `roam-postcard`.

## Design

### New Schema Types

Add to `roam-postcard/src/schema.ts`:

```typescript
/** A variant in an enum. */
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

/**
 * Tuple schema for fixed-size tuples like (String, MetadataValue).
 * 
 * Postcard encodes tuples by concatenating elements in order (no length prefix).
 */
export interface TupleSchema {
  kind: "tuple";
  
  /** Element schemas in order. */
  elements: Schema[];
}

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
```

### Updated Union Type

```typescript
export type Schema =
  | { kind: PrimitiveKind }
  | TxSchema
  | RxSchema
  | VecSchema
  | OptionSchema
  | MapSchema
  | StructSchema
  | EnumSchema
  | TupleSchema
  | RefSchema;
```

### Schema Registry

For `RefSchema` to work, we need a registry of named type schemas:

```typescript
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
export function resolveSchema(schema: Schema, registry: SchemaRegistry): Schema;
```

### Helper Functions

```typescript
/** Find a variant by discriminant value (for decoding). */
export function findVariantByDiscriminant(
  schema: EnumSchema,
  discriminant: number
): EnumVariant | undefined;

/** Find a variant by name (for encoding). */
export function findVariantByName(
  schema: EnumSchema,
  name: string
): EnumVariant | undefined;

/** Get the discriminant for a variant (uses index if not explicit). */
export function getVariantDiscriminant(
  schema: EnumSchema,
  variant: EnumVariant
): number;

/** Get the field schemas for a variant as an ordered array. */
export function getVariantFieldSchemas(variant: EnumVariant): Schema[];

/** Get the field names for a struct variant (for decoding into object). */
export function getVariantFieldNames(variant: EnumVariant): string[] | null;

/** Check if variant fields represent a newtype (single Schema, not array/record). */
export function isNewtypeVariant(variant: EnumVariant): boolean;

/** Check if a schema is a reference. */
export function isRefSchema(schema: Schema): schema is RefSchema;
```

### Backward Compatibility

The new `EnumSchema` format using `EnumVariant[]` replaces the old `Record<string, Schema[]>`.

The generated code in `testbed.ts` uses the old format:
```typescript
{ kind: 'enum', variants: { 'Circle': [{ kind: 'f64' }], 'Point': [] } }
```

This needs to be updated in `roam-codegen` to use the new format:
```typescript
{ kind: 'enum', variants: [
  { name: 'Circle', fields: [{ kind: 'f64' }] },
  { name: 'Point', fields: null },
] }
```

### Generated Schema Registry

For wire types, codegen will generate a registry:

```typescript
// Generated schema definitions
export const HelloSchema: EnumSchema = { ... };
export const MetadataValueSchema: EnumSchema = { ... };
export const MessageSchema: EnumSchema = { 
  kind: "enum",
  variants: [
    { name: "Hello", discriminant: 0, fields: { kind: "ref", name: "Hello" } },
    // ...
  ]
};

// Registry for resolving refs
export const wireSchemaRegistry: SchemaRegistry = new Map([
  ["Hello", HelloSchema],
  ["MetadataValue", MetadataValueSchema],
  ["Message", MessageSchema],
]);
```

## Implementation Steps

1. Create `roam-postcard/src/schema.ts` with the new types
2. Export from `roam-postcard/src/index.ts`
3. Update `roam-core/src/channeling/schema.ts` to re-export from `roam-postcard`
4. Update `roam-codegen` TypeScript schema generation to use new format
5. Add `RefSchema` and `SchemaRegistry` support
6. Add unit tests for helper functions

## Files to Create/Modify

| File | Action |
|------|--------|
| `typescript/packages/roam-postcard/src/schema.ts` | CREATE |
| `typescript/packages/roam-postcard/src/index.ts` | MODIFY (add exports) |
| `typescript/packages/roam-postcard/src/schema.test.ts` | CREATE |
| `typescript/packages/roam-core/src/channeling/schema.ts` | MODIFY (re-export from roam-postcard) |
| `rust/roam-codegen/src/targets/typescript/schema.rs` | MODIFY (new EnumSchema format) |

## Success Criteria

1. ✅ `EnumSchema` type is defined with `variants: EnumVariant[]` - DONE
2. ✅ `EnumVariant` supports unit, newtype, tuple, and struct variants - DONE
3. ✅ `EnumVariant` supports optional explicit `discriminant` - DONE
4. ✅ `TupleSchema` type is defined for fixed-size tuples - DONE
5. ✅ `RefSchema` type is defined for type references - DONE
6. ✅ `SchemaRegistry` and `resolveSchema` are defined - DONE
7. ✅ Helper functions compile and have correct type signatures - DONE
8. ✅ Unit tests pass for helper functions: - DONE (52 tests in roam-postcard)
   - `findVariantByDiscriminant` returns correct variant or undefined
   - `findVariantByName` returns correct variant or undefined
   - `getVariantDiscriminant` returns explicit discriminant or index
   - `getVariantFieldSchemas` returns schemas in correct order for all variant kinds
   - `getVariantFieldNames` returns names for struct variants, null otherwise
   - `resolveSchema` follows refs and throws on unknown types
9. ✅ `roam-codegen` updated to generate new format - DONE
10. ✅ Existing tests still pass after migration - DONE

## Test Cases

```typescript
// Wire types with explicit discriminants
const MessageSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Hello", discriminant: 0, fields: HelloSchema },
    { name: "Goodbye", discriminant: 1, fields: { reason: { kind: "string" } } },
    { name: "Cancel", discriminant: 4, fields: { requestId: { kind: "u64" } } },
  ],
};

// findVariantByDiscriminant(MessageSchema, 0) → { name: "Hello", ... }
// findVariantByDiscriminant(MessageSchema, 1) → { name: "Goodbye", ... }
// findVariantByDiscriminant(MessageSchema, 2) → undefined (no variant with this discriminant)
// findVariantByDiscriminant(MessageSchema, 4) → { name: "Cancel", ... }

// findVariantByName(MessageSchema, "Hello") → { name: "Hello", ... }
// findVariantByName(MessageSchema, "Unknown") → undefined

// getVariantDiscriminant(MessageSchema, variants[2]) → 4 (explicit)

// Simple enums without explicit discriminants (uses index)
const ColorSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Red", fields: null },
    { name: "Green", fields: null },
    { name: "Blue", fields: null },
  ],
};

// getVariantDiscriminant(ColorSchema, variants[0]) → 0 (index)
// getVariantDiscriminant(ColorSchema, variants[1]) → 1 (index)
// getVariantDiscriminant(ColorSchema, variants[2]) → 2 (index)

// Struct variant field handling
// getVariantFieldSchemas({ name: "Goodbye", fields: { reason: { kind: "string" } } })
//   → [{ kind: "string" }]
// getVariantFieldNames({ name: "Goodbye", fields: { reason: { kind: "string" } } })
//   → ["reason"]

// Newtype variant detection
// isNewtypeVariant({ name: "Hello", fields: HelloSchema }) → true
// isNewtypeVariant({ name: "Goodbye", fields: { reason: ... } }) → false
// isNewtypeVariant({ name: "Tuple", fields: [a, b] }) → false

// TupleSchema for metadata entries: (String, MetadataValue)
const MetadataEntrySchema: TupleSchema = {
  kind: "tuple",
  elements: [{ kind: "string" }, { kind: "ref", name: "MetadataValue" }],
};

// Used in Request/Response: Vec<(String, MetadataValue)>
const MetadataVecSchema: VecSchema = {
  kind: "vec",
  element: MetadataEntrySchema,
};

// RefSchema and registry tests
const registry: SchemaRegistry = new Map([
  ["MetadataValue", MetadataValueSchema],
  ["Message", MessageSchema],
]);

// resolveSchema({ kind: "ref", name: "MetadataValue" }, registry) → MetadataValueSchema
// resolveSchema({ kind: "string" }, registry) → { kind: "string" } (unchanged)
// resolveSchema({ kind: "ref", name: "Unknown" }, registry) → throws Error

// Circular type example (linked list node)
const NodeSchema: StructSchema = {
  kind: "struct",
  fields: {
    value: { kind: "i32" },
    next: { kind: "option", inner: { kind: "ref", name: "Node" } },
  },
};
const circularRegistry = new Map([["Node", NodeSchema]]);
// This works because we only resolve refs at encode/decode time, not at schema definition time
```

## Notes

- Discriminants are encoded as varints in postcard, not raw bytes
- Variant fields are encoded in declaration order (for struct variants, this means key order in the Record)
- When `discriminant` is omitted, it defaults to the variant's index in the array
- For sparse discriminants (0, 1, 4, ...), lookup by discriminant needs to scan the array
- Tuples are encoded by concatenating elements in order (no length prefix, unlike Vec)
- `TupleSchema` is essential for `Vec<(String, MetadataValue)>` in Request/Response metadata
- `RefSchema` enables:
  - **Deduplication**: Don't repeat the same struct schema inline multiple times
  - **Circular types**: A type can reference itself (e.g., linked list, tree)
  - **Readability**: Generated schemas are easier to read
- Type names in `RefSchema` use `module_path::type_identifier` from facet (or just `type_identifier` for types in the same module)
- `resolveSchema()` is called lazily during encode/decode, not at schema construction time