import { describe, expect, it } from "vitest";
import type { EnumSchema, Schema, SchemaRegistry, StructSchema } from "./schema.ts";
import {
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldNames,
  getVariantFieldSchemas,
  isNewtypeVariant,
  isRefSchema,
  resolveSchema,
} from "./schema.ts";

const PayloadSchema: StructSchema = {
  kind: "struct",
  fields: {
    message: { kind: "string" },
    next: { kind: "option", inner: { kind: "ref", name: "Payload" } },
  },
};

const EventSchema: EnumSchema = {
  kind: "enum",
  variants: [
    {
      name: "Started",
      fields: { kind: "ref", name: "Payload" },
    },
    {
      name: "Progress",
      discriminant: 7,
      fields: {
        current: { kind: "u32" },
        total: { kind: "u32" },
      },
    },
    {
      name: "Chunk",
      fields: [{ kind: "bytes" }, { kind: "u32" }],
    },
  ],
};

const registry: SchemaRegistry = new Map<string, Schema>([
  ["Payload", PayloadSchema],
  ["Event", EventSchema],
]);

describe("channeling schema compatibility", () => {
  it("resolves the top-level ref without eagerly resolving nested refs", () => {
    const resolved = resolveSchema({ kind: "ref", name: "Payload" }, registry);

    expect(resolved).toBe(PayloadSchema);
    expect((resolved as StructSchema).fields.next).toEqual({
      kind: "option",
      inner: { kind: "ref", name: "Payload" },
    });
  });

  it("provides enum metadata used by the channel binder for named variants", () => {
    const progress = findVariantByName(EventSchema, "Progress");

    expect(progress).toBeDefined();
    expect(isNewtypeVariant(progress!)).toBe(false);
    expect(getVariantFieldNames(progress!)).toEqual(["current", "total"]);
    expect(getVariantFieldSchemas(progress!)).toEqual([{ kind: "u32" }, { kind: "u32" }]);
  });

  it("recognizes newtype variants and sparse discriminants", () => {
    const started = findVariantByDiscriminant(EventSchema, 0);
    const progress = findVariantByDiscriminant(EventSchema, 7);

    expect(started?.name).toBe("Started");
    expect(progress?.name).toBe("Progress");
    expect(isNewtypeVariant(started!)).toBe(true);
    expect(getVariantDiscriminant(EventSchema, progress!)).toBe(7);
    expect(getVariantFieldSchemas(started!)).toEqual([{ kind: "ref", name: "Payload" }]);
  });

  it("exposes ref detection for runtime schema walking", () => {
    expect(isRefSchema({ kind: "ref", name: "Payload" })).toBe(true);
    expect(isRefSchema(PayloadSchema)).toBe(false);
  });
});
