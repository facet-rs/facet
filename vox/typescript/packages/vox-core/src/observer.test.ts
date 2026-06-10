import { afterEach, describe, expect, it } from "vitest";
import { observerMetricLabels } from "./observer.ts";
import { setVoxLogger, voxLogger } from "./logger.ts";

afterEach(() => {
  setVoxLogger(null);
});

describe("observerMetricLabels", () => {
  // r[verify rpc.observability.low-cardinality]
  it("keeps only the default low-cardinality metric labels", () => {
    const labels = observerMetricLabels({
      service: "Echo",
      method: "echo",
      side: "client",
      outcome: "ok",
      error_kind: "",
      channel_direction: "tx",
      connection_id: "13",
      request_id: "21",
      channel_id: "34",
      peer_address: "/tmp/vox.sock",
      metadata: "tenant",
    } as Parameters<typeof observerMetricLabels>[0]);

    expect(labels).toEqual({
      service: "Echo",
      method: "echo",
      side: "client",
      outcome: "ok",
      channel_direction: "tx",
    });
    expect(Object.keys(labels)).not.toContain("connection_id");
    expect(Object.keys(labels)).not.toContain("request_id");
    expect(Object.keys(labels)).not.toContain("channel_id");
    expect(Object.keys(labels)).not.toContain("metadata");
  });

  // r[verify rpc.observability.runtime]
  it("installs and clears a local runtime logger without a telemetry backend", () => {
    const events: Array<{ level: "debug" | "error"; message: string; args: unknown[] }> = [];
    const logger = {
      debug(message: string, ...args: unknown[]) {
        events.push({ level: "debug", message, args });
      },
      error(message: string, ...args: unknown[]) {
        events.push({ level: "error", message, args });
      },
    };

    setVoxLogger(logger);
    expect(voxLogger()).toBe(logger);

    voxLogger()?.debug("local event", { connectionId: 1n });
    voxLogger()?.error("local error", new Error("boom"));

    expect(events).toHaveLength(2);
    expect(events[0]).toMatchObject({
      level: "debug",
      message: "local event",
      args: [{ connectionId: 1n }],
    });
    expect(events[1].level).toBe("error");

    setVoxLogger(null);
    expect(voxLogger()).toBeNull();
  });
});
