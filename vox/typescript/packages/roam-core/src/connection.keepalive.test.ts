import { describe, it, expect } from "vitest";
import {
  decodeMessage,
  encodeMessage,
  helloYourself,
  messageHelloYourself,
  messagePong,
  messageResponse,
  parityEven,
  type Message,
} from "@bearcove/roam-wire";
import { defaultHello, helloExchangeInitiator } from "./connection.ts";
import type { MessageTransport } from "./transport.ts";

class ScriptedTransport implements MessageTransport {
  lastDecoded: Uint8Array = new Uint8Array(0);
  sentTags: string[] = [];

  private queue: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  constructor(
    private readonly options: {
      autoRespondPing?: boolean;
      autoRespondCalls?: boolean;
      responseDelayMs?: number;
      responsePayload?: Uint8Array;
    } = {},
  ) {}

  async send(payload: Uint8Array): Promise<void> {
    this.lastDecoded = payload;
    const msg = decodeMessage(payload).value as Message;
    this.sentTags.push(msg.payload.tag);

    if (msg.payload.tag === "Hello") {
      const hy = helloYourself(parityEven(), 64);
      this.enqueue(encodeMessage(messageHelloYourself(hy)));
      return;
    }

    if (msg.payload.tag === "Ping" && this.options.autoRespondPing) {
      this.enqueue(encodeMessage(messagePong(msg.payload.value.nonce)));
      return;
    }

    if (
      msg.payload.tag === "RequestMessage" &&
      msg.payload.value.body.tag === "Call" &&
      this.options.autoRespondCalls
    ) {
      const requestId = msg.payload.value.id;
      const responsePayload = this.options.responsePayload ?? new Uint8Array([0x42]);
      const delayMs = this.options.responseDelayMs ?? 0;
      setTimeout(() => {
        this.enqueue(encodeMessage(messageResponse(requestId, responsePayload)));
      }, delayMs);
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

  private enqueue(payload: Uint8Array): void {
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

describe("connection keepalive", () => {
  it("fails pending calls when pong responses are missing", async () => {
    const transport = new ScriptedTransport();
    const conn = await helloExchangeInitiator(transport, defaultHello(), {
      keepalive: { pingIntervalMs: 20, pongTimeoutMs: 50 },
    });

    await expect(settleWithin(conn.call(1n, new Uint8Array([1]), 250), 500)).rejects.toMatchObject({
      kind: "closed",
    });
    expect(transport.sentTags).toContain("Ping");
  });

  it("keeps calls healthy when ping/pong succeeds", async () => {
    const transport = new ScriptedTransport({
      autoRespondPing: true,
      autoRespondCalls: true,
      responseDelayMs: 220,
      responsePayload: new Uint8Array([7, 8, 9]),
    });
    const conn = await helloExchangeInitiator(transport, defaultHello(), {
      keepalive: { pingIntervalMs: 20, pongTimeoutMs: 50 },
    });

    const response = await settleWithin(conn.call(1n, new Uint8Array([1]), 1000), 1200);
    expect(Array.from(response)).toEqual([7, 8, 9]);
    expect(transport.sentTags).toContain("Ping");
  });
});
