//! HTTP/JSON bridge for roam RPC services.
//!
//! This crate provides an HTTP gateway that exposes roam services over HTTP with JSON encoding.
//! The bridge translates between HTTP/JSON and native roam (postcard/binary), allowing web
//! clients and standard HTTP tooling to interact with roam services.
//!
//! # Architecture
//!
//! ```text
//! Web Client (HTTP/JSON) → HTTP Bridge → roam Connection → Remote roam Service
//! ```
//!
//! The bridge is a **gateway** - it receives HTTP requests and forwards them over a roam
//! connection to a remote service. It handles JSON↔postcard transcoding at runtime using
//! `facet_value::Value` as the interchange format.
//!
//! # Usage
//!
//! ```ignore
//! use roam_http_bridge::BridgeRouter;
//!
//! let router = BridgeRouter::new()
//!     .service(CalculatorBridge::new(connection.clone()))
//!     .build();
//!
//! let app = axum::Router::new().nest("/api/roam", router);
//! ```

#![deny(unsafe_code)]

mod error;
mod metadata;
mod router;
mod service;
mod transcode;
pub(crate) mod ws;

pub use error::{BridgeError, ProtocolErrorKind};
pub use metadata::BridgeMetadata;
pub use router::BridgeRouter;
pub use service::GenericBridgeService;
pub use transcode::{json_args_to_postcard, postcard_to_json_with_shape};

use roam_schema::ServiceDetail;
use std::future::Future;
use std::pin::Pin;

/// Boxed future type for dyn-compatible async methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for services that can be bridged over HTTP.
///
/// This trait combines a roam client connection with service metadata.
/// The bridge handles JSON↔postcard transcoding at runtime using `facet_value::Value`.
pub trait BridgeService: Send + Sync + 'static {
    /// Returns metadata about this service (name, methods, types).
    fn service_detail(&self) -> &'static ServiceDetail;

    /// Call a method with JSON arguments and return a JSON response.
    ///
    /// # Arguments
    /// * `method_name` - The method to call (e.g., "add")
    /// * `json_body` - JSON array of arguments (e.g., `[3, 5]`)
    /// * `metadata` - Request metadata extracted from HTTP headers
    ///
    /// # Returns
    /// JSON-encoded response or error.
    fn call_json<'a>(
        &'a self,
        method_name: &'a str,
        json_body: &'a [u8],
        metadata: BridgeMetadata,
    ) -> BoxFuture<'a, Result<BridgeResponse, BridgeError>>;
}

/// Response from a bridged RPC call.
///
/// r[bridge.response.success]
/// r[bridge.response.user-error]
/// r[bridge.response.protocol-error]
#[derive(Debug)]
pub enum BridgeResponse {
    /// Successful response with JSON-encoded return value.
    /// HTTP 200, body is the JSON value directly.
    Success(Vec<u8>),

    /// Application error (RoamError::User(E)).
    /// HTTP 200, body is `{"error": "user", "value": ...}`.
    UserError(Vec<u8>),

    /// Protocol error (UnknownMethod, InvalidPayload, Cancelled).
    /// HTTP 200, body is `{"error": "unknown_method"}` etc.
    ProtocolError(ProtocolErrorKind),
}

impl BridgeResponse {
    /// Convert this response to JSON bytes suitable for HTTP response body.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        match self {
            BridgeResponse::Success(json) => json.clone(),
            BridgeResponse::UserError(value_json) => {
                // r[bridge.response.user-error]
                let mut out = br#"{"error":"user","value":"#.to_vec();
                out.extend_from_slice(value_json);
                out.push(b'}');
                out
            }
            BridgeResponse::ProtocolError(kind) => {
                // r[bridge.response.protocol-error]
                kind.to_json_bytes()
            }
        }
    }
}
