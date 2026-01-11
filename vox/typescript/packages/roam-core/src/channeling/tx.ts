// Tx channel handle - caller sends data to callee.

import { type ChannelId, ChannelError } from "./types.ts";
import { OutgoingSender, ChannelRegistry } from "./registry.ts";
import { type TaskSender } from "./task.ts";

// Forward declaration for pair reference
import type { Rx } from "./rx.ts";

/**
 * Sender abstraction for Tx channels.
 *
 * Supports two modes:
 * - Client-side: uses OutgoingSender (buffered channel to drain task)
 * - Server-side: uses TaskSender (direct to connection driver)
 */
type TxSender =
  | { mode: "client"; sender: OutgoingSender }
  | { mode: "server"; channelId: ChannelId; taskSender: TaskSender };

/**
 * Tx channel handle - caller sends data to callee.
 *
 * r[impl channeling.caller-pov] - From caller's perspective, Tx means "I send".
 * r[impl channeling.type] - Serializes as u64 channel ID on wire.
 * r[impl channeling.holder-semantics] - The holder sends on this channel.
 *
 * # Two modes of operation
 *
 * - **Unbound**: Created via `channel<T>()`, no channel ID yet.
 *   Must be bound before use by passing the paired Rx to a method.
 * - **Server side**: Uses TaskSender to send Data/Close directly to driver.
 *   Created via `createServerTx()` in generated dispatch code.
 *
 * @template T - The type of values being sent (needs a serializer).
 */
export class Tx<T> {
  private closed = false;
  private _channelId: ChannelId | undefined;
  private sender: TxSender | undefined;
  private serialize: ((value: T) => Uint8Array) | undefined;

  /** Reference to the paired Rx (set by channel<T>()). */
  _pair: Rx<T> | undefined;

  /** Whether this Tx has been consumed (bound to a call). */
  private _consumed = false;

  /** Create an unbound Tx (for use with channel<T>()). */
  constructor();
  /** Create a server-side Tx with a TaskSender. */
  constructor(channelId: ChannelId, taskSender: TaskSender, serialize: (value: T) => Uint8Array);
  constructor(
    channelId?: ChannelId,
    taskSender?: TaskSender,
    serialize?: (value: T) => Uint8Array,
  ) {
    if (channelId !== undefined && taskSender !== undefined && serialize !== undefined) {
      // Server-side constructor
      this._channelId = channelId;
      this.sender = {
        mode: "server",
        channelId: channelId,
        taskSender: taskSender,
      };
      this.serialize = serialize;
      this._consumed = true; // Server-side Tx is immediately bound
    }
    // Otherwise: unbound Tx, all fields stay undefined
  }

  /** Get the channel ID. Throws if not bound. */
  get channelId(): ChannelId {
    if (this._channelId === undefined) {
      throw ChannelError.notBound("Tx");
    }
    return this._channelId;
  }

  /** Check if this Tx is bound to a channel. */
  get isBound(): boolean {
    return this._channelId !== undefined;
  }

  /** Check if this Tx has been consumed. */
  get isConsumed(): boolean {
    return this._consumed;
  }

  /**
   * Bind this Tx to a channel ID and registry for SENDING.
   *
   * Called by the runtime binder when this Tx's paired Rx is passed to a method
   * (schema Rx = client sends, server receives).
   *
   * @param channelId - The allocated channel ID
   * @param registry - The channel registry to register with
   * @param serialize - Function to serialize values
   */
  bind(channelId: ChannelId, registry: ChannelRegistry, serialize: (value: T) => Uint8Array): void {
    if (this._consumed) {
      throw ChannelError.alreadyConsumed("Tx");
    }

    this._channelId = channelId;
    const outgoing = registry.registerOutgoing(channelId);
    this.sender = { mode: "client", sender: outgoing };
    this.serialize = serialize;
    this._consumed = true;
  }

  /**
   * Set just the channel ID without registering for sending.
   *
   * Used when this Tx is passed as an argument to a method
   * (schema Tx = server sends, client receives).
   * The client doesn't send on this channel - it just needs the ID for encoding.
   *
   * @param channelId - The allocated channel ID
   */
  setChannelIdOnly(channelId: ChannelId): void {
    if (this._consumed) {
      throw ChannelError.alreadyConsumed("Tx");
    }
    this._channelId = channelId;
    this._consumed = true;
  }

  /**
   * Send a value on this channel.
   *
   * r[impl channeling.data] - Data messages carry serialized values.
   *
   * @throws If the Tx is not bound yet
   */
  async send(value: T): Promise<void> {
    if (!this.isBound || this.sender === undefined || this.serialize === undefined) {
      throw ChannelError.notBound("Tx");
    }

    if (this.closed) {
      throw ChannelError.closed();
    }

    let bytes: Uint8Array;
    try {
      bytes = this.serialize(value);
    } catch (e) {
      throw ChannelError.serialize(e);
    }

    if (this.sender.mode === "client") {
      if (!this.sender.sender.sendData(bytes)) {
        throw ChannelError.closed();
      }
    } else {
      // Server-side: send directly via task channel
      this.sender.taskSender({
        kind: "data",
        channelId: this.sender.channelId,
        payload: bytes,
      });
    }
  }

  /**
   * Close this channel.
   *
   * r[impl channeling.lifecycle.caller-closes-pushes] - Caller sends Close when done.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;

    if (this.sender === undefined) {
      // Not bound yet, nothing to close
      return;
    }

    if (this.sender.mode === "client") {
      this.sender.sender.sendClose();
    } else {
      // Server-side: send Close via task channel
      this.sender.taskSender({
        kind: "close",
        channelId: this.sender.channelId,
      });
    }
  }
}

// Note: Symbol.dispose support for using-declarations would be nice but requires esnext target.
// For now, users should call close() explicitly or use try/finally.

/**
 * Create a server-side Tx channel that sends directly via the task channel.
 *
 * Used by generated dispatch code to hydrate Tx arguments.
 * When the handler calls tx.send(), Data messages go directly to the driver.
 * When the handler is done and calls tx.close(), a Close message is sent.
 *
 * @param channelId - The channel ID from the wire (allocated by caller)
 * @param taskSender - Callback to send TaskMessage to driver
 * @param serialize - Function to serialize values to bytes
 */
export function createServerTx<T>(
  channelId: ChannelId,
  taskSender: TaskSender,
  serialize: (value: T) => Uint8Array,
): Tx<T> {
  return new Tx(channelId, taskSender, serialize);
}
