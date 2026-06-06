// Cross-implementation compat conformance: replay the Rust-generated corpus
// (conformance/compat/vectors.json) through the TypeScript engine.
//
// For each case we build the writer->reader plan, decode the writer bytes with
// BOTH the interpreter and the new Function JIT, re-encode each result through
// the reader schema, and assert the bytes equal Rust's reader-shaped bytes
// (the oracle). Error cases assert both engines throw the expected error.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { Registry, bytesToHex, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Field, Primitive, Schema, SchemaRef } from "@bearcove/phon-schema";
import { buildPlan, compatDirection, compile, compileEncoder, decodeWithPlan, encode, IncompatibleError, WriterOnlyVariantError } from "./index.ts";
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

function primitiveRef(corpus: VectorFile, tag: Primitive): SchemaRef {
  const found = corpus.primitives.find((p) => p.tag === tag);
  if (!found) throw new Error(`missing primitive ${tag}`);
  return { kind: "concrete", id: BigInt(`0x${found.id}`), args: [] };
}

function schema(id: bigint, kind: Schema["kind"]): Schema {
  return { id, typeParams: [], kind };
}

function field(name: string, ref: SchemaRef, required: boolean): Field {
  return { name, schema: ref, required };
}

function localRegistry(corpus: VectorFile, schemas: Schema[]): Registry {
  const primitives = corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive }));
  return new Registry(schemas, primitives);
}

// r[verify compat.plan-first]
// r[verify compat.field-matching]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify compat.type-match]
// r[verify compat.enum]
// r[verify compact.schema-driven]
// r[verify compact.alignment]
// r[verify exec.interpreter-baseline]
// r[verify exec.jit-optional]
// r[verify crates.jit-opt-in]
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
      // must reproduce Rust's reader-shaped bytes.
      const interpValue = decodeWithPlan(writerBytes, plan, reg);
      const jitValue = jit(writerBytes);
      const optOutValue = compile(writerRoot, readerRoot, reg, { jit: false })(writerBytes);
      const optInValue = compile(writerRoot, readerRoot, reg, { jit: true })(writerBytes);

      const interpHex = bytesToHex(encode(interpValue, readerRoot, reg));
      const jitHex = bytesToHex(encode(jitValue, readerRoot, reg));
      const optOutHex = bytesToHex(encode(optOutValue, readerRoot, reg));
      const optInHex = bytesToHex(encode(optInValue, readerRoot, reg));
      // The encode JIT must also reproduce the reader-shaped bytes.
      const encJit = compileEncoder(readerRoot, reg, { jit: true });
      const encJitHex = bytesToHex(encJit(interpValue));
      const encInterp = compileEncoder(readerRoot, reg, { jit: false });
      const encInterpHex = bytesToHex(encInterp(interpValue));

      expect(interpHex).toBe(c.reader_hex);
      expect(jitHex).toBe(c.reader_hex);
      expect(optOutHex).toBe(c.reader_hex);
      expect(optInHex).toBe(c.reader_hex);
      expect(encJitHex).toBe(c.reader_hex);
      expect(encInterpHex).toBe(c.reader_hex);
    });
  }

  it("covers every corpus case", () => {
    expect(corpus.cases.length).toBe(33);
  });

  it("rejects required reader-only option fields", () => {
    const u32 = primitiveRef(corpus, "u32");
    const optionId = 0x7000_0000_0000_0001n;
    const writerId = 0x7000_0000_0000_0002n;
    const readerId = 0x7000_0000_0000_0003n;
    const schemas = [
      schema(optionId, { kind: "option", element: u32 }),
      schema(writerId, { kind: "struct", name: "P", fields: [field("x", u32, true)] }),
      schema(readerId, {
        kind: "struct",
        name: "P",
        fields: [
          field("x", u32, true),
          field("maybe", { kind: "concrete", id: optionId, args: [] }, true),
        ],
      }),
    ];

    expect(() => buildPlan(writerId, readerId, localRegistry(corpus, schemas))).toThrow(IncompatibleError);
  });

  // r[verify compat.direction]
  it("reports compat direction by planning both ways", () => {
    const u32 = primitiveRef(corpus, "u32");
    const u64 = primitiveRef(corpus, "u64");
    const oldId = 0x7000_0000_0000_0010n;
    const newOptionalId = 0x7000_0000_0000_0011n;
    const newRequiredId = 0x7000_0000_0000_0012n;
    const oldRequiredId = 0x7000_0000_0000_0013n;
    const newRemovedId = 0x7000_0000_0000_0014n;
    const differentId = 0x7000_0000_0000_0015n;
    const schemas = [
      schema(oldId, { kind: "struct", name: "P", fields: [field("x", u32, true)] }),
      schema(newOptionalId, {
        kind: "struct",
        name: "P",
        fields: [field("x", u32, true), field("y", u32, false)],
      }),
      schema(newRequiredId, {
        kind: "struct",
        name: "P",
        fields: [field("x", u32, true), field("y", u32, true)],
      }),
      schema(oldRequiredId, {
        kind: "struct",
        name: "P",
        fields: [field("x", u32, true), field("y", u32, true)],
      }),
      schema(newRemovedId, { kind: "struct", name: "P", fields: [field("x", u32, true)] }),
      schema(differentId, { kind: "struct", name: "P", fields: [field("x", u64, true)] }),
    ];
    const local = localRegistry(corpus, schemas);

    expect(compatDirection(oldId, newOptionalId, local)).toBe("bidirectional");
    expect(compatDirection(oldId, newRequiredId, local)).toBe("forward");
    expect(compatDirection(oldRequiredId, newRemovedId, local)).toBe("backward");
    expect(compatDirection(oldId, differentId, local)).toBe("incompatible");
  });
});
