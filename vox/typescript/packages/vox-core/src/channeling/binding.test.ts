import { describe, expect, it } from "vitest";
import type { Registry } from "@bearcove/phon-schema";
import type { SchemaTracker } from "../schema_tracker.ts";
import { ChannelIdAllocator } from "./allocator.ts";
import { bindPhonChannels } from "./binding.ts";
import { channel } from "./pair.ts";
import { ChannelRegistry } from "./registry.ts";
import { Role } from "./types.ts";
import { connectionEchoRegistry } from "../connection_echo.fixture.ts";

const U32_ROOT = 0x281c5be4f2ee63b4n;

describe("bindPhonChannels", () => {
  // r[verify rpc.channel.allocation]
  it("allocates caller channel ids using connection parity", () => {
    const initiator = new ChannelIdAllocator(Role.Initiator);
    const acceptor = new ChannelIdAllocator(Role.Acceptor);

    expect([initiator.next(), initiator.next()]).toEqual([1n, 3n]);
    expect([acceptor.next(), acceptor.next()]).toEqual([2n, 4n]);
  });

  // r[verify schema.interaction.channels]
  // r[verify schema.exchange.channels.tx-args]
  // r[verify rpc.channel]
  // r[verify rpc.channel.direction]
  // r[verify rpc.channel.binding.caller-args]
  // r[verify rpc.channel.binding.caller-args.tx]
  // r[verify rpc.channel.payload-encoding]
  // r[verify rpc.channel.pair]
  // r[verify rpc.channel.pair.binding-propagation]
  // r[verify rpc.channel.pair.rx-take]
  it("uses lazily advertised auxiliary schemas for caller-side channel receives", async () => {
    const [tx, rx] = channel<unknown>();
    const registry = new ChannelRegistry();
    const allocator = new ChannelIdAllocator(Role.Initiator);
    const seen: Array<[bigint, string, string, bigint]> = [];
    const tracker = {
      buildAuxiliaryDecoder(
        methodId: bigint,
        direction: "args" | "response",
        role: string,
        readerRoot: bigint,
      ) {
        seen.push([methodId, direction, role, readerRoot]);
        return (bytes: Uint8Array) => `aux:${bytes[0]}`;
      },
    } as unknown as SchemaTracker;

    const bound = bindPhonChannels(
      [tx],
      [{ index: 0, direction: "tx", elementRoot: 123n }],
      allocator,
      registry,
      {} as Registry,
      { incoming: 4, outgoing: 4 },
      { methodId: 55n, direction: "args", tracker },
    );

    registry.routeData(bound.channels[0]!, Uint8Array.of(9));

    expect(bound.channels).toEqual([1n]);
    expect(bound.values[0]).toEqual(Uint8Array.of(0, 0, 0, 0));
    expect(rx.isBound).toBe(true);
    await expect(rx.recv()).resolves.toBe("aux:9");
    expect(seen).toEqual([[55n, "args", "channel.arg.0.tx.element", 123n]]);
  });

  // r[verify rpc.channel.binding.caller-args]
  // r[verify rpc.channel.binding.caller-args.rx]
  // r[verify rpc.channel.direction]
  // r[verify rpc.channel.payload-encoding]
  // r[verify rpc.channel.pair]
  // r[verify rpc.channel.pair.binding-propagation]
  // r[verify rpc.channel.pair.tx-read]
  it("binds the paired Tx when the caller passes an Rx argument", async () => {
    const [tx, rx] = channel<unknown>();
    const registry = new ChannelRegistry();
    const allocator = new ChannelIdAllocator(Role.Initiator);

    const bound = bindPhonChannels(
      [rx],
      [{ index: 0, direction: "rx", elementRoot: U32_ROOT }],
      allocator,
      registry,
      connectionEchoRegistry,
      { incoming: 4, outgoing: 4 },
    );

    expect(bound.channels).toEqual([1n]);
    expect(bound.values[0]).toEqual(Uint8Array.of(0, 0, 0, 0));
    expect(tx.isBound).toBe(true);
    await tx.send(9);
    expect(registry.pollOutgoing()).toEqual({
      kind: "data",
      channelId: 1n,
      payload: Uint8Array.of(9, 0, 0, 0),
    });
  });

  // r[verify rpc.channel.discovery]
  it("discovers direct channel arguments left-to-right before assigning wire indexes", async () => {
    const [serverSends, callerReceives] = channel<unknown>();
    const [callerSends, serverReceives] = channel<unknown>();
    const registry = new ChannelRegistry();
    const allocator = new ChannelIdAllocator(Role.Initiator);

    const bound = bindPhonChannels(
      [serverSends, "ordinary", serverReceives],
      [
        { index: 2, direction: "rx", elementRoot: U32_ROOT },
        { index: 0, direction: "tx", elementRoot: U32_ROOT },
      ],
      allocator,
      registry,
      connectionEchoRegistry,
      { incoming: 4, outgoing: 4 },
    );

    expect(bound.channels).toEqual([1n, 3n]);
    expect(bound.values).toEqual([
      Uint8Array.of(0, 0, 0, 0),
      "ordinary",
      Uint8Array.of(1, 0, 0, 0),
    ]);

    registry.routeData(1n, Uint8Array.of(7, 0, 0, 0));
    await callerSends.send(8);

    await expect(callerReceives.recv()).resolves.toBe(7);
    expect(registry.pollOutgoing()).toEqual({
      kind: "data",
      channelId: 3n,
      payload: Uint8Array.of(8, 0, 0, 0),
    });
  });
});
