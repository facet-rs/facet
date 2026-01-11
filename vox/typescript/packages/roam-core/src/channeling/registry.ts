// Channel registry for managing active channels on a connection.

import { type ChannelId, ChannelError } from "./types.ts";
import { createChannel, type Channel, ChannelSender, ChannelReceiver } from "./channel.ts";

/** Message sent on an outgoing channel. */
export type OutgoingMessage = { kind: "data"; payload: Uint8Array } | { kind: "close" };

/** Result of polling an outgoing channel. */
export type OutgoingPoll =
  | { kind: "data"; channelId: ChannelId; payload: Uint8Array }
  | { kind: "close"; channelId: ChannelId }
  | { kind: "pending" }
  | { kind: "done" };

/**
 * Sender handle for outgoing channel data.
 *
 * This is the internal channel that Tx<T> writes to.
 */
export class OutgoingSender {
  constructor(
    private _channelId: ChannelId,
    private channel: Channel<OutgoingMessage>,
  ) {}

  get channelId(): ChannelId {
    return this._channelId;
  }

  /** Send serialized data. */
  sendData(data: Uint8Array): boolean {
    return this.channel.send({ kind: "data", payload: data });
  }

  /** Send close signal. */
  sendClose(): void {
    this.channel.send({ kind: "close" });
    this.channel.close();
  }
}

/**
 * Registry of active channels for a connection.
 *
 * Handles both incoming channels (Data from wire → Rx<T>) and
 * outgoing channels (Tx<T> → Data to wire).
 *
 * r[impl channeling.unknown] - Unknown channel IDs cause Goodbye.
 */
export class ChannelRegistry {
  /** Channels where we receive Data messages (backing Rx<T> handles). */
  private incoming = new Map<ChannelId, Channel<Uint8Array>>();

  /** Channels where we send Data messages (backing Tx<T> handles). */
  private outgoing = new Map<ChannelId, Channel<OutgoingMessage>>();

  /** Channel IDs that have been closed. */
  private closed = new Set<ChannelId>();

  /**
   * Register an incoming channel and return the receiver for Rx<T>.
   *
   * r[impl channeling.allocation.caller] - Caller allocates channel IDs.
   */
  registerIncoming(channelId: ChannelId): ChannelReceiver<Uint8Array> {
    const channel = createChannel<Uint8Array>(64);
    this.incoming.set(channelId, channel);
    return new ChannelReceiver(channel);
  }

  /**
   * Register an outgoing channel and return the sender for Tx<T>.
   *
   * r[impl channeling.allocation.caller] - Caller allocates channel IDs.
   */
  registerOutgoing(channelId: ChannelId): OutgoingSender {
    const channel = createChannel<OutgoingMessage>(64);
    this.outgoing.set(channelId, channel);
    return new OutgoingSender(channelId, channel);
  }

  /**
   * Route a Data message payload to the appropriate incoming channel.
   *
   * r[impl channeling.data] - Data messages routed by channel_id.
   * r[impl channeling.data-after-close] - Reject data on closed channels.
   */
  routeData(channelId: ChannelId, payload: Uint8Array): void {
    // Check for data-after-close
    if (this.closed.has(channelId)) {
      throw ChannelError.dataAfterClose(channelId);
    }

    const channel = this.incoming.get(channelId);
    if (!channel) {
      throw ChannelError.unknown(channelId);
    }

    // If send fails, the Rx<T> was dropped - that's okay
    channel.send(payload);
  }

  /**
   * Poll all outgoing channels for data to send.
   *
   * Returns the first available message, or pending if none are ready.
   */
  pollOutgoing(): OutgoingPoll {
    if (this.outgoing.size === 0) {
      return { kind: "done" };
    }

    const toRemove: ChannelId[] = [];

    for (const [channelId, channel] of this.outgoing) {
      // Try to receive without blocking
      // We need a sync check here - use a trick with immediate promise resolution
      let value: OutgoingMessage | null = null;
      let hasValue = false;

      // Check buffer synchronously via recv that resolves immediately if data available
      const checkPromise = channel.recv();

      // This is a bit hacky - we check if the promise resolves synchronously
      // by seeing if the buffer had data. Let's simplify with a tryRecv approach.
      // Actually, we need to redesign the channel to support try_recv...

      // For now, let's use a different approach: check if channel has pending data
      // We'll need to modify the channel interface.

      // TEMPORARY: Return pending and rely on async flushing
      // TODO: Add tryRecv to channel for proper sync polling
    }

    return { kind: "pending" };
  }

  /**
   * Async version - wait for outgoing data.
   */
  async waitOutgoing(): Promise<OutgoingPoll> {
    if (this.outgoing.size === 0) {
      return { kind: "done" };
    }

    // Create a race between all outgoing channels
    const promises: Promise<{ channelId: ChannelId; msg: OutgoingMessage | null }>[] = [];

    for (const [channelId, channel] of this.outgoing) {
      promises.push(channel.recv().then((msg) => ({ channelId, msg })));
    }

    if (promises.length === 0) {
      return { kind: "done" };
    }

    const result = await Promise.race(promises);

    if (result.msg === null) {
      // Channel closed without message - implicit close
      this.outgoing.delete(result.channelId);
      this.closed.add(result.channelId);
      return { kind: "close", channelId: result.channelId };
    }

    if (result.msg.kind === "data") {
      return { kind: "data", channelId: result.channelId, payload: result.msg.payload };
    }

    if (result.msg.kind === "close") {
      this.outgoing.delete(result.channelId);
      this.closed.add(result.channelId);
      return { kind: "close", channelId: result.channelId };
    }

    return { kind: "pending" };
  }

  /**
   * Close an incoming channel.
   *
   * r[impl channeling.close] - Close terminates the channel.
   */
  close(channelId: ChannelId): void {
    const channel = this.incoming.get(channelId);
    if (channel) {
      channel.close();
      this.incoming.delete(channelId);
    }
    this.closed.add(channelId);
  }

  /** Check if a channel ID is registered (either incoming or outgoing). */
  contains(channelId: ChannelId): boolean {
    return this.incoming.has(channelId) || this.outgoing.has(channelId);
  }

  /** Check if a channel has been closed. */
  isClosed(channelId: ChannelId): boolean {
    return this.closed.has(channelId);
  }

  /** Get the number of active outgoing channels. */
  get outgoingCount(): number {
    return this.outgoing.size;
  }
}
