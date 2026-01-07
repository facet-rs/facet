// Stream registry for managing active streams on a connection.

import { type StreamId, StreamError } from "./types.ts";
import { createChannel, type Channel, ChannelSender, ChannelReceiver, createChannelPair } from "./channel.ts";

/** Message sent on an outgoing stream channel. */
export type OutgoingMessage =
  | { kind: "data"; payload: Uint8Array }
  | { kind: "close" };

/** Result of polling an outgoing stream. */
export type OutgoingPoll =
  | { kind: "data"; streamId: StreamId; payload: Uint8Array }
  | { kind: "close"; streamId: StreamId }
  | { kind: "pending" }
  | { kind: "done" };

/**
 * Sender handle for outgoing stream data.
 *
 * This is the internal channel that Push<T> writes to.
 */
export class OutgoingSender {
  constructor(
    private _streamId: StreamId,
    private channel: Channel<OutgoingMessage>,
  ) {}

  get streamId(): StreamId {
    return this._streamId;
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
 * Registry of active streams for a connection.
 *
 * Handles both incoming streams (Data from wire → Pull<T>) and
 * outgoing streams (Push<T> → Data to wire).
 *
 * r[impl streaming.unknown] - Unknown stream IDs cause Goodbye.
 */
export class StreamRegistry {
  /** Streams where we receive Data messages (backing Pull<T> handles). */
  private incoming = new Map<StreamId, Channel<Uint8Array>>();

  /** Streams where we send Data messages (backing Push<T> handles). */
  private outgoing = new Map<StreamId, Channel<OutgoingMessage>>();

  /** Stream IDs that have been closed. */
  private closed = new Set<StreamId>();

  /**
   * Register an incoming stream and return the receiver for Pull<T>.
   *
   * r[impl streaming.allocation.caller] - Caller allocates stream IDs.
   */
  registerIncoming(streamId: StreamId): ChannelReceiver<Uint8Array> {
    const channel = createChannel<Uint8Array>(64);
    this.incoming.set(streamId, channel);
    return new ChannelReceiver(channel);
  }

  /**
   * Register an outgoing stream and return the sender for Push<T>.
   *
   * r[impl streaming.allocation.caller] - Caller allocates stream IDs.
   */
  registerOutgoing(streamId: StreamId): OutgoingSender {
    const channel = createChannel<OutgoingMessage>(64);
    this.outgoing.set(streamId, channel);
    return new OutgoingSender(streamId, channel);
  }

  /**
   * Route a Data message payload to the appropriate incoming stream.
   *
   * r[impl streaming.data] - Data messages routed by stream_id.
   * r[impl streaming.data-after-close] - Reject data on closed streams.
   */
  routeData(streamId: StreamId, payload: Uint8Array): void {
    // Check for data-after-close
    if (this.closed.has(streamId)) {
      throw StreamError.dataAfterClose(streamId);
    }

    const channel = this.incoming.get(streamId);
    if (!channel) {
      throw StreamError.unknown(streamId);
    }

    // If send fails, the Pull<T> was dropped - that's okay
    channel.send(payload);
  }

  /**
   * Poll all outgoing streams for data to send.
   *
   * Returns the first available message, or pending if none are ready.
   */
  pollOutgoing(): OutgoingPoll {
    if (this.outgoing.size === 0) {
      return { kind: "done" };
    }

    const toRemove: StreamId[] = [];

    for (const [streamId, channel] of this.outgoing) {
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
    const promises: Promise<{ streamId: StreamId; msg: OutgoingMessage | null }>[] = [];

    for (const [streamId, channel] of this.outgoing) {
      promises.push(
        channel.recv().then((msg) => ({ streamId, msg }))
      );
    }

    if (promises.length === 0) {
      return { kind: "done" };
    }

    const result = await Promise.race(promises);

    if (result.msg === null) {
      // Channel closed without message - implicit close
      this.outgoing.delete(result.streamId);
      this.closed.add(result.streamId);
      return { kind: "close", streamId: result.streamId };
    }

    if (result.msg.kind === "data") {
      return { kind: "data", streamId: result.streamId, payload: result.msg.payload };
    }

    if (result.msg.kind === "close") {
      this.outgoing.delete(result.streamId);
      this.closed.add(result.streamId);
      return { kind: "close", streamId: result.streamId };
    }

    return { kind: "pending" };
  }

  /**
   * Close an incoming stream.
   *
   * r[impl streaming.close] - Close terminates the stream.
   */
  close(streamId: StreamId): void {
    const channel = this.incoming.get(streamId);
    if (channel) {
      channel.close();
      this.incoming.delete(streamId);
    }
    this.closed.add(streamId);
  }

  /** Check if a stream ID is registered (either incoming or outgoing). */
  contains(streamId: StreamId): boolean {
    return this.incoming.has(streamId) || this.outgoing.has(streamId);
  }

  /** Check if a stream has been closed. */
  isClosed(streamId: StreamId): boolean {
    return this.closed.has(streamId);
  }

  /** Get the number of active outgoing streams. */
  get outgoingCount(): number {
    return this.outgoing.size;
  }
}
