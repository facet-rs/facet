export {
  RpcError,
  RpcErrorCode,
  decodeUserError,
} from "@bearcove/vox-wire";

export {
  Tx,
  Rx,
  channel,
  bindPhonChannels,
  type TrySendResult,
  type BoundChannels,
  type ChannelCredit,
  type MethodDescriptor,
  type ServiceDescriptor,
  type VoxCall,
} from "./channeling/index.ts";

export {
  SchemaTracker,
  SchemaSendTracker,
  type BindingDirection,
  type PhonMethodSchemas,
  type PhonChannelMeta,
} from "./schema_tracker.ts";

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

export {
  Connection,
  ConnectionHandle,
  Lane,
  PendingLane,
  ConnectionError,
  LaneRejection,
  LANE_REJECT_REASONS,
  VOX_LANE_REJECT_MESSAGE_METADATA_KEY,
  VOX_LANE_REJECT_REASON_METADATA_KEY,
  connect,
  accept,
  connectOnLink,
  acceptOnLink,
  connectLane,
  defaultLaneSettings,
  type IncomingCall,
  type LaneAcceptor,
  type LaneRequest,
  type LaneRejectReason,
  type LaneDebugSnapshot,
  type ConnectionBuilderOptions,
  type ConnectionTransportOptions,
  type LaneOpenOptions,
  type LaneClientConstructor,
  type ConnectLaneOptions,
} from "./connection.ts";

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
  observeEstablishmentFinished,
  observeEstablishmentStarted,
  observerMetricLabels,
  splitQualifiedMethodName,
  type EstablishmentContext,
  type EstablishmentEvent,
  type EstablishmentOutcome,
  type EstablishmentPhase,
  type EstablishmentRole,
  type ObserverMetricLabelInput,
  type ObserverMetricLabelKey,
  type ObserverMetricLabels,
  type VoxObserver,
} from "./observer.ts";

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
  clientMetadataToWire,
} from "./metadata.ts";

export {
  setVoxLogger,
  voxLogger,
  type VoxLogger,
} from "./logger.ts";

export {
  anonymousPeerIdentity,
  anonymousRequestAuthorizationContext,
  compositePeerIdentity,
  ConnectionDeclinedError,
  declineIdentity,
  emptyLaneGrant,
  ESTABLISHMENT_REJECT_REASONS,
  identityBasis,
  noPeerEvidence,
  peerIdentityFromBasis,
  requestAuthorizationContext,
  type EstablishmentRejectReason,
  type HandshakeResult,
  type HandshakePolicyOptions,
  type IdentityBasis,
  type IdentityBasisProvenance,
  type IdentityDecline,
  type IdentityResolution,
  type IdentityResolutionContext,
  type IdentityResolver,
  type LaneGrant,
  type Metadata,
  type PeerEvidence,
  type PeerEvidenceItem,
  type PeerIdentity,
  type PeerIdentityForm,
  type RequestAuthorizationContext,
  voxServiceMetadata,
} from "./handshake.ts";
