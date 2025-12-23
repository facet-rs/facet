#![allow(clippy::type_complexity)]
//! HTTP over Rapace - Shared Library
//!
//! This module contains the axum-based HttpService implementation shared between
//! the main demo binary and the cross-process test helper.
//!
//! # Architecture
//!
//! The plugin owns the axum router and all HTTP framework dependencies.
//! The host is lightweight - it just proxies HTTP requests over rapace RPC.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                              HOST PROCESS                               │
//! │                                                                         │
//! │   HTTP Request ──► hyper ──► HttpRequest ──► HttpServiceClient ─────────┤
//! │                              (rapace type)                              │
//! └────────────────────────────────────────────────────────────────────────┬┘
//!                                                                          │
//!                              rapace transport (TCP/Unix/SHM)             │
//!                                                                          │
//! ┌────────────────────────────────────────────────────────────────────────┴┐
//! │                             PLUGIN PROCESS                              │
//! │                                                                         │
//! │   HttpServiceServer ──► HttpRequest ──► axum::Router ──► HttpResponse  │
//! │                         (rapace type)                    (rapace type)  │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use std::pin::Pin;

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{Request, Response, StatusCode},
    routing::get,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use rapace::{Frame, RpcError};
use rapace_http::{HttpRequest, HttpResponse, HttpService, HttpServiceServer};
use tower_service::Service;

// Re-export for convenience
pub use rapace_http::HttpServiceClient;

/// Axum-based implementation of HttpService.
///
/// This wraps an axum Router and handles the conversion between
/// rapace HTTP types and axum/hyper types.
#[derive(Clone)]
pub struct AxumHttpService {
    router: Router,
}

impl AxumHttpService {
    /// Create a new AxumHttpService with the given router.
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    /// Create an AxumHttpService with the default demo routes.
    ///
    /// Routes:
    /// - `GET /health` → 200 OK, "ok"
    /// - `GET /hello/:name` → 200 OK, "Hello, {name}!"
    /// - `GET /json` → 200 OK, JSON response
    /// - `POST /echo` → echoes back the request body
    pub fn with_demo_routes() -> Self {
        let router = Router::new()
            .route("/health", get(health_handler))
            .route("/hello/{name}", get(hello_handler))
            .route("/json", get(json_handler))
            .route("/echo", axum::routing::post(echo_handler));

        Self::new(router)
    }
}

impl HttpService for AxumHttpService {
    async fn handle(&self, req: HttpRequest) -> HttpResponse {
        // Convert HttpRequest -> axum Request
        let axum_request = match convert_to_axum_request(req) {
            Ok(r) => r,
            Err(e) => {
                return HttpResponse::internal_error(format!("Failed to convert request: {}", e));
            }
        };

        // Call the router
        let mut router = self.router.clone();
        let response = match router.call(axum_request).await {
            Ok(r) => r,
            Err(e) => {
                // Infallible error, but still need to handle
                return HttpResponse::internal_error(format!("Router error: {:?}", e));
            }
        };

        // Convert axum Response -> HttpResponse
        match convert_from_axum_response(response).await {
            Ok(r) => r,
            Err(e) => HttpResponse::internal_error(format!("Failed to convert response: {}", e)),
        }
    }
}

/// Convert a rapace HttpRequest to an axum Request.
fn convert_to_axum_request(req: HttpRequest) -> Result<Request<Body>, String> {
    // Build URI
    let uri = if let Some(query) = req.query {
        format!("{}?{}", req.path, query)
    } else {
        req.path
    };

    // Parse method
    let method: http::Method = req
        .method
        .parse()
        .map_err(|e| format!("Invalid method: {}", e))?;

    // Build request
    let mut builder = Request::builder().method(method).uri(&uri);

    // Add headers
    for (key, value) in req.headers {
        builder = builder.header(&key, &value);
    }

    // Build with body
    builder
        .body(Body::from(req.body))
        .map_err(|e| format!("Failed to build request: {}", e))
}

/// Convert an axum Response to a rapace HttpResponse.
async fn convert_from_axum_response(response: Response<Body>) -> Result<HttpResponse, String> {
    let (parts, body) = response.into_parts();

    // Collect headers
    let headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Collect body
    let body_bytes: Bytes = body
        .collect()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?
        .to_bytes();

    Ok(HttpResponse {
        status: parts.status.as_u16(),
        headers,
        body: body_bytes.to_vec(),
    })
}

// ============================================================================
// Demo Route Handlers
// ============================================================================

async fn health_handler() -> &'static str {
    "ok"
}

async fn hello_handler(Path(name): Path<String>) -> String {
    format!("Hello, {}!", name)
}

#[derive(serde::Serialize)]
struct JsonResponse {
    message: String,
    status: String,
    version: u32,
}

async fn json_handler() -> axum::Json<JsonResponse> {
    axum::Json(JsonResponse {
        message: "This is a JSON response".to_string(),
        status: "success".to_string(),
        version: 1,
    })
}

async fn echo_handler(body: Bytes) -> (StatusCode, Bytes) {
    (StatusCode::OK, body)
}

// ============================================================================
// Hyper Conversion Helpers (for host side)
// ============================================================================

/// Convert a hyper Request to a rapace HttpRequest.
///
/// This is used on the host side to convert incoming HTTP requests
/// before sending them to the plugin via RPC.
pub async fn convert_hyper_to_rapace<B>(request: hyper::Request<B>) -> Result<HttpRequest, String>
where
    B: hyper::body::Body,
    B::Error: std::fmt::Display,
{
    let (parts, body) = request.into_parts();

    // Extract method and URI parts
    let method = parts.method.to_string();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().map(|q| q.to_string());

    // Collect headers
    let headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Collect body using http-body-util
    let body_bytes = http_body_util::BodyExt::collect(body)
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?
        .to_bytes();

    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body: body_bytes.to_vec(),
    })
}

/// Convert a rapace HttpResponse to a hyper Response.
///
/// This is used on the host side to convert responses from the plugin
/// back to HTTP responses for the client.
pub fn convert_rapace_to_hyper(
    response: HttpResponse,
) -> Result<hyper::Response<http_body_util::Full<Bytes>>, String> {
    let mut builder = hyper::Response::builder().status(response.status);

    // Add headers
    for (key, value) in response.headers {
        builder = builder.header(&key, &value);
    }

    builder
        .body(http_body_util::Full::new(Bytes::from(response.body)))
        .map_err(|e| format!("Failed to build response: {}", e))
}

// ============================================================================
// Dispatcher for RpcSession
// ============================================================================

/// Create a dispatcher for AxumHttpService.
///
/// This follows the same pattern as template_engine's dispatcher.
pub fn create_http_service_dispatcher(
    service: AxumHttpService,
) -> impl Fn(Frame) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request| {
        let service = service.clone();
        Box::pin(async move {
            let server = HttpServiceServer::new(service);
            server.dispatch(request.desc.method_id, &request).await
        })
    }
}
