// The typed front door against the Rust oracle: for every well-formed corpus
// case, decode the writer bytes into an ergonomic typed value shaped by the
// reader schema, re-encode it through the reader schema, and assert the bytes
// equal Rust's reader-shaped bytes. This proves the public typed shape is
// information-preserving: a vox TS peer can decode to ergonomic objects and
// re-encode them losslessly, with the JIT-enabled path constructing and
// consuming that shape directly.
//
// A few cases also assert the concrete ergonomic shape (numbers not bigints,
// plain objects not Maps, {tag,value} enums, canonical strings).

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { Registry, bytesToHex, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive, Schema, SchemaRef } from "@bearcove/phon-schema";
import { buildPlan, compiledTypedDecoderSource, compiledTypedEncoderSource, decodeTyped, encodeTyped } from "./index.ts";
import type { Typed, TypedEnum } from "./typed.ts";

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

const corpus = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../../../conformance/compat/vectors.json", import.meta.url)), "utf8"),
) as VectorFile;

const reg = new Registry(
  corpus.schemas.map((b) => schemaFromBytes(hexToBytes(b))),
  corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
);

function caseByName(name: string): Case {
  const c = corpus.cases.find((x) => x.name === name);
  if (!c) throw new Error(`no corpus case ${name}`);
  return c;
}

function rootOf(name: string): bigint {
  return BigInt(`0x${caseByName(name).writer_root}`);
}

function typedRoundTrip(c: Case): Typed {
  const writerRoot = BigInt(`0x${c.writer_root}`);
  const readerRoot = BigInt(`0x${c.reader_root}`);
  const writerBytes = hexToBytes(c.writer_hex);
  const typed = decodeTyped(writerBytes, writerRoot, readerRoot, reg, { jit: false });
  const typedJit = decodeTyped(writerBytes, writerRoot, readerRoot, reg, { jit: true });
  expect(typedJit).toEqual(typed);
  const reHex = bytesToHex(encodeTyped(typedJit, readerRoot, reg, { jit: true }));
  expect(reHex).toBe(c.reader_hex);
  expect(bytesToHex(encodeTyped(typedJit, readerRoot, reg, { jit: false }))).toBe(c.reader_hex);
  return typedJit;
}

describe("typed front door — round-trips through the Rust oracle", () => {
  for (const c of corpus.cases) {
    if (c.error_kind !== null) continue;
    it(c.name, () => {
      typedRoundTrip(c);
    });
  }
});

describe("typed front door — ergonomic shapes", () => {
  it("small integers decode to number, not bigint", () => {
    const t = typedRoundTrip(caseByName("scalar_u32")) as number;
    expect(typeof t).toBe("number");
    expect(t).toBe(123456);
  });

  it("structs decode to plain objects with numbers", () => {
    const t = typedRoundTrip(caseByName("struct_mixed_align")) as Record<string, Typed>;
    expect(t.constructor).toBe(Object);
    expect(t.a).toBe(7); // u8 -> number
    expect(typeof t.b).toBe("number"); // u32 -> number
    expect(typeof t.c).toBe("bigint"); // u64 -> bigint
    expect(typeof t.d).toBe("bigint"); // u128 -> bigint
    expect(t.f).toBe(true);
    expect(t.h).toBe("hi");
  });

  it("enums decode to an inlined { tag, … } discriminated union", () => {
    // newtype -> { tag, value }
    const t = typedRoundTrip(caseByName("enum_same")) as TypedEnum;
    expect(t.tag).toBe("B");
    expect(t.value).toBe(42); // newtype u32 -> number
    // unit -> { tag } (no value)
    expect(typedRoundTrip(caseByName("enum_unit_variant"))).toEqual({ tag: "A" });
    // tuple -> { tag, value: [...] }
    const tup = typedRoundTrip(caseByName("enum_tuple_variant")) as TypedEnum;
    expect(tup).toEqual({ tag: "C", value: [1, 2] }); // tuple of u8 -> number[]
    // struct variant -> { tag, ...fields } (fields inlined)
    expect(typedRoundTrip(caseByName("enum_struct_variant"))).toEqual({ tag: "Move", x: 3, y: 4 });
  });

  it("uses $tag when an enum struct variant has a real tag field", () => {
    const primitive = (tag: Primitive): SchemaRef => ({
      kind: "concrete",
      id: BigInt(`0x${corpus.primitives.find((p) => p.tag === tag)!.id}`),
      args: [],
    });
    const root = 0xfeed_cafe_0000_0001n;
    const schemas: Schema[] = [
      {
        id: root,
        typeParams: [],
        kind: {
          kind: "enum",
          name: "Taggy",
          variants: [
            {
              name: "Element",
              index: 0,
              payload: {
                kind: "struct",
                fields: [
                  { name: "tag", schema: primitive("string"), required: true },
                  { name: "x", schema: primitive("u32"), required: true },
                ],
              },
            },
            { name: "Text", index: 1, payload: { kind: "newtype", ref: primitive("string") } },
          ],
        },
      },
    ];
    const localReg = new Registry(
      schemas,
      corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
    );
    const typed = { $tag: "Element", tag: "main", x: 7 };
    const bytes = encodeTyped(typed, root, localReg, { jit: true });
    expect([...bytes]).toEqual([...encodeTyped(typed, root, localReg, { jit: false })]);
    expect(decodeTyped(bytes, root, root, localReg, { jit: true })).toEqual(typed);
    expect(decodeTyped(bytes, root, root, localReg, { jit: false })).toEqual(typed);
  });

  it("char decodes to a string", () => {
    expect(typedRoundTrip(caseByName("char_value"))).toBe("λ");
  });

  it("extended kinds decode to canonical strings", () => {
    const t = typedRoundTrip(caseByName("extended_kinds")) as Record<string, Typed>;
    expect(typeof t.dt).toBe("string");
    expect(t.id).toBe("01234567-89ab-cdef-fedc-ba9876543210");
    expect(t.qn).toBe("{http://ex.com/ns}el");
  });

  it("a reordered struct decodes to the same object regardless of wire order", () => {
    const t = typedRoundTrip(caseByName("struct_reorder")) as Record<string, Typed>;
    expect(t).toEqual({ x: 5, y: 9n }); // x:u32 -> number, y:u64 -> bigint
  });

  it("a defaulted reader-only field is null", () => {
    const t = typedRoundTrip(caseByName("struct_field_default")) as Record<string, Typed>;
    expect(t).toEqual({ x: 7, extra: null });
  });
});

// r[verify typed.no-dynamic-bounce]
describe("typed front door — direct public-shape JIT", () => {
  it("generates struct decode code that constructs plain objects, not Value Maps", () => {
    const root = rootOf("struct_mixed_align");
    const source = compiledTypedDecoderSource(buildPlan(root, root, reg), root, reg);
    expect(source).toContain("= {}");
    expect(source).not.toContain("new Map");

    const decoded = decodeTyped(hexToBytes(caseByName("struct_mixed_align").writer_hex), root, root, reg, { jit: true });
    expect(decoded?.constructor).toBe(Object);
  });

  it("generates struct encode code that reads public object properties, not Value Map entries", () => {
    const root = rootOf("struct_mixed_align");
    const source = compiledTypedEncoderSource(root, reg);
    expect(source).toContain('["a"]');
    expect(source).not.toContain(".get(");

    const typed = typedRoundTrip(caseByName("struct_mixed_align"));
    const jitHex = bytesToHex(encodeTyped(typed, root, reg, { jit: true }));
    const interpHex = bytesToHex(encodeTyped(typed, root, reg, { jit: false }));
    expect(jitHex).toBe(interpHex);
  });
});

describe("typed front door — recursion", () => {
  it("a recursive Tree round-trips through encodeTyped/decodeTyped", () => {
    // Tree { value: u32, children: list<Tree> } — both Tree and the list are on the
    // cycle, so each lowers to a callable block (callBlock) and the plan stays finite.
    const u32Id = BigInt(`0x${corpus.primitives.find((p) => p.tag === "u32")!.id}`);
    const treeId = 0x1111_1111n;
    const vecId = 0x2222_2222n;
    const tree = {
      id: treeId,
      typeParams: [] as string[],
      kind: {
        kind: "struct" as const,
        name: "Tree",
        fields: [
          { name: "value", schema: { kind: "concrete" as const, id: u32Id, args: [] }, required: true },
          { name: "children", schema: { kind: "concrete" as const, id: vecId, args: [] }, required: true },
        ],
      },
    };
    const vec = {
      id: vecId,
      typeParams: [] as string[],
      kind: { kind: "list" as const, element: { kind: "concrete" as const, id: treeId, args: [] } },
    };
    const recReg = new Registry(
      [tree, vec],
      corpus.primitives.map((p) => ({ id: BigInt(`0x${p.id}`), tag: p.tag as Primitive })),
    );

    const value: Typed = {
      value: 1,
      children: [
        { value: 2, children: [] },
        { value: 3, children: [{ value: 4, children: [] }] },
      ],
    };
    const wire = encodeTyped(value, treeId, recReg, { jit: true });
    expect([...wire]).toEqual([...encodeTyped(value, treeId, recReg, { jit: false })]);
    // Same-schema decode translates treeId -> treeId through the recursion blocks.
    const back = decodeTyped(wire, treeId, treeId, recReg, { jit: true });
    expect(back).toEqual(value);
    expect(decodeTyped(wire, treeId, treeId, recReg, { jit: false })).toEqual(value);
  });
});
