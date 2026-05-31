// The typed front door against the Rust oracle: for every well-formed corpus
// case, decode the writer bytes into an ergonomic typed value shaped by the
// reader schema, re-encode it through the reader schema, and assert the bytes
// equal Rust's reconciled reader bytes. This proves the typed remap is
// information-preserving: a vox TS peer can decode to ergonomic objects and
// re-encode them losslessly.
//
// A few cases also assert the concrete ergonomic shape (numbers not bigints,
// plain objects not Maps, {tag,value} enums, canonical strings).

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { Registry, bytesToHex, hexToBytes, schemaFromBytes } from "@bearcove/phon-schema";
import type { Primitive } from "@bearcove/phon-schema";
import { decodeTyped, encodeTyped } from "./index.ts";
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

function typedRoundTrip(c: Case): Typed {
  const writerRoot = BigInt(`0x${c.writer_root}`);
  const readerRoot = BigInt(`0x${c.reader_root}`);
  const typed = decodeTyped(hexToBytes(c.writer_hex), writerRoot, readerRoot, reg);
  const reHex = bytesToHex(encodeTyped(typed, readerRoot, reg));
  expect(reHex).toBe(c.reader_hex);
  return typed;
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

  it("enums decode to a { tag, value } discriminated union", () => {
    const t = typedRoundTrip(caseByName("enum_same")) as TypedEnum;
    expect(t.tag).toBe("B");
    expect(t.value).toBe(42); // newtype u32 -> number
    const unit = typedRoundTrip(caseByName("enum_unit_variant")) as TypedEnum;
    expect(unit).toEqual({ tag: "A", value: null });
    const tup = typedRoundTrip(caseByName("enum_tuple_variant")) as TypedEnum;
    expect(tup.tag).toBe("C");
    expect(tup.value).toEqual([1, 2]); // tuple of u8 -> number[]
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
