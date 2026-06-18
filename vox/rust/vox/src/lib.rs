//! vox — type-safe RPC with channels, service lanes, and automatic schema evolution.
//!
//! # Defining a service
//!
//! Services are ordinary Rust traits annotated with [`#[vox::service]`](macro@service):
//!
//! ```
//! #[vox::service]
//! trait Hello {
//!     async fn say_hello(&self) -> String;
//! }
//! ```
//!
//! The macro generates a `HelloClient` (typed caller), a `HelloDispatcher`
//! (request router), and serialization glue. You implement the trait on a
//! struct and hand it to a dispatcher.
//!
//! # Connecting
//!
//! [`connect()`] is the fastest path to calling a remote service:
//!
//! ```no_run
//! # #[vox::service]
//! # trait Hello {
//! #     async fn say_hello(&self) -> String;
//! # }
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client: HelloClient = vox::connect("127.0.0.1:9000").await?;
//! let reply = client.say_hello().await?;
//! # Ok(())
//! # }
//! ```
//!
//! The address string selects the transport:
//!
//! | Scheme | Transport |
//! |--------|-----------|
//! | `tcp://host:port` (or bare `host:port`) | TCP stream |
//! | `local://path` | Unix socket / Windows named pipe |
//! | `ws://host:port/path` | WebSocket |
//!
//! # Serving
//!
//! [`serve()`] accepts connections in a loop:
//!
//! ```no_run
//! # #[vox::service]
//! # trait Hello {
//! #     async fn say_hello(&self) -> String;
//! # }
//! # #[derive(Clone)]
//! # struct HelloService;
//! # impl Hello for HelloService {
//! #     async fn say_hello(&self) -> String { "hi".into() }
//! # }
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! vox::serve("0.0.0.0:9000", HelloDispatcher::new(HelloService)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! For multi-service routing, use [`lane_acceptor_fn()`]:
//!
//! ```ignore
//! vox::serve("0.0.0.0:9000", vox::lane_acceptor_fn(|req, conn| {
//!     match req.service() {
//!         "Hello" => { conn.handle_with(HelloDispatcher::new(HelloService)); Ok(()) }
//!         "Chat" => { conn.handle_with(ChatDispatcher::new(ChatService)); Ok(()) }
//!         _ => Err(vec![]),
//!     }
//! })).await?;
//! ```
//!
//! # Generated clients
//!
//! Generated clients expose public fields for connection lifecycle:
//!
//! ```ignore
//! client.caller.closed().await;     // wait for disconnect
//! client.caller.is_connected();     // check liveness
//! client.connection.as_ref();       // access connection handle (service lanes)
//! client.say_hello().await?;        // service method — no name clash
//! ```
//!
//! # Lower-level APIs
//!
//! For advanced use (custom transports, in-memory testing, multiple service lanes),
//! use the builder APIs directly:
//!
//! - [`initiator_on()`] / [`acceptor_on()`] — establish over a raw [`Link`]
//! - [`memory_link_pair()`] — in-process link pair for testing
//! - [`Driver`] — run inbound RPC on a service lane handle
//! - [`ConnectionHandle`] — open/close service lanes
//! - [`proxy_lanes()`] — bridge two service lane handles

#[cfg(feature = "runtime")]
mod highlevel;
#[cfg(feature = "runtime")]
pub use highlevel::*;

mod client_logging;
mod observer;
pub mod schema_deser;
mod server_logging;

// Re-export the proc macro
pub use client_logging::{ClientLogging, ClientLoggingOptions};
pub use observer::TracingObserver;
pub use server_logging::{ServerLogging, ServerLoggingOptions};
pub use vox_service_macros::service;

// Re-export facet (generated code uses `vox::facet::Facet`)
pub use facet;
pub use facet_reflect;
pub use facet_reflect::Peek;

// Re-export method identity functions (generated code uses `vox::hash::method_descriptor`)
// TODO: generated code should be updated to use vox::method_descriptor directly
pub mod hash {
    pub use vox_types::{
        MethodDescriptorOptions, method_descriptor, method_id_name_only, shape_contains_channel,
    };
}

// Re-export vox-types items used by generated code
pub use vox_types::{
    Backing,
    BoxMiddlewareFuture,
    // Traits
    Call,
    ChannelCloseReason,
    ChannelDebugContext,
    ChannelDebugSnapshot,
    ChannelEvent,
    ChannelEventContext,
    // Descriptors
    ChannelId,
    ChannelReceiverState,
    ChannelResetReason,
    ChannelSendOutcome,
    ChannelTrySendOutcome,
    ClientCallOutcome,
    ClientContext,
    ClientMiddleware,
    ClientRequest,
    Conduit,
    ConduitAcceptor,
    ConduitRx,
    ConduitTx,
    ConnectionCloseReason,
    ConnectionRole,
    ConnectionSettings,
    Decline,
    DecodeErrorKind,
    DriverEvent,
    DriverTaskStatus,
    EncodeErrorKind,
    EstablishmentContext,
    EstablishmentEvent,
    EstablishmentOutcome,
    EstablishmentPhase,
    EstablishmentRejectReason,
    Extensions,
    Handler,
    HandshakeResult,
    IdentityBasis,
    IdentityBasisProvenance,
    IdentityEpoch,
    IdentityResolutionContext,
    LaneDebugSnapshot,
    LaneDebugState,
    LaneGrant,
    // Types
    LaneId,
    Link,
    LinkRx,
    LinkTx,
    MaybeSend,
    MaybeSync,
    MessageFamily,
    Metadata,
    MetadataBuilder,
    MetadataExt,
    MethodDescriptor,
    MethodId,
    MsgFamily,
    ObserverMetricKind,
    ObserverMetricLabels,
    Parity,
    Payload,
    PeerEvidence,
    PeerEvidenceItem,
    PeerIdentity,
    PeerIdentityForm,
    ProtocolErrorKind,
    ReplySink,
    RequestAuthorizationContext,
    RequestCall,
    RequestContext,
    RequestDebugSnapshot,
    RequestDebugState,
    RequestResponse,
    ResponseParts,
    RpcEvent,
    RpcOutcome,
    RpcSide,
    Rx,
    RxError,
    SchemaRecvTracker,
    SelfRef,
    ServerCallOutcome,
    ServerMiddleware,
    ServerRequest,
    ServerResponse,
    ServerResponseContext,
    ServerResponsePayload,
    ServiceDescriptor,
    SinkCall,
    SourceLocation,
    TransportEvent,
    TrySendError,
    // Channels
    Tx,
    TxError,
    VoxDebugSnapshot,
    VoxError,
    VoxObserver,
    VoxObserverHandle,
    WithTracker,
    // Channels
    channel,
    // Metadata helpers
    metadata_get_str,
    metadata_get_u64,
    metadata_key_is_no_propagate,
    metadata_key_is_redacted,
    observe_reply,
};

// File-descriptor passing. `FrameFds`/`collect_fds`/`provide_fds` are
// portable (no-ops off-Unix) so generated client code needs no `cfg`;
// `Fd` itself is Unix-only.
#[cfg(unix)]
pub use vox_types::{Fd, FdAdapter, SCM_MAX_FD};
pub use vox_types::{FrameFds, collect_fds, frame_fds_len, provide_fds};
pub use vox_types::{meta_set, metadata};

// ── vox-core: curated public API ──────────────────────────────────────

// Connection establishment — builder entry points
#[cfg(feature = "runtime")]
pub use vox_core::{
    acceptor_conduit, acceptor_on, acceptor_transport, initiator, initiator_conduit, initiator_on,
    initiator_transport,
};

// Convenience helpers that do handshake + builder in one call
#[cfg(feature = "runtime")]
pub use vox_core::{acceptor_on_link, initiator_on_link};

// Connection types
#[cfg(feature = "runtime")]
pub use vox_core::{
    AnonymousIdentityResolver, Connection, ConnectionConfig, ConnectionError, ConnectionHandle,
    ConnectionKeepaliveConfig, IdentityResolver, IdentityResolverFn, LaneHandle, LaneRejectReason,
    LaneRejection, LaneRequest, LaneState, PendingLane, VOX_LANE_REJECT_MESSAGE_METADATA_KEY,
    VOX_LANE_REJECT_REASON_METADATA_KEY, identity_resolver_fn,
};

// Connection acceptor
#[cfg(feature = "runtime")]
pub use vox_core::{LaneAcceptor, LaneAcceptorFn, lane_acceptor_fn, proxy_lanes};

// Connection builders (for advanced customization)
#[cfg(feature = "runtime")]
pub use vox_core::{
    BoxConnectionFuture, ConnectionAcceptorBuilder, ConnectionInitiatorBuilder,
    ConnectionSourceInitiatorBuilder, ConnectionTransportAcceptorBuilder,
    ConnectionTransportInitiatorBuilder, VOX_SERVICE_METADATA_KEY,
};

// Driver — runs inbound RPC on a connection handle
#[cfg(feature = "runtime")]
pub use vox_core::{
    Caller, Driver, DriverCaller, DriverChannelSink, DriverReplySink, ErasedHandler, FromVoxLane,
    RequestTimeoutPolicy,
};

// Conduit types
#[cfg(feature = "runtime")]
pub use vox_core::{BareConduit, BareConduitError, IntoConduit, MessagePlan};

// Link source / attachment plumbing (not available on wasm)
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub use vox_core::{
    Attachment, LinkSource, SingleAttachmentSource, single_attachment_source, single_link_source,
};

// In-memory links for testing (not available on wasm)
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub use vox_core::{MemoryLink, MemoryLinkRx, MemoryLinkRxError, MemoryLinkTx, memory_link_pair};

// Handshake (low-level)
#[cfg(feature = "runtime")]
pub use vox_core::{HandshakeError, handshake_as_acceptor, handshake_as_initiator};

// Transport prologue (low-level)
#[cfg(feature = "runtime")]
pub use vox_core::{accept_transport, initiate_transport};

// Dynamic conduit traits (object-safe)
#[cfg(feature = "runtime")]
pub use vox_core::{DynConduitRx, DynConduitTx};

/// Transport implementations re-exported by the facade crate.
///
/// Enable with cargo features:
/// - `transport-tcp`
/// - `transport-local`
/// - `transport-websocket`
pub mod transport {
    /// TCP byte-stream transport (`vox-stream`).
    #[cfg(all(feature = "transport-tcp", not(target_arch = "wasm32")))]
    pub mod tcp {
        pub use vox_stream::{StreamLink, TcpLinkSource, tcp_link_source};
    }

    /// Local IPC transport (`vox-stream`): Unix sockets / Windows named pipes.
    #[cfg(all(feature = "transport-local", not(target_arch = "wasm32")))]
    pub mod local {
        pub use vox_stream::{
            LocalLink, LocalLinkAcceptor, LocalLinkSource, LocalListener, LocalServerStream,
            LocalStream, connect, endpoint_exists, local_link_source, path_to_pipe_name,
            remove_endpoint,
        };

        /// Descriptor-passing Unix-domain link (`SCM_RIGHTS`). The only
        /// transport over which [`Fd`](crate::Fd) values may travel.
        #[cfg(unix)]
        pub use vox_stream::{FdStreamLink, FdStreamLinkRx, FdStreamLinkTx};
    }

    /// WebSocket transport (`vox-websocket`).
    ///
    /// On native targets this is backed by tokio/tungstenite; on wasm targets
    /// it is backed by `web_sys::WebSocket`.
    #[cfg(feature = "transport-websocket")]
    pub mod websocket {
        pub use vox_websocket::*;
    }
}

// Channel binding via thread-local binder during deserialization, and the
// out-of-band channel-id table (RequestCall.channels) installed around args decode.
pub use vox_types::channel::{
    collect_channels, collect_channels_for_method, provide_channels, provide_channels_for_method,
    set_channel_binder, with_channel_binder,
};

// Re-export descriptor types used by generated clients and dispatchers.
pub mod connection {
    pub use vox_types::{MethodDescriptor, ServiceDescriptor};
}
