// Rx channel handle - caller receives data from callee.

import { type ChannelId, ChannelError } from "./types.ts";
import { ChannelReceiver } from "./channel.ts";
import { ChannelRegistry } from "./registry.ts";

// Forward declaration for pair reference
import type { Tx } from "./tx.ts";

/**
 * Rx channel handle - caller receives data from callee.
 *
 * r[impl channeling.caller-pov] - From caller's perspective, Rx means "I receive".
 * r[impl channeling.type] - Serializes as u64 channel ID on wire.
 * r[impl channeling.holder-semantics] - The holder receives from this channel.
 *
 * # Two modes of operation
 *
 * - **Unbound**: Created via `channel<T>()`, no channel ID yet.
 *   Must be bound before use by passing to a method call.
 * - **Server side**: Created in dispatch, channel registered for incoming Data routing.
 *   Created via `createServerRx()` in generated dispatch code.
 *
 * @template T - The type of values being received (needs a deserializer).
 */
export class Rx<T> {
  private _channelId: ChannelId | undefined;
  private receiver: ChannelReceiver<Uint8Array> | undefined;
  private deserialize: ((bytes: Uint8Array) => T) | undefined;

  /** Reference to the paired Tx (set by channel<T>()). */
  _pair: Tx<T> | undefined;

  /** Whether this Rx has been consumed (bound to a call). */
  private _consumed = false;

  /** Create an unbound Rx (for use with channel<T>()). */
  constructor();
  /** Create a server-side Rx with a receiver. */
  constructor(
    channelId: ChannelId,
    receiver: ChannelReceiver<Uint8Array>,
    deserialize: (bytes: Uint8Array) => T,
  );
  constructor(
    channelId?: ChannelId,
    receiver?: ChannelReceiver<Uint8Array>,
    deserialize?: (bytes: Uint8Array) => T,
  ) {
    if (channelId !== undefined && receiver !== undefined && deserialize !== undefined) {
      // Server-side constructor
      this._channelId = channelId;
      this.receiver = receiver;
      this.deserialize = deserialize;
      this._consumed = true; // Server-side Rx is immediately bound
    }
    // Otherwise: unbound Rx, all fields stay undefined
  }

  /** Get the channel ID. Throws if not bound. */
  get channelId(): ChannelId {
    if (this._channelId === undefined) {
      throw ChannelError.notBound("Rx");
    }
    return this._channelId;
  }

  /** Check if this Rx is bound to a channel. */
  get isBound(): boolean {
    return this._channelId !== undefined;
  }

  /** Check if this Rx has been consumed. */
  get isConsumed(): boolean {
    return this._consumed;
  }

  /**
   * Bind this Rx to a channel ID and registry.
   *
   * Called by the runtime binder when this Rx is passed to a method.
   * Also binds the paired Tx if present.
   *
   * @param channelId - The allocated channel ID
   * @param registry - The channel registry to register with
   * @param deserialize - Function to deserialize values
   */
  bind(
    channelId: ChannelId,
    registry: ChannelRegistry,
    deserialize: (bytes: Uint8Array) => T,
  ): void {
    if (this._consumed) {
      throw ChannelError.alreadyConsumed("Rx");
    }

    this._channelId = channelId;
    this.receiver = registry.registerIncoming(channelId);
    this.deserialize = deserialize;
    this._consumed = true;
  }

  /**
   * Receive the next value from this channel.
   *
   * Returns the value, or null when the channel is closed.
   *
   * r[impl channeling.data] - Deserialize Data message payloads.
   *
   * @throws If the Rx is not bound yet
   */
  async recv(): Promise<T | null> {
    if (!this.isBound || this.receiver === undefined || this.deserialize === undefined) {
      throw ChannelError.notBound("Rx");
    }

    const bytes = await this.receiver.recv();
    if (bytes === null) {
      return null; // Channel closed
    }

    try {
      return this.deserialize(bytes);
    } catch (e) {
      throw ChannelError.deserialize(e);
    }
  }

  /**
   * Iterate over all values in the channel.
   *
   * This is an async iterator that yields values until the channel closes.
   *
   * @throws If the Rx is not bound yet
   */
  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    while (true) {
      const value = await this.recv();
      if (value === null) {
        return;
      }
      yield value;
    }
  }
}

/**
 * Create a server-side Rx channel.
 *
 * Used by generated dispatch code to hydrate Rx arguments.
 * The channel is registered with the channel registry for Data routing.
 *
 * @param channelId - The channel ID from the wire (allocated by caller)
 * @param receiver - Channel receiver for incoming Data payloads
 * @param deserialize - Function to deserialize bytes to values
 */
export function createServerRx<T>(
  channelId: ChannelId,
  receiver: ChannelReceiver<Uint8Array>,
  deserialize: (bytes: Uint8Array) => T,
): Rx<T> {
  return new Rx(channelId, receiver, deserialize);
}
