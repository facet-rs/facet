// Tests for schema-driven encoding/decoding

import { describe, it, expect } from "vitest";
import { encodeWithSchema, decodeWithSchema } from "./schema_codec.ts";
import type { Schema, SchemaRegistry, EnumSchema, StructSchema, TupleSchema } from "./schema.ts";

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

// Wire-like enum with explicit discriminants
const MetadataValueSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "String", discriminant: 0, fields: { kind: "string" } },
    { name: "Bytes", discriminant: 1, fields: { kind: "bytes" } },
    { name: "U64", discriminant: 2, fields: { kind: "u64" } },
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

// Message-like enum with nested refs
const MessageSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "Hello", discriminant: 0, fields: { kind: "ref", name: "Hello" } },
    { name: "Goodbye", discriminant: 1, fields: { reason: { kind: "string" } } },
    {
      name: "Request",
      discriminant: 2,
      fields: {
        requestId: { kind: "u64" },
        methodId: { kind: "u64" },
        payload: { kind: "bytes" },
      },
    },
  ],
};

const MetadataEntrySchema: TupleSchema = {
  kind: "tuple",
  elements: [{ kind: "string" }, { kind: "ref", name: "MetadataValue" }],
};

// Registry for resolving refs
const testRegistry: SchemaRegistry = new Map<string, Schema>([
  ["Point", PointSchema as Schema],
  ["Color", ColorSchema as Schema],
  ["MetadataValue", MetadataValueSchema as Schema],
  ["Hello", HelloSchema as Schema],
  ["Message", MessageSchema as Schema],
  ["MetadataEntry", MetadataEntrySchema as Schema],
]);

// ============================================================================
// Primitive Tests
// ============================================================================

describe("encodeWithSchema/decodeWithSchema primitives", () => {
  it("roundtrips bool", () => {
    const schema: Schema = { kind: "bool" };
    for (const value of [true, false]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
      expect(decoded.next).toBe(encoded.length);
    }
  });

  it("roundtrips u8", () => {
    const schema: Schema = { kind: "u8" };
    for (const value of [0, 127, 255]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips i8", () => {
    const schema: Schema = { kind: "i8" };
    for (const value of [-128, 0, 127]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips u16", () => {
    const schema: Schema = { kind: "u16" };
    for (const value of [0, 1000, 65535]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips i32", () => {
    const schema: Schema = { kind: "i32" };
    for (const value of [-1000000, 0, 1000000]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips u64", () => {
    const schema: Schema = { kind: "u64" };
    for (const value of [0n, 1000000n, 0xffffffffffffffffn]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips i64", () => {
    const schema: Schema = { kind: "i64" };
    for (const value of [-9223372036854775808n, 0n, 9223372036854775807n]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips f32", () => {
    const schema: Schema = { kind: "f32" };
    const value = 3.14;
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBeCloseTo(value, 5);
  });

  it("roundtrips f64", () => {
    const schema: Schema = { kind: "f64" };
    const value = Math.PI;
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBe(value);
  });

  it("roundtrips string", () => {
    const schema: Schema = { kind: "string" };
    for (const value of ["", "hello", "こんにちは", "🎉"]) {
      const encoded = encodeWithSchema(value, schema);
      const decoded = decodeWithSchema(encoded, 0, schema);
      expect(decoded.value).toBe(value);
    }
  });

  it("roundtrips bytes", () => {
    const schema: Schema = { kind: "bytes" };
    const value = new Uint8Array([1, 2, 3, 4, 5]);
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });
});

// ============================================================================
// Container Tests
// ============================================================================

describe("encodeWithSchema/decodeWithSchema containers", () => {
  it("roundtrips vec of primitives", () => {
    const schema: Schema = { kind: "vec", element: { kind: "i32" } };
    const value = [1, 2, 3, 4, 5];
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips empty vec", () => {
    const schema: Schema = { kind: "vec", element: { kind: "string" } };
    const value: string[] = [];
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips option Some", () => {
    const schema: Schema = { kind: "option", inner: { kind: "string" } };
    const value = "hello";
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBe(value);
  });

  it("roundtrips option None (null)", () => {
    const schema: Schema = { kind: "option", inner: { kind: "string" } };
    const encoded = encodeWithSchema(null, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBeNull();
  });

  it("roundtrips option None (undefined)", () => {
    const schema: Schema = { kind: "option", inner: { kind: "string" } };
    const encoded = encodeWithSchema(undefined, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBeNull();
  });

  it("roundtrips map", () => {
    const schema: Schema = { kind: "map", key: { kind: "string" }, value: { kind: "i32" } };
    const value = new Map([
      ["a", 1],
      ["b", 2],
      ["c", 3],
    ]);
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });
});

// ============================================================================
// Composite Tests
// ============================================================================

describe("encodeWithSchema/decodeWithSchema composites", () => {
  it("roundtrips struct", () => {
    const value = { x: 10, y: 20 };
    const encoded = encodeWithSchema(value, PointSchema);
    const decoded = decodeWithSchema(encoded, 0, PointSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips tuple", () => {
    const schema: TupleSchema = {
      kind: "tuple",
      elements: [{ kind: "string" }, { kind: "i32" }],
    };
    const value = ["hello", 42];
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips unit enum variant", () => {
    const value = { tag: "Red" };
    const encoded = encodeWithSchema(value, ColorSchema);
    const decoded = decodeWithSchema(encoded, 0, ColorSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips different unit enum variants", () => {
    for (const tag of ["Red", "Green", "Blue"]) {
      const value = { tag };
      const encoded = encodeWithSchema(value, ColorSchema);
      const decoded = decodeWithSchema(encoded, 0, ColorSchema);
      expect(decoded.value).toEqual(value);
    }
  });

  it("roundtrips newtype enum variant", () => {
    const value = { tag: "String", value: "hello world" };
    const encoded = encodeWithSchema(value, MetadataValueSchema);
    const decoded = decodeWithSchema(encoded, 0, MetadataValueSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips bytes newtype variant", () => {
    const value = { tag: "Bytes", value: new Uint8Array([1, 2, 3]) };
    const encoded = encodeWithSchema(value, MetadataValueSchema);
    const decoded = decodeWithSchema(encoded, 0, MetadataValueSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips u64 newtype variant", () => {
    const value = { tag: "U64", value: 12345n };
    const encoded = encodeWithSchema(value, MetadataValueSchema);
    const decoded = decodeWithSchema(encoded, 0, MetadataValueSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips struct enum variant", () => {
    const value = { tag: "V1", maxPayloadSize: 65536, initialChannelCredit: 1024 };
    const encoded = encodeWithSchema(value, HelloSchema);
    const decoded = decodeWithSchema(encoded, 0, HelloSchema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips nested struct", () => {
    const schema: StructSchema = {
      kind: "struct",
      fields: {
        topLeft: PointSchema,
        bottomRight: PointSchema,
      },
    };
    const value = {
      topLeft: { x: 0, y: 0 },
      bottomRight: { x: 100, y: 100 },
    };
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips vec of structs", () => {
    const schema: Schema = { kind: "vec", element: PointSchema };
    const value = [
      { x: 1, y: 2 },
      { x: 3, y: 4 },
      { x: 5, y: 6 },
    ];
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips vec of enums", () => {
    const schema: Schema = { kind: "vec", element: ColorSchema };
    const value = [{ tag: "Red" }, { tag: "Green" }, { tag: "Blue" }];
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(value);
  });
});

// ============================================================================
// Registry/Ref Tests
// ============================================================================

describe("encodeWithSchema/decodeWithSchema with registry", () => {
  it("resolves ref for struct", () => {
    const schema: Schema = { kind: "ref", name: "Point" };
    const value = { x: 42, y: 24 };
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("resolves ref for enum", () => {
    const schema: Schema = { kind: "ref", name: "MetadataValue" };
    const value = { tag: "String", value: "test" };
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("resolves nested refs in enum variant", () => {
    const schema: Schema = { kind: "ref", name: "Message" };
    const value = {
      tag: "Hello",
      value: { tag: "V1", maxPayloadSize: 65536, initialChannelCredit: 1024 },
    };
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("resolves refs in tuple", () => {
    const schema: Schema = { kind: "ref", name: "MetadataEntry" };
    const value = ["key", { tag: "U64", value: 100n }];
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("throws on unresolved ref without registry", () => {
    const schema: Schema = { kind: "ref", name: "Point" };
    expect(() => encodeWithSchema({ x: 1, y: 2 }, schema)).toThrow(/Unresolved ref/);
  });

  it("throws on unknown ref", () => {
    const schema: Schema = { kind: "ref", name: "Unknown" };
    expect(() => encodeWithSchema({}, schema, testRegistry)).toThrow(/Unknown type ref/);
  });
});

// ============================================================================
// Wire Protocol Tests (Hello, Message)
// ============================================================================

describe("wire protocol types", () => {
  it("encodes Hello.V1 matching Rust format", () => {
    const value = { tag: "V1", maxPayloadSize: 65536, initialChannelCredit: 1024 };
    const encoded = encodeWithSchema(value, HelloSchema);

    // Expected: varint(0) for V1 discriminant, varint(65536), varint(1024)
    // 0 = 0x00
    // 65536 = 0x80 0x80 0x04
    // 1024 = 0x80 0x08
    expect(encoded[0]).toBe(0); // discriminant
  });

  it("roundtrips Message.Goodbye", () => {
    const schema: Schema = { kind: "ref", name: "Message" };
    const value = { tag: "Goodbye", reason: "shutting down" };
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips Message.Request", () => {
    const schema: Schema = { kind: "ref", name: "Message" };
    const value = {
      tag: "Request",
      requestId: 1n,
      methodId: 0x123456789abcdef0n,
      payload: new Uint8Array([1, 2, 3, 4]),
    };
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips vec of metadata entries", () => {
    const schema: Schema = {
      kind: "vec",
      element: { kind: "ref", name: "MetadataEntry" },
    };
    const value = [
      ["content-type", { tag: "String", value: "application/json" }],
      ["length", { tag: "U64", value: 1024n }],
    ];
    const encoded = encodeWithSchema(value, schema, testRegistry);
    const decoded = decodeWithSchema(encoded, 0, schema, testRegistry);
    expect(decoded.value).toEqual(value);
  });
});

// ============================================================================
// Streaming Type Tests
// ============================================================================

describe("streaming types (tx/rx)", () => {
  it("encodes tx as channel id", () => {
    const schema: Schema = { kind: "tx", element: { kind: "i32" } };
    const value = { channelId: 42n };
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect((decoded.value as { channelId: bigint }).channelId).toBe(42n);
  });

  it("encodes rx as channel id", () => {
    const schema: Schema = { kind: "rx", element: { kind: "string" } };
    const value = { channelId: 123n };
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect((decoded.value as { channelId: bigint }).channelId).toBe(123n);
  });
});

// ============================================================================
// Edge Cases
// ============================================================================

describe("edge cases", () => {
  it("handles empty string", () => {
    const schema: Schema = { kind: "string" };
    const encoded = encodeWithSchema("", schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBe("");
  });

  it("handles empty bytes", () => {
    const schema: Schema = { kind: "bytes" };
    const encoded = encodeWithSchema(new Uint8Array([]), schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toEqual(new Uint8Array([]));
  });

  it("handles deeply nested option", () => {
    const schema: Schema = {
      kind: "option",
      inner: { kind: "option", inner: { kind: "option", inner: { kind: "i32" } } },
    };
    const value = 42;
    const encoded = encodeWithSchema(value, schema);
    const decoded = decodeWithSchema(encoded, 0, schema);
    expect(decoded.value).toBe(value);
  });

  it("handles tuple with length mismatch error", () => {
    const schema: TupleSchema = {
      kind: "tuple",
      elements: [{ kind: "i32" }, { kind: "i32" }],
    };
    expect(() => encodeWithSchema([1], schema)).toThrow(/Tuple length mismatch/);
  });

  it("throws on unknown variant name", () => {
    expect(() => encodeWithSchema({ tag: "Unknown" }, ColorSchema)).toThrow(/Unknown variant/);
  });

  it("decodes at non-zero offset", () => {
    const schema: Schema = { kind: "i32" };
    const prefix = new Uint8Array([0xff, 0xff]); // 2 bytes of garbage
    const encoded = encodeWithSchema(42, schema);
    const combined = new Uint8Array([...prefix, ...encoded]);
    const decoded = decodeWithSchema(combined, 2, schema);
    expect(decoded.value).toBe(42);
  });
});
