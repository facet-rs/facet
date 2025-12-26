/**
 * @bearcove/rapace - TypeScript client for the Rapace RPC protocol.
 *
 * Rapace is a high-performance binary RPC protocol designed for efficient
 * communication between services. This library provides a TypeScript/JavaScript
 * implementation that works in both browser and Node.js environments.
 *
 * ## Features
 *
 * - **WebSocket transport** — Real-time bidirectional communication
 * - **Postcard serialization** — Compact binary format compatible with Rust
 * - **Zero dependencies** — Pure TypeScript implementation
 * - **Type-safe** — Full TypeScript support with generated client types
 *
 * ## Quick Start
 *
 * @example Basic RPC call
 * ```typescript
 * import { RapaceClient, PostcardEncoder, PostcardDecoder, computeMethodId } from '@bearcove/rapace';
 *
 * // Connect to server
 * const client = await RapaceClient.connect('ws://localhost:8080');
 *
 * // Encode request
 * const encoder = new PostcardEncoder();
 * encoder.u64(123n).string("hello");
 *
 * // Make RPC call
 * const methodId = computeMethodId('MyService', 'myMethod');
 * const response = await client.call(methodId, encoder.bytes);
 *
 * // Decode response
 * const decoder = new PostcardDecoder(response);
 * const result = decoder.string();
 *
 * // Clean up
 * client.close();
 * ```
 *
 * @example Using typed calls
 * ```typescript
 * const result = await client.callTyped(
 *   methodId,
 *   { name: "test", count: 42 },
 *   (enc, req) => { enc.string(req.name).u32(req.count); },
 *   (dec) => ({ success: dec.bool(), message: dec.string() })
 * );
 * ```
 *
 * @packageDocumentation
 * @module @bearcove/rapace
 */

// Re-export postcard serialization
export {
  PostcardEncoder,
  PostcardDecoder,
  encode,
  decode,
  ByteReader,
  VarintError,
  encodeVarint,
  decodeVarint,
  decodeVarintNumber,
  zigzagEncode,
  zigzagDecode,
  encodeSignedVarint,
  decodeSignedVarint,
  decodeSignedVarintNumber,
} from "./postcard/index.js";
export type { PostcardEncodable, PostcardDecodable } from "./postcard/index.js";

// Re-export rapace protocol
export {
  // Client
  RapaceClient,
  RapaceError,
  // Transport
  TransportError,
  WebSocketTransport,
  connectWebSocket,
  // Frame
  Frame,
  MIN_FRAME_SIZE,
  // Descriptor
  MsgDescHot,
  MsgDescHotError,
  INLINE_PAYLOAD_SIZE,
  INLINE_PAYLOAD_SLOT,
  NO_DEADLINE,
  MSG_DESC_HOT_SIZE,
  // Flags
  FrameFlags,
  hasFlag,
  setFlag,
  clearFlag,
  // Method ID
  computeMethodId,
  computeMethodIdFromFullName,
} from "./rapace/index.js";
export type { Transport, FrameFlagsType } from "./rapace/index.js";
