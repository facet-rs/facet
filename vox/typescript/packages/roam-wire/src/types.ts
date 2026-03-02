// Roam wire protocol types for TypeScript (canonical v7 model).

// Connection settings and handshake
export type Parity = { tag: "Odd" } | { tag: "Even" };

export interface ConnectionSettings {
  parity: Parity;
  max_concurrent_requests: number;
}

export interface Hello {
  version: number;
  connection_settings: ConnectionSettings;
  metadata: Metadata;
}

export interface HelloYourself {
  connection_settings: ConnectionSettings;
  metadata: Metadata;
}

export interface ProtocolError {
  description: string;
}

export interface Ping {
  nonce: bigint;
}

export interface Pong {
  nonce: bigint;
}

// Metadata
export interface MetadataValueString {
  tag: "String";
  value: string;
}

export interface MetadataValueBytes {
  tag: "Bytes";
  value: Uint8Array;
}

export interface MetadataValueU64 {
  tag: "U64";
  value: bigint;
}

export type MetadataValue = MetadataValueString | MetadataValueBytes | MetadataValueU64;

export type MetadataFlagsRepr = bigint;

export const MetadataFlags = {
  NONE: 0n,
  SENSITIVE: 1n << 0n,
  NO_PROPAGATE: 1n << 1n,
} as const;

export interface MetadataEntry {
  key: string;
  value: MetadataValue;
  flags: MetadataFlagsRepr;
}

export type Metadata = MetadataEntry[];

// Connection control
export interface ConnectionOpen {
  connection_settings: ConnectionSettings;
  metadata: Metadata;
}

export interface ConnectionAccept {
  connection_settings: ConnectionSettings;
  metadata: Metadata;
}

export interface ConnectionReject {
  metadata: Metadata;
}

export interface ConnectionClose {
  metadata: Metadata;
}

// RPC
export interface RequestCall {
  method_id: bigint;
  args: Uint8Array;
  channels: bigint[];
  metadata: Metadata;
}

export interface RequestResponse {
  ret: Uint8Array;
  channels: bigint[];
  metadata: Metadata;
}

export interface RequestCancel {
  metadata: Metadata;
}

export type RequestBody =
  | { tag: "Call"; value: RequestCall }
  | { tag: "Response"; value: RequestResponse }
  | { tag: "Cancel"; value: RequestCancel };

export interface RequestMessage {
  id: bigint;
  body: RequestBody;
}

// Channels
export interface ChannelItem {
  item: Uint8Array;
}

export interface ChannelClose {
  metadata: Metadata;
}

export interface ChannelReset {
  metadata: Metadata;
}

export interface ChannelGrantCredit {
  additional: number;
}

export type ChannelBody =
  | { tag: "Item"; value: ChannelItem }
  | { tag: "Close"; value: ChannelClose }
  | { tag: "Reset"; value: ChannelReset }
  | { tag: "GrantCredit"; value: ChannelGrantCredit };

export interface ChannelMessage {
  id: bigint;
  body: ChannelBody;
}

// Top-level message
export type MessagePayload =
  | { tag: "Hello"; value: Hello }
  | { tag: "HelloYourself"; value: HelloYourself }
  | { tag: "ProtocolError"; value: ProtocolError }
  | { tag: "Ping"; value: Ping }
  | { tag: "Pong"; value: Pong }
  | { tag: "ConnectionOpen"; value: ConnectionOpen }
  | { tag: "ConnectionAccept"; value: ConnectionAccept }
  | { tag: "ConnectionReject"; value: ConnectionReject }
  | { tag: "ConnectionClose"; value: ConnectionClose }
  | { tag: "RequestMessage"; value: RequestMessage }
  | { tag: "ChannelMessage"; value: ChannelMessage };

export interface Message {
  connection_id: bigint;
  payload: MessagePayload;
}

export type MessageHello = Message & { payload: { tag: "Hello"; value: Hello } };
export type MessageHelloYourself = Message & {
  payload: { tag: "HelloYourself"; value: HelloYourself };
};
export type MessageProtocolError = Message & {
  payload: { tag: "ProtocolError"; value: ProtocolError };
};
export type MessagePing = Message & {
  payload: { tag: "Ping"; value: Ping };
};
export type MessagePong = Message & {
  payload: { tag: "Pong"; value: Pong };
};
export type MessageConnect = Message & { payload: { tag: "ConnectionOpen"; value: ConnectionOpen } };
export type MessageAccept = Message & {
  payload: { tag: "ConnectionAccept"; value: ConnectionAccept };
};
export type MessageReject = Message & {
  payload: { tag: "ConnectionReject"; value: ConnectionReject };
};
export type MessageGoodbye = Message & {
  payload: { tag: "ConnectionClose"; value: ConnectionClose };
};
export type MessageRequest = Message & {
  payload: { tag: "RequestMessage"; value: { id: bigint; body: { tag: "Call"; value: RequestCall } } };
};
export type MessageResponse = Message & {
  payload: {
    tag: "RequestMessage";
    value: { id: bigint; body: { tag: "Response"; value: RequestResponse } };
  };
};
export type MessageCancel = Message & {
  payload: {
    tag: "RequestMessage";
    value: { id: bigint; body: { tag: "Cancel"; value: RequestCancel } };
  };
};
export type MessageData = Message & {
  payload: { tag: "ChannelMessage"; value: { id: bigint; body: { tag: "Item"; value: ChannelItem } } };
};
export type MessageClose = Message & {
  payload: {
    tag: "ChannelMessage";
    value: { id: bigint; body: { tag: "Close"; value: ChannelClose } };
  };
};
export type MessageReset = Message & {
  payload: {
    tag: "ChannelMessage";
    value: { id: bigint; body: { tag: "Reset"; value: ChannelReset } };
  };
};
export type MessageCredit = Message & {
  payload: {
    tag: "ChannelMessage";
    value: { id: bigint; body: { tag: "GrantCredit"; value: ChannelGrantCredit } };
  };
};

export const MessageDiscriminant = {
  Hello: 0,
  HelloYourself: 1,
  ProtocolError: 2,
  ConnectionOpen: 3,
  ConnectionAccept: 4,
  ConnectionReject: 5,
  ConnectionClose: 6,
  RequestMessage: 7,
  ChannelMessage: 8,
  Ping: 9,
  Pong: 10,
} as const;

export const MetadataValueDiscriminant = {
  String: 0,
  Bytes: 1,
  U64: 2,
} as const;


export const HelloDiscriminant = {
  V7: 7,
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

export function helloV7(
  parity: Parity,
  maxConcurrentRequests: number,
  metadata: Metadata = [],
): Hello {
  return {
    version: 7,
    connection_settings: connectionSettings(parity, maxConcurrentRequests),
    metadata,
  };
}

export function helloYourself(
  parity: Parity,
  maxConcurrentRequests: number,
  metadata: Metadata = [],
): HelloYourself {
  return {
    connection_settings: connectionSettings(parity, maxConcurrentRequests),
    metadata,
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
  flags: MetadataFlagsRepr = MetadataFlags.NONE,
): MetadataEntry {
  return { key, value, flags };
}

export function messageHello(hello: Hello): Message {
  return { connection_id: 0n, payload: { tag: "Hello", value: hello } };
}

export function messageHelloYourself(value: HelloYourself): Message {
  return { connection_id: 0n, payload: { tag: "HelloYourself", value } };
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
  channels: bigint[] = [],
  connId: bigint = 0n,
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
            channels,
            metadata,
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
  channels: bigint[] = [],
  connId: bigint = 0n,
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
            channels,
            metadata,
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
