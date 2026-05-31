// Hostile-input conformance: a crafted message must never crash the decoder or
// drive an unbounded allocation — it must become a DecodeError. Every guard
// (truncation, trailing bytes, oversized counts, bad bool/presence bytes, bad
// UTF-8, duplicate set/map entries, writer-only enum variants, bad variant
// indices) is exercised against BOTH the interpreter and the JIT, asserting they
// reject identically. The corpus (well-formed) covers the happy path; this
// covers the adversarial one.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { ByteSink, DecodeError, Registry, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive } from "@bearcove/phon-schema";
import { compile, decode, decodeCompact, WriterOnlyVariantError } from "./index.ts";

interface VectorFile {
  schemas: string[];
  primitives: { id: string; tag: string }[];
  cases: { name: string; writer_root: string }[];
}

const corpus = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url)), "utf8"),
) as VectorFile;

const reg = new Registry(
  corpus.schemas.map((b) => schemaFromBytes(hexToBytes(b))),
  corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
);

/// The root id of the schema a named corpus case writes against.
function rootOf(name: string): bigint {
  const c = corpus.cases.find((x) => x.name === name);
  if (!c) throw new Error(`no corpus case ${name}`);
  return BigInt(`0x${c.writer_root}`);
}

function bytes(build: (s: ByteSink) => void): Uint8Array {
  const s = new ByteSink();
  build(s);
  return s.finish();
}

// Each guard, with the malformed bytes that should trip it on `root`.
const malformed: { name: string; root: bigint; bytes: Uint8Array; want?: RegExp }[] = [
  { name: "truncated scalar", root: rootOf("scalar_u32"), bytes: new Uint8Array([1, 2]), want: /unexpected end of input/ },
  { name: "trailing bytes", root: rootOf("scalar_u32"), bytes: bytes((s) => { s.u32(5); s.u8(0); }), want: /trailing/ },
  { name: "oversized list count", root: rootOf("list_u64"), bytes: bytes((s) => s.u32(0xffffffff)), want: /exceeds/ },
  { name: "bad option presence byte", root: rootOf("option_some"), bytes: new Uint8Array([2]), want: /invalid bool byte/ },
  { name: "duplicate set element", root: rootOf("set_u32"), bytes: bytes((s) => { s.u32(2); s.u32(7); s.u32(7); }), want: /duplicate set element/ },
  {
    name: "duplicate map key",
    root: rootOf("map_string_u32"),
    bytes: bytes((s) => { s.u32(2); s.str("a"); s.padTo(4); s.u32(1); s.str("a"); s.padTo(4); s.u32(2); }),
    want: /duplicate map key/,
  },
  {
    name: "invalid UTF-8 in string",
    root: rootOf("string_and_bytes"),
    bytes: bytes((s) => { s.u32(1); s.u8(0xff); }),
    want: /invalid UTF-8/,
  },
];

describe("hostile input — interpreter and JIT reject identically", () => {
  for (const m of malformed) {
    it(m.name, () => {
      // Interpreter.
      let interpErr: unknown;
      try {
        decode(m.bytes, m.root, m.root, reg);
        throw new Error("interpreter did not reject");
      } catch (e) {
        interpErr = e;
      }
      expect(interpErr).toBeInstanceOf(DecodeError);
      if (m.want) expect((interpErr as Error).message).toMatch(m.want);

      // JIT.
      const jit = compile(m.root, m.root, reg, { jit: true });
      let jitErr: unknown;
      try {
        jit(m.bytes);
        throw new Error("JIT did not reject");
      } catch (e) {
        jitErr = e;
      }
      expect(jitErr).toBeInstanceOf(DecodeError);
      if (m.want) expect((jitErr as Error).message).toMatch(m.want);
    });
  }

  it("writer-only enum variant — both throw WriterOnlyVariantError", () => {
    const root = rootOf("enum_same");
    const idx99 = bytes((s) => s.u32(99));
    expect(() => decode(idx99, root, root, reg)).toThrow(WriterOnlyVariantError);
    expect(() => compile(root, root, reg, { jit: true })(idx99)).toThrow(WriterOnlyVariantError);
  });

  it("same-schema decode rejects an unknown variant index", () => {
    const root = rootOf("enum_same");
    const idx99 = bytes((s) => s.u32(99));
    expect(() => decodeCompact(idx99, root, reg)).toThrow(/bad variant index/);
  });

  it("a self-recursive schema is rejected at plan time, not at decode", () => {
    // A list whose element refers back to itself: planning recurses until the
    // depth guard fires (mirrors Rust plan_ref's MAX_DEPTH). Both engines route
    // through buildPlan, so neither can be built — no infinite loop, no crash.
    const selfId = 0xdead_beefn;
    const recursive = {
      id: selfId,
      typeParams: [] as string[],
      kind: { kind: "list" as const, element: { kind: "concrete" as const, id: selfId, args: [] } },
    };
    const recReg = new Registry(
      [recursive],
      corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
    );
    expect(() => compile(selfId, selfId, recReg)).toThrow(/nests too deep/);
  });
});
