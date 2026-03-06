import { describe, expect, it } from "vitest";
import { encodeWithSchema } from "@bearcove/roam-postcard";
import {
  decodeMessage,
  encodeMessage,
  helloYourself,
  messageData,
  messageHelloYourself,
  messageResponse,
  messageClose,
  parityEven,
  type Message,
} from "@bearcove/roam-wire";

import { bindChannels, channel, type Schema } from "./channeling/index.ts";
import { defaultHello, helloExchangeInitiator } from "./connection.ts";
import type { MessageTransport } from "./transport.ts";

class ScriptedTransport implements MessageTransport {
  lastDecoded: Uint8Array = new Uint8Array(0);

  private queue: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  async send(payload: Uint8Array): Promise<void> {
    this.lastDecoded = payload;
    const msg = decodeMessage(payload).value as Message;

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

describe("channeling connection liveness", () => {
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
});
