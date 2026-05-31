// Benchmarks: JIT vs interpreter, for both decode and encode, on representative
// payloads. Run with `vitest bench`. Pre-builds the plan / compiled functions so
// the measured loop is steady-state decode/encode; compile cost is separate.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { bench, describe } from "vitest";
import { Registry, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive, Value } from "@bearcove/phon-schema";
import { buildPlan, compileEncoder, decodeTyped, decodeWithPlan, encode } from "./index.ts";
import { compilePlan } from "./jit.ts";

interface VectorFile {
  schemas: string[];
  primitives: { id: string; tag: string }[];
  cases: { name: string; writer_root: string; writer_hex: string }[];
}
const corpus = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url)), "utf8"),
) as VectorFile;
const reg = new Registry(
  corpus.schemas.map((b) => schemaFromBytes(hexToBytes(b))),
  corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
);
const caseOf = (name: string) => corpus.cases.find((c) => c.name === name)!;
const rootOf = (name: string) => BigInt(`0x${caseOf(name).writer_root}`);

// A large list<u64> payload — element-loop heavy.
const listRoot = rootOf("list_u64");
const bigList: Value = Array.from({ length: 5000 }, (_, i) => BigInt(i) * 0x1_0000_0001n);
const listBytes = encode(bigList, listRoot, reg);
const listPlan = buildPlan(listRoot, listRoot, reg);
const listJit = compilePlan(listPlan, reg);
const listInterp = (b: Uint8Array) => decodeWithPlan(b, listPlan, reg);

// The mixed-alignment struct (8 fields) — per-field dispatch heavy.
const structRoot = rootOf("struct_mixed_align");
const structWire = hexToBytes(caseOf("struct_mixed_align").writer_hex);
const structPlan = buildPlan(structRoot, structRoot, reg);
const structJit = compilePlan(structPlan, reg);
const structInterp = (b: Uint8Array) => decodeWithPlan(b, structPlan, reg);
const structValue = structInterp(structWire);
const structEncJit = compileEncoder(structRoot, reg, { jit: true });

describe("decode list<u64> x5000", () => {
  bench("JIT", () => void listJit(listBytes));
  bench("interpreter", () => void listInterp(listBytes));
});

describe("decode struct (8 mixed fields)", () => {
  bench("JIT", () => void structJit(structWire));
  bench("interpreter", () => void structInterp(structWire));
  bench("JIT + typed remap", () => void decodeTyped(structWire, structRoot, structRoot, reg));
});

describe("encode struct (8 mixed fields)", () => {
  bench("JIT", () => void structEncJit(structValue));
  bench("interpreter", () => void encode(structValue, structRoot, reg));
});

describe("compile cost (cold decoder, uncached)", () => {
  bench("compilePlan", () => void compilePlan(structPlan, reg));
});
