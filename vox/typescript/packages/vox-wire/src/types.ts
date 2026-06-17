// Vox wire protocol types — the Message envelope and its payloads.
//
// The envelope types + the phon `registry`/`schemaId` are generated from the Rust
// `Message` shape into `wire.phon.generated.ts`; this module re-exports them and
// adds the hand-written metadata model + message constructors.

import type { Value } from "@bearcove/phon-schema";

export type {
  Message,
  MessagePayload,
  ProtocolError,
  LaneOpen,
  LaneAccept,
  LaneReject,
  LaneClose,
  RequestMessage,
  RequestBody,
  RequestCall,
  RequestResponse,
  RequestCancel,
  SchemaMessage,
  BindingDirection,
  ChannelMessage,
  ChannelBody,
  ChannelItem,
  ChannelClose,
  ChannelReset,
  ChannelGrantCredit,
  ConnectionSettings,
  Parity,
  Ping,
  Pong,
} from "./wire.phon.generated.ts";

import type {
  BindingDirection,
  ConnectionSettings,
  Message,
  Parity,
} from "./wire.phon.generated.ts";

// Branded id aliases (all `bigint` on the wire).
export type LaneId = bigint;
export type RequestId = bigint;
export type MethodId = bigint;
export type ChannelId = bigint;

// ---------------------------------------------------------------------------
// Metadata
//
// Metadata is a self-describing `Value` map (`r[rpc.metadata]`): keys are strings,
// values are phon `Value`s. Key sigils (`#`, `-`, `-#`) are conventions on the
// key string; there is no separate metadata flag map.
// r[impl rpc.metadata]
// r[impl rpc.metadata.value]
// r[impl rpc.metadata.keys]
// r[impl rpc.metadata.duplicates]
// r[impl rpc.metadata.unknown]
// r[impl rpc.metadata.records]
// r[impl schema.interaction.metadata]
// ---------------------------------------------------------------------------

export type Metadata = Map<string, Value>;

export function emptyMetadata(): Metadata {
  return new Map();
}

/**
 * Coerce a decoded `dynamic` metadata Value into a `Metadata` map. A peer that
 * carries no metadata encodes it as `Value::Null` (the Rust default), which
 * decodes to `null` here; normalize that (and any non-map value) to an empty map.
 */
export function coerceMetadata(value: unknown): Metadata {
  return value instanceof Map ? (value as Metadata) : new Map();
}

// r[impl rpc.metadata.sigils]
export function metadataKeyIsRedacted(key: string): boolean {
  const localKey = key.startsWith("-") ? key.slice(1) : key;
  return localKey.startsWith("#");
}

// r[impl rpc.metadata.sigils]
export function metadataKeyIsNoPropagate(key: string): boolean {
  return key.startsWith("-");
}

// ---------------------------------------------------------------------------
// Message constructors
// ---------------------------------------------------------------------------

export function parityOdd(): Parity {
  return { tag: "Odd" };
}

export function parityEven(): Parity {
  return { tag: "Even" };
}

export function connectionSettings(
  parity: Parity,
  maxConcurrentRequests: number,
  initialChannelCredit = 16,
): ConnectionSettings {
  return {
    parity,
    max_concurrent_requests: maxConcurrentRequests,
    initial_channel_credit: initialChannelCredit,
  };
}

export function messageProtocolError(description: string, laneId: bigint = 0n): Message {
  return { lane_id: laneId, payload: { tag: "ProtocolError", value: { description } } };
}

export function messagePing(nonce: bigint, laneId: bigint = 0n): Message {
  return { lane_id: laneId, payload: { tag: "Ping", value: { nonce } } };
}

export function messagePong(nonce: bigint, laneId: bigint = 0n): Message {
  return { lane_id: laneId, payload: { tag: "Pong", value: { nonce } } };
}

export function messageLaneOpen(
  laneId: bigint,
  connection_settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: { tag: "LaneOpen", value: { connection_settings, metadata } },
  };
}

export function messageLaneAccept(
  laneId: bigint,
  connection_settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: { tag: "LaneAccept", value: { connection_settings, metadata } },
  };
}

export function messageLaneReject(laneId: bigint, metadata: Metadata = emptyMetadata()): Message {
  // r[impl rpc.metadata.records]
  return { lane_id: laneId, payload: { tag: "LaneReject", value: { metadata } } };
}

export function messageLaneClose(laneId: bigint = 0n, metadata: Metadata = emptyMetadata()): Message {
  // r[impl rpc.metadata.records]
  return { lane_id: laneId, payload: { tag: "LaneClose", value: { metadata } } };
}

export function messageRequest(
  requestId: bigint,
  methodId: bigint,
  payload: Uint8Array,
  metadata: Metadata = emptyMetadata(),
  channels: bigint[] = [],
  laneId: bigint = 0n,
  schemas: number[] = [],
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: {
      tag: "RequestMessage",
      value: {
        id: requestId,
        body: {
          tag: "Call",
          value: { method_id: methodId, channels, metadata, args: payload, schemas },
        },
      },
    },
  };
}

export function messageResponse(
  requestId: bigint,
  payload: Uint8Array,
  metadata: Metadata = emptyMetadata(),
  laneId: bigint = 0n,
  schemas: number[] = [],
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: {
      tag: "RequestMessage",
      value: {
        id: requestId,
        body: { tag: "Response", value: { ret: payload, metadata, schemas } },
      },
    },
  };
}

export function messageSchema(
  methodId: bigint,
  direction: "args" | "response",
  schemas: number[],
  laneId: bigint = 0n,
): Message {
  const bindingDirection: BindingDirection =
    direction === "args" ? { tag: "Args" } : { tag: "Response" };
  return {
    lane_id: laneId,
    payload: {
      tag: "SchemaMessage",
      value: {
        method_id: methodId,
        direction: bindingDirection,
        schemas,
      },
    },
  };
}

export function messageCancel(
  requestId: bigint,
  laneId: bigint = 0n,
  metadata: Metadata = emptyMetadata(),
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: {
      tag: "RequestMessage",
      value: { id: requestId, body: { tag: "Cancel", value: { metadata } } },
    },
  };
}

export function messageData(channelId: bigint, payload: Uint8Array, laneId: bigint = 0n): Message {
  return {
    lane_id: laneId,
    payload: {
      tag: "ChannelMessage",
      value: { id: channelId, body: { tag: "Item", value: { item: payload } } },
    },
  };
}

export function messageClose(
  channelId: bigint,
  laneId: bigint = 0n,
  metadata: Metadata = emptyMetadata(),
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: {
      tag: "ChannelMessage",
      value: { id: channelId, body: { tag: "Close", value: { metadata } } },
    },
  };
}

export function messageReset(
  channelId: bigint,
  laneId: bigint = 0n,
  metadata: Metadata = emptyMetadata(),
): Message {
  // r[impl rpc.metadata.records]
  return {
    lane_id: laneId,
    payload: {
      tag: "ChannelMessage",
      value: { id: channelId, body: { tag: "Reset", value: { metadata } } },
    },
  };
}

export function messageCredit(channelId: bigint, additional: number, laneId: bigint = 0n): Message {
  return {
    lane_id: laneId,
    payload: {
      tag: "ChannelMessage",
      value: { id: channelId, body: { tag: "GrantCredit", value: { additional } } },
    },
  };
}
