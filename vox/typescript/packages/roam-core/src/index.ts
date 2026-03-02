// @bearcove/roam-runtime - TypeScript runtime for roam RPC
// This package provides the core primitives and dispatcher for roam services.

// RPC error types (for client-side error handling)
export {
  RpcError,
  RpcErrorCode,
  decodeUserError,
} from "@bearcove/roam-wire";

// Channeling API required by generated clients/dispatchers.
export {
  Tx,
  Rx,
  channel,
  type MethodDescriptor,
  type ServiceDescriptor,
  type RoamCall,
  bindChannels,
} from "./channeling/index.ts";

// Transport abstraction
export { type MessageTransport } from "./transport.ts";

// Connection and protocol handling
export {
  Connection,
  ConnectionCaller,
  ConnectionError,
  type Negotiated,
  type KeepaliveConfig,
  type ServiceDispatcher,
  type ChannelingDispatcher,
  type HelloExchangeOptions,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  defaultHello,
} from "./connection.ts";

// Client middleware types
export {
  Extensions,
  type ClientContext,
  type CallRequest,
  type CallOutcome,
  type RejectionCode,
  type Rejection,
  RejectionError,
  type ClientMiddleware,
} from "./middleware.ts";

// Caller abstraction
export { type CallerRequest, type Caller, MiddlewareCaller } from "./caller.ts";

// Call builder for fluent API
export { CallBuilder, withMeta, type CallExecutor } from "./call_builder.ts";

// Logging middleware
export { loggingMiddleware, type LoggingOptions, type ErrorDecoder } from "./logging.ts";

// Metadata conversion utilities
export {
  ClientMetadata,
  type ClientMetadataValue,
  clientMetadataToEntries,
  metadataEntriesToClientMetadata,
} from "./metadata.ts";
