/**
 * RapaceClient - RPC client for the rapace protocol.
 *
 * This module provides the main client class for making RPC calls to a Rapace
 * server over WebSocket. It handles connection management, request/response
 * multiplexing, and automatic reconnection on failure.
 *
 * @module
 */

import { Frame } from "./frame.js";
import { FrameFlags, hasFlag } from "./frame-flags.js";
import { Transport, connectWebSocket } from "./transport.js";
import { PostcardEncoder } from "../postcard/encoder.js";
import { PostcardDecoder } from "../postcard/decoder.js";

/**
 * Error thrown by RapaceClient operations.
 *
 * @example
 * ```typescript
 * try {
 *   await client.call(methodId, payload);
 * } catch (error) {
 *   if (error instanceof RapaceError) {
 *     console.error('RPC failed:', error.message, 'code:', error.code);
 *   }
 * }
 * ```
 */
export class RapaceError extends Error {
  /**
   * Creates a new RapaceError.
   *
   * @param message - Human-readable error description
   * @param code - Optional numeric error code (matches gRPC error codes when applicable)
   */
  constructor(
    message: string,
    public readonly code?: number
  ) {
    super(message);
    this.name = "RapaceError";
  }
}

/**
 * Internal state for a pending RPC request.
 * @internal
 */
interface PendingRequest {
  resolve: (payload: Uint8Array) => void;
  reject: (error: Error) => void;
}

/**
 * RapaceClient manages RPC calls over a WebSocket transport.
 *
 * The client handles:
 * - Connection establishment and lifecycle management
 * - Request/response multiplexing over a single connection
 * - Automatic message ID and channel ID allocation
 * - Error handling and connection failure recovery
 *
 * @example Basic usage
 * ```typescript
 * import { RapaceClient, PostcardEncoder, computeMethodId } from '@bearcove/rapace';
 *
 * // Connect to server
 * const client = await RapaceClient.connect('ws://localhost:8080');
 *
 * // Make an RPC call
 * const encoder = new PostcardEncoder();
 * encoder.string('hello');
 *
 * const response = await client.call(
 *   computeMethodId('Greeter', 'sayHello'),
 *   encoder.bytes
 * );
 *
 * // Always close when done
 * client.close();
 * ```
 *
 * @example With typed encoding/decoding
 * ```typescript
 * interface GreetRequest { name: string; }
 * interface GreetResponse { message: string; }
 *
 * const response = await client.callTyped<GreetRequest, GreetResponse>(
 *   methodId,
 *   { name: 'World' },
 *   (enc, req) => enc.string(req.name),
 *   (dec) => ({ message: dec.string() })
 * );
 * console.log(response.message); // "Hello, World!"
 * ```
 */
export class RapaceClient {
  private transport: Transport;
  private nextMsgId = 1n;
  private nextChannelId = 1;
  private pending = new Map<bigint, PendingRequest>();
  private receiveLoopRunning = false;
  private closed = false;

  /**
   * Creates a new RapaceClient with an existing transport.
   *
   * For most use cases, prefer the static {@link RapaceClient.connect} method
   * which handles transport creation and connection establishment.
   *
   * @param transport - The transport to use for communication
   */
  constructor(transport: Transport) {
    this.transport = transport;
  }

  /**
   * Connects to a Rapace server over WebSocket.
   *
   * This is the recommended way to create a RapaceClient. It establishes
   * the WebSocket connection, waits for it to be ready, and starts the
   * background receive loop.
   *
   * @param url - WebSocket URL to connect to (e.g., 'ws://localhost:8080' or 'wss://api.example.com/rpc')
   * @returns A connected RapaceClient ready for RPC calls
   * @throws {TransportError} If the connection fails
   *
   * @example
   * ```typescript
   * // Local development
   * const client = await RapaceClient.connect('ws://localhost:8080');
   *
   * // Production with TLS
   * const client = await RapaceClient.connect('wss://api.example.com/rpc');
   * ```
   */
  static async connect(url: string): Promise<RapaceClient> {
    const transport = await connectWebSocket(url);
    const client = new RapaceClient(transport);
    client.startReceiveLoop();
    return client;
  }

  /**
   * Starts the background receive loop.
   * @internal
   */
  private startReceiveLoop(): void {
    if (this.receiveLoopRunning) return;
    this.receiveLoopRunning = true;

    const loop = async () => {
      while (!this.closed && !this.transport.isClosed) {
        try {
          const frame = await this.transport.recv();
          this.handleFrame(frame);
        } catch (error) {
          if (!this.closed) {
            // Connection error - reject all pending requests
            const err =
              error instanceof Error
                ? error
                : new RapaceError(String(error));
            for (const [, pending] of this.pending) {
              pending.reject(err);
            }
            this.pending.clear();
          }
          break;
        }
      }
      this.receiveLoopRunning = false;
    };

    // Start loop in background
    loop().catch(() => {});
  }

  /**
   * Handles a received frame by routing it to the appropriate pending request.
   * @internal
   */
  private handleFrame(frame: Frame): void {
    const msgId = frame.desc.msgId;
    const pending = this.pending.get(msgId);

    if (!pending) {
      // No pending request for this message ID
      // Could be a control frame or unexpected response
      return;
    }

    this.pending.delete(msgId);

    // Check for error flag
    if (hasFlag(frame.desc.flags, FrameFlags.ERROR)) {
      pending.reject(new RapaceError("Server returned error"));
      return;
    }

    // Return payload
    pending.resolve(frame.getPayload());
  }

  /**
   * Makes an RPC call with raw request bytes.
   *
   * This is the low-level RPC method. For type-safe calls with automatic
   * encoding/decoding, use {@link callTyped} instead.
   *
   * @param methodId - The method ID, computed via {@link computeMethodId}
   * @param requestPayload - The encoded request payload as bytes
   * @returns The raw response payload as bytes
   * @throws {RapaceError} If the client is closed or the server returns an error
   *
   * @example
   * ```typescript
   * const encoder = new PostcardEncoder();
   * encoder.u64(itemId).u64(offset).u64(length);
   *
   * const response = await client.call(
   *   computeMethodId('Vfs', 'read'),
   *   encoder.bytes
   * );
   *
   * const decoder = new PostcardDecoder(response);
   * const data = decoder.bytes();
   * const errorCode = decoder.i32();
   * ```
   */
  async call(methodId: number, requestPayload: Uint8Array): Promise<Uint8Array> {
    if (this.closed) {
      throw new RapaceError("Client is closed");
    }

    // Allocate message and channel IDs
    const msgId = this.nextMsgId++;
    const channelId = this.nextChannelId++;

    // Create the frame
    const frame = Frame.data(msgId, channelId, methodId, requestPayload);

    // Register pending request
    const promise = new Promise<Uint8Array>((resolve, reject) => {
      this.pending.set(msgId, { resolve, reject });
    });

    // Send the frame
    await this.transport.send(frame);

    // Wait for response
    return promise;
  }

  /**
   * Makes an RPC call with typed request/response encoding.
   *
   * This method provides type-safe RPC calls by accepting encoder and decoder
   * functions that handle serialization automatically.
   *
   * @typeParam Req - The request type
   * @typeParam Res - The response type
   * @param methodId - The method ID, computed via {@link computeMethodId}
   * @param request - The request object to send
   * @param encode - Function to encode the request into a PostcardEncoder
   * @param decode - Function to decode the response from a PostcardDecoder
   * @returns The decoded response object
   * @throws {RapaceError} If the client is closed or the server returns an error
   *
   * @example
   * ```typescript
   * interface ReadRequest {
   *   itemId: bigint;
   *   offset: bigint;
   *   length: bigint;
   * }
   *
   * interface ReadResponse {
   *   data: Uint8Array;
   *   error: number;
   * }
   *
   * const response = await client.callTyped<ReadRequest, ReadResponse>(
   *   computeMethodId('Vfs', 'read'),
   *   { itemId: 123n, offset: 0n, length: 1024n },
   *   (enc, req) => {
   *     enc.u64(req.itemId).u64(req.offset).u64(req.length);
   *   },
   *   (dec) => ({
   *     data: dec.bytes(),
   *     error: dec.i32()
   *   })
   * );
   * ```
   */
  async callTyped<Req, Res>(
    methodId: number,
    request: Req,
    encode: (encoder: PostcardEncoder, req: Req) => void,
    decode: (decoder: PostcardDecoder) => Res
  ): Promise<Res> {
    const encoder = new PostcardEncoder();
    encode(encoder, request);

    const responsePayload = await this.call(methodId, encoder.bytes);
    const decoder = new PostcardDecoder(responsePayload);
    return decode(decoder);
  }

  /**
   * Closes the client and releases all resources.
   *
   * After calling close():
   * - The WebSocket connection is terminated
   * - All pending requests are rejected with a "Client closed" error
   * - Further calls to {@link call} or {@link callTyped} will throw
   *
   * It's safe to call close() multiple times.
   *
   * @example
   * ```typescript
   * const client = await RapaceClient.connect('ws://localhost:8080');
   * try {
   *   // ... use client ...
   * } finally {
   *   client.close();
   * }
   * ```
   */
  close(): void {
    if (!this.closed) {
      this.closed = true;
      this.transport.close();

      // Reject all pending requests
      for (const [, pending] of this.pending) {
        pending.reject(new RapaceError("Client closed"));
      }
      this.pending.clear();
    }
  }

  /**
   * Returns whether the client has been closed.
   *
   * @returns `true` if {@link close} has been called or the connection was lost
   */
  get isClosed(): boolean {
    return this.closed;
  }
}
