import { describe, expect, it } from "vitest";
import { parseSchemaClosure } from "./codec.ts";

const leU32 = (value: number): number[] => {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, true);
  return [...bytes];
};

const leU64 = (value: bigint): number[] => {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, value, true);
  return [...bytes];
};

describe("parseSchemaClosure", () => {
  // r[verify schema.format.binding-roots]
  it("parses auxiliary schema roots", () => {
    const role = new TextEncoder().encode("channel.arg.0.tx.element");
    const bytes = new Uint8Array([
      ...leU64(1n),
      ...leU32(0),
      ...leU32(1),
      ...leU32(role.length),
      ...role,
      ...leU64(2n),
    ]);

    expect(parseSchemaClosure(bytes)).toEqual({
      root: 1n,
      schemas: [],
      auxiliaryRoots: [{ role: "channel.arg.0.tx.element", root: 2n }],
    });
  });
});
