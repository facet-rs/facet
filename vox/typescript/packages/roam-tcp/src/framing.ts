// COBS framing for TCP streams.
//
// r[impl transport.bytestream.cobs] - Messages are COBS-encoded with 0x00 delimiter.

import net from "node:net";
import { cobsDecode, cobsEncode } from "@bearcove/roam-core";

/**
 * A COBS-framed TCP connection.
 *
 * Handles encoding/decoding of raw message bytes over a TCP socket using
 * COBS (Consistent Overhead Byte Stuffing) framing with 0x00 delimiters.
 */
export class CobsFramed {
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
      const idx = this.buf.indexOf(0x00);
      if (idx < 0) break;

      const frameBytes = this.buf.subarray(0, idx);
      this.buf = this.buf.subarray(idx + 1);

      if (frameBytes.length === 0) continue;

      // r[impl transport.bytestream.cobs] - decode COBS-encoded frame
      try {
        const decoded = cobsDecode(new Uint8Array(frameBytes));
        this.lastDecoded = decoded;

        if (this.waitingResolve) {
          this.waitingResolve(decoded);
          this.waitingResolve = null;
        } else {
          this.pendingFrames.push(decoded);
        }
      } catch {
        // COBS decode error - store as error for next recv
        this.error = new Error("cobs decode error");
        if (this.waitingResolve) {
          this.waitingResolve(null);
          this.waitingResolve = null;
        }
        return;
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
   * r[impl transport.bytestream.cobs] - COBS encode with 0x00 delimiter.
   */
  send(payload: Uint8Array): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const encoded = cobsEncode(payload);
      const framed = Buffer.alloc(encoded.length + 1);
      framed.set(encoded);
      framed[encoded.length] = 0x00;

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
    this.socket.end();
  }
}
