import { describe, expect, it } from "vitest";
import {
  performAcceptorTransportPrologue,
  performInitiatorTransportPrologue,
} from "./transport_prologue.ts";

class MemoryLink {
  #closed = false;
  #queue: Uint8Array[] = [];
  #waiters: Array<(value: Uint8Array | null) => void> = [];
  private readonly deliver: (payload: Uint8Array) => void;

  constructor(deliver: (payload: Uint8Array) => void) {
    this.deliver = deliver;
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.#closed) {
      throw new Error("closed");
    }
    this.deliver(payload.slice());
  }

  async recv(): Promise<Uint8Array | null> {
    const queued = this.#queue.shift();
    if (queued) {
      return queued;
    }
    if (this.#closed) {
      return null;
    }
    return new Promise((resolve) => {
      this.#waiters.push(resolve);
    });
  }

  close(): void {
    this.#closed = true;
    for (const waiter of this.#waiters.splice(0)) {
      waiter(null);
    }
  }

  isClosed(): boolean {
    return this.#closed;
  }

  push(payload: Uint8Array): void {
    const waiter = this.#waiters.shift();
    if (waiter) {
      waiter(payload);
      return;
    }
    this.#queue.push(payload);
  }
}

function memoryLinkPair(): [MemoryLink, MemoryLink] {
  let left!: MemoryLink;
  let right!: MemoryLink;
  left = new MemoryLink((payload) => right.push(payload));
  right = new MemoryLink((payload) => left.push(payload));
  return [left, right];
}

const rejectUnsupportedPrologue = new Uint8Array([
  0x56,
  0x4F,
  0x54,
  0x52,
  9,
  1,
  0,
  0,
]);

describe("transport prologue", () => {
  // r[verify transport.prologue]
  // r[verify transport.prologue.request]
  // r[verify transport.prologue.accept]
  it("accepts the transport prologue", async () => {
    const [initiator, acceptor] = memoryLinkPair();
    const accepted = performAcceptorTransportPrologue(acceptor);
    await performInitiatorTransportPrologue(initiator);
    await expect(accepted).resolves.toBeUndefined();
  });

  // r[verify transport.prologue.reject-close]
  it("rejects unsupported transport prologues", async () => {
    const [initiator, acceptor] = memoryLinkPair();
    const rejected = performInitiatorTransportPrologue(initiator);
    const hello = await acceptor.recv();
    expect(hello).not.toBeNull();
    await acceptor.send(rejectUnsupportedPrologue);
    await expect(rejected).rejects.toThrow("transport rejected unsupported prologue");
  });
});
