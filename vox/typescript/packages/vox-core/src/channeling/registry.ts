// Channel registry for managing active channels on a lane.

import { type ChannelId, ChannelError, DEFAULT_INITIAL_CREDIT } from "./types.ts";
import { createChannel, type Channel, ChannelReceiver } from "./channel.ts";
import { voxLogger } from "../logger.ts";

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

export type OutgoingTrySendDetail =
  | "sent"
  | "credit_exhausted"
  | "runtime_queue_full"
  | "closed";

export interface ChannelDebugContext {
  laneId?: bigint;
  requestId?: bigint;
  service?: string;
  method?: string;
  channelDirection?: "tx" | "rx";
  side?: "client" | "server";
}

export interface ChannelDebugSnapshot {
  channelId: ChannelId;
  state: "incoming" | "outgoing" | "pending-incoming" | "closed";
  context?: ChannelDebugContext;
}

export interface ChannelRegistryDebugSnapshot {
  channels: ChannelDebugSnapshot[];
  pendingCreditCount: number;
}

function logChannelEvent(event: string, channelId: ChannelId, fields: Record<string, unknown> = {}): void {
  voxLogger()?.debug(`[vox:channel] ${event}`, {
    channelId,
    ...fields,
  });
}

class AsyncQueue<T> {
  private items: T[] = [];
  private closed = false;
  private recvWaiters: Array<(value: T | null) => void> = [];
  private sendWaiters: Array<() => void> = [];

  private readonly capacity: number;

  constructor(capacity: number) {
    this.capacity = capacity;
  }

  // r[impl rpc.channel.delivery.reliable]
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

  tryEnqueue(value: T): "enqueued" | "full" | "closed" {
    if (this.closed) {
      return "closed";
    }

    const waiter = this.recvWaiters.shift();
    if (waiter) {
      waiter(value);
      return "enqueued";
    }

    if (this.items.length >= this.capacity) {
      return "full";
    }

    this.items.push(value);
    return "enqueued";
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

  // r[impl rpc.flow-control.credit]
  // r[impl rpc.flow-control.credit.exhaustion]
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

  tryConsume(): "consumed" | "full" | "closed" {
    if (this.closed) {
      return "closed";
    }
    if (this.available === 0) {
      return "full";
    }
    this.available -= 1;
    return "consumed";
  }

  // r[impl rpc.flow-control.credit.grant.additive]
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
  tryConsume(): "consumed" | "full" | "closed";
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

interface PendingIncomingState {
  items: Uint8Array[];
  terminal: boolean;
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
  private _channelId: ChannelId;
  private state: OutgoingState;
  private readonly notifyOutgoing?: () => void;
  private _keepaliveOwner?: object;

  constructor(
    _channelId: ChannelId,
    state: OutgoingState,
    notifyOutgoing?: () => void,
    _keepaliveOwner?: object,
  ) {
    this._channelId = _channelId;
    this.state = state;
    this.notifyOutgoing = notifyOutgoing;
    this._keepaliveOwner = _keepaliveOwner;
  }

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

  trySendData(data: Uint8Array): "sent" | "full" | "closed" {
    const outcome = this.trySendDataDetailed(data);
    if (outcome === "sent") {
      return "sent";
    }
    return outcome === "closed" ? "closed" : "full";
  }

  // r[impl rpc.observability.channel.try-send-detail]
  trySendDataDetailed(data: Uint8Array): OutgoingTrySendDetail {
    const credit = this.state.credit.tryConsume();
    if (credit === "full") {
      return "credit_exhausted";
    }
    if (credit === "closed") {
      return "closed";
    }

    const enqueued = this.state.queue.tryEnqueue({ kind: "data", payload: data });
    if (enqueued === "enqueued") {
      this.notifyOutgoing?.();
      return "sent";
    }

    this.state.credit.grant(1);
    return enqueued === "closed" ? "closed" : "runtime_queue_full";
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
 */
export class ChannelRegistry {
  private readonly keepaliveOwner?: object;
  private readonly notifyOutgoing?: () => void;

  constructor(keepaliveOwner?: object, notifyOutgoing?: () => void) {
    this.keepaliveOwner = keepaliveOwner;
    this.notifyOutgoing = notifyOutgoing;
  }

  /** Channels where we receive Data messages (backing Rx<T> handles). */
  private incoming = new Map<ChannelId, Channel<Uint8Array>>();
  private incomingCredit = new Map<ChannelId, IncomingCreditState>();
  private pendingIncoming = new Map<ChannelId, PendingIncomingState>();

  /** Channels where we send Data messages (backing Tx<T> handles). */
  private outgoing = new Map<ChannelId, OutgoingState>();

  /** Pending GrantCredit control messages. */
  private pendingCredits: Array<{ channelId: ChannelId; additional: number }> = [];
  private creditWaiter: ((value: { channelId: ChannelId; additional: number }) => void) | null = null;

  /** Channel IDs that have been closed. */
  private closed = new Set<ChannelId>();
  private contexts = new Map<ChannelId, ChannelDebugContext>();

  /**
   * Register an incoming channel and return the receiver for Rx<T>.
   *
   * r[impl rpc.channel.allocation] - Caller allocates channel IDs.
   * r[impl rpc.channel.binding.callee-args]
   * r[impl rpc.channel.binding.callee-args.rx] - Callee binds incoming Rx by channel ID.
   * r[impl rpc.flow-control.credit.initial]
   */
  registerIncoming(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
    onConsumed?: (additional: number) => void,
  ): ChannelReceiver<Uint8Array> {
    const channel = createChannel<Uint8Array>();
    const creditState = {
      consumedSinceGrant: 0,
      threshold: creditReplenishmentThreshold(initialCredit),
    };
    const pending = this.pendingIncoming.get(channelId);
    if (!pending?.terminal) {
      this.incoming.set(channelId, channel);
      this.incomingCredit.set(channelId, creditState);
    }
    logChannelEvent("open", channelId, { direction: "incoming", initialCredit });

    if (pending) {
      this.pendingIncoming.delete(channelId);
      for (const payload of pending.items) {
        channel.send(payload);
      }
      if (pending.terminal) {
        channel.close();
        this.incomingCredit.delete(channelId);
        this.closed.add(channelId);
      } else {
        this.incomingCredit.set(channelId, creditState);
      }
    } else {
      this.incomingCredit.set(channelId, creditState);
    }

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
   * r[impl rpc.channel.allocation] - Caller allocates channel IDs.
   * r[impl rpc.channel.binding.callee-args]
   * r[impl rpc.channel.binding.callee-args.tx] - Callee binds outgoing Tx by channel ID.
   * r[impl rpc.flow-control.credit.initial]
   */
  registerOutgoing(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
  ): OutgoingSender {
    const state = this.ensureOutgoing(channelId, initialCredit);
    logChannelEvent("open", channelId, { direction: "outgoing", initialCredit });
    return new OutgoingSender(channelId, state, this.notifyOutgoing, this.keepaliveOwner);
  }

  registerServerOutgoing(
    channelId: ChannelId,
    initialCredit: number = DEFAULT_INITIAL_CREDIT,
  ): OutgoingCreditController {
    return this.ensureOutgoing(channelId, initialCredit).credit;
  }

  // r[impl rpc.observability.channel.context]
  rememberContext(channelId: ChannelId, context: ChannelDebugContext): void {
    this.contexts.set(channelId, {
      ...this.contexts.get(channelId),
      ...context,
    });
  }

  /**
   * Route a Data message payload to the appropriate incoming channel.
   *
   * r[impl rpc.channel.item] - Channel items route by channel ID.
   * r[impl rpc.channel.binding] - Items may arrive before the callee registers the Rx handle.
   * r[impl rpc.channel.close] - Data after close is rejected.
   */
  routeData(channelId: ChannelId, payload: Uint8Array): void {
    // Check for data-after-close
    if (this.closed.has(channelId)) {
      throw ChannelError.dataAfterClose(channelId);
    }

    const channel = this.incoming.get(channelId);
    logChannelEvent("receive", channelId, { bytes: payload.length });
    if (!channel) {
      const pending = this.pendingIncoming.get(channelId);
      if (pending) {
        pending.items.push(payload);
        return;
      }
      this.pendingIncoming.set(channelId, { items: [payload], terminal: false });
      return;
    }

    channel.send(payload);
  }

  grantCredit(channelId: ChannelId, additional: number): void {
    logChannelEvent("credit", channelId, { direction: "incoming", additional });
    this.outgoing.get(channelId)?.credit.grant(additional);
  }

  // r[impl rpc.flow-control.credit.grant]
  queueGrantCredit(channelId: ChannelId, additional: number): void {
    if (additional <= 0) {
      return;
    }

    const credit = { channelId, additional };
    logChannelEvent("credit", channelId, { direction: "outgoing", additional });
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
        logChannelEvent("send", channelId, { bytes: msg.payload.length });
        return { kind: "data", channelId, payload: msg.payload };
      }

      if (msg.kind === "close") {
        this.outgoing.delete(channelId);
        this.closed.add(channelId);
        logChannelEvent("close", channelId, { direction: "outgoing" });
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
   * r[impl rpc.channel.close] - Close terminates the channel.
   * r[impl rpc.channel.reset] - Reset also terminates the channel locally.
   */
  close(channelId: ChannelId): void {
    logChannelEvent("close", channelId);
    const pending = this.pendingIncoming.get(channelId);
    if (pending) {
      pending.terminal = true;
    }

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

  closeAll(): void {
    // r[impl rpc.channel.connection-closure]
    for (const channelId of this.incoming.keys()) {
      this.close(channelId);
    }
    for (const channelId of this.outgoing.keys()) {
      this.close(channelId);
    }
    this.pendingIncoming.clear();
    this.pendingCredits.length = 0;
    this.creditWaiter = null;
  }

  /** Check if a channel ID is registered (either incoming or outgoing). */
  contains(channelId: ChannelId): boolean {
    return (
      this.incoming.has(channelId) ||
      this.outgoing.has(channelId) ||
      this.pendingIncoming.has(channelId)
    );
  }

  /** Check if a channel has been closed. */
  isClosed(channelId: ChannelId): boolean {
    return this.closed.has(channelId);
  }

  // r[impl rpc.debug.snapshot]
  // r[impl rpc.observability.channel.context]
  debugSnapshot(): ChannelRegistryDebugSnapshot {
    const channels: ChannelDebugSnapshot[] = [];
    const push = (channelId: ChannelId, state: ChannelDebugSnapshot["state"]) => {
      const context = this.contexts.get(channelId);
      channels.push(context ? { channelId, state, context: { ...context } } : { channelId, state });
    };

    for (const channelId of this.incoming.keys()) {
      push(channelId, "incoming");
    }
    for (const channelId of this.outgoing.keys()) {
      push(channelId, "outgoing");
    }
    for (const channelId of this.pendingIncoming.keys()) {
      push(channelId, "pending-incoming");
    }
    for (const channelId of this.closed) {
      if (!this.incoming.has(channelId) && !this.outgoing.has(channelId) && !this.pendingIncoming.has(channelId)) {
        push(channelId, "closed");
      }
    }

    return {
      channels,
      pendingCreditCount: this.pendingCredits.length,
    };
  }

  /** Get the number of active outgoing channels. */
  get outgoingCount(): number {
    return this.outgoing.size;
  }

  hasLiveChannels(): boolean {
    return (
      this.incoming.size > 0 ||
      this.outgoing.size > 0 ||
      this.pendingIncoming.size > 0 ||
      this.pendingCredits.length > 0
    );
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
