//! HTTP types and service trait for rapace RPC.
//!
//! This crate provides transport-agnostic HTTP types that can be serialized
//! via facet and sent over any rapace transport. It defines:
//!
//! - [`HttpRequest`]: An HTTP request with method, path, headers, and body
//! - [`HttpResponse`]: An HTTP response with status, headers, and body
//! - [`HttpService`]: A trait for handling HTTP requests
//!
//! # Architecture
//!
//! This crate is intentionally minimal and has no HTTP framework dependencies.
//! The types are designed to be easily converted to/from framework-specific types
//! (axum, hyper, etc.) on either side of the RPC boundary.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                              HOST PROCESS                               │
//! │  ┌─────────────┐       ┌──────────────────┐       ┌─────────────────┐  │
//! │  │ HTTP Server │──────►│ HttpRequest      │──────►│ HttpServiceClient│  │
//! │  │  (hyper)    │       │ (rapace types)   │       │  (RPC call)     │  │
//! │  └─────────────┘       └──────────────────┘       └────────┬────────┘  │
//! │                                                             │           │
//! └─────────────────────────────────────────────────────────────┼───────────┘
//!                                   rapace transport            │
//! ┌─────────────────────────────────────────────────────────────┼───────────┐
//! │                                                             ▼           │
//! │                             PLUGIN PROCESS                              │
//! │  ┌─────────────────┐       ┌──────────────────┐       ┌─────────────┐  │
//! │  │ HttpServiceServer│◄──────│ HttpRequest      │◄──────│ axum Router │  │
//! │  │  (dispatcher)   │       │ (rapace types)   │       │             │  │
//! │  └─────────────────┘       └──────────────────┘       └─────────────┘  │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use rapace_http::{HttpRequest, HttpResponse, HttpService};
//!
//! struct MyHttpHandler;
//!
//! impl HttpService for MyHttpHandler {
//!     async fn handle(&self, req: HttpRequest) -> HttpResponse {
//!         HttpResponse {
//!             status: 200,
//!             headers: vec![("content-type".to_string(), "text/plain".to_string())],
//!             body: b"Hello, World!".to_vec(),
//!         }
//!     }
//! }
//! ```

use std::sync::Arc;

use rapace_core::{RpcError, Transport};
use rapace_testkit::RpcSession;

// Required by the macro
#[allow(unused)]
use rapace_registry;

/// An HTTP request in rapace-native format.
///
/// This type is transport-agnostic and can be serialized via facet.
/// Convert to/from framework-specific types (axum, hyper) at the boundaries.
#[derive(Debug, Clone, facet::Facet)]
pub struct HttpRequest {
    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: String,
    /// Request path (e.g., "/api/users/123")
    pub path: String,
    /// Query string, if present (without the leading '?')
    pub query: Option<String>,
    /// HTTP headers as key-value pairs
    pub headers: Vec<(String, String)>,
    /// Request body (no streaming in v1)
    pub body: Vec<u8>,
}

/// An HTTP response in rapace-native format.
///
/// This type is transport-agnostic and can be serialized via facet.
/// Convert to/from framework-specific types (axum, hyper) at the boundaries.
#[derive(Debug, Clone, facet::Facet)]
pub struct HttpResponse {
    /// HTTP status code (e.g., 200, 404, 500)
    pub status: u16,
    /// HTTP headers as key-value pairs
    pub headers: Vec<(String, String)>,
    /// Response body (no streaming in v1)
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Create a new GET request.
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: "GET".to_string(),
            path: path.into(),
            query: None,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Create a new POST request.
    pub fn post(path: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            method: "POST".to_string(),
            path: path.into(),
            query: None,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// Add a query string to the request.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Add a header to the request.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    /// Set the request body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

impl HttpResponse {
    /// Create a 200 OK response with a text body.
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: body.into(),
        }
    }

    /// Create a 200 OK response with a JSON body.
    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: body.into(),
        }
    }

    /// Create a 404 Not Found response.
    pub fn not_found() -> Self {
        Self {
            status: 404,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: b"Not Found".to_vec(),
        }
    }

    /// Create a 500 Internal Server Error response.
    pub fn internal_error(message: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 500,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: message.into(),
        }
    }

    /// Add a header to the response.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    /// Set the status code.
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

/// Service trait for handling HTTP requests over rapace RPC.
///
/// Implement this trait in the plugin to handle HTTP requests.
/// The host sends requests via `HttpServiceClient`.
///
/// # Note on Send bounds
///
/// Implementations must ensure the `handle` future is `Send` for use
/// with the RpcSession dispatcher pattern.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait HttpService {
    /// Handle an HTTP request and return a response.
    ///
    /// This is the main entry point for HTTP request handling.
    /// The implementation can use any HTTP framework internally
    /// (e.g., axum Router) to process the request.
    async fn handle(&self, req: crate::HttpRequest) -> crate::HttpResponse;
}


/// Helper for calling HttpService from host side via RpcSession.
///
/// This wraps the low-level RPC calls with proper encoding/decoding.
pub struct HttpServiceRpcClient<T: Transport + Send + Sync + 'static> {
    session: Arc<RpcSession<T>>,
}

impl<T: Transport + Send + Sync + 'static> HttpServiceRpcClient<T> {
    /// Create a new client wrapping an RpcSession.
    pub fn new(session: Arc<RpcSession<T>>) -> Self {
        Self { session }
    }

    /// Send an HTTP request and receive the response.
    pub async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, RpcError> {
        let channel_id = self.session.next_channel_id();
        let payload = facet_postcard::to_vec(&req).map_err(|e| RpcError::Status {
            code: rapace_core::ErrorCode::Internal,
            message: format!("encode error: {:?}", e),
        })?;

        // method_id 1 = handle (HttpService's first method)
        let response = self.session.call(channel_id, 1, payload).await?;

        // Check for error
        if response.flags.contains(rapace_core::FrameFlags::ERROR) {
            return Err(rapace_testkit::parse_error_payload(&response.payload));
        }

        // Decode response
        let result: HttpResponse =
            facet_postcard::from_bytes(&response.payload).map_err(|e| RpcError::Status {
                code: rapace_core::ErrorCode::Internal,
                message: format!("decode error: {:?}", e),
            })?;

        Ok(result)
    }
}
