// Push stream handle - caller sends data to callee.

import { type StreamId, StreamError } from "./types.ts";
import { OutgoingSender } from "./registry.ts";

/**
 * Push stream handle - caller sends data to callee.
 *
 * r[impl streaming.caller-pov] - From caller's perspective, Push means "I send".
 * r[impl streaming.type] - Serializes as u64 stream ID on wire.
 * r[impl streaming.holder-semantics] - The holder sends on this stream.
 *
 * @template T - The type of values being sent (needs a serializer).
 */
export class Push<T> {
  private closed = false;

  constructor(
    private sender: OutgoingSender,
    private serialize: (value: T) => Uint8Array,
  ) {}

  /** Get the stream ID. */
  get streamId(): StreamId {
    return this.sender.streamId;
  }

  /**
   * Send a value on this stream.
   *
   * r[impl streaming.data] - Data messages carry serialized values.
   */
  send(value: T): void {
    if (this.closed) {
      throw StreamError.closed();
    }

    let bytes: Uint8Array;
    try {
      bytes = this.serialize(value);
    } catch (e) {
      throw StreamError.serialize(e);
    }

    if (!this.sender.sendData(bytes)) {
      throw StreamError.closed();
    }
  }

  /**
   * Close this stream.
   *
   * r[impl streaming.lifecycle.caller-closes-pushes] - Caller sends Close when done.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.sender.sendClose();
  }

}

// Note: Symbol.dispose support for using-declarations would be nice but requires esnext target.
// For now, users should call close() explicitly or use try/finally.

/**
 * Create a Push stream with a simple passthrough (for raw bytes).
 */
export function createRawPush(sender: OutgoingSender): Push<Uint8Array> {
  return new Push(sender, (v) => v);
}

/**
 * Create a Push stream with a typed serializer.
 *
 * r[impl streaming.type] - Push serializes as stream_id on wire.
 */
export function createTypedPush<T>(sender: OutgoingSender, serialize: (value: T) => Uint8Array): Push<T> {
  return new Push(sender, serialize);
}
