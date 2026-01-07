// @roam/tcp - TCP transport layer for roam RPC
//
// This module provides the protocol machinery for running roam services over TCP:
// - COBS framing for message boundaries
// - Hello exchange and parameter negotiation
// - Message loop with request dispatch
// - Stream ID validation
// - Flow control enforcement

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
