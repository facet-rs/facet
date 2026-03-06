import { describe, expect, it } from "vitest";
import { encodeWithSchema } from "@bearcove/roam-postcard";
import {
  decodeMessage,
  encodeMessage,
  helloYourself,
  messageData,
  messageCredit,
  messageHelloYourself,
  messageResponse,
  messageClose,
  parityEven,
  type Message,
} from "@bearcove/roam-wire";

import {
  bindChannels,
  channel,
  ChannelIdAllocator,
  ChannelRegistry,
  Role,
  type Schema,
} from "./channeling/index.ts";
import { DEFAULT_INITIAL_CREDIT } from "./channeling/types.ts";
import { defaultHello, helloExchangeInitiator } from "./connection.ts";
import type { MessageTransport } from "./transport.ts";

class ScriptedTransport implements MessageTransport {
  lastDecoded: Uint8Array = new Uint8Array(0);
  sent: Message[] = [];

  private queue: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  async send(payload: Uint8Array): Promise<void> {
    this.lastDecoded = payload;
    const msg = decodeMessage(payload).value as Message;
    this.sent.push(msg);

    if (msg.payload.tag === "Hello") {
      const hy = helloYourself(parityEven(), 64);
      this.enqueue(encodeMessage(messageHelloYourself(hy)));
      return;
    }

    if (msg.payload.tag === "RequestMessage" && msg.payload.value.body.tag === "Call") {
      this.enqueue(encodeMessage(messageResponse(msg.payload.value.id, new Uint8Array([0x2a]))));
    }
  }

  async recvTimeout(timeoutMs: number): Promise<Uint8Array | null> {
    if (this.queue.length > 0) {
      return this.queue.shift()!;
    }
    if (this.closed) {
      return null;
    }
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.waitingResolve = null;
        resolve(null);
      }, timeoutMs);
      this.waitingResolve = (payload) => {
        clearTimeout(timer);
        resolve(payload);
      };
    });
  }

  close(): void {
    this.closed = true;
    if (this.waitingResolve) {
      this.waitingResolve(null);
      this.waitingResolve = null;
    }
  }

  isClosed(): boolean {
    return this.closed;
  }

  enqueue(payload: Uint8Array): void {
    if (this.waitingResolve) {
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve(payload);
      return;
    }
    this.queue.push(payload);
  }
}

function settleWithin<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`promise did not settle within ${timeoutMs}ms`));
    }, timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

async function waitForGrantCredit(
  transport: ScriptedTransport,
  channelId: bigint,
  timeoutMs: number,
): Promise<Message> {
  return settleWithin(
    (async () => {
      while (true) {
        let grant: Message | undefined;
        for (let i = transport.sent.length - 1; i >= 0; i--) {
          const msg = transport.sent[i];
          if (
            msg.payload.tag === "ChannelMessage" &&
            msg.payload.value.id === channelId &&
            msg.payload.value.body.tag === "GrantCredit"
          ) {
            grant = msg;
            break;
          }
        }
        if (grant) {
          return grant;
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    })(),
    timeoutMs,
  );
}

describe("channeling connection liveness", () => {
  it("binds incoming buffers to the channel initial credit instead of the transport default", () => {
    const registry = new ChannelRegistry();
    const allocator = new ChannelIdAllocator(Role.Initiator);
    const [outputTx] = channel<number>();
    const [channelId] = bindChannels(
      [
        {
          kind: "tx",
          initial_credit: DEFAULT_INITIAL_CREDIT,
          element: { kind: "u32" },
        } satisfies Schema,
      ],
      [outputTx],
      allocator,
      registry,
    );

    for (let i = 0; i < DEFAULT_INITIAL_CREDIT; i++) {
      expect(() =>
        registry.routeData(channelId, encodeWithSchema(i, { kind: "u32" })),
      ).not.toThrow();
    }

    expect(() =>
      registry.routeData(channelId, encodeWithSchema(999, { kind: "u32" })),
    ).toThrow(/overflow/);
  });

  it("applies GrantCredit to blocked outgoing tx handles", async () => {
    const registry = new ChannelRegistry();
    const allocator = new ChannelIdAllocator(Role.Initiator);
    const [, inputRx] = channel<number>();
    const [inputTx] = [inputRx._pair!];
    const [channelId] = bindChannels(
      [
        {
          kind: "rx",
          initial_credit: 1,
          element: { kind: "u32" },
        } satisfies Schema,
      ],
      [inputRx],
      allocator,
      registry,
    );

    await inputTx.send(1);
    const secondSend = inputTx.send(2);
    await expect(settleWithin(secondSend, 50)).rejects.toThrow(/did not settle/);

    registry.grantCredit(channelId, 1);
    await expect(settleWithin(secondSend, 500)).resolves.toBeUndefined();
  });

  it("keeps servicing bound stream rx handles after the initial response", async () => {
    const transport = new ScriptedTransport();
    const conn = await helloExchangeInitiator(transport, defaultHello());

    const [updatesTx, updatesRx] = channel<number>();
    const channelIds = bindChannels(
      [{ kind: "tx", element: { kind: "u32" } } satisfies Schema],
      [updatesTx],
      conn.getChannelAllocator(),
      conn.getChannelRegistry(),
    );

    await conn.call(1n, new Uint8Array([1]), 200, channelIds);

    transport.enqueue(
      encodeMessage(messageData(channelIds[0], encodeWithSchema(123, { kind: "u32" }))),
    );
    await expect(settleWithin(updatesRx.recv(), 500)).resolves.toBe(123);

    transport.enqueue(encodeMessage(messageClose(channelIds[0])));
    await expect(settleWithin(updatesRx.recv(), 500)).resolves.toBeNull();
  });

  it("sends GrantCredit after the caller consumes a streamed item", async () => {
    const transport = new ScriptedTransport();
    const conn = await helloExchangeInitiator(transport, defaultHello());

    const [updatesTx, updatesRx] = channel<number>();
    const channelIds = bindChannels(
      [
        {
          kind: "tx",
          initial_credit: DEFAULT_INITIAL_CREDIT,
          element: { kind: "u32" },
        } satisfies Schema,
      ],
      [updatesTx],
      conn.getChannelAllocator(),
      conn.getChannelRegistry(),
    );

    await conn.call(1n, new Uint8Array([1]), 200, channelIds);

    transport.enqueue(
      encodeMessage(messageData(channelIds[0], encodeWithSchema(123, { kind: "u32" }))),
    );
    await expect(settleWithin(updatesRx.recv(), 500)).resolves.toBe(123);

    await expect(
      waitForGrantCredit(transport, channelIds[0], 500),
    ).resolves.toMatchObject(messageCredit(channelIds[0], 1));
  });
});
