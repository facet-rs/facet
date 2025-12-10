#![allow(clippy::type_complexity)]
//! Plugin side: HTTP server + TcpTunnel implementation.
//!
//! The plugin:
//! 1. Starts a local axum HTTP server on an internal port
//! 2. Implements TcpTunnel service
//! 3. For each tunnel open():
//!    - Connects to the local HTTP server via TCP
//!    - Bridges the rapace tunnel with the TCP connection

use std::pin::Pin;
use std::sync::Arc;

use rapace::{Frame, RpcError, RpcSession, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::metrics::{GlobalTunnelMetrics, TunnelMetrics};
use crate::protocol::{TcpTunnel, TcpTunnelServer, TunnelHandle};

/// Internal HTTP server port (plugin listens here).
pub const INTERNAL_HTTP_PORT: u16 = 9876;

/// Default buffer size for reads (4KB chunks).
pub const CHUNK_SIZE: usize = 4096;

/// Plugin-side implementation of TcpTunnel.
///
/// Each `open()` call:
/// 1. Allocates a new channel_id
/// 2. Connects to the local HTTP server
/// 3. Spawns tasks to bridge rapace ↔ TCP
pub struct TcpTunnelImpl<T: Transport> {
    session: Arc<RpcSession<T>>,
    internal_port: u16,
    metrics: Arc<GlobalTunnelMetrics>,
}

impl<T: Transport + Send + Sync + 'static> TcpTunnelImpl<T> {
    pub fn new(session: Arc<RpcSession<T>>, internal_port: u16) -> Self {
        Self {
            session,
            internal_port,
            metrics: Arc::new(GlobalTunnelMetrics::new()),
        }
    }

    pub fn with_metrics(
        session: Arc<RpcSession<T>>,
        internal_port: u16,
        metrics: Arc<GlobalTunnelMetrics>,
    ) -> Self {
        Self {
            session,
            internal_port,
            metrics,
        }
    }

    pub fn metrics(&self) -> &GlobalTunnelMetrics {
        &self.metrics
    }
}

impl<T: Transport + Send + Sync + 'static> TcpTunnel for TcpTunnelImpl<T> {
    async fn open(&self) -> TunnelHandle {
        // Allocate a channel for this tunnel
        let channel_id = self.session.next_channel_id();

        tracing::info!(channel_id, "tunnel open requested");
        self.metrics.tunnel_opened();

        // Register the tunnel to receive incoming chunks
        let mut tunnel_rx = self.session.register_tunnel(channel_id);

        // Connect to the internal HTTP server
        let addr = format!("127.0.0.1:{}", self.internal_port);
        let tcp_stream = match TcpStream::connect(&addr).await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!(channel_id, error = %e, "failed to connect to internal HTTP server");
                self.metrics.tunnel_errored();
                // Return the handle anyway - the tunnel tasks will fail gracefully
                return TunnelHandle { channel_id };
            }
        };

        let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();
        let session = self.session.clone();
        let metrics = self.metrics.clone();
        let tunnel_metrics = Arc::new(TunnelMetrics::new());

        // Task A: rapace → TCP (read from tunnel, write to TCP socket)
        let tunnel_metrics_a = tunnel_metrics.clone();
        tokio::spawn(async move {
            while let Some(chunk) = tunnel_rx.recv().await {
                if !chunk.payload.is_empty() {
                    tunnel_metrics_a.record_recv(chunk.payload.len());
                    if let Err(e) = tcp_write.write_all(&chunk.payload).await {
                        tracing::debug!(channel_id, error = %e, "TCP write error");
                        break;
                    }
                }
                if chunk.is_eos {
                    tracing::debug!(channel_id, "received EOS from host");
                    // Half-close the TCP write side
                    let _ = tcp_write.shutdown().await;
                    break;
                }
            }
            tracing::debug!(channel_id, "rapace→TCP task finished");
        });

        // Task B: TCP → rapace (read from TCP socket, write to tunnel)
        let tunnel_metrics_b = tunnel_metrics.clone();
        let metrics_b = metrics.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; CHUNK_SIZE];
            loop {
                match tcp_read.read(&mut buf).await {
                    Ok(0) => {
                        // TCP EOF - close the tunnel
                        tracing::debug!(channel_id, "TCP EOF, closing tunnel");
                        let _ = session.close_tunnel(channel_id).await;
                        break;
                    }
                    Ok(n) => {
                        tunnel_metrics_b.record_send(n);
                        if let Err(e) = session.send_chunk(channel_id, buf[..n].to_vec()).await {
                            tracing::debug!(channel_id, error = %e, "tunnel send error");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(channel_id, error = %e, "TCP read error");
                        let _ = session.close_tunnel(channel_id).await;
                        break;
                    }
                }
            }
            // Record final metrics
            metrics_b.add_bytes(tunnel_metrics_b.bytes_sent() + tunnel_metrics_b.bytes_received());
            metrics_b.tunnel_closed();
            tracing::debug!(
                channel_id,
                bytes_sent = tunnel_metrics_b.bytes_sent(),
                bytes_received = tunnel_metrics_b.bytes_received(),
                "TCP→rapace task finished"
            );
        });

        TunnelHandle { channel_id }
    }
}

/// Create a dispatcher for TcpTunnelImpl.
///
/// This is used to integrate the tunnel service with RpcSession's dispatcher.
pub fn create_tunnel_dispatcher<T: Transport + Send + Sync + 'static>(
    service: Arc<TcpTunnelImpl<T>>,
) -> impl Fn(
    u32,
    u32,
    Vec<u8>,
) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
       + Send
       + Sync
       + 'static {
    move |_channel_id, method_id, payload| {
        let service = service.clone();
        Box::pin(async move {
            let server = TcpTunnelServer::new(service.as_ref().clone());
            server.dispatch(method_id, &payload).await
        })
    }
}

// Need to implement Clone for TcpTunnelImpl to use with the server
impl<T: Transport + Send + Sync + 'static> Clone for TcpTunnelImpl<T> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            internal_port: self.internal_port,
            metrics: self.metrics.clone(),
        }
    }
}

/// Large response body - ~256KB of repeated text (same as baseline)
fn large_response() -> String {
    let pattern = "The quick brown fox jumps over the lazy dog. ";
    let repeat_count = (256 * 1024) / pattern.len();
    pattern.repeat(repeat_count)
}

/// Create a demo axum router with test routes.
pub fn create_demo_router() -> axum::Router {
    use axum::{routing::get, Router};

    Router::new()
        .route("/hello", get(hello_handler))
        .route("/health", get(health_handler))
        .route("/echo", axum::routing::post(echo_handler))
        // Benchmark routes (same as baseline)
        .route("/small", get(small_handler))
        .route("/large", get(large_handler))
}

async fn hello_handler() -> &'static str {
    "hello from tunnel"
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn echo_handler(body: bytes::Bytes) -> bytes::Bytes {
    body
}

async fn small_handler() -> &'static str {
    "ok"
}

async fn large_handler() -> String {
    large_response()
}

/// Run the internal HTTP server on the specified port.
pub async fn run_http_server(port: u16) -> std::io::Result<()> {
    let app = create_demo_router();
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!(port, "internal HTTP server listening");
    axum::serve(listener, app).await
}
