import { describe, expect, it } from "vitest";
import { primitiveId, resolveIds, Registry } from "@bearcove/phon-schema";
import { buildPlan, compile, compileEncoder, decodeCompact, decodeTyped, encode as encodeCompact, encodeTyped, recordJitFallbacks } from "./index.ts";
import { ID, REG, fixtures, root } from "./ecosystem_surface.fixtures.ts";
import type { Typed } from "./typed.ts";
import type { Field, Primitive, SchemaKind, SchemaRef, Value } from "@bearcove/phon-schema";

const NS = 0xec00_0000_0000_0000n;
const id = (n: number): bigint => NS + BigInt(n);
const ref = (schema: bigint): SchemaRef => ({ kind: "concrete", id: schema, args: [] });
const prim = (primitive: Primitive): SchemaRef => ref(primitiveId(primitive));
const field = (name: string, schema: SchemaRef, required = true): Field => ({ name, schema, required });
const schema = (schemaId: bigint, kind: SchemaKind) => ({ id: schemaId, typeParams: [], kind });

interface FlameNodeFixture {
  address: bigint;
  function_name: number | null;
  binary: number | null;
  on_cpu_ns: bigint;
  off_cpu: {
    sleep_ns: bigint;
    io_ns: bigint;
    mutex_ns: bigint;
  };
  children: FlameNodeFixture[];
}

function makeDeepStaxFlamegraphUpdate(depth: number): Typed {
  let node: FlameNodeFixture = {
    address: 0x1000n + BigInt(depth),
    function_name: 1,
    binary: 2,
    on_cpu_ns: 1n,
    off_cpu: { sleep_ns: 0n, io_ns: 0n, mutex_ns: 0n },
    children: [],
  };
  for (let i = depth - 1; i >= 0; i--) {
    node = {
      address: 0x1000n + BigInt(i),
      function_name: i % 2,
      binary: 2,
      on_cpu_ns: BigInt(depth - i + 1),
      off_cpu: { sleep_ns: BigInt(i), io_ns: 0n, mutex_ns: 0n },
      children: [node],
    };
  }
  return {
    total_on_cpu_ns: BigInt(depth + 1),
    strings: ["root", "poll", "libstax.dylib"],
    root: node,
  } as unknown as Typed;
}
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
// r[verify compact.schema-driven]
// r[verify compat.plan-first]
// r[verify exec.interpreter-baseline]
// r[verify exec.jit-optional]
// r[verify crates.jit-opt-in]
// r[verify validate.uniqueness]
describe("ecosystem surface fixtures", () => {
  for (const fixture of fixtures) {
    it(fixture.name, () => {
      const wire = encodeTyped(fixture.value, fixture.root, REG, { jit: true });
      expect([...wire]).toEqual([...encodeTyped(fixture.value, fixture.root, REG, { jit: false })]);

      expect(decodeTyped(wire, fixture.root, fixture.root, REG, { jit: false })).toEqual(fixture.value);
      expect(decodeTyped(wire, fixture.root, fixture.root, REG, { jit: true })).toEqual(fixture.value);

      const coarse = compile(fixture.root, fixture.root, REG, { jit: false })(wire);
      expect([...compileEncoder(fixture.root, REG, { jit: false })(coarse)]).toEqual([...wire]);
      expect([...compileEncoder(fixture.root, REG, { jit: true })(coarse)]).toEqual([...wire]);

      const fallbacks = recordJitFallbacks(buildPlan(fixture.root, fixture.root, REG));
      expect(fallbacks).toEqual([]);
    });
  }

  it("accepts production-depth Stax recursive flamegraph stacks", () => {
    const update = makeDeepStaxFlamegraphUpdate(96);
    const flameRoot = root(ID.staxFlamegraphUpdate);
    const wire = encodeTyped(update, flameRoot, REG, { jit: false });

    expect(decodeTyped(wire, flameRoot, flameRoot, REG, { jit: false })).toEqual(update);
    expect(decodeTyped(wire, flameRoot, flameRoot, REG, { jit: true })).toEqual(update);
    expect([...encodeTyped(update, flameRoot, REG, { jit: true })]).toEqual([...wire]);
  });
});

// r[verify type-system.external]
describe("Stax external fd capabilities", () => {
  it("stay explicit transport capabilities, not scalar payloads", () => {
    expect(() => encodeCompact(0n, root(ID.staxExternalFd), REG)).toThrow("compact encode unsupported for kind 'external'");
    expect(() => decodeCompact(new Uint8Array(8), root(ID.staxExternalFd), REG)).toThrow("compact decode unsupported for kind 'external'");
    expect(() => buildPlan(root(ID.staxLinuxPerfSessionFdsExternal), root(ID.staxLinuxPerfSessionFdsExternal), REG)).toThrow(
      "compat plan unsupported for external",
    );
  });

  // r[verify compat.type-match]
  // r[verify type-system.channel]
  // r[verify type-system.external]
  it("keeps capability roots out of payload compat while item and metadata schemas use compat", () => {
    const tmpWriterItem = id(901);
    const tmpReaderItem = id(902);
    const tmpChannelRoot = id(903);
    const tmpWriterMetadata = id(904);
    const tmpReaderMetadata = id(905);
    const tmpExternalRoot = id(906);
    const localSchemas = resolveIds([
      schema(tmpWriterItem, {
        kind: "struct",
        name: "DodecaTunnelItem",
        fields: [field("seq", prim("u64")), field("chunk_len", prim("u32")), field("transient_id", prim("u64"))],
      }),
      schema(tmpReaderItem, {
        kind: "struct",
        name: "DodecaTunnelItem",
        fields: [field("seq", prim("u64")), field("chunk_len", prim("u32"))],
      }),
      schema(tmpChannelRoot, { kind: "channel", direction: "tx", element: ref(tmpWriterItem) }),
      schema(tmpWriterMetadata, {
        kind: "struct",
        name: "StaxFdMetadata",
        fields: [field("path", prim("string")), field("flags", prim("u32")), field("probe_id", prim("u64"))],
      }),
      schema(tmpReaderMetadata, {
        kind: "struct",
        name: "StaxFdMetadata",
        fields: [field("path", prim("string")), field("flags", prim("u32"))],
      }),
      schema(tmpExternalRoot, { kind: "external", external: "fd", metadata: ref(tmpWriterMetadata) }),
    ]);
    const local = new Registry(localSchemas);
    const writerItem = localSchemas[0].id;
    const readerItem = localSchemas[1].id;
    const channelRoot = localSchemas[2].id;
    const writerMetadata = localSchemas[3].id;
    const readerMetadata = localSchemas[4].id;
    const externalRoot = localSchemas[5].id;

    expect(() => buildPlan(channelRoot, channelRoot, local)).toThrow("compat plan unsupported for channel");
    expect(() => buildPlan(externalRoot, externalRoot, local)).toThrow("compat plan unsupported for external");

    const itemWire = encodeCompact(
      new Map<string, Value>([["seq", 7n], ["chunk_len", 128n], ["transient_id", 99n]]),
      writerItem,
      local,
    );
    const item = new Map<string, Value>([["seq", 7n], ["chunk_len", 128n]]);
    expect(compile(writerItem, readerItem, local, { jit: false })(itemWire)).toEqual(item);
    expect(compile(writerItem, readerItem, local, { jit: true })(itemWire)).toEqual(item);

    const metadataWire = encodeCompact(
      new Map<string, Value>([["path", "/proc/self/fd/7"], ["flags", 0x800n], ["probe_id", 44n]]),
      writerMetadata,
      local,
    );
    const metadata = new Map<string, Value>([["path", "/proc/self/fd/7"], ["flags", 0x800n]]);
    expect(compile(writerMetadata, readerMetadata, local, { jit: false })(metadataWire)).toEqual(metadata);
    expect(compile(writerMetadata, readerMetadata, local, { jit: true })(metadataWire)).toEqual(metadata);
  });
});
