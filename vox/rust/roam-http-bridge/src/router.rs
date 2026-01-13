//! Axum router for the HTTP bridge.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};

use crate::{BridgeError, BridgeService, metadata, ws};

/// Builder for creating an axum Router that implements the HTTP bridge.
///
/// r[bridge.url.base]
/// The router can be nested at any base URL.
pub struct BridgeRouter {
    services: HashMap<String, Arc<dyn BridgeService>>,
}

impl BridgeRouter {
    /// Create a new bridge router builder.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Register a service with the bridge.
    ///
    /// The service name is extracted from `service.service_detail().name`.
    pub fn service<S: BridgeService>(mut self, service: S) -> Self {
        let name = service.service_detail().name.to_string();
        self.services.insert(name, Arc::new(service));
        self
    }

    /// Build the axum Router.
    ///
    /// r[bridge.url.methods] - Routes `POST /{service}/{method}`
    /// r[bridge.url.websocket] - Routes `GET /@ws`
    /// r[bridge.url.reserved] - Reserves `@`-prefixed paths
    pub fn build(self) -> Router {
        let state = Arc::new(BridgeState {
            services: self.services,
        });

        Router::new()
            // r[bridge.url.methods]
            .route("/{service}/{method}", post(handle_rpc))
            // r[bridge.url.websocket]
            .route("/@ws", get(handle_websocket))
            .with_state(state)
    }
}

impl Default for BridgeRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared state for the bridge router.
struct BridgeState {
    services: HashMap<String, Arc<dyn BridgeService>>,
}

/// Handle an RPC request.
///
/// r[bridge.request.method] - POST only
/// r[bridge.request.content-type] - application/json
/// r[bridge.request.body] - JSON array of arguments
async fn handle_rpc(
    State(state): State<Arc<BridgeState>>,
    Path((service_name, method_name)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Look up the service
    let service = match state.services.get(&service_name) {
        Some(s) => s,
        None => {
            return BridgeError::bad_request(format!("Unknown service: {}", service_name))
                .into_response();
        }
    };

    // Extract metadata from headers
    // r[bridge.request.metadata]
    // r[bridge.request.metadata.wellknown]
    // r[bridge.request.nonce]
    let metadata = match metadata::extract_metadata(&headers) {
        Ok(m) => m,
        Err(e) => return e.into_response(),
    };

    // Call the service
    match service.call_json(&method_name, &body, metadata).await {
        Ok(response) => {
            // r[bridge.response.content-type]
            // r[bridge.response.metadata] - TODO: return response metadata as Roam-* headers
            let json_bytes = response.to_json_bytes();
            (
                StatusCode::OK,
                [("content-type", "application/json")],
                json_bytes,
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        // r[bridge.response.bridge-error]
        let json_bytes = self.to_json_bytes();
        (
            self.status,
            [("content-type", "application/json")],
            json_bytes,
        )
            .into_response()
    }
}

/// Handle a WebSocket upgrade request.
///
/// r[bridge.ws.subprotocol] - Validates `roam-bridge.v1` subprotocol.
/// r[bridge.url.websocket] - Endpoint at `/@ws`.
async fn handle_websocket(ws: WebSocketUpgrade, State(state): State<Arc<BridgeState>>) -> Response {
    // r[bridge.ws.subprotocol]
    // Note: axum's WebSocketUpgrade handles the protocol negotiation for us.
    // The client should include `Sec-WebSocket-Protocol: roam-bridge.v1`.
    ws.protocols([ws::WS_SUBPROTOCOL])
        .on_upgrade(move |socket| ws::handle_websocket(socket, Arc::new(state.services.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_router_builds() {
        let router = BridgeRouter::new().build();
        // Just verify it compiles and builds
        let _ = router;
    }
}
