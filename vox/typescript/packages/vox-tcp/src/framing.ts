// Length-prefixed framing for TCP streams.
//
// r[impl transport.stream]
// r[impl transport.stream.kinds]
// r[impl link]
// r[impl link.message]
// r[impl link.message.empty]
// r[impl link.order]
// r[impl link.rx.recv]
// r[impl link.rx.eof]
// r[impl link.rx.error]
// r[impl link.tx.alloc.limits]
// r[impl link.tx.cancel-safe]
// r[impl link.tx.send]
// r[impl link.tx.close]

import net from "node:net";
import type { Link } from "@bearcove/vox-core";

const LINK_MAGIC = new Uint8Array([0x56, 0x4f, 0x58, 0x4c]); // VOXL
const LINK_VERSION = 1;
const LINK_FLAG_FD_CAPABLE = 0x01;
const LINK_PROLOGUE_LEN = 6;
const LINK_PROLOGUE = Buffer.from([
  ...LINK_MAGIC,
  LINK_VERSION,
  0,
]);

function sameBytes(lhs: Uint8Array, rhs: Uint8Array): boolean {
  return lhs.length === rhs.length && lhs.every((value, idx) => rhs[idx] === value);
}

/**
 * A length-prefixed TCP connection.
 *
 * Handles encoding/decoding of raw message bytes over a TCP socket using
 * 4-byte little-endian frame length prefixes.
 *
 * Implements the MessageTransport interface for use with Connection.
 */
export class LengthPrefixedFramed implements Link {
  private socket: net.Socket;
  private buf: Buffer = Buffer.alloc(0);
  private pendingFrames: Uint8Array[] = [];
  private waitingResolve: ((frame: Uint8Array | null) => void) | null = null;
  private closed = false;
  private error: Error | null = null;
  private peerPrologueReceived = false;

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

    socket.write(LINK_PROLOGUE);
  }

  private processBuffer() {
    if (!this.peerPrologueReceived) {
      if (this.buf.length < LINK_PROLOGUE_LEN) return;

      const prologue = this.buf.subarray(0, LINK_PROLOGUE_LEN);
      this.buf = this.buf.subarray(LINK_PROLOGUE_LEN);
      if (!sameBytes(prologue.subarray(0, 4), LINK_MAGIC)) {
        this.fail(new Error(
          `bad vox link magic: expected ${Array.from(LINK_MAGIC)}, got ${Array.from(prologue.subarray(0, 4))}`,
        ));
        return;
      }
      const version = prologue[4] ?? 0;
      if (version !== LINK_VERSION) {
        this.fail(new Error(`unsupported vox link version ${version}: this build speaks ${LINK_VERSION}`));
        return;
      }
      const peerFdCapable = ((prologue[5] ?? 0) & LINK_FLAG_FD_CAPABLE) !== 0;
      if (peerFdCapable) {
        this.fail(new Error("vox link fd-capability mismatch: peer=true, local=false"));
        return;
      }
      this.peerPrologueReceived = true;
    }

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
   * r[impl transport.stream] - 4-byte little-endian length + payload.
   */
  send(payload: Uint8Array): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      if (this.error) {
        reject(this.error);
        return;
      }
      if (this.closed) {
        reject(new Error("link is closed"));
        return;
      }
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
  // r[impl rpc.transport.stream.cancel-safe-recv]
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

  /** Returns true if the connection is permanently closed. */
  isClosed(): boolean {
    return this.closed;
  }

  private fail(error: Error): void {
    this.error = error;
    this.closed = true;
    if (this.waitingResolve) {
      this.waitingResolve(null);
      this.waitingResolve = null;
    }
    this.socket.destroy(error);
  }
}
