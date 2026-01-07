// Connection state machine and message loop.
//
// Handles the protocol state machine including Hello exchange,
// payload validation, and stream ID management.
//
// Generic over MessageTransport to support different transports:
// - CobsFramed for TCP (byte streams with COBS framing)
// - WsTransport for WebSocket (message-oriented transport)

import { concat, encodeBytes, encodeString } from "./binary/bytes.ts";
import { decodeVarint, decodeVarintNumber, encodeVarint } from "./binary/varint.ts";
import {
  StreamRegistry,
  StreamIdAllocator,
  StreamError,
  type OutgoingPoll,
  Role,
  Push,
  Pull,
  OutgoingSender,
  ChannelReceiver,
} from "./streaming/index.ts";
import { type MessageTransport } from "./transport.ts";

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

  static protocol(ruleId: string, context: string): ConnectionError {
    return new ConnectionError("protocol", context, ruleId);
  }

  static dispatch(message: string): ConnectionError {
    return new ConnectionError("dispatch", message);
  }

  static closed(): ConnectionError {
    return new ConnectionError("closed", "connection closed");
  }
}

/** Hello message version 1. */
interface HelloV1 {
  variant: 0;
  maxPayloadSize: number;
  initialStreamCredit: number;
}

type Hello = HelloV1;

/** Message discriminants. */
const MSG_HELLO = 0;
const MSG_GOODBYE = 1;
const MSG_REQUEST = 2;
const MSG_RESPONSE = 3;
// const MSG_CANCEL = 4;
const MSG_DATA = 5;
const MSG_CLOSE = 6;
const MSG_RESET = 7;
const MSG_CREDIT = 8;

/** Encode a Hello message. */
function encodeHello(hello: Hello): Uint8Array {
  return concat(
    encodeVarint(MSG_HELLO),
    encodeVarint(hello.variant),
    encodeVarint(hello.maxPayloadSize),
    encodeVarint(hello.initialStreamCredit),
  );
}

/** Encode a Goodbye message. */
function encodeGoodbye(reason: string): Uint8Array {
  return concat(encodeVarint(MSG_GOODBYE), encodeString(reason));
}

/** Encode a Response message. */
function encodeResponse(requestId: bigint, payload: Uint8Array): Uint8Array {
  return concat(
    encodeVarint(MSG_RESPONSE),
    encodeVarint(requestId),
    encodeVarint(0), // empty metadata vec
    encodeBytes(payload),
  );
}

/** Encode a Request message. */
function encodeRequest(requestId: bigint, methodId: bigint, payload: Uint8Array): Uint8Array {
  return concat(
    encodeVarint(MSG_REQUEST),
    encodeVarint(requestId),
    encodeVarint(methodId),
    encodeVarint(0), // empty metadata vec
    encodeBytes(payload),
  );
}

/** Encode a Data message. */
function encodeData(streamId: bigint, payload: Uint8Array): Uint8Array {
  return concat(
    encodeVarint(MSG_DATA),
    encodeVarint(streamId),
    encodeBytes(payload),
  );
}

/** Encode a Close message. */
function encodeClose(streamId: bigint): Uint8Array {
  return concat(
    encodeVarint(MSG_CLOSE),
    encodeVarint(streamId),
  );
}

/** Trait for dispatching unary requests to a service. */
export interface ServiceDispatcher {
  /**
   * Dispatch a unary request and return the response payload.
   *
   * The dispatcher is responsible for:
   * - Looking up the method by method_id
   * - Deserializing arguments from payload
   * - Calling the service method
   * - Serializing the response
   */
  dispatchUnary(methodId: bigint, payload: Uint8Array): Promise<Uint8Array>;
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
  private streamAllocator: StreamIdAllocator;
  private streamRegistry: StreamRegistry;
  private nextRequestId: bigint = 1n;

  constructor(
    io: T,
    role: Role,
    negotiated: Negotiated,
    ourHello: Hello,
  ) {
    this.io = io;
    this._role = role;
    this._negotiated = negotiated;
    this.ourHello = ourHello;
    this.streamAllocator = new StreamIdAllocator(role);
    this.streamRegistry = new StreamRegistry();
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
   * Get the stream ID allocator.
   *
   * r[impl streaming.allocation.caller] - Caller allocates ALL stream IDs.
   */
  getStreamAllocator(): StreamIdAllocator {
    return this.streamAllocator;
  }

  /**
   * Get the stream registry.
   */
  getStreamRegistry(): StreamRegistry {
    return this.streamRegistry;
  }

  /**
   * Create a Push stream handle for sending data.
   *
   * Allocates a unique stream ID and registers the stream for outgoing data.
   * The Push handle allows the caller to send values of type T.
   *
   * r[impl streaming.allocation.caller] - Caller allocates stream IDs.
   * r[impl streaming.type] - Push serializes as stream_id on wire.
   *
   * @param serialize - Function to serialize values to bytes
   * @returns [Push handle, stream ID for wire encoding]
   */
  createPush<T>(serialize: (value: T) => Uint8Array): [Push<T>, bigint] {
    const streamId = this.streamAllocator.next();
    const sender = this.streamRegistry.registerOutgoing(streamId);
    const push = new Push(sender, serialize);
    return [push, streamId];
  }

  /**
   * Create a Pull stream handle for receiving data.
   *
   * Allocates a unique stream ID and registers the stream for incoming data.
   * The Pull handle allows the caller to receive values of type T.
   *
   * r[impl streaming.allocation.caller] - Caller allocates stream IDs.
   * r[impl streaming.type] - Pull serializes as stream_id on wire.
   *
   * @param deserialize - Function to deserialize bytes to values
   * @returns [Pull handle, stream ID for wire encoding]
   */
  createPull<T>(deserialize: (bytes: Uint8Array) => T): [Pull<T>, bigint] {
    const streamId = this.streamAllocator.next();
    const receiver = this.streamRegistry.registerIncoming(streamId);
    const pull = new Pull(streamId, receiver, deserialize);
    return [pull, streamId];
  }

  /**
   * Send a Goodbye message and return an error.
   *
   * r[impl message.goodbye.send] - Send Goodbye with rule ID before closing.
   * r[impl core.error.goodbye-reason] - Reason contains violated rule ID.
   */
  async goodbye(ruleId: string): Promise<ConnectionError> {
    try {
      await this.io.send(encodeGoodbye(ruleId));
    } catch {
      // Ignore send errors when closing
    }
    this.io.close();
    return ConnectionError.protocol(ruleId, "");
  }

  /**
   * Validate a stream ID according to protocol rules.
   *
   * Returns the rule ID if validation fails.
   */
  validateStreamId(streamId: bigint): string | null {
    // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
    if (streamId === 0n) {
      return "streaming.id.zero-reserved";
    }

    // r[impl streaming.unknown] - Unknown stream IDs are connection errors.
    if (!this.streamRegistry.contains(streamId)) {
      return "streaming.unknown";
    }

    return null;
  }

  /**
   * Send all pending outgoing stream messages.
   *
   * Drains the outgoing stream channels and sends Data/Close messages
   * to the peer. Call this periodically or after processing requests.
   *
   * r[impl streaming.data] - Send Data messages for outgoing streams.
   * r[impl streaming.close] - Send Close messages when streams end.
   */
  async flushOutgoing(): Promise<void> {
    while (true) {
      const poll = await this.streamRegistry.waitOutgoing();
      if (poll.kind === "pending" || poll.kind === "done") {
        break;
      }
      if (poll.kind === "data") {
        await this.io.send(encodeData(poll.streamId, poll.payload));
      } else if (poll.kind === "close") {
        await this.io.send(encodeClose(poll.streamId));
      }
    }
  }

  /**
   * Validate payload size against negotiated limit.
   *
   * r[impl flow.unary.payload-limit] - Payloads bounded by max_payload_size.
   * r[impl message.hello.negotiation] - Effective limit is min of both peers.
   */
  validatePayloadSize(size: number): string | null {
    if (size > this._negotiated.maxPayloadSize) {
      return "flow.unary.payload-limit";
    }
    return null;
  }

  /**
   * Make a unary RPC call.
   *
   * r[impl core.call] - Caller sends Request, callee responds with Response.
   * r[impl unary.complete] - Request gets exactly one Response.
   *
   * @param methodId - The method ID to call
   * @param payload - The request payload (already encoded)
   * @param timeoutMs - Timeout in milliseconds (default: 30000)
   * @returns The response payload
   */
  async call(methodId: bigint, payload: Uint8Array, timeoutMs: number = 30000): Promise<Uint8Array> {
    const requestId = this.nextRequestId++;

    // Send request
    await this.io.send(encodeRequest(requestId, methodId, payload));

    // Wait for response
    while (true) {
      const data = await this.io.recvTimeout(timeoutMs);
      if (!data) {
        throw ConnectionError.io("timeout waiting for response");
      }

      // Parse message discriminant
      let offset = 0;
      const d0 = decodeVarintNumber(data, offset);
      const msgDisc = d0.value;
      offset = d0.next;

      if (msgDisc === MSG_GOODBYE) {
        throw ConnectionError.closed();
      }

      // Handle streaming messages while waiting for response
      if (msgDisc === MSG_DATA) {
        // Data { stream_id, payload }
        const sid = decodeVarint(data, offset);
        offset = sid.next;
        const pLen = decodeVarintNumber(data, offset);
        offset = pLen.next;
        const dataPayload = data.subarray(offset, offset + pLen.value);
        // Route to registered stream
        try {
          this.streamRegistry.routeData(sid.value, dataPayload);
        } catch {
          // Ignore stream errors during call - connection still valid
        }
        continue;
      }

      if (msgDisc === MSG_CLOSE) {
        // Close { stream_id }
        const sid = decodeVarint(data, offset);
        if (this.streamRegistry.contains(sid.value)) {
          this.streamRegistry.close(sid.value);
        }
        continue;
      }

      if (msgDisc === MSG_CREDIT) {
        // Credit { stream_id, amount } - flow control, currently ignored
        // TODO: Implement flow control tracking
        continue;
      }

      if (msgDisc !== MSG_RESPONSE) {
        // Ignore other messages (Hello after handshake, Reset, etc.)
        continue;
      }

      // Parse Response { request_id, metadata, payload }
      const reqIdResult = decodeVarint(data, offset);
      offset = reqIdResult.next;

      // Skip metadata: Vec<(String, MetadataValue)>
      const mdLen = decodeVarintNumber(data, offset);
      offset = mdLen.next;
      for (let i = 0; i < mdLen.value; i++) {
        // key string
        const kLen = decodeVarintNumber(data, offset);
        offset = kLen.next + kLen.value;
        // value enum
        const vDisc = decodeVarintNumber(data, offset);
        offset = vDisc.next;
        if (vDisc.value === 0) { // String
          const sLen = decodeVarintNumber(data, offset);
          offset = sLen.next + sLen.value;
        } else if (vDisc.value === 1) { // Bytes
          const bLen = decodeVarintNumber(data, offset);
          offset = bLen.next + bLen.value;
        } else if (vDisc.value === 2) { // U64
          const u = decodeVarint(data, offset);
          offset = u.next;
        }
      }

      // Payload
      const pLen = decodeVarintNumber(data, offset);
      offset = pLen.next;
      const responsePayload = data.subarray(offset, offset + pLen.value);

      // Check if this response is for our request
      if (reqIdResult.value === requestId) {
        return responsePayload;
      }
      // Otherwise continue waiting (might be response to different pipelined request)
    }
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
   * r[impl unary.pipelining.allowed] - Handle requests as they arrive.
   * r[impl unary.pipelining.independence] - Each request handled independently.
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

  private async handleMessage(
    payload: Uint8Array,
    dispatcher: ServiceDispatcher,
  ): Promise<void> {
    let offset = 0;
    const d0 = decodeVarintNumber(payload, offset);
    const msgDisc = d0.value;
    offset = d0.next;

    if (msgDisc === MSG_HELLO) {
      // Duplicate Hello after exchange - ignore
      return;
    }

    if (msgDisc === MSG_GOODBYE) {
      // Peer sent Goodbye, connection closing
      throw ConnectionError.closed();
    }

    if (msgDisc === MSG_REQUEST) {
      // Request { request_id, method_id, metadata, payload }
      let tmp = decodeVarint(payload, offset);
      const requestId = tmp.value;
      offset = tmp.next;

      tmp = decodeVarint(payload, offset);
      const methodId = tmp.value;
      offset = tmp.next;

      // Skip metadata: Vec<(String, MetadataValue)>
      const mdLen = decodeVarintNumber(payload, offset);
      offset = mdLen.next;
      for (let i = 0; i < mdLen.value; i++) {
        // key string
        const kLen = decodeVarintNumber(payload, offset);
        offset = kLen.next + kLen.value;
        // value enum
        const vDisc = decodeVarintNumber(payload, offset);
        offset = vDisc.next;
        if (vDisc.value === 0) {
          // String
          const sLen = decodeVarintNumber(payload, offset);
          offset = sLen.next + sLen.value;
        } else if (vDisc.value === 1) {
          // Bytes
          const bLen = decodeVarintNumber(payload, offset);
          offset = bLen.next + bLen.value;
        } else if (vDisc.value === 2) {
          // U64
          const u = decodeVarint(payload, offset);
          offset = u.next;
        } else {
          throw new Error("unknown MetadataValue");
        }
      }

      // payload: bytes
      const pLen = decodeVarintNumber(payload, offset);
      offset = pLen.next;

      // r[impl flow.unary.payload-limit] - enforce negotiated max payload size
      const payloadViolation = this.validatePayloadSize(pLen.value);
      if (payloadViolation) {
        throw await this.goodbye(payloadViolation);
      }

      const start = offset;
      const end = start + pLen.value;
      if (end > payload.length) throw new Error("request payload overrun");
      const payloadBytes = payload.subarray(start, end);

      // Dispatch to service
      const responsePayload = await dispatcher.dispatchUnary(methodId, payloadBytes);

      // r[impl core.call] - Callee sends Response for caller's Request.
      // r[impl core.call.request-id] - Response has same request_id.
      // r[impl unary.complete] - Send Response with matching request_id.
      // r[impl unary.lifecycle.single-response] - Exactly one Response per Request.
      await this.io.send(encodeResponse(requestId, responsePayload));

      // Flush any outgoing stream data that handlers may have queued
      await this.flushOutgoing();
      return;
    }

    if (msgDisc === MSG_RESPONSE) {
      // Server doesn't expect Response in basic mode - skip
      // Skip over the fields to not break parsing
      return;
    }

    if (msgDisc === MSG_DATA) {
      // Data { stream_id, payload }
      const sid = decodeVarint(payload, offset);
      offset = sid.next;

      // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
      if (sid.value === 0n) {
        throw await this.goodbye("streaming.id.zero-reserved");
      }

      // Decode payload
      const pLen = decodeVarintNumber(payload, offset);
      offset = pLen.next;
      const dataPayload = payload.subarray(offset, offset + pLen.value);

      // r[impl streaming.data] - Route Data to registered stream.
      try {
        this.streamRegistry.routeData(sid.value, dataPayload);
      } catch (e) {
        if (e instanceof StreamError) {
          if (e.kind === "unknown") {
            // r[impl streaming.unknown] - Unknown stream ID.
            throw await this.goodbye("streaming.unknown");
          }
          if (e.kind === "dataAfterClose") {
            // r[impl streaming.data-after-close] - Data after Close is error.
            throw await this.goodbye("streaming.data-after-close");
          }
        }
        throw e;
      }
      return;
    }

    if (msgDisc === MSG_CLOSE) {
      // Close { stream_id }
      const sid = decodeVarint(payload, offset);

      // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
      if (sid.value === 0n) {
        throw await this.goodbye("streaming.id.zero-reserved");
      }

      // r[impl streaming.close] - Close the stream.
      if (!this.streamRegistry.contains(sid.value)) {
        throw await this.goodbye("streaming.unknown");
      }
      this.streamRegistry.close(sid.value);
      return;
    }

    if (msgDisc === MSG_RESET) {
      // Reset { stream_id }
      const sid = decodeVarint(payload, offset);

      // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
      if (sid.value === 0n) {
        throw await this.goodbye("streaming.id.zero-reserved");
      }

      // r[impl streaming.reset] - Forcefully terminate stream.
      // For now, treat same as Close.
      // TODO: Signal error to Pull<T> instead of clean close.
      if (!this.streamRegistry.contains(sid.value)) {
        throw await this.goodbye("streaming.unknown");
      }
      this.streamRegistry.close(sid.value);
      return;
    }

    if (msgDisc === MSG_CREDIT) {
      // Credit { stream_id, amount }
      const sid = decodeVarint(payload, offset);

      // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
      if (sid.value === 0n) {
        throw await this.goodbye("streaming.id.zero-reserved");
      }

      // TODO: Implement flow control.
      // For now, validate stream exists but ignore credit.
      if (!this.streamRegistry.contains(sid.value)) {
        throw await this.goodbye("streaming.unknown");
      }
      return;
    }

    // Unknown message type - ignore
  }
}

/**
 * Perform Hello exchange as the acceptor (server).
 *
 * r[impl message.hello.timing] - Send Hello immediately after connection.
 * r[impl message.hello.ordering] - Hello sent before any other message.
 */
export async function helloExchangeAcceptor<T extends MessageTransport>(
  io: T,
  ourHello: Hello,
): Promise<Connection<T>> {
  // Send our Hello immediately
  await io.send(encodeHello(ourHello));

  // Wait for peer Hello
  const peerHello = await waitForPeerHello(io, ourHello);

  // r[impl message.hello.negotiation] - Effective limit is min of both peers.
  const negotiated: Negotiated = {
    maxPayloadSize: Math.min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
    initialCredit: Math.min(
      ourHello.initialStreamCredit,
      peerHello.initialStreamCredit,
    ),
  };

  return new Connection(io, Role.Acceptor, negotiated, ourHello);
}

/**
 * Perform Hello exchange as the initiator (client).
 *
 * r[impl message.hello.timing] - Send Hello immediately after connection.
 * r[impl message.hello.ordering] - Hello sent before any other message.
 */
export async function helloExchangeInitiator<T extends MessageTransport>(
  io: T,
  ourHello: Hello,
): Promise<Connection<T>> {
  // Send our Hello immediately
  await io.send(encodeHello(ourHello));

  // Wait for peer Hello
  const peerHello = await waitForPeerHello(io, ourHello);

  const negotiated: Negotiated = {
    maxPayloadSize: Math.min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
    initialCredit: Math.min(
      ourHello.initialStreamCredit,
      peerHello.initialStreamCredit,
    ),
  };

  return new Connection(io, Role.Initiator, negotiated, ourHello);
}

async function waitForPeerHello<T extends MessageTransport>(io: T, _ourHello: Hello): Promise<Hello> {
  while (true) {
    let payload: Uint8Array | null;
    try {
      payload = await io.recvTimeout(5000);
    } catch {
      // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
      const raw = io.lastDecoded;
      if (raw.length >= 2 && raw[0] === 0x00 && raw[1] !== 0x00) {
        await io.send(encodeGoodbye("message.hello.unknown-version"));
        io.close();
        throw ConnectionError.protocol(
          "message.hello.unknown-version",
          "unknown Hello variant",
        );
      }
      throw ConnectionError.io("failed to receive peer Hello");
    }

    if (!payload) {
      throw ConnectionError.closed();
    }

    // Parse message discriminant
    const d0 = decodeVarintNumber(payload, 0);
    const msgDisc = d0.value;
    let offset = d0.next;

    if (msgDisc === MSG_HELLO) {
      // Parse Hello
      const d1 = decodeVarintNumber(payload, offset);
      const helloVariant = d1.value;
      offset = d1.next;

      // r[impl message.hello.unknown-version] - reject unknown Hello versions
      if (helloVariant !== 0) {
        await io.send(encodeGoodbye("message.hello.unknown-version"));
        io.close();
        throw ConnectionError.protocol(
          "message.hello.unknown-version",
          "unknown Hello variant",
        );
      }

      const maxPayload = decodeVarintNumber(payload, offset);
      offset = maxPayload.next;
      const initialCredit = decodeVarintNumber(payload, offset);

      return {
        variant: 0,
        maxPayloadSize: maxPayload.value,
        initialStreamCredit: initialCredit.value,
      };
    }

    // Received non-Hello before Hello exchange completed
    await io.send(encodeGoodbye("message.hello.ordering"));
    io.close();
    throw ConnectionError.protocol(
      "message.hello.ordering",
      "received non-Hello before Hello exchange",
    );
  }
}

/** Default Hello message. */
export function defaultHello(): Hello {
  return {
    variant: 0,
    maxPayloadSize: 1024 * 1024,
    initialStreamCredit: 64 * 1024,
  };
}
