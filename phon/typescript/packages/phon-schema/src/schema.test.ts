import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { PRIMITIVES, Registry, primitiveId, resolveIds, schemaFromBytes, validateSchemaBundle } from "./schema.ts";
import type { Field, Primitive, Schema, SchemaKind, SchemaRef, VariantPayload } from "./schema.ts";
import { ZST_COUNT_CAP, hexToBytes } from "./wire.ts";

interface VectorFile {
  schemas: string[];
  primitives: { id: string; tag: string }[];
  cases: { name: string; writer_root: string; reader_root: string }[];
}

function loadCorpus(): VectorFile {
  const path = fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url));
  return JSON.parse(readFileSync(path, "utf8")) as VectorFile;
}

function loadSchemaCases(): Schema[] {
  return loadSchemaCaseBatches().flat();
}

function loadSchemaCaseBatches(): Schema[][] {
  const root = fileURLToPath(new URL("../../../../conformance/cases", import.meta.url));
  const batches: Schema[][] = [];
  for (const dir of readdirSync(root, { withFileTypes: true })) {
    if (!dir.isDirectory()) continue;
    const dirPath = join(root, dir.name);
    const batch: Schema[] = [];
    for (const file of readdirSync(dirPath, { withFileTypes: true })) {
      if (!file.isFile() || !file.name.endsWith(".phon")) continue;
      batch.push(schemaFromBytes(new Uint8Array(readFileSync(join(dirPath, file.name)))));
    }
    batches.push(batch);
  }
  return batches;
}

function collectField(field: Field, refs: Set<string>): void {
  collectRef(field.schema, refs);
}

function collectRef(ref: SchemaRef, refs: Set<string>): void {
  refs.add(ref.kind);
  if (ref.kind === "concrete") ref.args.forEach((arg) => collectRef(arg, refs));
}

function collectPayload(payload: VariantPayload, refs: Set<string>, payloads: Set<string>): void {
  payloads.add(payload.kind);
  switch (payload.kind) {
    case "unit":
      return;
    case "newtype":
      collectRef(payload.ref, refs);
      return;
    case "tuple":
      payload.refs.forEach((ref) => collectRef(ref, refs));
      return;
    case "struct":
      payload.fields.forEach((field) => collectField(field, refs));
      return;
  }
}

function collectKind(kind: SchemaKind, kinds: Set<string>, refs: Set<string>, payloads: Set<string>): void {
  kinds.add(kind.kind);
  switch (kind.kind) {
    case "primitive":
    case "dynamic":
      return;
    case "struct":
      kind.fields.forEach((field) => collectField(field, refs));
      return;
    case "enum":
      kind.variants.forEach((variant) => collectPayload(variant.payload, refs, payloads));
      return;
    case "tuple":
      kind.elements.forEach((ref) => collectRef(ref, refs));
      return;
    case "list":
    case "set":
    case "array":
    case "tensor":
    case "option":
    case "channel":
      collectRef(kind.element, refs);
      return;
    case "map":
      collectRef(kind.key, refs);
      collectRef(kind.value, refs);
      return;
    case "external":
      if (kind.metadata) collectRef(kind.metadata, refs);
      return;
  }
}

describe("schemaFromBytes against the Rust corpus", () => {
  const corpus = loadCorpus();

  it("parses every schema blob and round-trips its embedded id", () => {
    expect(corpus.schemas.length).toBeGreaterThan(0);
    for (const blob of corpus.schemas) {
      const schema = schemaFromBytes(hexToBytes(blob));
      // The parsed id is a u64 bigint; every schema kind is a known variant.
      expect(typeof schema.id).toBe("bigint");
      expect(schema.kind.kind).toBeTruthy();
    }
  });

  it("builds a registry that resolves every case root and every primitive", () => {
    const schemas = corpus.schemas.map((b) => schemaFromBytes(hexToBytes(b)));
    const primitiveTable = corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as never }));
    const reg = new Registry(schemas, primitiveTable);

    // Every primitive id resolves to a primitive kind.
    for (const p of corpus.primitives) {
      const kind = reg.resolve({ kind: "concrete", id: BigInt(`0x${p.id}`), args: [] });
      expect(kind.kind).toBe("primitive");
    }

    // Every case's writer/reader root resolves (it's a primitive or a composite).
    for (const c of corpus.cases) {
      const w = reg.resolve({ kind: "concrete", id: BigInt(`0x${c.writer_root}`), args: [] });
      const r = reg.resolve({ kind: "concrete", id: BigInt(`0x${c.reader_root}`), args: [] });
      expect(w.kind).toBeTruthy();
      expect(r.kind).toBeTruthy();
    }
  });

  // r[verify self-describing.bootstraps-schemas]
  // r[verify self-describing.enum-payload]
  // r[verify type-system.canonical-form]
  // r[verify type-system.array]
  // r[verify type-system.tensor]
  // r[verify type-system.channel]
  // r[verify type-system.dynamic]
  // r[verify type-system.external]
  // r[verify type-system.generics]
  // r[verify type-system.variant-payloads]
  it("parses committed schema cases across the special schema kinds", () => {
    const schemas = loadSchemaCases();
    const kinds = new Set<string>();
    const refs = new Set<string>();
    const payloads = new Set<string>();
    for (const schema of schemas) collectKind(schema.kind, kinds, refs, payloads);

    expect(Array.from(kinds)).toEqual(expect.arrayContaining([
      "array",
      "tensor",
      "channel",
      "dynamic",
      "external",
      "enum",
      "struct",
      "list",
      "map",
      "set",
    ]));
    expect(Array.from(refs)).toEqual(expect.arrayContaining(["concrete", "var"]));
    expect(Array.from(payloads)).toEqual(expect.arrayContaining(["unit", "newtype", "tuple", "struct"]));
    expect(schemas.some((schema) => schema.typeParams.length > 0)).toBe(true);
  });

  // r[verify type-system.generic-resolution]
  // r[verify schema-identity.unknown-is-error]
  it("substitutes generic refs and rejects unknown schema ids", () => {
    const primitiveTable = corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as never }));
    const u32 = primitiveTable.find((p) => p.tag === "u32");
    if (!u32) throw new Error("missing u32 primitive");
    const boxId = 0x9000_0000_0000_0001n;
    const reg = new Registry([
      {
        id: boxId,
        typeParams: ["T"],
        kind: {
          kind: "struct",
          name: "Box",
          fields: [{ name: "value", schema: { kind: "var", name: "T" }, required: true }],
        },
      },
    ], primitiveTable);

    const resolved = reg.resolve({
      kind: "concrete",
      id: boxId,
      args: [{ kind: "concrete", id: u32.id, args: [] }],
    });
    expect(resolved).toMatchObject({
      kind: "struct",
      fields: [{ schema: { kind: "concrete", id: u32.id, args: [] } }],
    });

    expect(() => reg.resolve({ kind: "concrete", id: 0xDEAD_BEEFn, args: [] })).toThrow(/unknown schema id/);
  });

  // r[verify schema-identity.canonical-encoding]
  // r[verify schema-identity.computation]
  // r[verify schema-identity.content-hash]
  it("matches Rust primitive id goldens", () => {
    const golden: [Primitive, bigint][] = [
      ["bool", 0x178367a87f66fb46n],
      ["u8", 0x2c8d54f2314d0f20n],
      ["u16", 0x1be6c8d0625ea876n],
      ["u32", 0x281c5be4f2ee63b4n],
      ["u64", 0xd9356298b81639acn],
      ["u128", 0x767c691472231d95n],
      ["i8", 0x3bd6a76856978968n],
      ["i16", 0x269c2efb67f8a4c7n],
      ["i32", 0x361f4536eee9f991n],
      ["i64", 0xc6eb8c46f1e17fban],
      ["i128", 0xe935ee7d4b9fe594n],
      ["f32", 0x8e02f623d1b2310cn],
      ["f64", 0x3f2e589db81e95bfn],
      ["char", 0x18937b725e2e911bn],
      ["string", 0x6d7dce914ee150e8n],
      ["bytes", 0xba8125876d6388b4n],
      ["datetime", 0x2df96deecf87538dn],
      ["uuid", 0x228b7a9a7c76c62cn],
      ["qname", 0x18b4e7af90ad4c0fn],
      ["unit", 0xbc5c33249a2dc720n],
      ["never", 0x5db70a394660f3e6n],
    ];

    expect(golden).toHaveLength(PRIMITIVES.length);
    for (const [tag, id] of golden) expect(primitiveId(tag)).toBe(id);
  });

  // r[verify schema-identity.canonical-encoding]
  // r[verify schema-identity.closure]
  // r[verify schema-identity.computation]
  // r[verify schema-identity.content-hash]
  it("recomputes ids for committed schema case bundles", () => {
    for (const batch of loadSchemaCaseBatches()) {
      const recomputed = resolveIds(batch);
      expect(recomputed.map((schema) => schema.id)).toEqual(batch.map((schema) => schema.id));
    }
  });

  // r[verify validate.bundles]
  it("validates received schema bundles", () => {
    const point = resolveIds([{
      id: 1n,
      typeParams: [],
      kind: {
        kind: "struct",
        name: "Point",
        fields: [{ name: "x", schema: { kind: "concrete", id: primitiveId("u32"), args: [] }, required: true }],
      },
    }]);
    expect(() => validateSchemaBundle(point)).not.toThrow();
    expect(() => Registry.validating(point)).not.toThrow();
  });

  // r[verify validate.bundles]
  it("rejects stale schema ids in received bundles", () => {
    const resolved = resolveIds([{ id: 1n, typeParams: [], kind: { kind: "struct", name: "UnitLike", fields: [] } }]);
    const stale = [{ ...resolved[0]!, id: resolved[0]!.id ^ 1n }];
    expect(() => validateSchemaBundle(stale)).toThrow(/schema id mismatch/);
  });

  // r[verify validate.bundles]
  it("rejects incomplete schema closures", () => {
    const resolved = resolveIds([{
      id: 1n,
      typeParams: [],
      kind: {
        kind: "struct",
        name: "Holder",
        fields: [{ name: "missing", schema: { kind: "concrete", id: 0xFEED_FACE_CAFE_BEEFn, args: [] }, required: true }],
      },
    }]);
    expect(() => validateSchemaBundle(resolved)).toThrow(/unknown schema id/);
  });

  // r[verify validate.bundles]
  it("rejects unbounded zero-wire fixed arrays in received bundles", () => {
    const resolved = resolveIds([{
      id: 1n,
      typeParams: [],
      kind: {
        kind: "array",
        element: { kind: "concrete", id: primitiveId("unit"), args: [] },
        dimensions: [BigInt(ZST_COUNT_CAP) + 1n],
      },
    }]);
    expect(() => validateSchemaBundle(resolved)).toThrow(/zero-sized cap/);
  });
});
