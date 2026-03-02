//! Roam — Rust-native RPC where traits are the schema.
//!
//! This is the facade crate. It re-exports everything needed by both
//! hand-written code and `#[roam::service]` macro-generated code.

// Re-export the proc macro
pub use roam_service_macros::service;

// Re-export facet (generated code uses `roam::facet::Facet`)
pub use facet;

// Re-export facet-postcard (generated code uses `roam::facet_postcard::from_slice_borrowed`)
pub use facet_postcard;

// Re-export roam-hash (generated code uses `roam::hash::method_descriptor`)
pub use roam_hash as hash;

// Re-export roam-types items used by generated code
pub use roam_types::{
    // Traits
    Call,
    Caller,
    // Descriptors
    ChannelId,
    Handler,
    MethodDescriptor,
    MethodId,
    // Types
    Payload,
    ReplySink,
    RequestCall,
    RequestResponse,
    ResponseParts,
    RoamError,
    RpcPlan,
    Rx,
    SelfRef,
    ServiceDescriptor,
    SinkCall,
    // Channels
    Tx,
    // Channels
    channel,
};

// Channel binding is only available on non-wasm32 targets
#[cfg(not(target_arch = "wasm32"))]
pub use roam_types::{bind_channels_callee_args, bind_channels_caller_args};

// Re-export the session module (generated code uses `roam::session::ServiceDescriptor`)
pub mod session {
    pub use roam_types::{MethodDescriptor, ServiceDescriptor};
}
