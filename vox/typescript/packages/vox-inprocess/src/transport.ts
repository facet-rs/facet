import type { Link } from "@bearcove/vox-core";

// r[impl link]
// r[impl link.message]
// r[impl link.message.empty]
// r[impl link.order]
// r[impl link.rx.recv]
// r[impl link.rx.eof]
// r[impl link.tx.send]
// r[impl link.tx.close]
export class InProcessLink implements Link {
  lastReceived: Uint8Array | undefined;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((payload: Uint8Array | null) => void) | null = null;
  private closed = false;
  private readonly deliverToPeer: (payload: Uint8Array) => void;
  private readonly closePeer?: () => void;

  constructor(deliverToPeer: (payload: Uint8Array) => void, closePeer?: () => void) {
    this.deliverToPeer = deliverToPeer;
    this.closePeer = closePeer;
  }

  pushMessage(payload: Uint8Array): void {
    if (this.closed) {
      return;
    }
    const owned = payload.slice();
    this.lastReceived = owned;
    if (this.waitingResolve) {
      const resolve = this.waitingResolve;
      this.waitingResolve = null;
      resolve(owned);
    } else {
      this.pendingMessages.push(owned);
    }
  }

  pushClose(): void {
    this.closeLocal();
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new Error("InProcessLink closed");
    }
    this.deliverToPeer(payload.slice());
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
    const wasClosed = this.closed;
    this.closeLocal();
    if (!wasClosed) {
      this.closePeer?.();
    }
  }

  private closeLocal(): void {
    this.closed = true;
    const resolve = this.waitingResolve;
    this.waitingResolve = null;
    resolve?.(null);
  }

  isClosed(): boolean {
    return this.closed;
  }
}
