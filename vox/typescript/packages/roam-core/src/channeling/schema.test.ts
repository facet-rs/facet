// Tests to verify schema types are correctly re-exported from roam-postcard

import { describe, it, expect } from "vitest";
import {
  type Schema,
  type EnumSchema,
  type StructSchema,
  type TupleSchema,
  type RefSchema,
  type SchemaRegistry,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  isRefSchema,
  resolveSchema,
} from "./schema.ts";

describe("Schema re-exports", () => {
  it("exports EnumSchema type correctly", () => {
    const schema: EnumSchema = {
      kind: "enum",
      variants: [
        { name: "A", fields: null },
        { name: "B", discriminant: 5, fields: { kind: "string" } },
      ],
    };
    expect(schema.kind).toBe("enum");
    expect(schema.variants).toHaveLength(2);
  });

  it("exports TupleSchema type correctly", () => {
    const schema: TupleSchema = {
      kind: "tuple",
      elements: [{ kind: "string" }, { kind: "u32" }],
    };
    expect(schema.kind).toBe("tuple");
    expect(schema.elements).toHaveLength(2);
  });

  it("exports RefSchema type correctly", () => {
    const schema: RefSchema = {
      kind: "ref",
      name: "MyType",
    };
    expect(schema.kind).toBe("ref");
    expect(schema.name).toBe("MyType");
  });

  it("exports helper functions", () => {
    // Just verify the functions are exported and callable
    expect(typeof findVariantByDiscriminant).toBe("function");
    expect(typeof findVariantByName).toBe("function");
    expect(typeof getVariantDiscriminant).toBe("function");
    expect(typeof getVariantFieldSchemas).toBe("function");
    expect(typeof getVariantFieldNames).toBe("function");
    expect(typeof isNewtypeVariant).toBe("function");
    expect(typeof isRefSchema).toBe("function");
    expect(typeof resolveSchema).toBe("function");
  });

  it("isRefSchema works on re-exported function", () => {
    expect(isRefSchema({ kind: "ref", name: "Test" })).toBe(true);
    expect(isRefSchema({ kind: "string" })).toBe(false);
  });

  it("resolveSchema works with registry", () => {
    const pointSchema: StructSchema = {
      kind: "struct",
      fields: { x: { kind: "i32" }, y: { kind: "i32" } },
    };
    const registry: SchemaRegistry = new Map<string, Schema>([["Point", pointSchema as Schema]]);

    const resolved = resolveSchema({ kind: "ref", name: "Point" }, registry);
    expect(resolved).toBe(pointSchema);
  });
});
