// @bearcove/roam-tcp - TCP transport for roam RPC (Node.js only)
//
// Provides TCP-specific I/O: socket framing, connection state machine, server.

export { CobsFramed } from "./framing.ts";
export {
  Connection,
  type Negotiated,
  Role,
  ConnectionError,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  type ServiceDispatcher,
} from "./connection.ts";
export { Server, type ServerConfig } from "./server.ts";

// Re-export streaming types from core for convenience
export {
  type StreamId,
  StreamError,
  StreamIdAllocator,
  StreamRegistry,
  OutgoingSender,
  Push,
  Pull,
  createRawPush,
  createRawPull,
  type OutgoingMessage,
  type OutgoingPoll,
} from "@bearcove/roam-core";
