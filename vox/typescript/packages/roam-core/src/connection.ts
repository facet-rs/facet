// Connection state machine and message loop.
//
// Handles the protocol state machine including Hello exchange,
// payload validation, and stream ID management.
//
// Generic over MessageTransport to support different transports:
// - CobsFramed for TCP (byte streams with COBS framing)
// - WsTransport for WebSocket (message-oriented transport)

import {
  type Hello,
  type MetadataEntry,
  helloV2,
  messageHello,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageAccept,
  messageReject,
  messageData,
  messageClose,
  encodeMessage,
  decodeMessage,
} from "@bearcove/roam-wire";
import {
  ChannelRegistry,
  ChannelIdAllocator,
  ChannelError,
  Role,
  type TaskMessage,
  type TaskSender,
} from "./channeling/index.ts";
import { type MessageTransport } from "./transport.ts";
import type { Caller, CallerRequest } from "./caller.ts";
import { MiddlewareCaller } from "./caller.ts";
import type { ClientMiddleware } from "./middleware.ts";
import { metadataMapToEntries } from "./metadata.ts";

// Note: Role is exported from streaming/index.ts in roam-core's main export

/** Negotiated connection parameters after Hello exchange. */
export interface Negotiated {
  /** Effective max payload size (min of both peers). */
  maxPayloadSize: number;
  /** Initial stream credit (min of both peers). */
  initialCredit: number;
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

/** Trait for dispatching RPC requests to a service. */
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
 * Streaming-aware service dispatcher.
 *
 * Unlike ServiceDispatcher which returns a response directly, this interface
 * sends responses (and streaming Data/Close messages) via a TaskSender callback.
 * This ensures proper ordering: all Data/Close messages are sent before Response.
 */
export interface StreamingDispatcher {
  /**
   * Dispatch a request that may involve streaming.
   *
   * The dispatcher is responsible for:
   * - Looking up the method by method_id
   * - Deserializing arguments from payload
   * - Creating Rx/Tx handles for stream arguments using the registry
   * - Calling the service method
   * - Sending Data/Close messages for any Tx streams via taskSender
   * - Sending the Response message via taskSender when done
   *
   * @param methodId - The method ID to dispatch
   * @param payload - The request payload
   * @param requestId - The request ID for the response
   * @param registry - Stream registry for binding stream arguments
   * @param taskSender - Callback to send TaskMessage (Data/Close/Response)
   */
  dispatch(
    methodId: bigint,
    payload: Uint8Array,
    requestId: bigint,
    registry: ChannelRegistry,
    taskSender: TaskSender,
  ): Promise<void>;
}

/**
 * A live connection with completed Hello exchange.
 *
 * Generic over MessageTransport to support different transports
 * (CobsFramed for TCP, WsTransport for WebSocket).
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
  ) {
    this.io = io;
    this._role = role;
    this._negotiated = negotiated;
    this.ourHello = ourHello;
    this.channelAllocator = new ChannelIdAllocator(role);
    this.channelRegistry = new ChannelRegistry();
    this._acceptConnections = acceptConnections;
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
      await this.io.send(encodeMessage(messageGoodbye(ruleId)));
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
    // r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
    if (channelId === 0n) {
      return "channeling.id.zero-reserved";
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
    while (true) {
      const poll = await this.channelRegistry.waitOutgoing();
      if (poll.kind === "pending" || poll.kind === "done") {
        break;
      }
      if (poll.kind === "data") {
        await this.io.send(encodeMessage(messageData(poll.channelId, poll.payload)));
      } else if (poll.kind === "close") {
        await this.io.send(encodeMessage(messageClose(poll.channelId)));
      }
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

  /**
   * Start the message pump if not already running.
   * The pump receives messages and routes responses to pending requests.
   */
  private startMessagePump(): void {
    if (this.messagePumpRunning) return;
    this.messagePumpRunning = true;

    this.messagePumpPromise = (async () => {
      try {
        while (this.pendingRequests.size > 0) {
          const data = await this.io.recvTimeout(100); // Short timeout to check for new requests
          if (!data) {
            // No message received, but keep running if there are pending requests
            continue;
          }

          // Parse message using wire codec
          const result = decodeMessage(data);
          const msg = result.value;

          if (msg.tag === "Goodbye") {
            // Reject all pending requests
            const error = ConnectionError.closed();
            for (const [, pending] of this.pendingRequests) {
              clearTimeout(pending.timer);
              pending.reject(error);
            }
            this.pendingRequests.clear();
            return;
          }

          // Handle streaming messages
          if (msg.tag === "Data") {
            try {
              this.channelRegistry.routeData(msg.channelId, msg.payload);
            } catch {
              // Ignore stream errors - connection still valid
            }
            continue;
          }

          if (msg.tag === "Close") {
            if (this.channelRegistry.contains(msg.channelId)) {
              this.channelRegistry.close(msg.channelId);
            }
            continue;
          }

          if (msg.tag === "Credit") {
            // Flow control, currently ignored
            continue;
          }

          if (msg.tag === "Response") {
            // Route response to the correct pending request
            const pending = this.pendingRequests.get(msg.requestId);
            if (pending) {
              clearTimeout(pending.timer);
              this.pendingRequests.delete(msg.requestId);
              pending.resolve(msg.payload);
            }
            // Ignore responses for unknown request IDs (already timed out?)
            continue;
          }

          // Ignore other messages (Hello after handshake, Reset, etc.)
        }
      } finally {
        this.messagePumpRunning = false;
        this.messagePumpPromise = null;
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
    await this.io.send(encodeMessage(messageRequest(requestId, methodId, payload, metadata, channels)));

    // Flush any pending outgoing stream data (for client-to-server streaming)
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
   * Run the message loop with a streaming-aware dispatcher.
   *
   * This is the main event loop that:
   * - Receives messages from the peer
   * - Validates them according to protocol rules
   * - Dispatches requests to the service with stream binding
   * - Collects TaskMessages and sends them in order (Data/Close before Response)
   *
   * r[impl call.pipelining.allowed] - Handle requests as they arrive.
   * r[impl call.pipelining.independence] - Each request handled independently.
   */
  async runStreaming(dispatcher: StreamingDispatcher): Promise<void> {
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

    while (true) {
      // Flush any pending task messages from handlers
      await flushTaskQueue();

      // Start receiving if we don't have a pending receive
      if (!pendingRecv) {
        pendingRecv = this.io
          .recvTimeout(30000)
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
        // Connection closed - wait for all in-flight handlers to complete
        await Promise.all(inFlightHandlers);
        await flushTaskQueue();
        return;
      }

      try {
        const handlerPromise = this.handleStreamingMessage(payload, dispatcher, taskSender);

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
   * Handle a message in streaming mode.
   *
   * Returns a Promise for Request messages (the handler running concurrently),
   * or undefined for other message types that are processed synchronously.
   */
  private handleStreamingMessage(
    payload: Uint8Array,
    dispatcher: StreamingDispatcher,
    taskSender: TaskSender,
  ): Promise<void> | undefined {
    // Parse message using wire codec
    const result = decodeMessage(payload);
    const msg = result.value;

    if (msg.tag === "Hello") {
      return undefined; // Duplicate Hello after exchange - ignore
    }

    // r[impl message.connect.initiate] - Handle Connect requests for virtual connections.
    if (msg.tag === "Connect") {
      // r[impl core.conn.accept-required] - Accept or reject the connection request.
      if (this._acceptConnections) {
        // r[impl core.conn.id-allocation] - Allocate a new connection ID.
        const connId = this.nextConnId++;
        this.virtualConnections.add(connId);
        // r[impl message.accept.response] - Send Accept with new conn_id.
        const acceptMsg = messageAccept(msg.requestId, connId, []);
        this.io.send(encodeMessage(acceptMsg));
      } else {
        // r[impl message.reject.response] - Reject since not listening.
        const rejectMsg = messageReject(msg.requestId, "not listening", []);
        this.io.send(encodeMessage(rejectMsg));
      }
      return undefined;
    }

    // Accept and Reject are responses to our Connect requests - ignore in server mode
    if (msg.tag === "Accept" || msg.tag === "Reject") {
      return undefined;
    }

    if (msg.tag === "Goodbye") {
      // r[impl message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link.
      if (msg.connId === 0n) {
        throw ConnectionError.closed();
      }
      // r[impl core.conn.lifecycle] - Close virtual connection if it exists.
      // r[impl core.conn.independence] - Ignore Goodbye on unknown connection.
      if (this.virtualConnections.has(msg.connId)) {
        this.virtualConnections.delete(msg.connId);
      }
      // Ignore Goodbye on unknown connection IDs
      return undefined;
    }

    if (msg.tag === "Request") {
      // r[impl flow.call.payload-limit] - Validate payload size
      const payloadViolation = this.validatePayloadSize(msg.payload.length);
      if (payloadViolation) {
        throw ConnectionError.protocol({
          ruleId: payloadViolation,
          context: "payload exceeds max size",
        });
      }

      // Dispatch with streaming support - return the promise, don't await it!
      // This allows the handler to run concurrently while we continue receiving messages.
      return dispatcher.dispatch(
        msg.methodId,
        msg.payload,
        msg.requestId,
        this.channelRegistry,
        taskSender,
      );
    }

    // Handle other message types (Data, Close, etc.) synchronously
    if (msg.tag === "Response") {
      return undefined;
    }

    if (msg.tag === "Data") {
      if (msg.channelId === 0n) {
        // Can't send goodbye synchronously - throw protocol error
        throw ConnectionError.protocol({
          ruleId: "channeling.id.zero-reserved",
          context: "channel ID 0 is reserved",
        });
      }
      try {
        this.channelRegistry.routeData(msg.channelId, msg.payload);
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
        }
        throw e;
      }
      return undefined;
    }

    if (msg.tag === "Close") {
      if (msg.channelId === 0n) {
        throw ConnectionError.protocol({
          ruleId: "channeling.id.zero-reserved",
          context: "channel ID 0 is reserved",
        });
      }
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw ConnectionError.protocol({
          ruleId: "channeling.unknown",
          context: "unknown channel ID",
        });
      }
      this.channelRegistry.close(msg.channelId);
      return undefined;
    }

    if (msg.tag === "Reset") {
      if (msg.channelId === 0n) {
        throw ConnectionError.protocol({
          ruleId: "channeling.id.zero-reserved",
          context: "channel ID 0 is reserved",
        });
      }
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw ConnectionError.protocol({
          ruleId: "channeling.unknown",
          context: "unknown channel ID",
        });
      }
      // TODO: Signal error to Rx<T> instead of clean close
      this.channelRegistry.close(msg.channelId);
      return undefined;
    }

    if (msg.tag === "Credit") {
      if (msg.channelId === 0n) {
        throw ConnectionError.protocol({
          ruleId: "channeling.id.zero-reserved",
          context: "channel ID 0 is reserved",
        });
      }
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw ConnectionError.protocol({
          ruleId: "channeling.unknown",
          context: "unknown channel ID",
        });
      }
      return undefined;
    }

    return undefined; // Unknown message type - ignore
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
    while (true) {
      let payload: Uint8Array | null;
      try {
        payload = await this.io.recvTimeout(30000);
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
        return; // Connection closed or timeout
      }

      try {
        await this.handleMessage(payload, dispatcher);
      } catch (e) {
        if (e instanceof ConnectionError) throw e;
        // r[impl message.decode-error] - send goodbye on decode failure
        throw await this.goodbye("message.decode-error");
      }
    }
  }

  private async handleMessage(payload: Uint8Array, dispatcher: ServiceDispatcher): Promise<void> {
    // Parse message using wire codec
    const result = decodeMessage(payload);
    const msg = result.value;

    if (msg.tag === "Hello") {
      // Duplicate Hello after exchange - ignore
      return;
    }

    if (msg.tag === "Goodbye") {
      // Peer sent Goodbye, connection closing
      throw ConnectionError.closed();
    }

    if (msg.tag === "Request") {
      // r[impl flow.call.payload-limit] - enforce negotiated max payload size
      const payloadViolation = this.validatePayloadSize(msg.payload.length);
      if (payloadViolation) {
        throw await this.goodbye(payloadViolation);
      }

      // Dispatch to service
      const responsePayload = await dispatcher.dispatchRpc(msg.methodId, msg.payload);

      // r[impl core.call] - Callee sends Response for caller's Request.
      // r[impl core.call.request-id] - Response has same request_id.
      // r[impl call.complete] - Send Response with matching request_id.
      // r[impl call.lifecycle.single-response] - Exactly one Response per Request.
      await this.io.send(encodeMessage(messageResponse(msg.requestId, responsePayload)));

      // Flush any outgoing stream data that handlers may have queued
      await this.flushOutgoing();
      return;
    }

    if (msg.tag === "Response") {
      // Server doesn't expect Response in basic mode - skip
      return;
    }

    if (msg.tag === "Data") {
      // r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
      if (msg.channelId === 0n) {
        throw await this.goodbye("channeling.id.zero-reserved");
      }

      // r[impl channeling.data] - Route Data to registered channel.
      try {
        this.channelRegistry.routeData(msg.channelId, msg.payload);
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
        }
        throw e;
      }
      return;
    }

    if (msg.tag === "Close") {
      // r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
      if (msg.channelId === 0n) {
        throw await this.goodbye("channeling.id.zero-reserved");
      }

      // r[impl channeling.close] - Close the channel.
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      this.channelRegistry.close(msg.channelId);
      return;
    }

    if (msg.tag === "Reset") {
      // r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
      if (msg.channelId === 0n) {
        throw await this.goodbye("channeling.id.zero-reserved");
      }

      // r[impl channeling.reset] - Forcefully terminate channel.
      // For now, treat same as Close.
      // TODO: Signal error to Rx<T> instead of clean close.
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      this.channelRegistry.close(msg.channelId);
      return;
    }

    if (msg.tag === "Credit") {
      // r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
      if (msg.channelId === 0n) {
        throw await this.goodbye("channeling.id.zero-reserved");
      }

      // TODO: Implement flow control.
      // For now, validate channel exists but ignore credit.
      if (!this.channelRegistry.contains(msg.channelId)) {
        throw await this.goodbye("channeling.unknown");
      }
      return;
    }

    // Unknown message type (Cancel, etc.) - ignore
  }
}

/** Options for hello exchange. */
export interface HelloExchangeOptions {
  /** Whether to accept incoming virtual connections. Default: false. */
  acceptConnections?: boolean;
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
  // Send our Hello immediately
  await io.send(encodeMessage(messageHello(ourHello)));

  // Wait for peer Hello
  const peerHello = await waitForPeerHello(io, ourHello);

  const negotiated: Negotiated = {
    maxPayloadSize: Math.min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
    initialCredit: Math.min(ourHello.initialChannelCredit, peerHello.initialChannelCredit),
  };

  return new Connection(io, Role.Initiator, negotiated, ourHello, options.acceptConnections);
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
  // Wait for peer Hello
  const peerHello = await waitForPeerHello(io, ourHello);

  const negotiated: Negotiated = {
    maxPayloadSize: Math.min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
    initialCredit: Math.min(ourHello.initialChannelCredit, peerHello.initialChannelCredit),
  };

  // Send our Hello
  await io.send(encodeMessage(messageHello(ourHello)));

  return new Connection(io, Role.Acceptor, negotiated, ourHello, options.acceptConnections);
}

async function waitForPeerHello<T extends MessageTransport>(
  io: T,
  _ourHello: Hello,
): Promise<Hello> {
  while (true) {
    let payload: Uint8Array | null;
    try {
      payload = await io.recvTimeout(5000);
    } catch {
      // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
      const raw = io.lastDecoded;
      if (raw.length >= 2 && raw[0] === 0x00 && raw[1] !== 0x00) {
        await io.send(encodeMessage(messageGoodbye("message.hello.unknown-version")));
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
      // Check if this is an unknown Hello variant: [Message::Hello=0][Hello::unknown=1+]
      if (payload.length >= 2 && payload[0] === 0x00 && payload[1] !== 0x00) {
        await io.send(encodeMessage(messageGoodbye("message.hello.unknown-version")));
        io.close();
        throw ConnectionError.protocol({
          ruleId: "message.hello.unknown-version",
          context: "unknown Hello variant",
        });
      }
      throw ConnectionError.io("failed to decode message");
    }
    const msg = result.value;

    if (msg.tag === "Hello") {
      // r[impl message.hello.unknown-version] - reject unknown Hello versions
      // Accept V1 (deprecated) and V2 (current)
      if (msg.value.tag !== "V1" && msg.value.tag !== "V2") {
        await io.send(encodeMessage(messageGoodbye("message.hello.unknown-version")));
        io.close();
        throw ConnectionError.protocol({
          ruleId: "message.hello.unknown-version",
          context: "unknown Hello variant",
        });
      }

      return msg.value;
    }

    // Received non-Hello before Hello exchange completed
    await io.send(encodeMessage(messageGoodbye("message.hello.ordering")));
    io.close();
    throw ConnectionError.protocol({
      ruleId: "message.hello.ordering",
      context: "received non-Hello before Hello exchange",
    });
  }
}

/** Default Hello message (V2 for virtual connection support). */
export function defaultHello(): Hello {
  return helloV2(1024 * 1024, 64 * 1024);
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

  async call(request: CallerRequest): Promise<Uint8Array> {
    // Encode the payload using the deferred encoding function
    const payload = request.encode(request.args);

    // Convert metadata map to wire format entries
    const metadataEntries = request.metadata
      ? metadataMapToEntries(request.metadata)
      : [];

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
        messageRequest(requestId, request.methodId, payload, metadataEntries, channels)
      )
    );

    // Flush outgoing streams
    await this.conn.flushOutgoing();

    return responsePromise;
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
