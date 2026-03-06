// Connection state machine and message loop.
//
// Handles the protocol state machine including Hello exchange,
// payload validation, and channel ID management.
//
// Generic over MessageTransport to support different transports:
// - LengthPrefixedFramed for TCP (byte streams with length-prefix framing)
// - WsTransport for WebSocket (message-oriented transport)

import {
  type Hello,
  type HelloYourself,
  type Message,
  type MetadataEntry,
  helloV7,
  helloYourself,
  parityEven,
  parityOdd,
  messagePing,
  messageHello,
  messageHelloYourself,
  messagePong,
  messageProtocolError,
  messageRequest,
  messageResponse,
  messageAccept,
  messageReject,
  messageData,
  messageClose,
  messageCredit,
  encodeMessage,
  decodeMessage,
} from "@bearcove/roam-wire";
import {
  ChannelRegistry,
  ChannelIdAllocator,
  ChannelError,
  DEFAULT_INITIAL_CREDIT,
  Role,
  Tx,
  Rx,
  createServerTx,
  createServerRx,
  type TaskMessage,
  type TaskSender,
  type MethodDescriptor,
  type ServiceDescriptor,
  type RoamCall,
  type SchemaRegistry,
} from "./channeling/index.ts";
import { type MessageTransport } from "./transport.ts";
import type { Caller, CallerRequest } from "./caller.ts";
import { MiddlewareCaller } from "./caller.ts";
import type { ClientMiddleware } from "./middleware.ts";
import { clientMetadataToEntries } from "./metadata.ts";
import { encodeWithSchema, decodeWithSchema } from "@bearcove/roam-postcard";
import { RpcError, RpcErrorCode } from "@bearcove/roam-wire";

// Note: Role is exported from streaming/index.ts in roam-core's main export

/** Negotiated connection parameters after Hello exchange. */
export interface Negotiated {
  /** Effective max payload size (min of both peers). */
  maxPayloadSize: number;
  /** Initial channel credit (min of both peers). */
  initialCredit: number;
  /** Maximum concurrent in-flight requests (min of both peers). */
  maxConcurrentRequests: number;
}

/** Optional proactive protocol keepalive settings. */
export interface KeepaliveConfig {
  pingIntervalMs: number;
  pongTimeoutMs: number;
}

interface KeepaliveRuntime {
  pingIntervalMs: number;
  pongTimeoutMs: number;
  nextPingAtMs: number;
  waitingPongNonce: bigint | null;
  pongDeadlineMs: number;
  nextPingNonce: bigint;
}

/** Error during connection handling. */
export class ConnectionError extends Error {
  constructor(
    public kind: "io" | "protocol" | "dispatch" | "closed",
    message: string,
    public ruleId?: string,
  ) {
    super(message);
    this.name = "ConnectionError";
  }

  static io(message: string): ConnectionError {
    return new ConnectionError("io", message);
  }

  static protocol({ ruleId, context }: { ruleId: string; context: string }): ConnectionError {
    return new ConnectionError("protocol", context, ruleId);
  }

  static dispatch(message: string): ConnectionError {
    return new ConnectionError("dispatch", message);
  }

  static closed(): ConnectionError {
    return new ConnectionError("closed", "connection closed");
  }
}

function msgTag(msg: Message): Message["payload"]["tag"] {
  return msg.payload.tag;
}

/** Trait for dispatching RPC requests to a service (simple non-channeling mode). */
export interface ServiceDispatcher {
  /**
   * Dispatch an RPC request and return the response payload.
   *
   * The dispatcher is responsible for:
   * - Looking up the method by method_id
   * - Deserializing arguments from payload
   * - Calling the service method
   * - Serializing the response
   */
  dispatchRpc(methodId: bigint, payload: Uint8Array): Promise<Uint8Array>;
}

/**
 * Channel-aware service dispatcher using service descriptors.
 *
 * The runtime handles all arg decoding and response encoding using the
 * descriptor's schemas. Generated dispatchers only do routing and call
 * handler methods with pre-decoded args.
 */
export interface ChannelingDispatcher {
  /** Return the service descriptor for schema-driven dispatch. */
  getDescriptor(): ServiceDescriptor;

  /**
   * Dispatch a decoded request to the appropriate handler method.
   *
   * Called by the runtime after:
   * - Finding the method in the descriptor by ID
   * - Decoding args with the method's args tuple schema
   * - Binding any channel args (Tx/Rx) using the registry
   * - Creating a RoamCall for the response
   *
   * @param method - The matched method descriptor
   * @param args - Pre-decoded argument values (channels already bound)
   * @param call - Interface for sending the response
   */
  dispatch(method: MethodDescriptor, args: unknown[], call: RoamCall): Promise<void>;
}

/** Implementation of RoamCall that encodes responses using a method descriptor. */
class RoamCallImpl implements RoamCall {
  private responded = false;

  constructor(
    private readonly method: MethodDescriptor,
    private readonly requestId: bigint,
    private readonly taskSender: TaskSender,
    private readonly schemaRegistry?: SchemaRegistry,
  ) {}

  reply(value: unknown): void {
    if (this.responded) return;
    this.responded = true;
    const payload = encodeWithSchema(
      { tag: "Ok", value },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }

  replyErr(error: unknown): void {
    if (this.responded) return;
    this.responded = true;
    const payload = encodeWithSchema(
      { tag: "Err", value: { tag: "User", value: error } },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }

  replyInternalError(): void {
    if (this.responded) return;
    this.responded = true;
    const payload = encodeWithSchema(
      { tag: "Err", value: { tag: "InvalidPayload" } },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }
}

/**
 * A live connection with completed Hello exchange.
 *
 * Generic over MessageTransport to support different transports
 * (LengthPrefixedFramed for TCP, WsTransport for WebSocket).
 */
export class Connection<T extends MessageTransport = MessageTransport> {
  private io: T;
  private _role: Role;
  private _negotiated: Negotiated;
  private ourHello: Hello;
  private channelAllocator: ChannelIdAllocator;
  private channelRegistry: ChannelRegistry;
  private nextRequestId: bigint = 1n;

  // Virtual connection tracking
  // r[impl core.conn.id-allocation] - Connection IDs are allocated by the acceptor.
  private nextConnId: bigint = 1n;
  private virtualConnections: Set<bigint> = new Set();
  private _acceptConnections: boolean;

  // Pending request tracking for concurrent calls
  private pendingRequests = new Map<
    bigint,
    {
      resolve: (payload: Uint8Array) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private messagePumpRunning = false;
  private messagePumpPromise: Promise<void> | null = null;
  private messagePumpWakeupResolve: (() => void) | null = null;
  private flushOutgoingPromise: Promise<void> | null = null;
  private keepalive: KeepaliveConfig | null;

  /**
   * Optional interceptor to add metadata to outgoing requests.
   * Called before each call() to get additional metadata entries.
   */
  public metadataInterceptor?: () => MetadataEntry[];

  constructor(
    io: T,
    role: Role,
    negotiated: Negotiated,
    ourHello: Hello,
    acceptConnections: boolean = false,
    keepalive: KeepaliveConfig | null = null,
  ) {
    this.io = io;
    this._role = role;
    this._negotiated = negotiated;
    this.ourHello = ourHello;
    this.channelAllocator = new ChannelIdAllocator(role);
    this.channelRegistry = new ChannelRegistry(this, () => this.wakeMessagePump());
    this._acceptConnections = acceptConnections;
    this.keepalive = keepalive;
  }

  /** Get the underlying transport. */
  getIo(): T {
    return this.io;
  }

  /** Get the negotiated parameters. */
  negotiated(): Negotiated {
    return this._negotiated;
  }

  /** Get the connection role. */
  role(): Role {
    return this._role;
  }

  /**
   * Get the channel ID allocator.
   *
   * r[impl channeling.allocation.caller] - Caller allocates ALL channel IDs.
   */
  getChannelAllocator(): ChannelIdAllocator {
    return this.channelAllocator;
  }

  /**
   * Get the channel registry.
   */
  getChannelRegistry(): ChannelRegistry {
    return this.channelRegistry;
  }

  /**
   * Send a Goodbye message and return an error.
   *
   * r[impl message.goodbye.send] - Send Goodbye with rule ID before closing.
   * r[impl core.error.goodbye-reason] - Reason contains violated rule ID.
   */
  async goodbye(ruleId: string): Promise<ConnectionError> {
    try {
      await this.io.send(encodeMessage(messageProtocolError(ruleId)));
    } catch {
      // Ignore send errors when closing
    }
    this.io.close();
    return ConnectionError.protocol({ ruleId, context: "" });
  }

  /**
   * Validate a channel ID according to protocol rules.
   *
   * Returns the rule ID if validation fails.
   */
  validateChannelId(channelId: bigint): string | null {
    // r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    if (channelId === 0n) {
      return "rpc.channel.allocation";
    }

    // r[impl channeling.unknown] - Unknown channel IDs are connection errors.
    if (!this.channelRegistry.contains(channelId)) {
      return "channeling.unknown";
    }

    return null;
  }

  /**
   * Send all pending outgoing channel messages.
   *
   * Drains the outgoing channels and sends Data/Close messages
   * to the peer. Call this periodically or after processing requests.
   *
   * r[impl channeling.data] - Send Data messages for outgoing channels.
   * r[impl channeling.close] - Send Close messages when channels end.
   */
  async flushOutgoing(): Promise<void> {
    if (this.flushOutgoingPromise) {
      await this.flushOutgoingPromise;
      return;
    }

    const flush = (async () => {
      while (true) {
        const poll = this.channelRegistry.pollOutgoing();
        if (poll.kind === "pending" || poll.kind === "done") {
          break;
        }
        if (poll.kind === "data") {
          await this.io.send(encodeMessage(messageData(poll.channelId, poll.payload)));
        } else if (poll.kind === "close") {
          await this.io.send(encodeMessage(messageClose(poll.channelId)));
        } else if (poll.kind === "credit") {
          await this.io.send(encodeMessage(messageCredit(poll.channelId, poll.additional)));
        }
      }
    })();

    this.flushOutgoingPromise = flush;
    try {
      await flush;
    } finally {
      if (this.flushOutgoingPromise === flush) {
        this.flushOutgoingPromise = null;
      }
    }
  }

  private wakeMessagePump(): void {
    this.startMessagePump();
    const resolve = this.messagePumpWakeupResolve;
    if (resolve) {
      this.messagePumpWakeupResolve = null;
      resolve();
    }
  }

  /**
   * Validate payload size against negotiated limit.
   *
   * r[impl flow.call.payload-limit] - Payloads bounded by max_payload_size.
   * r[impl message.hello.negotiation] - Effective limit is min of both peers.
   */
  validatePayloadSize(size: number): string | null {
    if (size > this._negotiated.maxPayloadSize) {
      return "flow.call.payload-limit";
    }
    return null;
  }

  private failPendingRequests(error: ConnectionError): void {
    for (const [, pending] of this.pendingRequests) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pendingRequests.clear();
  }

  private makeKeepaliveRuntime(): KeepaliveRuntime | null {
    if (!this.keepalive) {
      return null;
    }
    if (this.keepalive.pingIntervalMs <= 0 || this.keepalive.pongTimeoutMs <= 0) {
      return null;
    }
    const now = Date.now();
    return {
      pingIntervalMs: this.keepalive.pingIntervalMs,
      pongTimeoutMs: this.keepalive.pongTimeoutMs,
      nextPingAtMs: now + this.keepalive.pingIntervalMs,
      waitingPongNonce: null,
      pongDeadlineMs: 0,
      nextPingNonce: 1n,
    };
  }

  private handleKeepalivePong(nonce: bigint, runtime: KeepaliveRuntime | null): void {
    if (!runtime) {
      return;
    }
    if (runtime.waitingPongNonce !== nonce) {
      return;
    }
    runtime.waitingPongNonce = null;
    runtime.pongDeadlineMs = 0;
    runtime.nextPingAtMs = Date.now() + runtime.pingIntervalMs;
  }

  private async handleKeepaliveTick(runtime: KeepaliveRuntime | null): Promise<boolean> {
    if (!runtime) {
      return true;
    }
    const now = Date.now();

    if (runtime.waitingPongNonce !== null) {
      if (now >= runtime.pongDeadlineMs) {
        this.failPendingRequests(ConnectionError.closed());
        this.io.close();
        return false;
      }
      return true;
    }

    if (now < runtime.nextPingAtMs) {
      return true;
    }

    const nonce = runtime.nextPingNonce;
    try {
      await this.io.send(encodeMessage(messagePing(nonce)));
    } catch {
      this.failPendingRequests(ConnectionError.closed());
      this.io.close();
      return false;
    }
    runtime.waitingPongNonce = nonce;
    runtime.pongDeadlineMs = now + runtime.pongTimeoutMs;
    runtime.nextPingAtMs = now + runtime.pingIntervalMs;
    runtime.nextPingNonce = nonce + 1n;
    return true;
  }

  /**
   * Start the message pump if not already running.
   * The pump receives messages and routes responses to pending requests.
   */
  private startMessagePump(): void {
    if (this.messagePumpRunning) return;
    this.messagePumpRunning = true;

    this.messagePumpPromise = (async () => {
      const keepaliveRuntime = this.makeKeepaliveRuntime();
      let pendingRecv: Promise<
        { kind: "message"; payload: Uint8Array | null } | { kind: "error"; error: unknown }
      > | null = null;
      try {
        while (this.pendingRequests.size > 0 || this.channelRegistry.hasLiveChannels()) {
          if (!(await this.handleKeepaliveTick(keepaliveRuntime))) {
            return;
          }

          await this.flushOutgoing();

          if (!pendingRecv) {
            pendingRecv = this.io
              .recvTimeout(100)
              .then((payload) => ({ kind: "message" as const, payload }))
              .catch((error) => ({ kind: "error" as const, error }));
          }

          const wakeupPromise = new Promise<void>((resolve) => {
            this.messagePumpWakeupResolve = resolve;
          });

          const raceResult = await Promise.race([
            pendingRecv.then((result) => ({ source: "recv" as const, result })),
            wakeupPromise.then(() => ({ source: "wakeup" as const })),
          ]);

          if (raceResult.source === "wakeup") {
            continue;
          }

          pendingRecv = null;
          const recvResult = raceResult.result;

          if (recvResult.kind === "error") {
            throw recvResult.error;
          }

          const data = recvResult.payload;
          if (!data) {
            if (this.io.isClosed()) {
              this.failPendingRequests(ConnectionError.closed());
              return;
            }
            continue;
          }

          // Parse message using wire codec
          const result = decodeMessage(data);
          const msg = result.value as any;

          if (msgTag(msg) === "ConnectionClose" || msgTag(msg) === "ProtocolError") {
            // Reject all pending requests
            this.failPendingRequests(ConnectionError.closed());
            return;
          }

          if (msgTag(msg) === "Ping") {
            await this.io.send(encodeMessage(messagePong(msg.payload.value.nonce)));
            continue;
          }

          if (msgTag(msg) === "Pong") {
            if (msg.connection_id === 0n) {
              this.handleKeepalivePong(msg.payload.value.nonce, keepaliveRuntime);
            }
            continue;
          }

          // Handle channel messages
          if (msgTag(msg) === "ChannelMessage" && msg.payload.value.body.tag === "Item") {
            try {
              this.channelRegistry.routeData(
                msg.payload.value.id,
                msg.payload.value.body.value.item,
              );
            } catch (e) {
              if (e instanceof ChannelError && e.kind === "overflow") {
                this.io.close();
                this.failPendingRequests(ConnectionError.closed());
                return;
              }
            }
            continue;
          }

          if (msgTag(msg) === "ChannelMessage" && msg.payload.value.body.tag === "Close") {
            if (this.channelRegistry.contains(msg.payload.value.id)) {
              this.channelRegistry.close(msg.payload.value.id);
            }
            continue;
          }

          if (msgTag(msg) === "ChannelMessage" && msg.payload.value.body.tag === "GrantCredit") {
            if (this.channelRegistry.contains(msg.payload.value.id)) {
              this.channelRegistry.grantCredit(
                msg.payload.value.id,
                msg.payload.value.body.value.additional,
              );
            }
            continue;
          }

          if (msgTag(msg) === "RequestMessage" && msg.payload.value.body.tag === "Response") {
            // Route response to the correct pending request
            const requestId = msg.payload.value.id;
            const pending = this.pendingRequests.get(requestId);
            if (pending) {
              clearTimeout(pending.timer);
              this.pendingRequests.delete(requestId);
              pending.resolve(msg.payload.value.body.value.ret);
            }
            // Ignore responses for unknown request IDs (already timed out?)
            continue;
          }

          // Ignore other messages (Hello after handshake, Reset, etc.)
        }
      } catch {
        this.failPendingRequests(ConnectionError.closed());
      } finally {
        this.messagePumpRunning = false;
        this.messagePumpPromise = null;
        this.messagePumpWakeupResolve = null;
      }
    })();
  }

  /**
   * Make an RPC call.
   *
   * r[impl core.call] - Caller sends Request, callee responds with Response.
   * r[impl call.complete] - Request gets exactly one Response.
   *
   * @param methodId - The method ID to call
   * @param payload - The request payload (already encoded)
   * @param timeoutMs - Timeout in milliseconds (default: 30000)
   * @returns The response payload
   */
  async call(
    methodId: bigint,
    payload: Uint8Array,
    timeoutMs: number = 30000,
    channels: bigint[] = [],
  ): Promise<Uint8Array> {
    const requestId = this.nextRequestId++;

    // Create a promise that will be resolved when the response arrives
    const responsePromise = new Promise<Uint8Array>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(ConnectionError.io("timeout waiting for response"));
      }, timeoutMs);

      this.pendingRequests.set(requestId, { resolve, reject, timer });
    });

    // Start the message pump if not already running
    this.startMessagePump();

    // Send request
    // r[impl call.request.channels] - Include channel IDs in Request.
    const metadata = this.metadataInterceptor?.() ?? [];
    await this.io.send(
      encodeMessage(messageRequest(requestId, methodId, payload, metadata, channels)),
    );

    // Flush any pending outgoing channel data (for client-to-server channels)
    // r[impl channeling.data] - Send queued Data/Close messages after Request.
    await this.flushOutgoing();

    // Wait for the response to be routed by the message pump
    return responsePromise;
  }

  /**
   * Get a Caller interface for this connection.
   *
   * The returned Caller can be used with generated clients and supports
   * middleware composition via the with() method.
   *
   * @example
   * ```typescript
   * const caller = connection.asCaller();
   * const client = new TestbedClient(caller);
   *
   * // With middleware
   * const authedCaller = caller.with(authMiddleware);
   * const authedClient = new TestbedClient(authedCaller);
   * ```
   */
  asCaller(): Caller {
    return new ConnectionCaller(this);
  }

  /**
   * Run the message loop with a channel-aware dispatcher.
   *
   * This is the main event loop that:
   * - Receives messages from the peer
   * - Validates them according to protocol rules
   * - Dispatches requests to the service with channel binding
   * - Collects TaskMessages and sends them in order (Data/Close before Response)
   *
   * r[impl call.pipelining.allowed] - Handle requests as they arrive.
   * r[impl call.pipelining.independence] - Each request handled independently.
   */
  async runChanneling(dispatcher: ChannelingDispatcher): Promise<void> {
    // Queue for task messages from handlers - handlers push, we flush
    const taskQueue: TaskMessage[] = [];

    // Track in-flight handler promises
    const inFlightHandlers: Set<Promise<void>> = new Set();

    // Signal for when a handler produces output or completes (to wake up the event loop)
    let wakeupResolve: (() => void) | null = null;
    const signalWakeup = () => {
      if (wakeupResolve) {
        wakeupResolve();
        wakeupResolve = null;
      }
    };

    // Task sender that queues messages and signals wakeup
    const taskSender: TaskSender = (msg) => {
      taskQueue.push(msg);
      signalWakeup(); // Wake up the event loop to flush
    };

    // Helper to flush task queue to wire
    const flushTaskQueue = async () => {
      while (taskQueue.length > 0) {
        const msg = taskQueue.shift()!;
        switch (msg.kind) {
          case "data":
            await this.io.send(encodeMessage(messageData(msg.channelId, msg.payload)));
            break;
          case "close":
            await this.io.send(encodeMessage(messageClose(msg.channelId)));
            break;
          case "grantCredit":
            await this.io.send(encodeMessage(messageCredit(msg.channelId, msg.additional)));
            break;
          case "response":
            await this.io.send(encodeMessage(messageResponse(msg.requestId, msg.payload)));
            break;
        }
      }
    };

    // Pending receive promise (reused across iterations)
    let pendingRecv: Promise<
      { kind: "message"; payload: Uint8Array | null } | { kind: "error"; error: unknown }
    > | null = null;
    const keepaliveRuntime = this.makeKeepaliveRuntime();
    const recvTimeoutMs = keepaliveRuntime
      ? Math.max(1, Math.min(100, Math.floor(keepaliveRuntime.pingIntervalMs)))
      : 30000;

    while (true) {
      if (!(await this.handleKeepaliveTick(keepaliveRuntime))) {
        throw ConnectionError.closed();
      }

      // Flush any pending task messages from handlers
      await flushTaskQueue();

      // Start receiving if we don't have a pending receive
      if (!pendingRecv) {
        pendingRecv = this.io
          .recvTimeout(recvTimeoutMs)
          .then((payload) => ({ kind: "message" as const, payload }))
          .catch((error) => ({ kind: "error" as const, error }));
      }

      // Create a promise that resolves when a handler produces output or completes
      const wakeupPromise = new Promise<void>((resolve) => {
        wakeupResolve = resolve;
      });

      // Always race between recv and wakeup (for task queue flushing)
      let recvResult:
        | { kind: "message"; payload: Uint8Array | null }
        | { kind: "error"; error: unknown }
        | null = null;

      const raceResult = await Promise.race([
        pendingRecv.then((r) => ({ source: "recv" as const, result: r })),
        wakeupPromise.then(() => ({ source: "wakeup" as const })),
      ]);

      if (raceResult.source === "wakeup") {
        // Wakeup signal (handler output or completion) - loop again to flush and continue
        continue;
      }
      recvResult = raceResult.result;

      // Clear pending recv since we consumed it
      pendingRecv = null;

      if (recvResult.kind === "error") {
        const raw = this.io.lastDecoded;
        if (raw.length >= 2 && raw[0] === 0x00 && raw[1] !== 0x00) {
          throw await this.goodbye("message.hello.unknown-version");
        }
        throw ConnectionError.io(String(recvResult.error));
      }

      const payload = recvResult.payload;
      if (!payload) {
        if (keepaliveRuntime && !this.io.isClosed()) {
          continue;
        }
        // Connection closed - wait for all in-flight handlers to complete
        await Promise.all(inFlightHandlers);
        await flushTaskQueue();
        return;
      }

      try {
        const handlerPromise = this.handleChannelingMessage(
          payload,
          dispatcher,
          taskSender,
          keepaliveRuntime,
        );

        // If this returned a handler promise, track it
        if (handlerPromise) {
          inFlightHandlers.add(handlerPromise);
          handlerPromise.finally(() => {
            inFlightHandlers.delete(handlerPromise);
            signalWakeup();
          });
        }
      } catch (e) {
        if (e instanceof ConnectionError) {
          // For protocol errors, send Goodbye before closing
          if (e.kind === "protocol" && e.ruleId) {
            throw await this.goodbye(e.ruleId);
          }
          throw e;
        }
        throw await this.goodbye("message.decode-error");
      }
    }
  }

  /**
   * Handle a message in channeling mode.
   *
   * Returns a Promise for Request messages (the handler running concurrently),
   * or undefined for other message types that are processed synchronously.
   */
  private handleChannelingMessage(
    payload: Uint8Array,
    dispatcher: ChannelingDispatcher,
    taskSender: TaskSender,
    keepaliveRuntime: KeepaliveRuntime | null,
  ): Promise<void> | undefined {
    // Parse message using wire codec
    const result = decodeMessage(payload);
    const msg = result.value as any;
    const tag = msgTag(msg);

    if (tag === "Hello") {
      return undefined; // Duplicate Hello after exchange - ignore
    }

    if (tag === "Ping") {
      this.io.send(encodeMessage(messagePong(msg.payload.value.nonce)));
      return undefined;
    }

    if (tag === "Pong") {
      if (msg.connection_id === 0n) {
        this.handleKeepalivePong(msg.payload.value.nonce, keepaliveRuntime);
      }
      return undefined;
    }

    if (tag === "ConnectionOpen") {
      const connId = msg.connection_id;
      if (this._acceptConnections) {
        this.virtualConnections.add(connId);
        const acceptMsg = messageAccept(
          connId,
          msg.payload.value.connection_settings,
          [],
        );
        this.io.send(encodeMessage(acceptMsg));
      } else {
        const rejectMsg = messageReject(connId, []);
        this.io.send(encodeMessage(rejectMsg));
      }
      return undefined;
    }

    if (tag === "ConnectionAccept" || tag === "ConnectionReject") {
      return undefined;
    }

    if (tag === "ConnectionClose" || tag === "ProtocolError") {
      if (tag === "ProtocolError" || msg.connection_id === 0n) {
        throw ConnectionError.closed();
      }
      if (this.virtualConnections.has(msg.connection_id)) {
        this.virtualConnections.delete(msg.connection_id);
      }
      return undefined;
    }

    if (tag === "RequestMessage") {
      const request = msg.payload.value;
      if (request.body.tag === "Call") {
        const payloadViolation = this.validatePayloadSize(request.body.value.args.length);
        if (payloadViolation) {
          throw ConnectionError.protocol({
            ruleId: payloadViolation,
            context: "payload exceeds max size",
          });
        }

        const methodId = request.body.value.method_id;
        const rawPayload = request.body.value.args;
        const requestId = request.id;
        const descriptor = dispatcher.getDescriptor();
        const method = descriptor.methods.find((m) => m.id === methodId);

        if (!method) {
          // r[impl call.error.unknown-method]
          taskSender({
            kind: "response",
            requestId,
            payload: new Uint8Array([0x01, 0x01]),
          });
          return undefined;
        }

        // Decode args using the method's tuple schema
        let rawArgs: unknown[];
        let decodeEnd: number;
        try {
          const decoded = decodeWithSchema(rawPayload, 0, method.args, descriptor.schema_registry);
          rawArgs = decoded.value as unknown[];
          decodeEnd = decoded.next;
        } catch {
          // r[impl call.error.invalid-payload]
          taskSender({
            kind: "response",
            requestId,
            payload: new Uint8Array([0x01, 0x02]),
          });
          return undefined;
        }

        if (decodeEnd !== rawPayload.length) {
          // Trailing bytes in args
          taskSender({
            kind: "response",
            requestId,
            payload: new Uint8Array([0x01, 0x02]),
          });
          return undefined;
        }

        // Bind channel args using channel IDs from Request.channels (declaration order)
        // r[impl call.request.channels.schema-driven] - Channel IDs from Request.channels, not payload.
        const requestChannels = request.body.value.channels as bigint[];
        let channelIdx = 0;
        const args: unknown[] = rawArgs.map((raw, i) => {
          const argSchema = method.args.elements[i];
          if (argSchema.kind === "tx") {
            const channelId = requestChannels[channelIdx++];
            return createServerTx(
              channelId,
              taskSender,
              this.channelRegistry,
              argSchema.initial_credit ?? DEFAULT_INITIAL_CREDIT,
              (v: unknown) => encodeWithSchema(v, argSchema.element, descriptor.schema_registry),
            );
          } else if (argSchema.kind === "rx") {
            const channelId = requestChannels[channelIdx++];
            const receiver = this.channelRegistry.registerIncoming(
              channelId,
              argSchema.initial_credit ?? DEFAULT_INITIAL_CREDIT,
              (additional) => {
                taskSender({
                  kind: "grantCredit",
                  channelId,
                  additional,
                });
              },
            );
            return createServerRx(channelId, receiver, (b: Uint8Array) =>
              decodeWithSchema(b, 0, argSchema.element, descriptor.schema_registry).value,
            );
          }
          return raw;
        });

        const call = new RoamCallImpl(method, requestId, taskSender, descriptor.schema_registry);
        return dispatcher.dispatch(method, args, call);
      }
      return undefined;
    }

    if (tag === "ChannelMessage") {
      const channelId = msg.payload.value.id;
      const body = msg.payload.value.body;
      if (channelId === 0n) {
        throw ConnectionError.protocol({
          ruleId: "rpc.channel.allocation",
          context: "channel ID 0 is reserved",
        });
      }

      if (body.tag === "Item") {
        try {
          this.channelRegistry.routeData(channelId, body.value.item);
        } catch (e) {
          if (e instanceof ChannelError) {
            if (e.kind === "unknown") {
              throw ConnectionError.protocol({
                ruleId: "channeling.unknown",
                context: "unknown channel ID",
              });
            }
            if (e.kind === "dataAfterClose") {
              throw ConnectionError.protocol({
                ruleId: "channeling.data-after-close",
                context: "data after close",
              });
            }
            if (e.kind === "overflow") {
              throw ConnectionError.protocol({
                ruleId: "rpc.flow-control.credit.exhaustion",
                context: "incoming channel buffer overflow",
              });
            }
          }
          throw e;
        }
        return undefined;
      }

      if (!this.channelRegistry.contains(channelId)) {
        throw ConnectionError.protocol({
          ruleId: "channeling.unknown",
          context: "unknown channel ID",
        });
      }

      if (body.tag === "Close" || body.tag === "Reset") {
        this.channelRegistry.close(channelId);
        return undefined;
      }

      if (body.tag === "GrantCredit") {
        this.channelRegistry.grantCredit(channelId, body.value.additional);
      }
      return undefined;
    }

    return undefined;
  }

  /**
   * Run the message loop with a dispatcher.
   *
   * This is the main event loop that:
   * - Receives messages from the peer
   * - Validates them according to protocol rules
   * - Dispatches requests to the service
   * - Sends responses back
   *
   * r[impl call.pipelining.allowed] - Handle requests as they arrive.
   * r[impl call.pipelining.independence] - Each request handled independently.
   */
  async run(dispatcher: ServiceDispatcher): Promise<void> {
    const keepaliveRuntime = this.makeKeepaliveRuntime();
    const recvTimeoutMs = keepaliveRuntime
      ? Math.max(1, Math.min(100, Math.floor(keepaliveRuntime.pingIntervalMs)))
      : 30000;

    while (true) {
      if (!(await this.handleKeepaliveTick(keepaliveRuntime))) {
        throw ConnectionError.closed();
      }

      let payload: Uint8Array | null;
      try {
        payload = await this.io.recvTimeout(recvTimeoutMs);
      } catch (e) {
        // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
        // Check for unknown Hello variant: [Message::Hello=0][Hello::unknown=1+]
        const raw = this.io.lastDecoded;
        if (raw.length >= 2 && raw[0] === 0x00 && raw[1] !== 0x00) {
          throw await this.goodbye("message.hello.unknown-version");
        }
        throw ConnectionError.io(String(e));
      }

      if (!payload) {
        if (keepaliveRuntime && !this.io.isClosed()) {
          continue;
        }
        return; // Connection closed or timeout
      }

      try {
        await this.handleMessage(payload, dispatcher, keepaliveRuntime);
      } catch (e) {
        if (e instanceof ConnectionError) throw e;
        // r[impl message.decode-error] - send goodbye on decode failure
        throw await this.goodbye("message.decode-error");
      }
    }
  }

  private async handleMessage(
    payload: Uint8Array,
    dispatcher: ServiceDispatcher,
    keepaliveRuntime: KeepaliveRuntime | null,
  ): Promise<void> {
    // Parse message using wire codec
    const result = decodeMessage(payload);
    const msg = result.value as any;

    const tag = msgTag(msg);

    if (tag === "Hello") {
      // Duplicate Hello after exchange - ignore
      return;
    }

    if (tag === "Ping") {
      await this.io.send(encodeMessage(messagePong(msg.payload.value.nonce)));
      return;
    }

    if (tag === "Pong") {
      if (msg.connection_id === 0n) {
        this.handleKeepalivePong(msg.payload.value.nonce, keepaliveRuntime);
      }
      return;
    }

    if (tag === "ConnectionClose" || tag === "ProtocolError") {
      // Peer sent Goodbye, connection closing
      throw ConnectionError.closed();
    }

    if (tag === "RequestMessage" && msg.payload.value.body.tag === "Call") {
      const request = msg.payload.value;
      // r[impl flow.call.payload-limit] - enforce negotiated max payload size
      const payloadViolation = this.validatePayloadSize(request.body.value.args.length);
      if (payloadViolation) {
        throw await this.goodbye(payloadViolation);
      }

      // Dispatch to service
      const responsePayload = await dispatcher.dispatchRpc(
        request.body.value.method_id,
        request.body.value.args,
      );

      // r[impl core.call] - Callee sends Response for caller's Request.
      // r[impl core.call.request-id] - Response has same request_id.
      // r[impl call.complete] - Send Response with matching request_id.
      // r[impl call.lifecycle.single-response] - Exactly one Response per Request.
      await this.io.send(encodeMessage(messageResponse(request.id, responsePayload)));

      // Flush any outgoing channel data that handlers may have queued
      await this.flushOutgoing();
      return;
    }

    if (tag === "RequestMessage" && msg.payload.value.body.tag === "Response") {
      // Server doesn't expect Response in basic mode - skip
      return;
    }

    if (tag === "ChannelMessage" && msg.payload.value.body.tag === "Item") {
      const channelId = msg.payload.value.id;
      // r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
      if (channelId === 0n) {
        throw await this.goodbye("rpc.channel.allocation");
      }

      // r[impl channeling.data] - Route Data to registered channel.
      try {
        this.channelRegistry.routeData(channelId, msg.payload.value.body.value.item);
      } catch (e) {
        if (e instanceof ChannelError) {
          if (e.kind === "unknown") {
            // r[impl channeling.unknown] - Unknown channel ID.
            throw await this.goodbye("channeling.unknown");
          }
          if (e.kind === "dataAfterClose") {
            // r[impl channeling.data-after-close] - Data after Close is error.
            throw await this.goodbye("channeling.data-after-close");
          }
          if (e.kind === "overflow") {
            throw await this.goodbye("rpc.flow-control.credit.exhaustion");
          }
        }
        throw e;
      }
      return;
    }

    if (tag === "ChannelMessage" && msg.payload.value.body.tag === "Close") {
      const channelId = msg.payload.value.id;
      // r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
      if (channelId === 0n) {
        throw await this.goodbye("rpc.channel.allocation");
      }

      // r[impl channeling.close] - Close the channel.
      if (!this.channelRegistry.contains(channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      this.channelRegistry.close(channelId);
      return;
    }

    if (tag === "ChannelMessage" && msg.payload.value.body.tag === "Reset") {
      const channelId = msg.payload.value.id;
      // r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
      if (channelId === 0n) {
        throw await this.goodbye("rpc.channel.allocation");
      }

      // r[impl channeling.reset] - Forcefully terminate channel.
      // For now, treat same as Close.
      // TODO: Signal error to Rx<T> instead of clean close.
      if (!this.channelRegistry.contains(channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      this.channelRegistry.close(channelId);
      return;
    }

    if (tag === "ChannelMessage" && msg.payload.value.body.tag === "GrantCredit") {
      const channelId = msg.payload.value.id;
      // r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
      if (channelId === 0n) {
        throw await this.goodbye("rpc.channel.allocation");
      }

      if (!this.channelRegistry.contains(channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      this.channelRegistry.grantCredit(channelId, msg.payload.value.body.value.additional);
      return;
    }

    // Unknown message type (Cancel, etc.) - ignore
  }
}

/** Options for hello exchange. */
export interface HelloExchangeOptions {
  /** Whether to accept incoming virtual connections. Default: false. */
  acceptConnections?: boolean;
  /** Optional proactive keepalive config. Default: disabled. */
  keepalive?: KeepaliveConfig;
}

/**
 * Perform Hello exchange as the acceptor (server).
 *
 * r[impl message.hello.timing] - Send Hello immediately after connection.
 * r[impl message.hello.ordering] - Hello sent before any other message.
 */
export async function helloExchangeInitiator<T extends MessageTransport>(
  io: T,
  ourHello: Hello,
  options: HelloExchangeOptions = {},
): Promise<Connection<T>> {
  const hello: Hello = {
    ...ourHello,
    connection_settings: {
      ...ourHello.connection_settings,
      parity: parityOdd(),
    },
  };

  // Send our Hello immediately
  await io.send(encodeMessage(messageHello(hello)));

  // Wait for peer HelloYourself
  const peerHelloYourself = await waitForPeerHelloYourself(io);

  const negotiated: Negotiated = {
    maxPayloadSize: 1024 * 1024,
    initialCredit: 64 * 1024,
    maxConcurrentRequests: Math.min(
      hello.connection_settings.max_concurrent_requests,
      peerHelloYourself.connection_settings.max_concurrent_requests,
    ),
  };

  return new Connection(
    io,
    Role.Initiator,
    negotiated,
    hello,
    options.acceptConnections,
    options.keepalive ?? null,
  );
}

/**
 * Perform Hello exchange as the initiator (client).
 *
 * r[impl message.hello.timing] - Send Hello immediately after connection.
 * r[impl message.hello.ordering] - Hello sent before any other message.
 */
export async function helloExchangeAcceptor<T extends MessageTransport>(
  io: T,
  ourHello: Hello,
  options: HelloExchangeOptions = {},
): Promise<Connection<T>> {
  const hello: Hello = {
    ...ourHello,
    connection_settings: {
      ...ourHello.connection_settings,
      parity: parityEven(),
    },
  };

  // Wait for peer Hello
  const peerHello = await waitForPeerHello(io);

  const negotiated: Negotiated = {
    maxPayloadSize: 1024 * 1024,
    initialCredit: 64 * 1024,
    maxConcurrentRequests: Math.min(
      hello.connection_settings.max_concurrent_requests,
      peerHello.connection_settings.max_concurrent_requests,
    ),
  };

  // Send HelloYourself
  const hy = helloYourself(parityEven(), hello.connection_settings.max_concurrent_requests);
  await io.send(encodeMessage(messageHelloYourself(hy)));

  return new Connection(
    io,
    Role.Acceptor,
    negotiated,
    hello,
    options.acceptConnections,
    options.keepalive ?? null,
  );
}

async function waitForPeerHello<T extends MessageTransport>(io: T): Promise<Hello> {
  while (true) {
    let payload: Uint8Array | null;
    try {
      payload = await io.recvTimeout(5000);
    } catch {
      // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
      const raw = io.lastDecoded;
      if (raw.length >= 2 && raw[1] > 0x07) {
        await io.send(encodeMessage(messageProtocolError("message.hello.unknown-version")));
        io.close();
        throw ConnectionError.protocol({
          ruleId: "message.hello.unknown-version",
          context: "unknown Hello variant",
        });
      }
      throw ConnectionError.io("failed to receive peer Hello");
    }

    if (!payload) {
      throw ConnectionError.closed();
    }

    // Parse message using wire codec
    // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
    let result;
    try {
      result = decodeMessage(payload);
    } catch {
      throw ConnectionError.io("failed to decode message");
    }
    const msg = result.value as any;

    if (msgTag(msg) === "Hello") {
      // r[impl message.hello.unknown-version] - reject unknown Hello versions
      if (msg.payload.value.version !== 7) {
        await io.send(encodeMessage(messageProtocolError("message.hello.unknown-version")));
        io.close();
        throw ConnectionError.protocol({
          ruleId: "message.hello.unknown-version",
          context: "unknown Hello variant",
        });
      }

      return msg.payload.value;
    }

    // Received non-Hello before Hello exchange completed
    await io.send(encodeMessage(messageProtocolError("message.hello.ordering")));
    io.close();
    throw ConnectionError.protocol({
      ruleId: "message.hello.ordering",
      context: "received non-Hello before Hello exchange",
    });
  }
}

async function waitForPeerHelloYourself<T extends MessageTransport>(
  io: T,
): Promise<HelloYourself> {
  while (true) {
    let payload: Uint8Array | null;
    try {
      payload = await io.recvTimeout(5000);
    } catch {
      throw ConnectionError.io("failed to receive peer HelloYourself");
    }

    if (!payload) {
      throw ConnectionError.closed();
    }

    let result;
    try {
      result = decodeMessage(payload);
    } catch {
      throw ConnectionError.io("failed to decode message");
    }
    const msg = result.value as any;

    if (msgTag(msg) === "HelloYourself") {
      return msg.payload.value;
    }

    // Received unexpected message before HelloYourself
    await io.send(encodeMessage(messageProtocolError("message.hello.ordering")));
    io.close();
    throw ConnectionError.protocol({
      ruleId: "message.hello.ordering",
      context: "received non-HelloYourself before Hello exchange",
    });
  }
}

/** Default Hello message (V7). */
export function defaultHello(): Hello {
  return helloV7(parityOdd(), 64);
}

/**
 * Caller implementation backed by a Connection.
 *
 * Converts CallerRequest to wire format and handles metadata conversion.
 */
export class ConnectionCaller<T extends MessageTransport = MessageTransport> implements Caller {
  private conn: Connection<T>;

  constructor(conn: Connection<T>) {
    this.conn = conn;
  }

  async call(request: CallerRequest): Promise<unknown> {
    // Encode args using the method's tuple schema
    const values = Object.values(request.args);
    const payload =
      values.length === 0
        ? new Uint8Array(0)
        : encodeWithSchema(values, request.descriptor.args, request.schemaRegistry);

    // Convert metadata to wire format entries
    const metadataEntries = request.metadata ? clientMetadataToEntries(request.metadata) : [];

    // Build the wire message
    const requestId = this.conn["nextRequestId"]++;
    const channels = request.channels ?? [];
    const timeoutMs = request.timeoutMs ?? 30000;

    // Create pending request tracking
    const responsePromise = new Promise<Uint8Array>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.conn["pendingRequests"].delete(requestId);
        reject(ConnectionError.io("timeout waiting for response"));
      }, timeoutMs);

      this.conn["pendingRequests"].set(requestId, { resolve, reject, timer });
    });

    // Start message pump
    this.conn["startMessagePump"]();

    // Send request with metadata
    await this.conn["io"].send(
      encodeMessage(
        messageRequest(requestId, request.descriptor.id, payload, metadataEntries, channels),
      ),
    );

    // Flush outgoing channels
    await this.conn.flushOutgoing();

    // Wait for response and decode full Result<T, RoamError<E>> using descriptor.result schema
    const responsePayload = await responsePromise;
    const decoded = decodeWithSchema(
      responsePayload,
      0,
      request.descriptor.result,
      request.schemaRegistry,
    ).value as {
      tag: string;
      value?: unknown;
    };

    if (decoded.tag === "Ok") {
      return decoded.value;
    } else {
      const err = decoded.value as { tag: string; value?: unknown };
      switch (err.tag) {
        case "User":
          throw new RpcError(RpcErrorCode.USER, null, err.value);
        case "UnknownMethod":
          throw new RpcError(RpcErrorCode.UNKNOWN_METHOD);
        case "InvalidPayload":
          throw new RpcError(RpcErrorCode.INVALID_PAYLOAD);
        case "Cancelled":
          throw new RpcError(RpcErrorCode.CANCELLED);
        default:
          throw new RpcError(RpcErrorCode.INVALID_PAYLOAD);
      }
    }
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.conn.getChannelAllocator();
  }

  getChannelRegistry(): ChannelRegistry {
    return this.conn.getChannelRegistry();
  }

  with(middleware: ClientMiddleware): Caller {
    return new MiddlewareCaller(this, [middleware]);
  }
}
