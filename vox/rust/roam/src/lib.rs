//! Roam — Rust-native RPC where traits are the schema.
//!
//! This is the facade crate. It re-exports everything needed by both
//! hand-written code and `#[roam::service]` macro-generated code.

mod client_logging;
pub mod schema_deser;
mod server_logging;

// Re-export the proc macro
pub use client_logging::{ClientLogging, ClientLoggingOptions};
pub use roam_service_macros::service;
pub use server_logging::{ServerLogging, ServerLoggingOptions};

// Re-export facet (generated code uses `roam::facet::Facet`)
pub use facet;

// Re-export roam-postcard as facet_postcard for backwards compatibility with generated code.
// Generated code uses `roam::facet_postcard::from_slice_borrowed`.
pub use roam_postcard as facet_postcard;

// Re-export method identity functions (generated code uses `roam::hash::method_descriptor`)
// TODO: generated code should be updated to use roam::method_descriptor directly
pub mod hash {
    pub use roam_types::{
        method_descriptor, method_descriptor_with_retry, method_id_name_only,
        shape_contains_channel,
    };
}

// Re-export roam-types items used by generated code
pub use roam_types::{
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
    RoamError,
    Rx,
    RxError,
    SchemaRecvTracker,
    SelfRef,
    ServerCallOutcome,
    ServerMiddleware,
    ServiceDescriptor,
    SessionRole,
    SinkCall,
    TransportMode,
    // Channels
    Tx,
    TxError,
    WithTracker,
    WriteSlot,
    // Channels
    channel,
    ensure_channel_retry_mode,
    observe_reply,
};

// Re-export runtime/session primitives from `roam-core`.
// This keeps user-facing setup to `roam` + a transport crate.
#[cfg(feature = "runtime")]
pub use roam_core::*;

// Channel binding via thread-local binder during deserialization
pub use roam_types::channel::with_channel_binder;

// Re-export the session module (generated code uses `roam::session::ServiceDescriptor`)
pub mod session {
    pub use roam_types::{MethodDescriptor, RetryPolicy, ServiceDescriptor};
}
