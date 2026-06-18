import { describe, expect, it } from "vitest";
import { SchemaSendTracker, SchemaTracker } from "./schema_tracker.ts";

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

describe("SchemaSendTracker", () => {
  // r[verify schema.format.delivery]
  // r[verify schema.tracking.sent]
  // r[verify schema.tracking.bindings]
  // r[verify schema.exchange.idempotent]
  // r[verify schema.principles.sender-driven]
  // r[verify schema.principles.no-roundtrips]
  it("advertises a method-direction binding once", () => {
    const tracker = new SchemaSendTracker();
    const closure = "010203";

    expect(tracker.prepareSchemas(7n, "args", closure)).toEqual([1, 2, 3]);
    expect(tracker.prepareSchemas(7n, "args", closure)).toEqual([]);
    expect(tracker.prepareSchemas(7n, "response", closure)).toEqual([1, 2, 3]);
  });
});

describe("SchemaTracker", () => {
  // r[verify schema.exchange.required]
  it("requires a received method-direction binding before decode", () => {
    const bytes = new Uint8Array([...leU64(1n), ...leU32(0)]);
    const tracker = new SchemaTracker();

    expect(() => tracker.requireReceived(7n, "args")).toThrow(
      "sender must send schemas before data",
    );

    tracker.recordReceived(7n, "args", bytes);

    expect(() => tracker.requireReceived(7n, "args")).not.toThrow();
    expect(() => tracker.requireReceived(7n, "response")).toThrow(
      "sender must send schemas before data",
    );
  });

  // r[verify schema.tracking.received]
  // r[verify schema.tracking.bindings]
  // r[verify schema.format.binding-roots]
  // r[verify schema.exchange.channels]
  it("records channel auxiliary roots from a schema binding", () => {
    const role = new TextEncoder().encode("channel.arg.0.rx.element");
    const bytes = new Uint8Array([
      ...leU64(1n),
      ...leU32(0),
      ...leU32(1),
      ...leU32(role.length),
      ...role,
      ...leU64(2n),
    ]);
    const tracker = new SchemaTracker();

    tracker.recordReceived(7n, "args", bytes);

    expect(tracker.auxiliaryRoot(7n, "args", "channel.arg.0.rx.element")).toBe(2n);
    expect(tracker.auxiliaryRoot(7n, "args", "channel.arg.1.rx.element")).toBeNull();
  });

  // r[verify schema.type-id.per-connection]
  it("does not share received bindings across tracker instances", () => {
    const bytes = new Uint8Array([...leU64(1n), ...leU32(0)]);
    const firstConnection = new SchemaTracker();
    const secondConnection = new SchemaTracker();

    firstConnection.recordReceived(7n, "args", bytes);

    expect(firstConnection.hasReceived(7n, "args")).toBe(true);
    expect(secondConnection.hasReceived(7n, "args")).toBe(false);
  });
});
