use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use camino::Utf8Path;
use color_eyre::Result;
use futures_util::{SinkExt, StreamExt};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use tower_http::services::ServeDir;

/// Shared state for live reload
#[derive(Clone)]
pub struct LiveReloadState {
    /// Broadcast channel for reload notifications
    pub reload_tx: broadcast::Sender<()>,
}

impl LiveReloadState {
    pub fn new() -> Self {
        let (reload_tx, _) = broadcast::channel(16);
        Self { reload_tx }
    }

    /// Notify all connected browsers to reload
    pub fn trigger_reload(&self) {
        // Ignore errors (no receivers is fine)
        let _ = self.reload_tx.send(());
    }
}

/// Start the HTTP server on a single address (build should be done before calling this)
pub async fn run(output_dir: &Utf8Path, address: &str, port: u16) -> Result<()> {
    let app = Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()));

    let addr = format!("{}:{}", address, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// WebSocket handler for live reload
async fn livereload_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<LiveReloadState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_livereload_socket(socket, state))
}

async fn handle_livereload_socket(socket: WebSocket, state: Arc<LiveReloadState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut reload_rx = state.reload_tx.subscribe();

    // Send initial connection confirmation
    let _ = sender.send(Message::Text("connected".into())).await;

    loop {
        tokio::select! {
            // Wait for reload signal
            result = reload_rx.recv() => {
                match result {
                    Ok(()) => {
                        if sender.send(Message::Text("reload".into())).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(_) => break, // Channel closed
                }
            }
            // Handle incoming messages (for ping/pong or close)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {} // Ignore other messages
                }
            }
        }
    }
}

/// Start HTTP servers on multiple specific IP addresses
/// Each IP gets its own listener - we do NOT bind to 0.0.0.0
pub async fn run_on_ips(
    output_dir: &Utf8Path,
    ips: &[Ipv4Addr],
    port: u16,
    mut shutdown_rx: watch::Receiver<bool>,
    livereload: Option<Arc<LiveReloadState>>,
) -> Result<()> {
    use tokio::task::JoinSet;

    // Build the router with optional live reload
    let app = if let Some(state) = livereload {
        Router::new()
            .route("/__livereload", get(livereload_handler))
            .with_state(state)
            .fallback_service(ServeDir::new(output_dir.as_std_path()))
    } else {
        Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()))
    };

    let mut join_set = JoinSet::new();

    // Create a listener for each specific IP
    for ip in ips {
        let addr = format!("{}:{}", ip, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let app_clone = app.clone();
        let mut shutdown_rx_clone = shutdown_rx.clone();

        join_set.spawn(async move {
            let shutdown_future = async move {
                while !*shutdown_rx_clone.borrow() {
                    if shutdown_rx_clone.changed().await.is_err() {
                        break;
                    }
                }
            };

            axum::serve(listener, app_clone)
                .with_graceful_shutdown(shutdown_future)
                .await
        });
    }

    // Wait for shutdown signal, then wait for all servers to stop
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            result = join_set.join_next() => {
                if let Some(res) = result {
                    // A server task finished (probably error)
                    if let Err(e) = res {
                        eprintln!("Server task error: {}", e);
                    }
                } else {
                    // All tasks done
                    break;
                }
            }
        }
    }

    // Wait for remaining tasks to finish gracefully
    while let Some(_) = join_set.join_next().await {}

    Ok(())
}
