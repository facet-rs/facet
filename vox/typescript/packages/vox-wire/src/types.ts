// Re-export all generated wire protocol types
export * from "./wire.generated.ts";

// Hand-written additions that aren't derivable from Rust shapes
import type {
  ConnectionSettings,
  Message,
  MetadataEntry,
  MetadataFlags,
  MetadataValue,
  Parity,
} from "./wire.generated.ts";

export type Metadata = MetadataEntry[];

export const MetadataFlagValues = {
  NONE: 0n as MetadataFlags,
  SENSITIVE: (1n << 0n) as MetadataFlags,
  NO_PROPAGATE: (1n << 1n) as MetadataFlags,
} as const;

// Helpers
export function parityOdd(): Parity {
  return { tag: "Odd" };
}

export function parityEven(): Parity {
  return { tag: "Even" };
}

export function connectionSettings(parity: Parity, maxConcurrentRequests: number): ConnectionSettings {
  return {
    parity,
    max_concurrent_requests: maxConcurrentRequests,
  };
}



export function metadataString(value: string): MetadataValue {
  return { tag: "String", value };
}

export function metadataBytes(value: Uint8Array): MetadataValue {
  return { tag: "Bytes", value };
}

export function metadataU64(value: bigint): MetadataValue {
  return { tag: "U64", value };
}

export function metadataEntry(
  key: string,
  value: MetadataValue,
  flags: MetadataFlags = MetadataFlagValues.NONE,
): MetadataEntry {
  return { key, value, flags };
}

export function messageProtocolError(description: string): Message {
  return {
    connection_id: 0n,
    payload: { tag: "ProtocolError", value: { description } },
  };
}

export function messagePing(nonce: bigint): Message {
  return {
    connection_id: 0n,
    payload: { tag: "Ping", value: { nonce } },
  };
}

export function messagePong(nonce: bigint): Message {
  return {
    connection_id: 0n,
    payload: { tag: "Pong", value: { nonce } },
  };
}

export function messageConnect(
  connId: bigint,
  connection_settings: ConnectionSettings,
  metadata: Metadata = [],
): Message {
  return {
    connection_id: connId,
    payload: { tag: "ConnectionOpen", value: { connection_settings, metadata } },
  };
}

export function messageAccept(
  connId: bigint,
  connection_settings: ConnectionSettings,
  metadata: Metadata = [],
): Message {
  return {
    connection_id: connId,
    payload: { tag: "ConnectionAccept", value: { connection_settings, metadata } },
  };
}

export function messageReject(connId: bigint, metadata: Metadata = []): Message {
  return {
    connection_id: connId,
    payload: { tag: "ConnectionReject", value: { metadata } },
  };
}

export function messageGoodbye(connId: bigint = 0n, metadata: Metadata = []): Message {
  return {
    connection_id: connId,
    payload: { tag: "ConnectionClose", value: { metadata } },
  };
}

export function messageRequest(
  requestId: bigint,
  methodId: bigint,
  payload: Uint8Array,
  metadata: Metadata = [],
  _channels: bigint[] = [],
  connId: bigint = 0n,
  schemas: Uint8Array = new Uint8Array(0),
): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "RequestMessage",
      value: {
        id: requestId,
        body: {
          tag: "Call",
          value: {
            method_id: methodId,
            args: payload,
            metadata,
            schemas,
          },
        },
      },
    },
  };
}

export function messageResponse(
  requestId: bigint,
  payload: Uint8Array,
  metadata: Metadata = [],
  _channels: bigint[] = [],
  connId: bigint = 0n,
  schemas: Uint8Array = new Uint8Array(0),
): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "RequestMessage",
      value: {
        id: requestId,
        body: {
          tag: "Response",
          value: {
            ret: payload,
            metadata,
            schemas,
          },
        },
      },
    },
  };
}

export function messageCancel(
  requestId: bigint,
  connId: bigint = 0n,
  metadata: Metadata = [],
): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "RequestMessage",
      value: {
        id: requestId,
        body: {
          tag: "Cancel",
          value: {
            metadata,
          },
        },
      },
    },
  };
}

export function messageData(channelId: bigint, payload: Uint8Array, connId: bigint = 0n): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "ChannelMessage",
      value: {
        id: channelId,
        body: {
          tag: "Item",
          value: { item: payload },
        },
      },
    },
  };
}

export function messageClose(channelId: bigint, connId: bigint = 0n, metadata: Metadata = []): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "ChannelMessage",
      value: {
        id: channelId,
        body: {
          tag: "Close",
          value: { metadata },
        },
      },
    },
  };
}

export function messageReset(channelId: bigint, connId: bigint = 0n, metadata: Metadata = []): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "ChannelMessage",
      value: {
        id: channelId,
        body: {
          tag: "Reset",
          value: { metadata },
        },
      },
    },
  };
}

export function messageCredit(channelId: bigint, bytes: number, connId: bigint = 0n): Message {
  return {
    connection_id: connId,
    payload: {
      tag: "ChannelMessage",
      value: {
        id: channelId,
        body: {
          tag: "GrantCredit",
          value: { additional: bytes },
        },
      },
    },
  };
}
