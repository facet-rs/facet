//! Vox — Rust-native RPC where traits are the schema.
//!
//! This is the facade crate. It re-exports everything needed by both
//! hand-written code and `#[vox::service]` macro-generated code.

mod client_logging;
pub mod schema_deser;
mod server_logging;

// Re-export the proc macro
pub use client_logging::{ClientLogging, ClientLoggingOptions};
pub use vox_service_macros::service;
pub use server_logging::{ServerLogging, ServerLoggingOptions};

// Re-export facet (generated code uses `vox::facet::Facet`)
pub use facet;

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
    VoxError,
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

// Re-export runtime/session primitives from `vox-core`.
// This keeps user-facing setup to `vox` + a transport crate.
#[cfg(feature = "runtime")]
pub use vox_core::*;

// Channel binding via thread-local binder during deserialization
pub use vox_types::channel::with_channel_binder;

// Re-export the session module (generated code uses `vox::session::ServiceDescriptor`)
pub mod session {
    pub use vox_types::{MethodDescriptor, RetryPolicy, ServiceDescriptor};
}
