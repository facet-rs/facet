// @bearcove/roam-tcp - TCP transport for roam RPC (Node.js only)
//
// Provides TCP-specific I/O: socket framing, connection state machine, server.

export { LengthPrefixedFramed } from "./framing.ts";
export { Server, type ConnectOptions } from "./transport.ts";

// Re-export Connection and protocol types from core
export {
  Connection,
  type Negotiated,
  ConnectionError,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  type HelloExchangeOptions,
  type ServiceDispatcher,
  type StreamingDispatcher,
  defaultHello,
} from "@bearcove/roam-core";

// Re-export channel types from core for convenience
export {
  type ChannelId,
  Role,
  ChannelError,
  ChannelIdAllocator,
  ChannelRegistry,
  OutgoingSender,
  Tx,
  Rx,
  createServerTx,
  createServerRx,
  type OutgoingMessage,
  type OutgoingPoll,
  type TaskMessage,
  type TaskSender,
} from "@bearcove/roam-core";
