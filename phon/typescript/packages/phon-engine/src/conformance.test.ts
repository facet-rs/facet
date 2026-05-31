// Cross-implementation compat conformance: replay the Rust-generated corpus
// (conformance/compat/vectors.json) through the TypeScript engine.
//
// For each case we build the writer->reader plan, decode the writer bytes with
// BOTH the interpreter and the new Function JIT, re-encode each result through
// the reader schema, and assert the bytes equal Rust's reconciled reader bytes
// (the oracle). Error cases assert both engines throw the expected error.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { Registry, bytesToHex, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive } from "@bearcove/phon-schema";
import { buildPlan, decodeWithPlan, encode, WriterOnlyVariantError } from "./index.ts";
import { compilePlan } from "./jit.ts";

interface Case {
  name: string;
  writer_root: string;
  reader_root: string;
  writer_hex: string;
  reader_hex: string | null;
  error_kind: string | null;
}
interface VectorFile {
  schemas: string[];
  primitives: { id: string; tag: string }[];
  cases: Case[];
}

function loadCorpus(): VectorFile {
  const path = fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url));
  return JSON.parse(readFileSync(path, "utf8")) as VectorFile;
}

function buildRegistry(corpus: VectorFile): Registry {
  const schemas = corpus.schemas.map((b) => schemaFromBytes(hexToBytes(b)));
  const primitives = corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive }));
  return new Registry(schemas, primitives);
}

describe("compat conformance corpus", () => {
  const corpus = loadCorpus();
  const reg = buildRegistry(corpus);

  for (const c of corpus.cases) {
    it(c.name, () => {
      const writerRoot = BigInt(`0x${c.writer_root}`);
      const readerRoot = BigInt(`0x${c.reader_root}`);
      const writerBytes = hexToBytes(c.writer_hex);
      const plan = buildPlan(writerRoot, readerRoot, reg);
      const jit = compilePlan(plan, reg);

      if (c.error_kind !== null) {
        // Both engines must reject, with the expected error.
        expect(() => decodeWithPlan(writerBytes, plan, reg)).toThrow();
        expect(() => jit(writerBytes)).toThrow();
        if (c.error_kind === "WriterOnlyVariant") {
          expect(() => decodeWithPlan(writerBytes, plan, reg)).toThrow(WriterOnlyVariantError);
          expect(() => jit(writerBytes)).toThrow(WriterOnlyVariantError);
        }
        return;
      }

      // Interpreter and JIT both decode; re-encoding through the reader schema
      // must reproduce Rust's reconciled reader bytes.
      const interpValue = decodeWithPlan(writerBytes, plan, reg);
      const jitValue = jit(writerBytes);

      const interpHex = bytesToHex(encode(interpValue, readerRoot, reg));
      const jitHex = bytesToHex(encode(jitValue, readerRoot, reg));

      expect(interpHex).toBe(c.reader_hex);
      expect(jitHex).toBe(c.reader_hex);
    });
  }

  it("covers every corpus case", () => {
    expect(corpus.cases.length).toBe(23);
  });
});
