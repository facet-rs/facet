//! axum HTTP peer server hosting a vox endpoint on a websocket route.
//!
//! Unlike `ws-peer-server` (a bare `tokio-tungstenite` accept loop), this is a
//! real `axum` HTTP application: it serves a plain route at `/` and upgrades
//! `/ws` into a vox `Link` via [`AxumWsLink`], running the shared
//! [`TestbedService`]. Clients connect to `ws://host:port/ws`.

use std::env;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use spec_proto::TestbedDispatcher;
use subject_rust::TestbedService;
use vox_core::acceptor_transport;
use vox_websocket::AxumWsLink;

const PEER_SERVER_RUNTIME_STACK_BYTES: usize = 32 * 1024 * 1024;

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(PEER_SERVER_RUNTIME_STACK_BYTES)
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;
    rt.block_on(run())
}

async fn run() -> Result<(), String> {
    let port = env::var("WS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("127.0.0.1:{port}");

    let app = Router::new()
        .route("/", get(|| async { "vox axum peer server" }))
        .route("/ws", get(ws_handler));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    eprintln!("axum peer server listening on http://{addr} (vox ws at /ws)");

    // Print port on stdout for Playwright to parse.
    println!("{port}");

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("serve: {e}"))
}

/// Upgrade an incoming HTTP request to a websocket and drive a vox connection
/// over it, using the shared [`TestbedService`].
async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| async move {
        let link = AxumWsLink::new(socket);

        let connection = match acceptor_transport(link)
            .on_lane(TestbedDispatcher::new(TestbedService))
            .establish_connection()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Connection establishment failed: {e:?}");
                return;
            }
        };

        eprintln!("Connection established (axum)");
        connection.closed().await;
    })
}
