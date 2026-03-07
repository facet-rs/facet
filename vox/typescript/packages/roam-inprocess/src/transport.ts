// In-process transport for roam messages.
//
// Enables direct WASM ↔ TypeScript communication within the same browser tab
// via in-memory message passing, with no network involved.

import type { MessageTransport } from "@bearcove/roam-core";

/**
 * In-process transport for roam messages.
 *
 * Communicates with Rust WASM via callback functions rather than a network socket.
 * - `send()` calls `deliverToRust` to push bytes into the Rust receive channel.
 * - `pushMessage()` is called by Rust (via js_sys::Function callback) to deliver bytes to TS.
 */
export class InProcessTransport implements MessageTransport {
  private deliverToRust: (payload: Uint8Array) => void;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((msg: Uint8Array | null) => void) | null = null;
  private closed = false;

  /** Last decoded bytes (for error detection). */
  lastDecoded: Uint8Array = new Uint8Array(0);

  constructor(deliverToRust: (payload: Uint8Array) => void) {
    this.deliverToRust = deliverToRust;
  }

  /**
   * Called by Rust (via the on_message callback) to push a message into the TS side.
   */
  pushMessage(payload: Uint8Array): void {
    if (this.closed) return;

    this.lastDecoded = payload;

    if (this.waitingResolve) {
      this.waitingResolve(payload);
      this.waitingResolve = null;
    } else {
      this.pendingMessages.push(payload);
    }
  }

  /**
   * Send a message to Rust.
   */
  async send(payload: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new Error("InProcessTransport is closed");
    }
    this.deliverToRust(payload);
  }

  /**
   * Receive a message with timeout.
   *
   * Returns null if timeout expires or transport is closed.
   */
  async recvTimeout(timeoutMs: number): Promise<Uint8Array | null> {
    if (this.pendingMessages.length > 0) {
      return this.pendingMessages.shift()!;
    }

    if (this.closed) {
      return null;
    }

    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.waitingResolve = null;
        resolve(null);
      }, timeoutMs);

      this.waitingResolve = (msg) => {
        clearTimeout(timer);
        resolve(msg);
      };
    });
  }

  /**
   * Close the transport.
   */
  close(): void {
    this.closed = true;
    if (this.waitingResolve) {
      this.waitingResolve(null);
      this.waitingResolve = null;
    }
  }

  /**
   * Returns true if the transport is permanently closed.
   */
  isClosed(): boolean {
    return this.closed;
  }
}
