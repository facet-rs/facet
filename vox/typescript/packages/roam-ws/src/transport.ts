// WebSocket transport for roam messages.
//
// r[impl transport.message.one-to-one] - Each WebSocket message = one roam message.
// r[impl transport.message.binary] - Uses binary WebSocket frames.

import type { MessageTransport } from "@bearcove/roam-core";

/**
 * WebSocket transport for roam messages.
 *
 * Works in both browser (native WebSocket) and Node.js environments.
 * Messages are postcard-encoded directly without COBS framing
 * since WebSocket provides native message boundaries.
 */
export class WsTransport implements MessageTransport {
  private ws: WebSocket;
  private pendingMessages: Uint8Array[] = [];
  private waitingResolve: ((msg: Uint8Array | null) => void) | null = null;
  private closed = false;
  private error: Error | null = null;

  /** Last decoded bytes (for error detection). */
  lastDecoded: Uint8Array = new Uint8Array(0);

  constructor(ws: WebSocket) {
    this.ws = ws;

    ws.binaryType = "arraybuffer";

    ws.addEventListener("message", (event: MessageEvent) => {
      if (event.data instanceof ArrayBuffer) {
        const data = new Uint8Array(event.data);
        this.lastDecoded = data;

        if (this.waitingResolve) {
          this.waitingResolve(data);
          this.waitingResolve = null;
        } else {
          this.pendingMessages.push(data);
        }
      }
      // Text frames are ignored (protocol violation but we handle gracefully)
    });

    ws.addEventListener("error", () => {
      this.error = new Error("WebSocket error");
      this.closed = true;
      if (this.waitingResolve) {
        this.waitingResolve(null);
        this.waitingResolve = null;
      }
    });

    ws.addEventListener("close", () => {
      this.closed = true;
      if (this.waitingResolve) {
        this.waitingResolve(null);
        this.waitingResolve = null;
      }
    });
  }

  /**
   * Send a message over WebSocket.
   *
   * r[impl transport.message.binary] - Send as binary frame.
   */
  async send(payload: Uint8Array): Promise<void> {
    if (this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("WebSocket not open");
    }
    this.ws.send(payload);
  }

  /**
   * Receive a message with timeout.
   *
   * Returns null if timeout expires or connection closes.
   */
  async recvTimeout(timeoutMs: number): Promise<Uint8Array | null> {
    // Check for queued messages first
    if (this.pendingMessages.length > 0) {
      return this.pendingMessages.shift()!;
    }

    // Check for errors or closed connection
    if (this.error) {
      const err = this.error;
      this.error = null;
      throw err;
    }
    if (this.closed) {
      return null;
    }

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waitingResolve = null;
        resolve(null);
      }, timeoutMs);

      this.waitingResolve = (msg) => {
        clearTimeout(timer);
        if (this.error) {
          const err = this.error;
          this.error = null;
          reject(err);
        } else {
          resolve(msg);
        }
      };
    });
  }

  /**
   * Close the WebSocket.
   */
  close(): void {
    this.ws.close();
  }
}

/**
 * Connect to a WebSocket server and return a transport.
 */
export function connectWs(url: string): Promise<WsTransport> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";

    ws.addEventListener("open", () => {
      resolve(new WsTransport(ws));
    });

    ws.addEventListener("error", () => {
      reject(new Error(`Failed to connect to ${url}`));
    });
  });
}
