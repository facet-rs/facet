// Benchmarks: JIT vs interpreter, for both decode and encode, on representative
// payloads. Run with `vitest bench`. Pre-builds the plan / compiled functions so
// the measured loop is steady-state decode/encode; compile cost is separate.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { bench, describe } from "vitest";
import { Registry, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive, Value } from "@bearcove/phon-schema";
import { buildPlan, compileEncoder, compileTypedDecoder, compileTypedEncoder, decodeTyped, decodeWithPlan, encode, encodeTyped } from "./index.ts";
import type { Typed } from "./index.ts";
import { compilePlan } from "./jit.ts";
import { REG as ecosystemReg, fixtures as ecosystemFixtures } from "./ecosystem_surface.fixtures.ts";

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

// A recursive rose-list payload — exercises callBlock codegen instead of the
// previous interpreter fallback.
const recursiveRoot = 0xdead_beefn;
const recursiveSchema = {
  id: recursiveRoot,
  typeParams: [] as string[],
  kind: { kind: "list" as const, element: { kind: "concrete" as const, id: recursiveRoot, args: [] } },
};
const recursiveReg = new Registry(
  [recursiveSchema],
  corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
);
function rose(depth: number, fanout: number): Value {
  if (depth === 0) return [];
  return Array.from({ length: fanout }, () => rose(depth - 1, fanout));
}
const recursiveValue = rose(5, 3);
const recursiveBytes = encode(recursiveValue, recursiveRoot, recursiveReg);
const recursivePlan = buildPlan(recursiveRoot, recursiveRoot, recursiveReg);
const recursiveJit = compilePlan(recursivePlan, recursiveReg);
const recursiveInterp = (b: Uint8Array) => decodeWithPlan(b, recursivePlan, recursiveReg);
const recursiveEncJit = compileEncoder(recursiveRoot, recursiveReg, { jit: true });

function ecosystemFixture(name: string) {
  const fixture = ecosystemFixtures.find((candidate) => candidate.name === name);
  if (!fixture) throw new Error(`missing ${name} fixture`);
  return fixture;
}

function byteRamp(length: number, seed: number): Uint8Array {
  return Uint8Array.from({ length }, (_, i) => (seed + i) & 0xff);
}

function dodecaDecodedImage(seed: number, width: number, height: number): Typed {
  return {
    pixels: byteRamp(width * height * 4, seed),
    width,
    height,
    channels: 4,
  };
}

function dodecaImageProcessorBenchmarkValue(): Typed {
  const decoded = dodecaDecodedImage(0x20, 96, 64) as {
    pixels: Uint8Array;
    width: number;
    height: number;
    channels: number;
  };
  const resized = dodecaDecodedImage(0x80, 48, 32);
  return {
    png_data: byteRamp(16_384, 0),
    decoded_result: { tag: "Success", image: decoded },
    resize_input: {
      pixels: decoded.pixels,
      width: decoded.width,
      height: decoded.height,
      channels: decoded.channels,
      target_width: 48,
    },
    resize_result: { tag: "Success", image: resized },
    thumbhash_input: {
      pixels: decoded.pixels,
      width: decoded.width,
      height: decoded.height,
    },
    thumbhash_result: { tag: "ThumbhashSuccess", data_url: "data:image/thumbhash;base64,BwgJCgsMDQ4PEA==" },
    error_result: { tag: "Error", message: "unsupported color profile in source image" },
  };
}

function dodecaSearchIndexerBenchmarkValue(): Typed {
  return {
    pages: Array.from({ length: 32 }, (_, i) => ({
      url: `/guide/topic-${i}/`,
      source: `content/guide/topic-${i}.md`,
      html: `<article><h1>Topic ${i}</h1><p>Search body ${i}</p></article>`,
    })),
    result: {
      tag: "Success",
      files: Array.from({ length: 8 }, (_, i) => ({
        path: `public/search/chunk-${i}.json`,
        contents: byteRamp(1_024, i * 17),
      })),
    },
    error_result: { tag: "Error", message: "search index could not write public/search/index.json" },
  };
}

function typedBenchFixture(name: string, value: Typed) {
  const fixture = ecosystemFixture(name);
  const bytes = encodeTyped(value, fixture.root, ecosystemReg);
  return {
    root: fixture.root,
    value,
    bytes,
    typedDecJit: compileTypedDecoder(fixture.root, fixture.root, ecosystemReg, { jit: true }),
    typedEncJit: compileTypedEncoder(fixture.root, ecosystemReg, { jit: true }),
  };
}

const dodecaImage = typedBenchFixture("Dodeca image processor roots", dodecaImageProcessorBenchmarkValue());
const dodecaSearch = typedBenchFixture("Dodeca search indexer roots", dodecaSearchIndexerBenchmarkValue());
const dodecaSmallCells = typedBenchFixture("Dodeca small-cell service roots", ecosystemFixture("Dodeca small-cell service roots").value);

// The broad Helix TraceService aggregate from the checked-in ecosystem corpus.
const helixTraceService = ecosystemFixture("Helix trace service surface");
const helixRoot = helixTraceService.root;
const helixTypedValue = helixTraceService.value;
const helixBytes = encodeTyped(helixTypedValue, helixRoot, ecosystemReg);
const helixPlan = buildPlan(helixRoot, helixRoot, ecosystemReg);
const helixJit = compilePlan(helixPlan, ecosystemReg);
const helixInterp = (b: Uint8Array) => decodeWithPlan(b, helixPlan, ecosystemReg);
const helixCoarseValue = helixInterp(helixBytes);
const helixEncJit = compileEncoder(helixRoot, ecosystemReg, { jit: true });
const helixTypedDecJit = compileTypedDecoder(helixRoot, helixRoot, ecosystemReg, { jit: true });
const helixTypedEncJit = compileTypedEncoder(helixRoot, ecosystemReg, { jit: true });

describe("decode list<u64> x5000", () => {
  bench("JIT", () => void listJit(listBytes));
  bench("interpreter", () => void listInterp(listBytes));
});

describe("decode recursive callBlock rose-list", () => {
  bench("JIT", () => void recursiveJit(recursiveBytes));
  bench("interpreter", () => void recursiveInterp(recursiveBytes));
});

describe("encode recursive callBlock rose-list", () => {
  bench("JIT", () => void recursiveEncJit(recursiveValue));
  bench("interpreter", () => void encode(recursiveValue, recursiveRoot, recursiveReg));
});

describe("decode Helix TraceService aggregate", () => {
  bench("coarse Value JIT", () => void helixJit(helixBytes));
  bench("coarse Value interpreter", () => void helixInterp(helixBytes));
  bench("typed fallback (Value interpreter + remap)", () => void decodeTyped(helixBytes, helixRoot, helixRoot, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void helixTypedDecJit(helixBytes));
});

describe("encode Helix TraceService aggregate", () => {
  bench("coarse Value JIT", () => void helixEncJit(helixCoarseValue));
  bench("coarse Value interpreter", () => void encode(helixCoarseValue, helixRoot, ecosystemReg));
  bench("typed fallback (remap + Value interpreter)", () => void encodeTyped(helixTypedValue, helixRoot, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void helixTypedEncJit(helixTypedValue));
});

describe("decode Dodeca image processor roots", () => {
  bench("typed fallback (Value interpreter + remap)", () => void decodeTyped(dodecaImage.bytes, dodecaImage.root, dodecaImage.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaImage.typedDecJit(dodecaImage.bytes));
});

describe("encode Dodeca image processor roots", () => {
  bench("typed fallback (remap + Value interpreter)", () => void encodeTyped(dodecaImage.value, dodecaImage.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaImage.typedEncJit(dodecaImage.value));
});

describe("decode Dodeca search indexer roots", () => {
  bench("typed fallback (Value interpreter + remap)", () => void decodeTyped(dodecaSearch.bytes, dodecaSearch.root, dodecaSearch.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaSearch.typedDecJit(dodecaSearch.bytes));
});

describe("encode Dodeca search indexer roots", () => {
  bench("typed fallback (remap + Value interpreter)", () => void encodeTyped(dodecaSearch.value, dodecaSearch.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaSearch.typedEncJit(dodecaSearch.value));
});

describe("decode Dodeca small-cell service roots", () => {
  bench("typed fallback (Value interpreter + remap)", () => void decodeTyped(dodecaSmallCells.bytes, dodecaSmallCells.root, dodecaSmallCells.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaSmallCells.typedDecJit(dodecaSmallCells.bytes));
});

describe("encode Dodeca small-cell service roots", () => {
  bench("typed fallback (remap + Value interpreter)", () => void encodeTyped(dodecaSmallCells.value, dodecaSmallCells.root, ecosystemReg, { jit: false }));
  bench("direct-shape typed JIT", () => void dodecaSmallCells.typedEncJit(dodecaSmallCells.value));
});

describe("decode struct (8 mixed fields)", () => {
  bench("JIT", () => void structJit(structWire));
  bench("interpreter", () => void structInterp(structWire));
  bench("direct-shape typed JIT", () => void decodeTyped(structWire, structRoot, structRoot, reg, { jit: true }));
});

describe("encode struct (8 mixed fields)", () => {
  bench("JIT", () => void structEncJit(structValue));
  bench("interpreter", () => void encode(structValue, structRoot, reg));
});

describe("compile cost (cold decoder, uncached)", () => {
  bench("compilePlan", () => void compilePlan(structPlan, reg));
});
