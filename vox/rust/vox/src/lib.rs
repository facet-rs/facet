//! vox — type-safe RPC with channels, virtual connections, and automatic schema evolution.
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
//! | `shm:///path/to/control.sock` | Shared memory (Unix) |
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
//! For multi-service routing, use [`acceptor_fn()`]:
//!
//! ```ignore
//! vox::serve("0.0.0.0:9000", vox::acceptor_fn(|req, conn| {
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
//! client.session.as_ref();          // access session handle (virtual connections)
//! client.say_hello().await?;        // service method — no name clash
//! ```
//!
//! # Lower-level APIs
//!
//! For advanced use (custom transports, in-memory testing, virtual connections),
//! use the builder APIs directly:
//!
//! - [`initiator_on()`] / [`acceptor_on()`] — establish over a raw [`Link`]
//! - [`memory_link_pair()`] — in-process link pair for testing
//! - [`Driver`] — run inbound RPC on a connection handle
//! - [`SessionHandle`] — open/close virtual connections
//! - [`proxy_connections()`] — bridge two connection handles

mod highlevel;
pub use highlevel::*;

mod client_logging;
pub mod schema_deser;
mod server_logging;

// Re-export the proc macro
pub use client_logging::{ClientLogging, ClientLoggingOptions};
pub use server_logging::{ServerLogging, ServerLoggingOptions};
pub use vox_service_macros::service;

// Re-export facet (generated code uses `vox::facet::Facet`)
pub use facet;
pub use facet_reflect;
pub use facet_reflect::Peek;

// Re-export vox-postcard for generated code and downstream helpers.
pub use vox_postcard;

// Re-export method identity functions (generated code uses `vox::hash::method_descriptor`)
// TODO: generated code should be updated to use vox::method_descriptor directly
pub mod hash {
    pub use vox_types::{
        method_descriptor, method_descriptor_with_retry, method_id_name_only,
        shape_contains_channel,
    };
}

// Re-export vox-types items used by generated code
pub use vox_types::{
    Backing,
    BoxMiddlewareFuture,
    // Traits
    Call,
    // Descriptors
    ChannelId,
    ChannelRetryMode,
    ClientCallOutcome,
    ClientContext,
    ClientMiddleware,
    ClientRequest,
    Conduit,
    ConduitAcceptor,
    ConduitRx,
    ConduitTx,
    ConduitTxPermit,
    // Types
    ConnectionId,
    ConnectionSettings,
    Extensions,
    Handler,
    HandshakeResult,
    Link,
    LinkRx,
    LinkTx,
    LinkTxPermit,
    MaybeSend,
    MaybeSync,
    MessageFamily,
    Metadata,
    MetadataEntry,
    MetadataFlags,
    MetadataValue,
    MethodDescriptor,
    MethodId,
    MsgFamily,
    OPERATION_ID_METADATA_KEY,
    Parity,
    Payload,
    RETRY_SUPPORT_METADATA_KEY,
    RETRY_SUPPORT_VERSION,
    ReplySink,
    RequestCall,
    RequestContext,
    RequestResponse,
    ResponseParts,
    RetryPolicy,
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
    SessionRole,
    SinkCall,
    TransportMode,
    // Channels
    Tx,
    TxError,
    VoxError,
    WithTracker,
    WriteSlot,
    // Channels
    channel,
    ensure_channel_retry_mode,
    // Metadata helpers
    metadata_get_str,
    metadata_get_u64,
    observe_reply,
};

// ── vox-core: curated public API ──────────────────────────────────────

// Session establishment — builder entry points
#[cfg(feature = "runtime")]
pub use vox_core::{
    acceptor_conduit, acceptor_on, acceptor_transport, initiator, initiator_conduit, initiator_on,
    initiator_transport,
};

// Convenience helpers that do handshake + builder in one call
#[cfg(feature = "runtime")]
pub use vox_core::{acceptor_on_link, initiator_on_link};

// Session types
#[cfg(feature = "runtime")]
pub use vox_core::{
    ConnectionHandle, ConnectionRequest, ConnectionState, PendingConnection, Session,
    SessionAcceptOutcome, SessionConfig, SessionError, SessionHandle, SessionKeepaliveConfig,
    SessionRegistry,
};

// Connection acceptor
#[cfg(feature = "runtime")]
pub use vox_core::{AcceptorFn, ConnectionAcceptor, acceptor_fn, proxy_connections};

// Session builders (for advanced customization)
#[cfg(feature = "runtime")]
pub use vox_core::{
    BoxSessionFuture, SessionAcceptorBuilder, SessionInitiatorBuilder,
    SessionSourceInitiatorBuilder, SessionTransportAcceptorBuilder,
    SessionTransportInitiatorBuilder, VOX_SERVICE_METADATA_KEY,
};

// Driver — runs inbound RPC on a connection handle
#[cfg(feature = "runtime")]
pub use vox_core::{
    Caller, Driver, DriverCaller, DriverChannelSink, DriverReplySink, ErasedHandler,
    FromVoxSession, NoopClient,
};

// Conduit types
#[cfg(feature = "runtime")]
pub use vox_core::{BareConduit, BareConduitError, IntoConduit, MessagePlan};

// Stable conduit + reconnection (not available on wasm)
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub use vox_core::{
    Attachment, LinkSource, SingleAttachmentSource, SplitLink, StableConduit, StableConduitError,
    prepare_acceptor_attachment, recv_client_hello, single_attachment_source, single_link_source,
};

// In-memory links for testing (not available on wasm)
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub use vox_core::{
    MemoryLink, MemoryLinkRx, MemoryLinkRxError, MemoryLinkTx, MemoryLinkTxPermit, memory_link_pair,
};

// Handshake (low-level)
#[cfg(feature = "runtime")]
pub use vox_core::{HandshakeError, handshake_as_acceptor, handshake_as_initiator};

// Transport prologue (low-level)
#[cfg(feature = "runtime")]
pub use vox_core::{accept_transport, initiate_transport};

// Operation store (exactly-once delivery)
#[cfg(feature = "runtime")]
pub use vox_core::{InMemoryOperationStore, OperationState, OperationStore, SealedResponse};

// Dynamic conduit traits (object-safe)
#[cfg(feature = "runtime")]
pub use vox_core::{DynConduitRx, DynConduitTx};

/// Transport implementations re-exported by the facade crate.
///
/// Enable with cargo features:
/// - `transport-tcp`
/// - `transport-local`
/// - `transport-shm`
/// - `transport-websocket`
pub mod transport {
    /// TCP byte-stream transport (`vox-stream`).
    #[cfg(feature = "transport-tcp")]
    pub mod tcp {
        pub use vox_stream::{StreamLink, TcpLinkSource, tcp_link_source};
    }

    /// Local IPC transport (`vox-stream`): Unix sockets / Windows named pipes.
    #[cfg(feature = "transport-local")]
    pub mod local {
        pub use vox_stream::{
            LocalLink, LocalLinkAcceptor, LocalLinkSource, LocalListener, LocalServerStream,
            LocalStream, connect, endpoint_exists, local_link_source, path_to_pipe_name,
            remove_endpoint,
        };
    }

    /// Shared-memory transport (`vox-shm`).
    #[cfg(feature = "transport-shm")]
    pub mod shm {
        pub use vox_shm::*;
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

// Channel binding via thread-local binder during deserialization
pub use vox_types::channel::{set_channel_binder, with_channel_binder};

// Re-export the session module (generated code uses `vox::session::ServiceDescriptor`)
pub mod session {
    pub use vox_types::{MethodDescriptor, RetryPolicy, ServiceDescriptor};
}
