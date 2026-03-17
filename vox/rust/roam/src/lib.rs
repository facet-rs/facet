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
    pub use roam_types::{method_descriptor, method_descriptor_with_retry, method_id_name_only};
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
    RpcPlan,
    Rx,
    RxError,
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
    WriteSlot,
    // Channels
    channel,
    observe_reply,
};

// Re-export runtime/session primitives from `roam-core`.
// This keeps user-facing setup to `roam` + a transport crate.
#[cfg(feature = "runtime")]
pub use roam_core::*;

#[cfg(feature = "runtime")]
pub use roam_core::{InMemoryOperationStore, OperationAdmit, OperationCancel, OperationStore};

// Channel binding is only available on non-wasm32 targets
#[cfg(not(target_arch = "wasm32"))]
pub use roam_types::{bind_channels_callee_args, bind_channels_caller_args};

// Re-export the session module (generated code uses `roam::session::ServiceDescriptor`)
pub mod session {
    pub use roam_types::{MethodDescriptor, RetryPolicy, ServiceDescriptor};
}
