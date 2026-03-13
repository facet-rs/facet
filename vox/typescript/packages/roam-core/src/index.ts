export {
  RpcError,
  RpcErrorCode,
  decodeUserError,
} from "@bearcove/roam-wire";

export {
  Tx,
  Rx,
  channel,
  bindChannels,
  type Schema,
  type SchemaRegistry,
  type MethodDescriptor,
  type ServiceDescriptor,
  type RoamCall,
} from "./channeling/index.ts";

export {
  type Link,
  type LinkAttachment,
  type LinkSource,
  singleLinkSource,
} from "./link.ts";

export {
  type Conduit,
  BareConduit,
} from "./conduit.ts";

export { StableConduit } from "./stable_conduit.ts";

export {
  Session,
  SessionHandle,
  ConnectionHandle,
  SessionError,
  SessionRegistry,
  session,
  type SessionAcceptOutcome,
  type IncomingCall,
  type SessionBuilderOptions,
  type SessionConduitKind,
  type SessionTransportOptions,
} from "./session.ts";

export {
  Driver,
  type Dispatcher,
} from "./driver.ts";

export { RequestContext } from "./request_context.ts";

export {
  type ServerMiddleware,
  type ServerCallOutcome,
} from "./server_middleware.ts";

export {
  serverLoggingMiddleware,
  type ServerLoggingOptions,
} from "./server_logging.ts";

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

export {
  type CallerRequest,
  type Caller,
  MiddlewareCaller,
} from "./caller.ts";

export {
  loggingMiddleware,
  type LoggingOptions,
  type ErrorDecoder,
} from "./logging.ts";

export {
  ClientMetadata,
  type ClientMetadataValue,
  clientMetadataToEntries,
  metadataEntriesToClientMetadata,
} from "./metadata.ts";

export {
  OPERATION_ID_METADATA_KEY,
  RETRY_SUPPORT_METADATA_KEY,
  RETRY_SUPPORT_VERSION,
} from "./retry.ts";
