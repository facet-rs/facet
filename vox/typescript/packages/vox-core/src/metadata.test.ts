import { describe, expect, it } from "vitest";
import { metadataKeyIsNoPropagate, metadataKeyIsRedacted } from "@bearcove/vox-wire";

import { ClientMetadata, clientMetadataToWire } from "./metadata.ts";

describe("ClientMetadata", () => {
  // r[verify rpc.metadata]
  // r[verify rpc.metadata.value]
  // r[verify rpc.metadata.keys]
  // r[verify rpc.metadata.duplicates]
  // r[verify rpc.metadata.unknown]
  // r[verify schema.interaction.metadata]
  it("exposes metadata as a self-describing wire Value map", () => {
    const metadata = new ClientMetadata();
    const bytes = new Uint8Array([1, 2, 3]);

    metadata.set("trace-id", "abc");
    metadata.set("attempt", 7n);
    metadata.set("blob", bytes);
    metadata.set("Trace-Id", "case-sensitive");
    metadata.set("unknown-key", "ignored unless read explicitly");
    metadata.set("trace-id", "replacement");

    const wire = clientMetadataToWire(metadata);

    expect(wire).toBe(metadata.toWire());
    expect(wire.get("trace-id")).toBe("replacement");
    expect(wire.get("Trace-Id")).toBe("case-sensitive");
    expect(wire.get("TRACE-ID")).toBeUndefined();
    expect(wire.get("attempt")).toBe(7n);
    expect(wire.get("blob")).toBe(bytes);
    expect(wire.get("unknown-key")).toBe("ignored unless read explicitly");
    expect(wire.size).toBe(5);
  });

  // r[verify rpc.metadata.sigils]
  it("treats metadata sigils as key-string conventions", () => {
    expect(metadataKeyIsRedacted("regular.metadata")).toBe(false);
    expect(metadataKeyIsNoPropagate("regular.metadata")).toBe(false);

    expect(metadataKeyIsRedacted("#sensitive.metadata")).toBe(true);
    expect(metadataKeyIsNoPropagate("#sensitive.metadata")).toBe(false);

    expect(metadataKeyIsRedacted("-no-propagate-metadata")).toBe(false);
    expect(metadataKeyIsNoPropagate("-no-propagate-metadata")).toBe(true);

    expect(metadataKeyIsRedacted("-#sensitive-and-no-propagate-metadata")).toBe(true);
    expect(metadataKeyIsNoPropagate("-#sensitive-and-no-propagate-metadata")).toBe(true);
  });
});
