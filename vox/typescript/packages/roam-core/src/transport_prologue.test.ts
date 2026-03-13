import { describe, expect, it } from "vitest";
import {
  acceptTransportMode,
  requestTransportMode,
} from "./transport_prologue.ts";

class MemoryLink {
  #closed = false;
  #queue: Uint8Array[] = [];
  #waiters: Array<(value: Uint8Array | null) => void> = [];

  constructor(private readonly deliver: (payload: Uint8Array) => void) {}

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

describe("transport prologue", () => {
  it("accepts bare mode", async () => {
    const [initiator, acceptor] = memoryLinkPair();
    const accepted = acceptTransportMode(acceptor);
    await requestTransportMode(initiator, "bare");
    await expect(accepted).resolves.toBe("bare");
  });

  it("accepts stable mode", async () => {
    const [initiator, acceptor] = memoryLinkPair();
    const accepted = acceptTransportMode(acceptor);
    await requestTransportMode(initiator, "stable");
    await expect(accepted).resolves.toBe("stable");
  });
});
