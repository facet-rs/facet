// Channel registry for managing active channels on a connection.

import { type ChannelId, ChannelError, DEFAULT_INITIAL_CREDIT } from "./types.ts";
import { createChannel, type Channel, ChannelReceiver } from "./channel.ts";

/** Message sent on an outgoing channel. */
export type OutgoingMessage =
  | { kind: "data"; payload: Uint8Array }
  | { kind: "close" }
  | { kind: "credit"; additional: number };

/** Result of polling an outgoing channel. */
export type OutgoingPoll =
  | { kind: "data"; channelId: ChannelId; payload: Uint8Array }
  | { kind: "close"; channelId: ChannelId }
  | { kind: "credit"; channelId: ChannelId; additional: number }
  | { kind: "pending" }
  | { kind: "done" };

class AsyncQueue<T> {
  private items: T[] = [];
  private closed = false;
  private recvWaiters: Array<(value: T | null) => void> = [];
  private sendWaiters: Array<() => void> = [];

  constructor(private readonly capacity: number) {}

  async enqueue(value: T): Promise<boolean> {
    while (!this.closed && this.recvWaiters.length === 0 && this.items.length >= this.capacity) {
      await new Promise<void>((resolve) => {
        this.sendWaiters.push(resolve);
      });
    }

    if (this.closed) {
      return false;
    }

    const waiter = this.recvWaiters.shift();
    if (waiter) {
      waiter(value);
      return true;
    }

    this.items.push(value);
    return true;
  }

  async dequeue(): Promise<T | null> {
    if (this.items.length > 0) {
      const value = this.items.shift()!;
      this.signalSpace();
      return value;
    }

    if (this.closed) {
      return null;
    }

    return new Promise((resolve) => {
      this.recvWaiters.push(resolve);
    });
  }

  tryDequeue(): T | null | undefined {
    if (this.items.length > 0) {
      const value = this.items.shift()!;
      this.signalSpace();
      return value;
    }

    if (this.closed) {
      return null;
    }

    return undefined;
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;

    for (const waiter of this.recvWaiters) {
      waiter(null);
    }
    this.recvWaiters.length = 0;

    for (const waiter of this.sendWaiters) {
      waiter();
    }
    this.sendWaiters.length = 0;
  }

  private signalSpace(): void {
    const waiter = this.sendWaiters.shift();
    waiter?.();
  }
}

class CreditWindow {
  private available: number;
  private closed = false;
  private waiters: Array<() => void> = [];

  constructor(initialCredit: number) {
    this.available = initialCredit;
  }

  async consume(): Promise<void> {
    while (true) {
      if (this.closed) {
        throw ChannelError.closed();
      }
      if (this.available > 0) {
        this.available -= 1;
        return;
      }
      await new Promise<void>((resolve) => {
        this.waiters.push(resolve);
      });
    }
  }

  grant(additional: number): void {
    if (this.closed || additional <= 0) {
      return;
    }
    this.available += additional;
    const waiters = this.waiters.splice(0, this.waiters.length);
    for (const waiter of waiters) {
      waiter();
    }
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    const waiters = this.waiters.splice(0, this.waiters.length);
    for (const waiter of waiters) {
      waiter();
    }
  }
}

export interface OutgoingCreditController {
  consume(): Promise<void>;
  close(): void;
}

interface OutgoingState {
  queue: AsyncQueue<OutgoingMessage>;
  credit: CreditWindow;
}

interface IncomingCreditState {
  consumedSinceGrant: number;
  threshold: number;
}

function creditReplenishmentThreshold(initialCredit: number): number {
  return Math.max(1, Math.floor(initialCredit / 2));
}

/**
 * Sender handle for outgoing channel data.
 *
 * This is the internal channel that Tx<T> writes to.
 */
export class OutgoingSender {
  constructor(
    private _channelId: ChannelId,
    private state: OutgoingState,
    private readonly notifyOutgoing?: () => void,
    private readonly _keepaliveOwner?: object,
  ) {}

  get channelId(): ChannelId {
    return this._channelId;
  }

  /** Send serialized data. */
  async sendData(data: Uint8Array): Promise<void> {
    await this.state.credit.consume();
    const enqueued = await this.state.queue.enqueue({ kind: "data", payload: data });
    if (!enqueued) {
      throw ChannelError.closed();
    }
    this.notifyOutgoing?.();
  }

  /** Send close signal. */
  sendClose(): void {
    void this.state.queue.enqueue({ kind: "close" }).then((enqueued) => {
      if (enqueued) {
        this.notifyOutgoing?.();
      }
      this.state.queue.close();
      this.state.credit.close();
    });
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
  constructor(
    private readonly keepaliveOwner?: object,
    private readonly notifyOutgoing?: () => void,
  ) {}

  /** Channels where we receive Data messages (backing Rx<T> handles). */
  private incoming = new Map<ChannelId, Channel<Uint8Array>>();
  private incomingCredit = new Map<ChannelId, IncomingCreditState>();

  /** Channels where we send Data messages (backing Tx<T> handles). */
  private outgoing = new Map<ChannelId, OutgoingState>();

  /** Pending GrantCredit control messages. */
  private pendingCredits: Array<{ channelId: ChannelId; additional: number }> = [];
  private creditWaiter: ((value: { channelId: ChannelId; additional: number }) => void) | null = null;

  /** Channel IDs that have been closed. */
  private closed = new Set<ChannelId>();

  /**
   * Register an incoming channel and return the receiver for Rx<T>.
   *
   * r[impl channeling.allocation.caller] - Caller allocates channel IDs.
   */
  registerIncoming(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
    onConsumed?: (additional: number) => void,
  ): ChannelReceiver<Uint8Array> {
    const channel = createChannel<Uint8Array>(initialCredit);
    this.incoming.set(channelId, channel);
    this.incomingCredit.set(channelId, {
      consumedSinceGrant: 0,
      threshold: creditReplenishmentThreshold(initialCredit),
    });
    return new ChannelReceiver(
      channel,
      this.keepaliveOwner,
      () => {
        const state = this.incomingCredit.get(channelId);
        if (!state) {
          return;
        }
        state.consumedSinceGrant += 1;
        if (state.consumedSinceGrant < state.threshold) {
          return;
        }

        const additional = state.consumedSinceGrant;
        state.consumedSinceGrant = 0;
        if (onConsumed) {
          onConsumed(additional);
        } else {
          this.queueGrantCredit(channelId, additional);
        }
      },
    );
  }

  /**
   * Register an outgoing channel and return the sender for Tx<T>.
   *
   * r[impl channeling.allocation.caller] - Caller allocates channel IDs.
   */
  registerOutgoing(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
  ): OutgoingSender {
    const state = this.ensureOutgoing(channelId, initialCredit);
    return new OutgoingSender(channelId, state, this.notifyOutgoing, this.keepaliveOwner);
  }

  registerServerOutgoing(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
  ): OutgoingCreditController {
    return this.ensureOutgoing(channelId, initialCredit).credit;
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

    if (!channel.send(payload)) {
      throw ChannelError.overflow(channelId);
    }
  }

  grantCredit(channelId: ChannelId, additional: number): void {
    this.outgoing.get(channelId)?.credit.grant(additional);
  }

  queueGrantCredit(channelId: ChannelId, additional: number): void {
    if (additional <= 0) {
      return;
    }

    const credit = { channelId, additional };
    if (this.creditWaiter) {
      const waiter = this.creditWaiter;
      this.creditWaiter = null;
      waiter(credit);
    } else {
      this.pendingCredits.push(credit);
    }
    this.notifyOutgoing?.();
  }

  pollOutgoing(): OutgoingPoll {
    const pendingCredit = this.pendingCredits.shift();
    if (pendingCredit) {
      return { kind: "credit", ...pendingCredit };
    }

    for (const [channelId, state] of this.outgoing) {
      const msg = state.queue.tryDequeue();
      if (msg === undefined) {
        continue;
      }

      if (msg === null) {
        this.outgoing.delete(channelId);
        this.closed.add(channelId);
        return this.pollOutgoing();
      }

      if (msg.kind === "data") {
        return { kind: "data", channelId, payload: msg.payload };
      }

      if (msg.kind === "close") {
        this.outgoing.delete(channelId);
        this.closed.add(channelId);
        return { kind: "close", channelId };
      }

      return {
        kind: "credit",
        channelId,
        additional: msg.additional,
      };
    }

    if (this.outgoing.size === 0) {
      return { kind: "done" };
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
    this.incomingCredit.delete(channelId);

    const outgoing = this.outgoing.get(channelId);
    if (outgoing) {
      outgoing.credit.close();
      outgoing.queue.close();
      this.outgoing.delete(channelId);
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

  hasLiveChannels(): boolean {
    return this.incoming.size > 0 || this.outgoing.size > 0 || this.pendingCredits.length > 0;
  }

  private ensureOutgoing(channelId: ChannelId, initialCredit: number): OutgoingState {
    let state = this.outgoing.get(channelId);
    if (state) {
      return state;
    }

    state = {
      queue: new AsyncQueue<OutgoingMessage>(64),
      credit: new CreditWindow(initialCredit),
    };
    this.outgoing.set(channelId, state);
    return state;
  }
}
