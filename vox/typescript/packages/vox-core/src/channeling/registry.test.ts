import { afterEach, describe, expect, it } from "vitest";

import { ChannelRegistry } from "./registry.ts";
import { Tx } from "./tx.ts";
import { setVoxLogger } from "../logger.ts";

afterEach(() => {
  setVoxLogger(null);
});

describe("ChannelRegistry", () => {
  // r[verify rpc.channel.binding]
  // r[verify rpc.channel.binding.callee-args]
  // r[verify rpc.channel.binding.callee-args.rx]
  // r[verify rpc.channel.delivery.reliable]
  // r[verify rpc.channel.item]
  // r[verify rpc.flow-control.credit.initial]
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

  // r[verify rpc.channel.binding.callee-args.tx]
  // r[verify rpc.flow-control.credit.initial]
  it("registers outgoing Tx handles by channel id", async () => {
    const registry = new ChannelRegistry();
    const channelId = 11n;
    const payload = Uint8Array.of(8, 9);

    const tx = registry.registerOutgoing(channelId, 1);
    await tx.sendData(payload);

    expect(registry.pollOutgoing()).toEqual({ kind: "data", channelId, payload });
  });

  // r[verify rpc.flow-control.credit]
  // r[verify rpc.flow-control.credit.exhaustion]
  // r[verify rpc.flow-control]
  it("blocks outgoing data when credit is exhausted until credit is granted", async () => {
    const registry = new ChannelRegistry();
    const channelId = 13n;
    const sender = registry.registerOutgoing(channelId, 1);
    const first = Uint8Array.of(1);
    const second = Uint8Array.of(2);

    await sender.sendData(first);
    let sentSecond = false;
    const blocked = sender.sendData(second).then(() => {
      sentSecond = true;
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sentSecond).toBe(false);
    expect(registry.pollOutgoing()).toEqual({ kind: "data", channelId, payload: first });

    registry.grantCredit(channelId, 1);
    await blocked;

    expect(sentSecond).toBe(true);
    expect(registry.pollOutgoing()).toEqual({ kind: "data", channelId, payload: second });
  });

  // r[verify rpc.flow-control.credit.try-send]
  // r[verify rpc.observability.channel.try-send-detail]
  it("trySend returns full or closed with the original value", () => {
    const unbound = new Tx<number>();
    expect(unbound.trySendDetailed(0)).toEqual({
      kind: "full",
      detail: "unbound",
      value: 0,
    });

    const registry = new ChannelRegistry();
    const channelId = 14n;
    const tx = new Tx<number>();
    tx.bind(channelId, registry, (value) => Uint8Array.of(value), 1);

    expect(tx.trySend(1)).toEqual({ kind: "sent" });
    expect(tx.trySend(2)).toEqual({ kind: "full", value: 2 });
    expect(registry.pollOutgoing()).toEqual({
      kind: "data",
      channelId,
      payload: Uint8Array.of(1),
    });
    expect(registry.pollOutgoing()).toEqual({ kind: "pending" });

    tx.close();
    expect(tx.trySend(3)).toEqual({ kind: "closed", value: 3 });
    expect(tx.trySendDetailed(3)).toEqual({ kind: "closed", detail: "closed", value: 3 });

    const creditLimited = new Tx<number>();
    creditLimited.bind(16n, new ChannelRegistry(), (value) => Uint8Array.of(value), 0);
    expect(creditLimited.trySendDetailed(4)).toEqual({
      kind: "full",
      detail: "credit_exhausted",
      value: 4,
    });

    const queueLimited = new Tx<number>();
    queueLimited.bind(17n, new ChannelRegistry(), (value) => Uint8Array.of(value), 65);
    for (let value = 0; value < 64; value += 1) {
      expect(queueLimited.trySendDetailed(value)).toEqual({ kind: "sent", detail: "sent" });
    }
    expect(queueLimited.trySendDetailed(64)).toEqual({
      kind: "full",
      detail: "runtime_queue_full",
      value: 64,
    });
  });

  // r[verify rpc.flow-control.credit.grant]
  // r[verify rpc.flow-control.credit.grant.additive]
  it("queues additive credit grants as incoming items are consumed", async () => {
    const registry = new ChannelRegistry();
    const channelId = 15n;
    const payload = Uint8Array.of(3);
    const rx = registry.registerIncoming(channelId, 2);

    registry.routeData(channelId, payload);

    await expect(rx.recv()).resolves.toEqual(payload);
    expect(registry.pollOutgoing()).toEqual({ kind: "credit", channelId, additional: 1 });
  });

  // r[verify rpc.channel.close]
  // r[verify rpc.channel.lifecycle]
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

  // r[verify rpc.channel.connection-closure]
  it("closeAll terminates live incoming receivers and blocked outgoing senders", async () => {
    const registry = new ChannelRegistry();
    const rx = registry.registerIncoming(31n, 1);
    const tx = registry.registerOutgoing(33n, 0);

    const blockedSend = tx.sendData(Uint8Array.of(9));
    await new Promise((resolve) => setTimeout(resolve, 0));

    registry.closeAll();

    await expect(rx.recv()).resolves.toBeNull();
    await expect(blockedSend).rejects.toMatchObject({ kind: "closed" });
    expect(registry.pollOutgoing()).toEqual({ kind: "done" });
  });

  // r[verify rpc.debug.snapshot]
  // r[verify rpc.observability.channel]
  // r[verify rpc.observability.channel.context]
  it("preserves channel debug context in snapshots after close", () => {
    const registry = new ChannelRegistry();
    const channelId = 21n;
    registry.registerOutgoing(channelId, 1);
    registry.rememberContext(channelId, {
      laneId: 0n,
      requestId: 5n,
      service: "Echo",
      method: "stream",
      channelDirection: "tx",
      side: "client",
    });

    expect(registry.debugSnapshot()).toEqual({
      channels: [
        {
          channelId,
          state: "outgoing",
          context: {
            laneId: 0n,
            requestId: 5n,
            service: "Echo",
            method: "stream",
            channelDirection: "tx",
            side: "client",
          },
        },
      ],
      pendingCreditCount: 0,
    });

    registry.close(channelId);
    expect(registry.debugSnapshot().channels).toContainEqual({
      channelId,
      state: "closed",
      context: {
        laneId: 0n,
        requestId: 5n,
        service: "Echo",
        method: "stream",
        channelDirection: "tx",
        side: "client",
      },
    });
  });

  // r[verify rpc.observability.channel]
  it("emits local channel observer events through the installed logger", async () => {
    const events: Array<{ message: string; data: unknown }> = [];
    setVoxLogger({
      debug(message, data) {
        events.push({ message, data });
      },
      error(message, ...args) {
        events.push({ message, data: args });
      },
    });

    const registry = new ChannelRegistry();
    const incoming = registry.registerIncoming(41n, 2);
    registry.routeData(41n, Uint8Array.of(1, 2));
    await expect(incoming.recv()).resolves.toEqual(Uint8Array.of(1, 2));

    const outgoing = registry.registerOutgoing(43n, 2);
    await outgoing.sendData(Uint8Array.of(3, 4, 5));
    expect(registry.pollOutgoing()).toEqual({
      kind: "credit",
      channelId: 41n,
      additional: 1,
    });
    expect(registry.pollOutgoing()).toEqual({
      kind: "data",
      channelId: 43n,
      payload: Uint8Array.of(3, 4, 5),
    });
    registry.grantCredit(43n, 1);
    registry.close(41n);
    outgoing.sendClose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(registry.pollOutgoing()).toEqual({ kind: "close", channelId: 43n });

    expect(events).toEqual(
      expect.arrayContaining([
        {
          message: "[vox:channel] open",
          data: expect.objectContaining({ channelId: 41n, direction: "incoming" }),
        },
        {
          message: "[vox:channel] receive",
          data: expect.objectContaining({ channelId: 41n, bytes: 2 }),
        },
        {
          message: "[vox:channel] open",
          data: expect.objectContaining({ channelId: 43n, direction: "outgoing" }),
        },
        {
          message: "[vox:channel] send",
          data: expect.objectContaining({ channelId: 43n, bytes: 3 }),
        },
        {
          message: "[vox:channel] credit",
          data: expect.objectContaining({
            channelId: 41n,
            direction: "outgoing",
            additional: 1,
          }),
        },
        {
          message: "[vox:channel] credit",
          data: expect.objectContaining({
            channelId: 43n,
            direction: "incoming",
            additional: 1,
          }),
        },
        {
          message: "[vox:channel] close",
          data: expect.objectContaining({ channelId: 41n }),
        },
        {
          message: "[vox:channel] close",
          data: expect.objectContaining({ channelId: 43n, direction: "outgoing" }),
        },
      ]),
    );
  });
});
