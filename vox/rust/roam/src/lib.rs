//! Rapace - High-performance RPC framework
//!
//! This crate provides a unified API for the Rapace RPC protocol.
//! Users should depend on this crate rather than the individual component crates.

#![deny(unsafe_code)]

// Macro hygiene: Allow `::rapace::` paths to work both externally and internally.
// When used in tests within this workspace, `::rapace::` would normally
// fail because it would look for a `rapace` module within `rapace`. This
// self-referential module makes `::rapace::session::...` etc. work everywhere.
#[doc(hidden)]
pub mod rapace {
    pub use crate::*;
}

// Re-export the service macro
pub use rapace_service_macros::service;

// Re-export session types for macro-generated code
pub use rapace_session as session;

// Re-export schema types
pub use rapace_schema as schema;

// Re-export hash utilities
pub use rapace_hash as hash;

// Re-export reflection utilities
pub use rapace_reflect as reflect;

// Re-export facet for derive macros in service types
pub use facet;

/// Private module for proc-macro re-exports. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use facet_postcard;
}

/// Prelude module for convenient imports.
///
/// ```ignore
/// use rapace::prelude::*;
/// ```
pub mod prelude {
    pub use crate::service;
    pub use facet::Facet;
}
