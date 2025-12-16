//! Cell lifecycle protocol
//!
//! Standard protocol for cells to signal readiness to the host.
//! This is automatically handled by `run_with_session` when `--peer-id` is present.

use facet::Facet;

/// Message sent by cell to indicate it's ready to handle RPC requests
#[derive(Debug, Clone, Facet)]
pub struct ReadyMsg {
    /// Authoritative peer ID assigned by host
    pub peer_id: u16,
    /// Cell name (e.g. "ddc-cell-markdown", "ddc-cell-http")
    pub cell_name: String,
    /// Process ID (optional, for diagnostics)
    pub pid: Option<u32>,
    /// Version (optional, e.g. git SHA or crate version)
    pub version: Option<String>,
    /// Feature flags (optional; can be empty)
    pub features: Vec<String>,
}

/// Acknowledgment sent by host
#[derive(Debug, Clone, Facet)]
pub struct ReadyAck {
    /// Always true unless host rejects
    pub ok: bool,
    /// Host time in unix milliseconds (optional; helps debug clock/timing)
    pub host_time_unix_ms: Option<u64>,
}

/// Cell lifecycle service implemented by the **host**.
///
/// Cells call these methods to signal readiness states.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait CellLifecycle {
    /// Cell calls this after starting its demux loop to signal it's ready for RPC requests
    ///
    /// This proves the cell can receive and respond to RPC calls, establishing RPC-readiness.
    async fn ready(&self, msg: ReadyMsg) -> ReadyAck;
}
