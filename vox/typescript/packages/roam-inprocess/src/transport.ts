import type { Link } from "@bearcove/roam-core";

export class InProcessLink implements Link {
  lastReceived: Uint8Array | undefined;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;

  constructor(private readonly deliverToPeer: (payload: Uint8Array) => void) {}

  pushMessage(payload: Uint8Array): void {
    if (this.closed) {
      return;
    }
    this.lastReceived = payload;
    if (this.waitingResolve) {
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve(payload);
    } else {
      this.pendingMessages.push(payload);
    }
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new Error("InProcessLink closed");
    }
    this.deliverToPeer(payload);
  }

  recv(): Promise<Uint8Array | null> {
    if (this.pendingMessages.length > 0) {
      return Promise.resolve(this.pendingMessages.shift()!);
    }
    if (this.closed) {
      return Promise.resolve(null);
    }
    return new Promise((resolve) => {
      this.waitingResolve = resolve;
    });
  }

  close(): void {
    this.closed = true;
    const resolve = this.waitingResolve;
    this.waitingResolve = null;
    resolve?.(null);
  }

  isClosed(): boolean {
    return this.closed;
  }
}

