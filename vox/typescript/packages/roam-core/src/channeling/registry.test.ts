import { describe, expect, it } from "vitest";

import { ChannelRegistry } from "./registry.ts";

describe("ChannelRegistry", () => {
  // r[verify rpc.channel.binding]
  // r[verify rpc.channel.item]
  it("buffers incoming data until the receiver is registered", async () => {
    const registry = new ChannelRegistry();
    const channelId = 7n;
    const first = Uint8Array.of(1, 2, 3);
    const second = Uint8Array.of(4, 5, 6);

    registry.routeData(channelId, first);
    registry.routeData(channelId, second);

    const rx = registry.registerIncoming(channelId, 2);
    await expect(rx.recv()).resolves.toEqual(first);
    await expect(rx.recv()).resolves.toEqual(second);
  });

  // r[verify rpc.channel.close]
  // r[verify rpc.channel.reset]
  it("preserves buffered terminal close before the receiver is registered", async () => {
    const registry = new ChannelRegistry();
    const channelId = 9n;
    const payload = Uint8Array.of(42);

    registry.routeData(channelId, payload);
    registry.close(channelId);

    const rx = registry.registerIncoming(channelId, 1);
    await expect(rx.recv()).resolves.toEqual(payload);
    await expect(rx.recv()).resolves.toBeNull();
    expect(() => registry.routeData(channelId, Uint8Array.of(7))).toThrow(/data after close/i);
  });
});
