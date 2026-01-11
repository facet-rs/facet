// Tests for schema types and helper functions

import { describe, it, expect } from "vitest";
import {
  type EnumSchema,
  type EnumVariant,
  type Schema,
  type SchemaRegistry,
  type TupleSchema,
  type StructSchema,
  type RefSchema,
  resolveSchema,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  isRefSchema,
} from "./schema.ts";

// ============================================================================
// Test Schemas
// ============================================================================

const PointSchema: StructSchema = {
  kind: "struct",
  fields: {
    x: { kind: "i32" },
    y: { kind: "i32" },
  },
};

const ColorSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Red", fields: null },
    { name: "Green", fields: null },
    { name: "Blue", fields: null },
  ],
};

const ShapeSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Circle", fields: [{ kind: "f64" }] },
    { name: "Rectangle", fields: [{ kind: "f64" }, { kind: "f64" }] },
    { name: "Point", fields: null },
  ],
};

// Wire types with explicit discriminants (like Message)
const MessageSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Hello", discriminant: 0, fields: { kind: "ref", name: "Hello" } },
    { name: "Goodbye", discriminant: 1, fields: { reason: { kind: "string" } } },
    { name: "Cancel", discriminant: 4, fields: { requestId: { kind: "u64" } } },
  ],
};

const HelloSchema: EnumSchema = {
  kind: "enum",
  variants: [
    {
      name: "V1",
      discriminant: 0,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
      },
    },
  ],
};

const MetadataValueSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "String", discriminant: 0, fields: { kind: "string" } },
    { name: "Bytes", discriminant: 1, fields: { kind: "bytes" } },
    { name: "U64", discriminant: 2, fields: { kind: "u64" } },
  ],
};

const MetadataEntrySchema: TupleSchema = {
  kind: "tuple",
  elements: [{ kind: "string" }, { kind: "ref", name: "MetadataValue" }],
};

// Circular type example (linked list node)
const NodeSchema: StructSchema = {
  kind: "struct",
  fields: {
    value: { kind: "i32" },
    next: { kind: "option", inner: { kind: "ref", name: "Node" } },
  },
};

// ============================================================================
// Test Registry
// ============================================================================

const testRegistry: SchemaRegistry = new Map<string, Schema>([
  ["Point", PointSchema as Schema],
  ["Color", ColorSchema as Schema],
  ["Shape", ShapeSchema as Schema],
  ["Message", MessageSchema as Schema],
  ["Hello", HelloSchema as Schema],
  ["MetadataValue", MetadataValueSchema as Schema],
  ["Node", NodeSchema as Schema],
]);

// ============================================================================
// Tests
// ============================================================================

describe("resolveSchema", () => {
  it("returns non-ref schemas unchanged", () => {
    const schema: Schema = { kind: "string" };
    expect(resolveSchema(schema, testRegistry)).toBe(schema);
  });

  it("resolves ref schemas", () => {
    const ref: RefSchema = { kind: "ref", name: "Point" };
    expect(resolveSchema(ref, testRegistry)).toBe(PointSchema);
  });

  it("throws on unknown ref", () => {
    const ref: RefSchema = { kind: "ref", name: "Unknown" };
    expect(() => resolveSchema(ref, testRegistry)).toThrow(/Unknown type ref/);
  });
});

describe("findVariantByDiscriminant", () => {
  it("finds variant by explicit discriminant", () => {
    const variant = findVariantByDiscriminant(MessageSchema, 0);
    expect(variant?.name).toBe("Hello");
  });

  it("finds variant by implicit discriminant (index)", () => {
    const variant = findVariantByDiscriminant(ColorSchema, 1);
    expect(variant?.name).toBe("Green");
  });

  it("handles sparse discriminants", () => {
    // Cancel has discriminant 4, not index 2
    const variant = findVariantByDiscriminant(MessageSchema, 4);
    expect(variant?.name).toBe("Cancel");
  });

  it("returns undefined for unknown discriminant", () => {
    const variant = findVariantByDiscriminant(MessageSchema, 99);
    expect(variant).toBeUndefined();
  });

  it("returns undefined for gap in sparse discriminants", () => {
    // Discriminants 2, 3 don't exist in MessageSchema
    expect(findVariantByDiscriminant(MessageSchema, 2)).toBeUndefined();
    expect(findVariantByDiscriminant(MessageSchema, 3)).toBeUndefined();
  });
});

describe("findVariantByName", () => {
  it("finds variant by name", () => {
    const variant = findVariantByName(ColorSchema, "Green");
    expect(variant?.name).toBe("Green");
  });

  it("returns undefined for unknown name", () => {
    const variant = findVariantByName(ColorSchema, "Yellow");
    expect(variant).toBeUndefined();
  });

  it("is case-sensitive", () => {
    const variant = findVariantByName(ColorSchema, "red");
    expect(variant).toBeUndefined();
  });
});

describe("getVariantDiscriminant", () => {
  it("returns explicit discriminant", () => {
    const variant = MessageSchema.variants[0]; // Hello with discriminant: 0
    expect(getVariantDiscriminant(MessageSchema, variant)).toBe(0);
  });

  it("returns explicit discriminant for sparse value", () => {
    const variant = MessageSchema.variants[2]; // Cancel with discriminant: 4
    expect(getVariantDiscriminant(MessageSchema, variant)).toBe(4);
  });

  it("returns index when no explicit discriminant", () => {
    const variant = ColorSchema.variants[1]; // Green at index 1
    expect(getVariantDiscriminant(ColorSchema, variant)).toBe(1);
  });

  it("throws for variant not in schema", () => {
    const foreignVariant: EnumVariant = { name: "Foreign", fields: null };
    expect(() => getVariantDiscriminant(ColorSchema, foreignVariant)).toThrow(
      /not found in schema/,
    );
  });
});

describe("getVariantFieldSchemas", () => {
  it("returns empty array for unit variant", () => {
    const variant = ColorSchema.variants[0]; // Red
    expect(getVariantFieldSchemas(variant)).toEqual([]);
  });

  it("returns single schema for newtype variant", () => {
    const variant = MetadataValueSchema.variants[0]; // String(String)
    const schemas = getVariantFieldSchemas(variant);
    expect(schemas).toHaveLength(1);
    expect(schemas[0]).toEqual({ kind: "string" });
  });

  it("returns array for tuple variant", () => {
    const variant = ShapeSchema.variants[1]; // Rectangle(f64, f64)
    const schemas = getVariantFieldSchemas(variant);
    expect(schemas).toHaveLength(2);
    expect(schemas[0]).toEqual({ kind: "f64" });
    expect(schemas[1]).toEqual({ kind: "f64" });
  });

  it("returns schemas in order for struct variant", () => {
    const variant = HelloSchema.variants[0]; // V1 { maxPayloadSize, initialChannelCredit }
    const schemas = getVariantFieldSchemas(variant);
    expect(schemas).toHaveLength(2);
    expect(schemas[0]).toEqual({ kind: "u32" });
    expect(schemas[1]).toEqual({ kind: "u32" });
  });
});

describe("getVariantFieldNames", () => {
  it("returns null for unit variant", () => {
    const variant = ColorSchema.variants[0]; // Red
    expect(getVariantFieldNames(variant)).toBeNull();
  });

  it("returns null for newtype variant", () => {
    const variant = MetadataValueSchema.variants[0]; // String(String)
    expect(getVariantFieldNames(variant)).toBeNull();
  });

  it("returns null for tuple variant", () => {
    const variant = ShapeSchema.variants[0]; // Circle(f64)
    expect(getVariantFieldNames(variant)).toBeNull();
  });

  it("returns field names for struct variant", () => {
    const variant = HelloSchema.variants[0]; // V1 { maxPayloadSize, initialChannelCredit }
    const names = getVariantFieldNames(variant);
    expect(names).toEqual(["maxPayloadSize", "initialChannelCredit"]);
  });

  it("returns field names for struct variant in Message", () => {
    const variant = MessageSchema.variants[1]; // Goodbye { reason }
    const names = getVariantFieldNames(variant);
    expect(names).toEqual(["reason"]);
  });
});

describe("isNewtypeVariant", () => {
  it("returns false for unit variant", () => {
    const variant = ColorSchema.variants[0]; // Red
    expect(isNewtypeVariant(variant)).toBe(false);
  });

  it("returns true for newtype variant", () => {
    const variant = MetadataValueSchema.variants[0]; // String(String)
    expect(isNewtypeVariant(variant)).toBe(true);
  });

  it("returns true for ref newtype variant", () => {
    const variant = MessageSchema.variants[0]; // Hello(Hello) - ref
    expect(isNewtypeVariant(variant)).toBe(true);
  });

  it("returns false for tuple variant", () => {
    const variant = ShapeSchema.variants[1]; // Rectangle(f64, f64)
    expect(isNewtypeVariant(variant)).toBe(false);
  });

  it("returns false for struct variant", () => {
    const variant = HelloSchema.variants[0]; // V1 { ... }
    expect(isNewtypeVariant(variant)).toBe(false);
  });
});

describe("isRefSchema", () => {
  it("returns true for ref schema", () => {
    const schema: Schema = { kind: "ref", name: "Point" };
    expect(isRefSchema(schema)).toBe(true);
  });

  it("returns false for primitive schema", () => {
    const schema: Schema = { kind: "string" };
    expect(isRefSchema(schema)).toBe(false);
  });

  it("returns false for struct schema", () => {
    expect(isRefSchema(PointSchema)).toBe(false);
  });

  it("returns false for enum schema", () => {
    expect(isRefSchema(ColorSchema)).toBe(false);
  });
});

describe("Schema type structure", () => {
  it("TupleSchema has correct structure", () => {
    expect(MetadataEntrySchema.kind).toBe("tuple");
    expect(MetadataEntrySchema.elements).toHaveLength(2);
    expect(MetadataEntrySchema.elements[0]).toEqual({ kind: "string" });
    expect(MetadataEntrySchema.elements[1]).toEqual({
      kind: "ref",
      name: "MetadataValue",
    });
  });

  it("EnumSchema with explicit discriminants has correct structure", () => {
    expect(MessageSchema.kind).toBe("enum");
    expect(MessageSchema.variants).toHaveLength(3);
    expect(MessageSchema.variants[0].discriminant).toBe(0);
    expect(MessageSchema.variants[1].discriminant).toBe(1);
    expect(MessageSchema.variants[2].discriminant).toBe(4);
  });

  it("circular type schema is valid", () => {
    expect(NodeSchema.kind).toBe("struct");
    expect(NodeSchema.fields.next).toEqual({
      kind: "option",
      inner: { kind: "ref", name: "Node" },
    });
    // Can resolve the ref
    const resolved = resolveSchema({ kind: "ref", name: "Node" }, testRegistry);
    expect(resolved).toBe(NodeSchema);
  });
});
