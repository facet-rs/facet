//! Host side: TCP acceptor + tunnel client.
//!
//! The host:
//! 1. Listens on a public port for browser connections
//! 2. For each connection, opens a tunnel via TcpTunnelClient
//! 3. Bridges the browser TCP connection with the rapace tunnel

use std::sync::Arc;

use rapace::{RpcSession, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::metrics::{GlobalTunnelMetrics, TunnelMetrics};
use crate::protocol::TcpTunnelClient;

/// Default buffer size for reads (4KB chunks).
pub const CHUNK_SIZE: usize = 4096;

/// Host-side tunnel handler.
///
/// Manages the connection between browser and plugin through rapace tunnels.
pub struct TunnelHost<T: Transport + Send + Sync + 'static> {
    client: TcpTunnelClient<T>,
    metrics: Arc<GlobalTunnelMetrics>,
}

impl<T: Transport + Send + Sync + 'static> TunnelHost<T> {
    pub fn new(session: Arc<RpcSession<T>>) -> Self {
        let client = TcpTunnelClient::new(session);
        Self {
            client,
            metrics: Arc::new(GlobalTunnelMetrics::new()),
        }
    }

    pub fn with_metrics(session: Arc<RpcSession<T>>, metrics: Arc<GlobalTunnelMetrics>) -> Self {
        let client = TcpTunnelClient::new(session);
        Self { client, metrics }
    }

    pub fn metrics(&self) -> &GlobalTunnelMetrics {
        &self.metrics
    }

    /// Handle a single browser connection.
    ///
    /// Opens a tunnel to the plugin and bridges the browser TCP stream with it.
    pub async fn handle_connection(&self, browser_stream: TcpStream) -> Result<(), String> {
        // Open a tunnel to the plugin
        let handle = self
            .client
            .open()
            .await
            .map_err(|e| format!("failed to open tunnel: {:?}", e))?;

        let channel_id = handle.channel_id;
        tracing::info!(channel_id, "tunnel opened for browser connection");
        self.metrics.tunnel_opened();

        // Register the tunnel to receive incoming chunks from plugin
        let session = self.client.session().clone();
        let mut tunnel_rx = session.register_tunnel(channel_id);

        let (mut browser_read, mut browser_write) = browser_stream.into_split();
        let metrics = self.metrics.clone();
        let tunnel_metrics = Arc::new(TunnelMetrics::new());

        // Task A: Browser → rapace (read from browser, send to tunnel)
        let tunnel_metrics_a = tunnel_metrics.clone();
        let session_a = session.clone();
        let metrics_a = metrics.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; CHUNK_SIZE];
            loop {
                match browser_read.read(&mut buf).await {
                    Ok(0) => {
                        // Browser closed connection
                        tracing::debug!(channel_id, "browser closed connection");
                        let _ = session_a.close_tunnel(channel_id).await;
                        break;
                    }
                    Ok(n) => {
                        tunnel_metrics_a.record_send(n);
                        if let Err(e) = session_a.send_chunk(channel_id, buf[..n].to_vec()).await {
                            tracing::debug!(channel_id, error = %e, "tunnel send error");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(channel_id, error = %e, "browser read error");
                        let _ = session_a.close_tunnel(channel_id).await;
                        break;
                    }
                }
            }
            // Record final metrics
            metrics_a.add_bytes(tunnel_metrics_a.bytes_sent() + tunnel_metrics_a.bytes_received());
            metrics_a.tunnel_closed();
            tracing::debug!(
                channel_id,
                bytes_sent = tunnel_metrics_a.bytes_sent(),
                bytes_received = tunnel_metrics_a.bytes_received(),
                "browser→rapace task finished"
            );
        });

        // Task B: rapace → Browser (read from tunnel, write to browser)
        let tunnel_metrics_b = tunnel_metrics.clone();
        tokio::spawn(async move {
            while let Some(chunk) = tunnel_rx.recv().await {
                if !chunk.payload.is_empty() {
                    tunnel_metrics_b.record_recv(chunk.payload.len());
                    if let Err(e) = browser_write.write_all(&chunk.payload).await {
                        tracing::debug!(channel_id, error = %e, "browser write error");
                        break;
                    }
                }
                if chunk.is_eos {
                    tracing::debug!(channel_id, "received EOS from plugin");
                    // Half-close the browser write side
                    let _ = browser_write.shutdown().await;
                    break;
                }
            }
            tracing::debug!(channel_id, "rapace→browser task finished");
        });

        Ok(())
    }
}

/// Run the host server that accepts browser connections and tunnels them to the plugin.
pub async fn run_host_server<T: Transport + Send + Sync + 'static>(
    host: Arc<TunnelHost<T>>,
    listen_port: u16,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port)).await?;
    tracing::info!(
        port = listen_port,
        "host server listening for browser connections"
    );

    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::debug!(?addr, "accepted browser connection");

        let host = host.clone();
        tokio::spawn(async move {
            if let Err(e) = host.handle_connection(stream).await {
                tracing::error!(error = %e, "failed to handle connection");
            }
        });
    }
}
