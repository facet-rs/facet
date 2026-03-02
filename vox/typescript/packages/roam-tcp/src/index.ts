// @bearcove/roam-tcp - TCP transport for roam RPC (Node.js only)
//
// Provides TCP-specific I/O: socket framing, connection state machine, server.

export { LengthPrefixedFramed } from "./framing.ts";
export { Server, type ConnectOptions } from "./transport.ts";

// Re-export only the minimal connection surface needed by TCP consumers.
export {
  Connection,
  type Negotiated,
  ConnectionError,
  type HelloExchangeOptions,
} from "@bearcove/roam-core";
