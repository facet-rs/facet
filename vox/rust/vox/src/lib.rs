//! vox — RPC with channels, virtual connections, and some backwards compatibility.
//!
//! Vox services are Rust traits:
//!
//! ```
//! #[vox::service]
//! trait Hello {
//!   async fn say_hello(&self) -> String;
//! }
//! ```
//!
//! And the basic idea is that you should be able to connect to any number of
//! transports to call those methods:
//!
//! ```no_run
//! use vox::transport::tcp::tcp_connector;
//! use vox::{TransportMode, initiator};
//! # use tokio::net::TcpListener;
//! # use vox::transport::tcp::StreamLink;
//! # use vox::acceptor_on;
//!
//! # #[vox::service]
//! # trait Hello {
//! #     async fn say_hello(&self) -> String;
//! # }
//! #
//! # #[derive(Clone)]
//! # struct HelloService;
//! #
//! # impl Hello for HelloService {
//! #     async fn say_hello(&self) -> String {
//! #         "hello".to_string()
//! #     }
//! # }
//! #
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let addr = "127.0.0.1:50051";
//! # let listener = TcpListener::bind(addr).await?;
//! #
//! # let server = tokio::spawn(async move {
//! #     let (stream, _) = listener.accept().await?;
//! #     let (_server_caller, _server_session) = acceptor_on(StreamLink::tcp(stream))
//! #         .establish::<HelloClient>(HelloDispatcher::new(HelloService))
//! #         .await?;
//! #     Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! # });
//! #
//! let (client, _session) = initiator(tcp_connector(addr), TransportMode::Bare)
//!     .establish::<HelloClient>(())
//!     .await?;
//!
//! let reply = client.say_hello().await?;
//! assert_eq!(reply, "hello");
//! # server.await.expect("server task panicked").expect("server failed");
//! # Ok(())
//! # }
//! ```

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
    Caller,
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
    ErasedCaller,
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
    MiddlewareCaller,
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
    VoxClient,
    VoxError,
    WithTracker,
    WriteSlot,
    // Channels
    channel,
    closed,
    ensure_channel_retry_mode,
    is_connected,
    observe_reply,
};

// Re-export runtime/session primitives from `vox-core`.
// This keeps user-facing setup to `vox` + a transport crate.
#[cfg(feature = "runtime")]
pub use vox_core::*;

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
pub use vox_types::channel::with_channel_binder;

// Re-export the session module (generated code uses `vox::session::ServiceDescriptor`)
pub mod session {
    pub use vox_types::{MethodDescriptor, RetryPolicy, ServiceDescriptor};
}
