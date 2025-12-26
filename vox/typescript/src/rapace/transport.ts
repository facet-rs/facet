/**
 * Transport abstraction for rapace.
 *
 * Supports both WebSocket (browser + Node) and can be extended for TCP.
 */

import { Frame } from "./frame.js";

/**
 * Transport errors.
 */
export class TransportError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportError";
  }
}

/**
 * Transport interface for sending and receiving frames.
 */
export interface Transport {
  /** Send a frame. */
  send(frame: Frame): Promise<void>;

  /** Receive the next frame. */
  recv(): Promise<Frame>;

  /** Close the transport. */
  close(): void;

  /** Check if the transport is closed. */
  readonly isClosed: boolean;
}

/**
 * WebSocket-based transport.
 *
 * Works in both browser and Node.js environments.
 * Uses the standard WebSocket API which is available in:
 * - Browsers natively
 * - Node.js 22+ natively
 * - Node.js <22 with ws package (which provides compatible API)
 */
export class WebSocketTransport implements Transport {
  private ws: WebSocket;
  private closed = false;
  private receiveQueue: Frame[] = [];
  private receiveWaiters: Array<{
    resolve: (frame: Frame) => void;
    reject: (error: Error) => void;
  }> = [];
  private pendingData: Uint8Array | null = null;
  private connectionPromise: Promise<void>;
  private connectionError: Error | null = null;

  constructor(url: string) {
    this.ws = new WebSocket(url);
    this.ws.binaryType = "arraybuffer";

    this.connectionPromise = new Promise((resolve, reject) => {
      this.ws.onopen = () => resolve();
      this.ws.onerror = () => {
        const error = new TransportError("WebSocket error");
        this.connectionError = error;
        reject(error);
      };
    });

    this.ws.onmessage = (event) => {
      this.handleMessage(event.data);
    };

    this.ws.onclose = () => {
      this.closed = true;
      // Reject all pending waiters
      for (const waiter of this.receiveWaiters) {
        waiter.reject(new TransportError("Connection closed"));
      }
      this.receiveWaiters = [];
    };
  }

  /**
   * Wait for the connection to be established.
   */
  async waitForConnection(): Promise<void> {
    await this.connectionPromise;
  }

  private handleMessage(data: ArrayBuffer): void {
    // Append to pending data
    const newData = new Uint8Array(data);
    if (this.pendingData) {
      const combined = new Uint8Array(this.pendingData.length + newData.length);
      combined.set(this.pendingData);
      combined.set(newData, this.pendingData.length);
      this.pendingData = combined;
    } else {
      this.pendingData = newData;
    }

    // Try to parse complete frames
    while (this.pendingData && this.pendingData.length >= 4) {
      const view = new DataView(
        this.pendingData.buffer,
        this.pendingData.byteOffset,
        this.pendingData.byteLength
      );
      const frameLen = view.getUint32(0, true);
      const totalLen = 4 + frameLen;

      if (this.pendingData.length < totalLen) {
        // Not enough data yet
        break;
      }

      // Parse the frame
      try {
        const frameData = this.pendingData.slice(0, totalLen);
        const frame = Frame.parse(frameData);

        // Remove parsed data
        if (this.pendingData.length === totalLen) {
          this.pendingData = null;
        } else {
          this.pendingData = this.pendingData.slice(totalLen);
        }

        // Deliver to waiter or queue
        if (this.receiveWaiters.length > 0) {
          const waiter = this.receiveWaiters.shift()!;
          waiter.resolve(frame);
        } else {
          this.receiveQueue.push(frame);
        }
      } catch (error) {
        // Parsing error - close connection
        this.close();
        for (const waiter of this.receiveWaiters) {
          waiter.reject(error instanceof Error ? error : new Error(String(error)));
        }
        this.receiveWaiters = [];
        break;
      }
    }
  }

  async send(frame: Frame): Promise<void> {
    if (this.closed) {
      throw new TransportError("Transport is closed");
    }

    await this.connectionPromise;

    const data = frame.serialize();
    this.ws.send(data);
  }

  async recv(): Promise<Frame> {
    if (this.connectionError) {
      throw this.connectionError;
    }

    // Return queued frame if available
    if (this.receiveQueue.length > 0) {
      return this.receiveQueue.shift()!;
    }

    if (this.closed) {
      throw new TransportError("Transport is closed");
    }

    // Wait for next frame
    return new Promise((resolve, reject) => {
      this.receiveWaiters.push({ resolve, reject });
    });
  }

  close(): void {
    if (!this.closed) {
      this.closed = true;
      this.ws.close();
    }
  }

  get isClosed(): boolean {
    return this.closed;
  }
}

/**
 * Connect to a rapace server over WebSocket.
 *
 * @param url - WebSocket URL (e.g., "ws://localhost:8080")
 * @returns Connected transport
 */
export async function connectWebSocket(url: string): Promise<WebSocketTransport> {
  const transport = new WebSocketTransport(url);
  await transport.waitForConnection();
  return transport;
}
