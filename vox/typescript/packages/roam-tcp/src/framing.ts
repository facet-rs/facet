// Length-prefixed framing for TCP streams.
//
// r[impl transport.bytestream.length-prefix] - Messages are prefixed with a
// 4-byte little-endian length header.

import net from "node:net";
import { type MessageTransport } from "@bearcove/roam-core";

/**
 * A length-prefixed TCP connection.
 *
 * Handles encoding/decoding of raw message bytes over a TCP socket using
 * 4-byte little-endian frame length prefixes.
 *
 * Implements the MessageTransport interface for use with Connection.
 */
export class LengthPrefixedFramed implements MessageTransport {
  private socket: net.Socket;
  private buf: Buffer = Buffer.alloc(0);
  private pendingFrames: Uint8Array[] = [];
  private waitingResolve: ((frame: Uint8Array | null) => void) | null = null;
  private closed = false;
  private error: Error | null = null;

  /** Last successfully decoded frame bytes (for error recovery/debugging). */
  lastDecoded: Uint8Array = new Uint8Array(0);

  constructor(socket: net.Socket) {
    this.socket = socket;

    socket.on("data", (chunk: Buffer) => {
      this.buf = Buffer.concat([this.buf, chunk]);
      this.processBuffer();
    });

    socket.on("error", (err: Error) => {
      this.error = err;
      this.closed = true;
      if (this.waitingResolve) {
        this.waitingResolve(null);
        this.waitingResolve = null;
      }
    });

    socket.on("close", () => {
      this.closed = true;
      if (this.waitingResolve) {
        this.waitingResolve(null);
        this.waitingResolve = null;
      }
    });
  }

  private processBuffer() {
    while (true) {
      if (this.buf.length < 4) break;

      const frameLen = this.buf.readUInt32LE(0);
      const needed = 4 + frameLen;
      if (this.buf.length < needed) break;

      const frameBytes = this.buf.subarray(4, needed);
      this.buf = this.buf.subarray(needed);
      const decoded = new Uint8Array(frameBytes);
      this.lastDecoded = decoded;

      if (this.waitingResolve) {
        this.waitingResolve(decoded);
        this.waitingResolve = null;
      } else {
        this.pendingFrames.push(decoded);
      }
    }
  }

  /** Get the underlying socket. */
  getSocket(): net.Socket {
    return this.socket;
  }

  /**
   * Send raw payload bytes over the connection.
   *
   * r[impl transport.bytestream.length-prefix] - 4-byte little-endian length + payload.
   */
  send(payload: Uint8Array): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      if (payload.length > 0xffff_ffff) {
        reject(new Error("frame too large for u32 length prefix"));
        return;
      }

      const framed = Buffer.alloc(4 + payload.length);
      framed.writeUInt32LE(payload.length, 0);
      framed.set(payload, 4);

      this.socket.write(framed, (err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  /**
   * Receive raw frame bytes with a timeout.
   *
   * Returns `null` if no frame received within timeout or connection closed.
   */
  recvTimeout(timeoutMs: number): Promise<Uint8Array | null> {
    // Check for queued frames first
    if (this.pendingFrames.length > 0) {
      return Promise.resolve(this.pendingFrames.shift()!);
    }

    // Check for errors or closed connection
    if (this.error) {
      const err = this.error;
      this.error = null;
      return Promise.reject(err);
    }
    if (this.closed) {
      return Promise.resolve(null);
    }

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waitingResolve = null;
        resolve(null);
      }, timeoutMs);

      this.waitingResolve = (frame) => {
        clearTimeout(timer);
        if (this.error) {
          const err = this.error;
          this.error = null;
          reject(err);
        } else {
          resolve(frame);
        }
      };
    });
  }

  /**
   * Receive raw frame bytes (blocking until one arrives or connection closes).
   */
  recv(): Promise<Uint8Array | null> {
    return this.recvTimeout(30000);
  }

  /** Close the connection. */
  close(): void {
    this.socket.destroy();
  }
}
