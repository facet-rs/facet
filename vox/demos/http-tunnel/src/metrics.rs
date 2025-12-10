//! Tunnel metrics for observability and zero-copy verification.

use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics for a single tunnel connection.
#[derive(Debug, Default)]
pub struct TunnelMetrics {
    /// Bytes sent through the tunnel (browser → plugin direction for host)
    pub bytes_sent: AtomicU64,
    /// Bytes received through the tunnel (plugin → browser direction for host)
    pub bytes_received: AtomicU64,
    /// Number of chunks sent
    pub chunks_sent: AtomicU64,
    /// Number of chunks received
    pub chunks_received: AtomicU64,
}

impl TunnelMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_send(&self, bytes: usize) {
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
        self.chunks_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_recv(&self, bytes: usize) {
        self.bytes_received
            .fetch_add(bytes as u64, Ordering::Relaxed);
        self.chunks_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    pub fn chunks_sent(&self) -> u64 {
        self.chunks_sent.load(Ordering::Relaxed)
    }

    pub fn chunks_received(&self) -> u64 {
        self.chunks_received.load(Ordering::Relaxed)
    }
}

/// Aggregate metrics across all tunnels.
#[derive(Debug, Default)]
pub struct GlobalTunnelMetrics {
    /// Total tunnels opened
    pub tunnels_opened: AtomicU64,
    /// Total tunnels closed normally
    pub tunnels_closed: AtomicU64,
    /// Total tunnels that errored
    pub tunnels_errored: AtomicU64,
    /// Total bytes transferred (both directions combined)
    pub total_bytes: AtomicU64,
}

impl GlobalTunnelMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tunnel_opened(&self) {
        self.tunnels_opened.fetch_add(1, Ordering::Relaxed);
    }

    pub fn tunnel_closed(&self) {
        self.tunnels_closed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn tunnel_errored(&self) {
        self.tunnels_errored.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn summary(&self) -> String {
        format!(
            "tunnels: {} opened, {} closed, {} errored; total bytes: {}",
            self.tunnels_opened.load(Ordering::Relaxed),
            self.tunnels_closed.load(Ordering::Relaxed),
            self.tunnels_errored.load(Ordering::Relaxed),
            self.total_bytes.load(Ordering::Relaxed),
        )
    }
}
