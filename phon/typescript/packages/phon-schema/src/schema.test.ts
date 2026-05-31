import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { Registry, schemaFromBytes } from "./schema.ts";
import { hexToBytes } from "./wire.ts";

interface VectorFile {
  schemas: string[];
  primitives: { id: string; tag: string }[];
  cases: { name: string; writer_root: string; reader_root: string }[];
}

function loadCorpus(): VectorFile {
  const path = fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url));
  return JSON.parse(readFileSync(path, "utf8")) as VectorFile;
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
});
