// @bearcove/roam-tcp - TCP transport for roam RPC (Node.js only)
//
// Provides TCP-specific I/O: socket framing, connection state machine, server.

export { CobsFramed } from "./framing.ts";
export { Server, type ServerConfig } from "./server.ts";

// Re-export Connection and protocol types from core
export {
  Connection,
  type Negotiated,
  ConnectionError,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  type ServiceDispatcher,
  defaultHello,
} from "@bearcove/roam-core";

// Re-export streaming types from core for convenience
export {
  type StreamId,
  Role,
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
