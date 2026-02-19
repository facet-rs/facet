//! Diagnostic state tracking for SIGUSR1 dumps.
//!
//! Tracks in-flight RPC requests, open channels, and recent completed operations
//! to help debug hung connections.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// A callback that appends diagnostic info to a string.
pub type DiagnosticCallback = Box<dyn Fn(&mut String) + Send + Sync>;

/// Global registry of all diagnostic states.
/// Each connection/driver registers its state here.
static DIAGNOSTIC_REGISTRY: LazyLock<Mutex<Vec<Weak<DiagnosticState>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Whether to record extra debug info (checked once at startup).
/// Set ROAM_DEBUG=1 to enable.
static DEBUG_ENABLED: LazyLock<bool> = LazyLock::new(|| std::env::var("ROAM_DEBUG").is_ok());

/// Maximum number of recent completions to keep per connection.
const MAX_RECENT_COMPLETIONS: usize = 16;

fn unix_now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

/// Check if debug recording is enabled.
pub fn debug_enabled() -> bool {
    *DEBUG_ENABLED
}

/// Register a method name for diagnostic display.
pub fn register_method_name(_method_id: u64, _name: &'static str) {}

/// Look up a method name by ID.
pub fn get_method_name(_method_id: u64) -> Option<&'static str> {
    None
}

/// Register a diagnostic state for SIGUSR1 dumps.
pub fn register_diagnostic_state(state: &Arc<DiagnosticState>) {
    if let Ok(mut registry) = DIAGNOSTIC_REGISTRY.lock() {
        // Clean up dead entries while we're here
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(state));
    }
}

/// Dump all diagnostic states to a string.
///
/// Always produces output for every registered connection, even when idle.
pub fn dump_all_diagnostics() -> String {
    let mut output = String::new();

    let states: Vec<Arc<DiagnosticState>> = {
        // Use try_read to avoid deadlocking if called from signal handler
        let Ok(registry) = DIAGNOSTIC_REGISTRY.try_lock() else {
            return "ERROR: Could not acquire diagnostic registry lock (held by another thread)\n"
                .to_string();
        };
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    };

    if states.is_empty() {
        output.push_str("(no roam connections registered)\n");
        return output;
    }

    // Count occurrences of each role name for numbering
    let mut role_counts: HashMap<String, usize> = HashMap::new();
    for state in &states {
        *role_counts.entry(state.name.clone()).or_insert(0) += 1;
    }

    // Number connections that share a role name
    let mut role_indices: HashMap<String, usize> = HashMap::new();
    for state in &states {
        let count = role_counts.get(&state.name).copied().unwrap_or(1);
        if count > 1 {
            let idx = role_indices.entry(state.name.clone()).or_insert(0);
            *idx += 1;
            // Temporarily modify the name for this dump
            let numbered_name = format!("{} {}", state.name, *idx);
            let dump = state.dump_with_name(&numbered_name);
            let _ = write!(output, "{}", dump);
        } else {
            let _ = write!(output, "{}", state.dump());
        }
    }

    output
}

/// Direction of an RPC request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDirection {
    /// We sent the request, waiting for response
    Outgoing,
    /// We received the request, processing it
    Incoming,
}

impl std::fmt::Display for RequestDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestDirection::Outgoing => write!(f, "⬆"),
            RequestDirection::Incoming => write!(f, "⬇"),
        }
    }
}

/// An in-flight RPC request.
#[derive(Debug, Clone)]
pub struct InFlightRequest {
    pub request_id: u64,
    pub method_id: u64,
    pub started: Instant,
    pub started_at_ns: u64,
    pub handled_at: Option<Instant>,
    pub handled_at_ns: Option<u64>,
    pub direction: RequestDirection,
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    /// Structured arguments (captured when diagnostics feature is enabled).
    pub args: Option<HashMap<String, String>>,
    /// Backtrace at call site (captured when diagnostics feature is enabled).
    pub backtrace: Option<String>,
    /// The local task handling this incoming request (server side).
    pub server_task_id: Option<u64>,
    /// Name of the local task handling this incoming request.
    pub server_task_name: Option<String>,
}

/// A recently completed RPC request.
#[derive(Debug, Clone)]
pub struct CompletedRequest {
    pub method_id: u64,
    pub direction: RequestDirection,
    pub duration: std::time::Duration,
    pub completed_at: Instant,
}

/// Direction of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDirection {
    /// We're sending on this channel
    Tx,
    /// We're receiving on this channel
    Rx,
}

/// An open channel.
#[derive(Debug, Clone)]
pub struct OpenChannel {
    pub channel_id: u64,
    pub started: Instant,
    pub direction: ChannelDirection,
    /// The request that opened this channel (if known).
    pub request_id: Option<u64>,
    /// The peeps task that created/opened this channel.
    pub task_id: Option<u64>,
    /// Name of the task that created/opened this channel.
    pub task_name: Option<String>,
    /// Whether this channel has been closed.
    pub closed: bool,
}

/// Diagnostic state for a single connection.
pub struct DiagnosticState {
    /// Human-readable name for this connection (e.g., "client", "server")
    pub name: String,

    /// Peer's self-reported name (from Hello V6 metadata).
    pub(crate) peer_name: Mutex<Option<String>>,

    /// Negotiated max concurrent requests.
    pub(crate) max_concurrent_requests: AtomicU32,

    /// Negotiated initial channel credit.
    pub(crate) initial_credit: AtomicU32,

    /// When this connection was established
    pub(crate) created_at: Instant,

    /// Total requests completed over the lifetime of this connection
    pub(crate) total_completed: AtomicU64,

    /// In-flight requests
    pub(crate) requests: Mutex<HashMap<(u64, u64), InFlightRequest>>,

    /// Recently completed requests (ring buffer, newest last)
    pub(crate) recent_completions: Mutex<VecDeque<CompletedRequest>>,

    /// Open channels
    pub(crate) channels: Mutex<HashMap<u64, OpenChannel>>,

    /// Custom diagnostic callbacks
    pub(crate) custom_diagnostics: Mutex<Vec<DiagnosticCallback>>,

    // ── Transport-level stats ────────────────────────────────
    /// Total frames sent on this connection.
    pub(crate) frames_sent: AtomicU64,

    /// Total frames received on this connection.
    pub(crate) frames_received: AtomicU64,

    /// Total bytes sent (frame payloads, not including length prefixes).
    pub(crate) bytes_sent: AtomicU64,

    /// Total bytes received (frame payloads, not including length prefixes).
    pub(crate) bytes_received: AtomicU64,

    /// Timestamp of last frame sent (ms since created_at, 0 = never).
    pub(crate) last_frame_sent_ms: AtomicU64,

    /// Timestamp of last frame received (ms since created_at, 0 = never).
    pub(crate) last_frame_received_ms: AtomicU64,

    /// Per-channel flow control credit snapshot (updated by the driver).
    /// Vec of (channel_id, incoming_credit, outgoing_credit).
    pub(crate) channel_credits: Mutex<Vec<ChannelCreditInfo>>,

    /// Connection identity used for peeps node/edge attributes.
    pub(crate) connection_identity: Mutex<ConnectionIdentity>,
    /// Stable correlation key shared by both peers for this link.
    pub(crate) connection_correlation_id: Mutex<Option<String>>,
    /// Stable per-connection context id used for request/response context edges.
    pub(crate) connection_context_id: OnceLock<String>,
    /// Connection scope handle used to attach request/response/channel entities.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) connection_scope: OnceLock<peeps::ScopeHandle>,
    /// Monotonic revision for mutable connection-context metadata.
    pub(crate) connection_context_revision: AtomicU64,
    /// Last revision published to diagnostics metadata sinks.
    pub(crate) connection_context_published_revision: AtomicU64,
    /// Latest mutable connection-context metadata snapshot.
    pub(crate) connection_context_metadata: Mutex<BTreeMap<String, String>>,

    /// Unix timestamp when this connection closed (0 = still open).
    pub(crate) connection_closed_at_ns: AtomicU64,

    /// Last select-arm that made progress in Driver::run.
    pub(crate) last_driver_arm: Mutex<String>,
    /// Unix timestamp of last recorded driver arm activity.
    pub(crate) last_driver_arm_at_ns: AtomicU64,
    /// Driver loop arm hit counters.
    pub(crate) driver_arm_driver_rx_hits: AtomicU64,
    pub(crate) driver_arm_io_recv_hits: AtomicU64,
    pub(crate) driver_arm_incoming_response_hits: AtomicU64,
    pub(crate) driver_arm_sweep_hits: AtomicU64,

    /// Pending-response map transition counters/state.
    pub(crate) pending_map_inserts: AtomicU64,
    pub(crate) pending_map_removes: AtomicU64,
    pub(crate) pending_map_failures: AtomicU64,
    pub(crate) pending_map_last_event: Mutex<String>,
    pub(crate) pending_map_last_conn_id: AtomicU64,
    pub(crate) pending_map_last_request_id: AtomicU64,
    pub(crate) pending_map_last_len_before: AtomicU64,
    pub(crate) pending_map_last_len_after: AtomicU64,
    pub(crate) pending_map_last_at_ns: AtomicU64,
}

/// Per-channel flow control credit info for diagnostics.
#[derive(Debug, Clone)]
pub struct ChannelCreditInfo {
    pub channel_id: u64,
    /// Credit we granted to peer (bytes they can still send us).
    pub incoming_credit: u32,
    /// Credit peer granted us (bytes we can still send them).
    pub outgoing_credit: u32,
}

/// Stable identity for a directional connection.
#[derive(Debug, Clone)]
pub struct ConnectionIdentity {
    pub src: String,
    pub dst: String,
    pub transport: String,
    pub opened_at_ns: u64,
}

pub struct RequestRecord<'a> {
    pub conn_id: u64,
    pub request_id: u64,
    pub method_id: u64,
    pub metadata: Option<&'a roam_wire::Metadata>,
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
    pub args: Option<HashMap<String, String>>,
}

impl DiagnosticState {
    fn metadata_to_debug_map(
        metadata: Option<&roam_wire::Metadata>,
    ) -> Option<HashMap<String, String>> {
        let metadata = metadata?;
        if metadata.is_empty() {
            return None;
        }
        let mut out = HashMap::new();
        for (k, v, flags) in metadata {
            let val = match v {
                roam_wire::MetadataValue::String(s) => s.clone(),
                roam_wire::MetadataValue::U64(n) => n.to_string(),
                roam_wire::MetadataValue::Bytes(b) => format!("<{} bytes>", b.len()),
            };
            if *flags == 0 {
                out.insert(k.clone(), val);
            } else {
                out.insert(k.clone(), format!("{val} [flags={flags}]"));
            }
        }
        Some(out)
    }

    /// Create a new diagnostic state.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let now_ns = unix_now_ns();
        Self {
            name: name.clone(),
            peer_name: Mutex::new(None),
            max_concurrent_requests: AtomicU32::new(0),
            initial_credit: AtomicU32::new(0),
            created_at: Instant::now(),
            total_completed: AtomicU64::new(0),
            requests: Mutex::new(HashMap::new()),
            recent_completions: Mutex::new(VecDeque::with_capacity(MAX_RECENT_COMPLETIONS)),
            channels: Mutex::new(HashMap::new()),
            custom_diagnostics: Mutex::new(Vec::new()),
            frames_sent: AtomicU64::new(0),
            frames_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            last_frame_sent_ms: AtomicU64::new(0),
            last_frame_received_ms: AtomicU64::new(0),
            channel_credits: Mutex::new(Vec::new()),
            connection_identity: Mutex::new(ConnectionIdentity {
                src: name.clone(),
                dst: "unknown".to_string(),
                transport: "unknown".to_string(),
                opened_at_ns: now_ns,
            }),
            connection_correlation_id: Mutex::new(None),
            connection_context_id: OnceLock::new(),
            #[cfg(not(target_arch = "wasm32"))]
            connection_scope: OnceLock::new(),
            connection_context_revision: AtomicU64::new(1),
            connection_context_published_revision: AtomicU64::new(0),
            connection_context_metadata: Mutex::new(BTreeMap::new()),
            connection_closed_at_ns: AtomicU64::new(0),
            last_driver_arm: Mutex::new("startup".to_string()),
            last_driver_arm_at_ns: AtomicU64::new(now_ns),
            driver_arm_driver_rx_hits: AtomicU64::new(0),
            driver_arm_io_recv_hits: AtomicU64::new(0),
            driver_arm_incoming_response_hits: AtomicU64::new(0),
            driver_arm_sweep_hits: AtomicU64::new(0),
            pending_map_inserts: AtomicU64::new(0),
            pending_map_removes: AtomicU64::new(0),
            pending_map_failures: AtomicU64::new(0),
            pending_map_last_event: Mutex::new("none".to_string()),
            pending_map_last_conn_id: AtomicU64::new(0),
            pending_map_last_request_id: AtomicU64::new(0),
            pending_map_last_len_before: AtomicU64::new(0),
            pending_map_last_len_after: AtomicU64::new(0),
            pending_map_last_at_ns: AtomicU64::new(now_ns),
        }
    }

    /// Set the peer's name (from Hello V6 metadata).
    pub fn set_peer_name(&self, name: String) {
        if let Ok(mut peer_name) = self.peer_name.lock() {
            *peer_name = Some(name);
        }
    }

    /// Set directional identity and lifecycle origin for this connection.
    pub fn set_connection_identity(
        &self,
        src: impl Into<String>,
        dst: impl Into<String>,
        transport: impl Into<String>,
        opened_at_ns: u64,
    ) {
        if let Ok(mut identity) = self.connection_identity.lock() {
            identity.src = src.into();
            identity.dst = dst.into();
            identity.transport = transport.into();
            identity.opened_at_ns = opened_at_ns;
        }
        self.mark_connection_context_dirty();
    }

    /// Set the cross-process correlation key shared by both connection legs.
    pub fn set_connection_correlation_id(&self, correlation_id: impl Into<String>) {
        if let Ok(mut slot) = self.connection_correlation_id.lock() {
            *slot = Some(correlation_id.into());
        }
        self.mark_connection_context_dirty();
    }

    /// Read the cross-process correlation key, if available.
    pub fn connection_correlation_id(&self) -> Option<String> {
        self.connection_correlation_id
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// Mark connection closure time.
    pub fn mark_connection_closed(&self, closed_at_ns: u64) {
        self.connection_closed_at_ns
            .store(closed_at_ns, Ordering::Relaxed);
        self.mark_connection_context_dirty();
    }

    /// Snapshot the current directional connection identity.
    pub fn connection_identity(&self) -> ConnectionIdentity {
        self.connection_identity
            .lock()
            .map(|id| id.clone())
            .unwrap_or_else(|_| ConnectionIdentity {
                src: self.name.clone(),
                dst: "unknown".to_string(),
                transport: "unknown".to_string(),
                opened_at_ns: unix_now_ns(),
            })
    }

    /// Directional `rpc.connection` token.
    pub fn rpc_connection_token(&self) -> String {
        let id = self.connection_identity();
        format!(
            "{}->{}:{}:{}",
            id.src, id.dst, id.transport, id.opened_at_ns
        )
    }

    /// Stable per-connection context id for context-edge wiring.
    pub fn ensure_connection_context_id(&self) -> String {
        self.connection_context_id
            .get_or_init(|| {
                let identity = self.connection_identity();
                let correlation = self
                    .connection_correlation_id()
                    .unwrap_or_else(|| self.rpc_connection_token());
                format!(
                    "connection-context:{correlation}:{}->{}",
                    identity.src, identity.dst
                )
            })
            .clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn ensure_connection_scope(&self) -> peeps::ScopeHandle {
        self.connection_scope
            .get_or_init(|| {
                let identity = self.connection_identity();
                let correlation = self
                    .connection_correlation_id()
                    .unwrap_or_else(|| self.rpc_connection_token());
                peeps::ScopeHandle::new(
                    format!(
                        "roam.connection.{correlation}:{}->{}",
                        identity.src, identity.dst
                    ),
                    peeps_types::ScopeBody::Connection(peeps_types::ConnectionScopeBody {
                        local_addr: None,
                        peer_addr: None,
                    }),
                    peeps::SourceRight::caller(),
                )
            })
            .clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn link_entity_to_connection_scope(&self, entity: &peeps::EntityHandle) {
        let scope = self.ensure_connection_scope();
        entity.link_to_scope_handle(&scope);
    }

    /// Monotonic revision increment for mutable connection metadata.
    pub fn mark_connection_context_dirty(&self) {
        self.connection_context_revision
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Apply stable connection context attrs (scope identity, not mutable counters).
    pub fn apply_connection_context_attrs(&self, attrs: &mut BTreeMap<String, String>) {
        let identity = self.connection_identity();
        attrs.insert("rpc.connection".to_string(), self.rpc_connection_token());
        attrs.insert(
            "connection.context_id".to_string(),
            self.ensure_connection_context_id(),
        );
        if let Some(correlation_id) = self.connection_correlation_id() {
            attrs.insert("connection.correlation_id".to_string(), correlation_id);
        }
        attrs.insert("connection.src".to_string(), identity.src);
        attrs.insert("connection.dst".to_string(), identity.dst);
        attrs.insert("connection.transport".to_string(), identity.transport);
    }

    fn build_connection_context_mutable_attrs(&self) -> BTreeMap<String, String> {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "connection.state".to_string(),
            self.connection_state().to_string(),
        );
        attrs.insert(
            "connection.opened_at_ns".to_string(),
            self.connection_identity().opened_at_ns.to_string(),
        );
        if let Some(closed_at_ns) = self.connection_closed_at_ns() {
            attrs.insert(
                "connection.closed_at_ns".to_string(),
                closed_at_ns.to_string(),
            );
        }
        if let Some(last_sent_at_ns) = self.last_frame_sent_at_ns() {
            attrs.insert(
                "connection.last_frame_sent_at_ns".to_string(),
                last_sent_at_ns.to_string(),
            );
        }
        if let Some(last_recv_at_ns) = self.last_frame_received_at_ns() {
            attrs.insert(
                "connection.last_frame_recv_at_ns".to_string(),
                last_recv_at_ns.to_string(),
            );
        }
        attrs.insert(
            "connection.pending_requests".to_string(),
            self.pending_requests().to_string(),
        );
        attrs.insert(
            "connection.pending_requests_outgoing".to_string(),
            self.pending_requests_outgoing().to_string(),
        );
        attrs.insert(
            "connection.pending_responses".to_string(),
            self.pending_responses().to_string(),
        );
        attrs.insert(
            "connection.driver.last_arm".to_string(),
            self.last_driver_arm(),
        );
        attrs.insert(
            "connection.driver.last_arm_at_ns".to_string(),
            self.last_driver_arm_at_ns().to_string(),
        );
        attrs.insert(
            "connection.driver.driver_rx_hits".to_string(),
            self.driver_arm_driver_rx_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.io_recv_hits".to_string(),
            self.driver_arm_io_recv_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.incoming_response_hits".to_string(),
            self.driver_arm_incoming_response_hits().to_string(),
        );
        attrs.insert(
            "connection.driver.sweep_hits".to_string(),
            self.driver_arm_sweep_hits().to_string(),
        );
        attrs.insert(
            "connection.pending_map.inserts".to_string(),
            self.pending_map_inserts().to_string(),
        );
        attrs.insert(
            "connection.pending_map.removes".to_string(),
            self.pending_map_removes().to_string(),
        );
        attrs.insert(
            "connection.pending_map.failures".to_string(),
            self.pending_map_failures().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_event".to_string(),
            self.pending_map_last_event(),
        );
        attrs.insert(
            "connection.pending_map.last_conn_id".to_string(),
            self.pending_map_last_conn_id().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_request_id".to_string(),
            self.pending_map_last_request_id().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_len_before".to_string(),
            self.pending_map_last_len_before().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_len_after".to_string(),
            self.pending_map_last_len_after().to_string(),
        );
        attrs.insert(
            "connection.pending_map.last_at_ns".to_string(),
            self.pending_map_last_at_ns().to_string(),
        );
        attrs
    }

    /// Returns refreshed connection-context metadata only when dirty.
    pub fn take_connection_context_refresh_if_dirty(&self) -> Option<String> {
        let revision = self.connection_context_revision.load(Ordering::Relaxed);
        let published = self
            .connection_context_published_revision
            .load(Ordering::Relaxed);
        if revision == published {
            return None;
        }

        let context_id = self.ensure_connection_context_id();
        let attrs = self.build_connection_context_mutable_attrs();
        if let Ok(mut metadata) = self.connection_context_metadata.lock() {
            *metadata = attrs;
        }
        self.connection_context_published_revision
            .store(revision, Ordering::Relaxed);
        Some(context_id)
    }

    /// Whether this connection is currently open.
    pub fn connection_state(&self) -> &'static str {
        if self.connection_closed_at_ns.load(Ordering::Relaxed) == 0 {
            "open"
        } else {
            "closed"
        }
    }

    /// Unix nanos when connection was closed, if known.
    pub fn connection_closed_at_ns(&self) -> Option<u64> {
        let value = self.connection_closed_at_ns.load(Ordering::Relaxed);
        if value == 0 { None } else { Some(value) }
    }

    /// Set negotiated flow control parameters.
    pub fn set_negotiated_params(&self, max_concurrent_requests: u32, initial_credit: u32) {
        self.max_concurrent_requests
            .store(max_concurrent_requests, Ordering::Relaxed);
        self.initial_credit.store(initial_credit, Ordering::Relaxed);
        self.mark_connection_context_dirty();
    }

    /// Record an outgoing request (we're calling remote).
    pub fn record_outgoing_request(&self, record: RequestRecord<'_>) {
        let RequestRecord {
            conn_id,
            request_id,
            method_id,
            metadata,
            task_id,
            task_name,
            args,
        } = record;
        let backtrace = Some(format_short_backtrace());
        let metadata = Self::metadata_to_debug_map(metadata);
        let now = Instant::now();
        let now_unix_ns = unix_now_ns();
        if let Ok(mut requests) = self.requests.lock() {
            requests.insert(
                (conn_id, request_id),
                InFlightRequest {
                    request_id,
                    method_id,
                    started: now,
                    started_at_ns: now_unix_ns,
                    handled_at: None,
                    handled_at_ns: None,
                    direction: RequestDirection::Outgoing,
                    task_id,
                    task_name,
                    metadata,
                    args,
                    backtrace,
                    server_task_id: None,
                    server_task_name: None,
                },
            );
        }
        self.mark_connection_context_dirty();
    }

    /// Record an incoming request (remote is calling us).
    pub fn record_incoming_request(&self, record: RequestRecord<'_>) {
        let RequestRecord {
            conn_id,
            request_id,
            method_id,
            metadata,
            task_id,
            task_name,
            args,
        } = record;
        let metadata = Self::metadata_to_debug_map(metadata);
        // Task tracking APIs removed — set to None
        let server_task_id = None;
        let server_task_name = None;
        let now = Instant::now();
        let now_unix_ns = unix_now_ns();
        if let Ok(mut requests) = self.requests.lock() {
            requests.insert(
                (conn_id, request_id),
                InFlightRequest {
                    request_id,
                    method_id,
                    started: now,
                    started_at_ns: now_unix_ns,
                    handled_at: None,
                    handled_at_ns: None,
                    direction: RequestDirection::Incoming,
                    task_id,
                    task_name,
                    metadata,
                    args,
                    backtrace: None, // no backtrace for incoming — the remote captured it
                    server_task_id,
                    server_task_name,
                },
            );
        }
        self.mark_connection_context_dirty();
    }

    /// Mark a request as completed and record it in the recent completions ring buffer.
    pub fn complete_request(&self, conn_id: u64, request_id: u64) {
        let completed = if let Ok(mut requests) = self.requests.lock() {
            requests.remove(&(conn_id, request_id))
        } else {
            None
        };

        if let Some(req) = completed {
            self.total_completed.fetch_add(1, Ordering::Relaxed);
            let duration = req.started.elapsed();
            if let Ok(mut completions) = self.recent_completions.lock() {
                if completions.len() >= MAX_RECENT_COMPLETIONS {
                    completions.pop_front();
                }
                completions.push_back(CompletedRequest {
                    method_id: req.method_id,
                    direction: req.direction,
                    duration,
                    completed_at: Instant::now(),
                });
            }
        }
        self.mark_connection_context_dirty();
    }

    /// Mark an in-flight request as handled by the server-side handler.
    pub fn mark_request_handled(&self, conn_id: u64, request_id: u64) {
        if let Ok(mut requests) = self.requests.lock()
            && let Some(req) = requests.get_mut(&(conn_id, request_id))
            && req.handled_at.is_none()
        {
            req.handled_at = Some(Instant::now());
            req.handled_at_ns = Some(unix_now_ns());
        }
    }

    /// Unix timestamp in nanoseconds for when a request was first recorded.
    pub fn inflight_request_started_at_ns(&self, conn_id: u64, request_id: u64) -> Option<u64> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        Some(req.started_at_ns)
    }

    /// Unix timestamp in nanoseconds for when handler logic completed (if known).
    pub fn inflight_request_handled_at_ns(&self, conn_id: u64, request_id: u64) -> Option<u64> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        req.handled_at_ns
    }

    /// Elapsed nanoseconds since request started, if still in-flight.
    pub fn inflight_request_elapsed_ns(&self, conn_id: u64, request_id: u64) -> Option<u64> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        Some(req.started.elapsed().as_nanos() as u64)
    }

    /// Elapsed nanoseconds from request start to handler-complete moment, if known.
    pub fn inflight_request_handled_elapsed_ns(
        &self,
        conn_id: u64,
        request_id: u64,
    ) -> Option<u64> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        let handled_at = req.handled_at?;
        Some(handled_at.duration_since(req.started).as_nanos() as u64)
    }

    /// Method id for an in-flight request.
    pub fn inflight_request_method_id(&self, conn_id: u64, request_id: u64) -> Option<u64> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        Some(req.method_id)
    }

    /// Record a channel being opened.
    pub fn record_channel_open(
        &self,
        channel_id: u64,
        direction: ChannelDirection,
        request_id: Option<u64>,
    ) {
        let task_id = None;
        let task_name = None;
        if let Ok(mut channels) = self.channels.lock() {
            channels.insert(
                channel_id,
                OpenChannel {
                    channel_id,
                    started: Instant::now(),
                    direction,
                    request_id,
                    task_id,
                    task_name,
                    closed: false,
                },
            );
        }
    }

    /// Record a channel being closed.
    ///
    /// Marks the channel as closed rather than removing it, so it still
    /// appears in diagnostic snapshots until the connection is dropped.
    pub fn record_channel_close(&self, channel_id: u64) {
        if let Ok(mut channels) = self.channels.lock()
            && let Some(ch) = channels.get_mut(&channel_id)
        {
            ch.closed = true;
        }
    }

    /// Associate channels with a request (called after channels are opened but before request is sent).
    pub fn associate_channels_with_request(&self, channel_ids: &[u64], request_id: u64) {
        if let Ok(mut channels) = self.channels.lock() {
            for &channel_id in channel_ids {
                if let Some(channel) = channels.get_mut(&channel_id) {
                    channel.request_id = Some(request_id);
                }
            }
        }
    }

    /// Read a metadata string value from an in-flight request, if present.
    pub fn inflight_request_metadata_string(
        &self,
        conn_id: u64,
        request_id: u64,
        key: &str,
    ) -> Option<String> {
        let requests = self.requests.lock().ok()?;
        let req = requests.get(&(conn_id, request_id))?;
        let meta = req.metadata.as_ref()?;
        meta.get(key).cloned()
    }

    /// Add a custom diagnostic callback.
    pub fn add_custom_diagnostic<F>(&self, callback: F)
    where
        F: Fn(&mut String) + Send + Sync + 'static,
    {
        if let Ok(mut diagnostics) = self.custom_diagnostics.lock() {
            diagnostics.push(Box::new(callback));
        }
    }

    /// Record a frame being sent (call after successful transport send).
    pub fn record_frame_sent(&self, payload_bytes: usize) {
        self.frames_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent
            .fetch_add(payload_bytes as u64, Ordering::Relaxed);
        let ms = self.created_at.elapsed().as_millis() as u64;
        self.last_frame_sent_ms.store(ms, Ordering::Relaxed);
        self.mark_connection_context_dirty();
    }

    /// Record a frame being received (call after successful transport recv).
    pub fn record_frame_received(&self, payload_bytes: usize) {
        self.frames_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(payload_bytes as u64, Ordering::Relaxed);
        let ms = self.created_at.elapsed().as_millis() as u64;
        self.last_frame_received_ms.store(ms, Ordering::Relaxed);
        self.mark_connection_context_dirty();
    }

    /// Update the per-channel credit snapshot.
    pub fn update_channel_credits(&self, credits: Vec<ChannelCreditInfo>) {
        if let Ok(mut cc) = self.channel_credits.lock() {
            *cc = credits;
        }
    }

    /// Record which Driver::run select arm made progress most recently.
    pub fn record_driver_arm(&self, arm: &'static str) {
        if let Ok(mut last) = self.last_driver_arm.lock() {
            *last = arm.to_string();
        }
        self.last_driver_arm_at_ns
            .store(unix_now_ns(), Ordering::Relaxed);
        match arm {
            "driver_rx" => {
                self.driver_arm_driver_rx_hits
                    .fetch_add(1, Ordering::Relaxed);
            }
            "io.recv" => {
                self.driver_arm_io_recv_hits.fetch_add(1, Ordering::Relaxed);
            }
            "incoming_response_rx" => {
                self.driver_arm_incoming_response_hits
                    .fetch_add(1, Ordering::Relaxed);
            }
            "sweep_pending_responses" => {
                self.driver_arm_sweep_hits.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        self.mark_connection_context_dirty();
    }

    pub fn last_driver_arm(&self) -> String {
        self.last_driver_arm
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    pub fn last_driver_arm_at_ns(&self) -> u64 {
        self.last_driver_arm_at_ns.load(Ordering::Relaxed)
    }

    pub fn driver_arm_driver_rx_hits(&self) -> u64 {
        self.driver_arm_driver_rx_hits.load(Ordering::Relaxed)
    }

    pub fn driver_arm_io_recv_hits(&self) -> u64 {
        self.driver_arm_io_recv_hits.load(Ordering::Relaxed)
    }

    pub fn driver_arm_incoming_response_hits(&self) -> u64 {
        self.driver_arm_incoming_response_hits
            .load(Ordering::Relaxed)
    }

    pub fn driver_arm_sweep_hits(&self) -> u64 {
        self.driver_arm_sweep_hits.load(Ordering::Relaxed)
    }

    /// Record a pending-response map transition (insert/remove/fail).
    pub fn record_pending_map_event(
        &self,
        event: &'static str,
        conn_id: u64,
        request_id: u64,
        len_before: usize,
        len_after: usize,
    ) {
        match event {
            "insert" => {
                self.pending_map_inserts.fetch_add(1, Ordering::Relaxed);
            }
            "remove" => {
                self.pending_map_removes.fetch_add(1, Ordering::Relaxed);
            }
            "fail" => {
                self.pending_map_failures.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        if let Ok(mut last) = self.pending_map_last_event.lock() {
            *last = event.to_string();
        }
        self.pending_map_last_conn_id
            .store(conn_id, Ordering::Relaxed);
        self.pending_map_last_request_id
            .store(request_id, Ordering::Relaxed);
        self.pending_map_last_len_before
            .store(len_before as u64, Ordering::Relaxed);
        self.pending_map_last_len_after
            .store(len_after as u64, Ordering::Relaxed);
        self.pending_map_last_at_ns
            .store(unix_now_ns(), Ordering::Relaxed);
        self.mark_connection_context_dirty();
    }

    pub fn pending_map_inserts(&self) -> u64 {
        self.pending_map_inserts.load(Ordering::Relaxed)
    }

    pub fn pending_map_removes(&self) -> u64 {
        self.pending_map_removes.load(Ordering::Relaxed)
    }

    pub fn pending_map_failures(&self) -> u64 {
        self.pending_map_failures.load(Ordering::Relaxed)
    }

    pub fn pending_map_last_event(&self) -> String {
        self.pending_map_last_event
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    pub fn pending_map_last_conn_id(&self) -> u64 {
        self.pending_map_last_conn_id.load(Ordering::Relaxed)
    }

    pub fn pending_map_last_request_id(&self) -> u64 {
        self.pending_map_last_request_id.load(Ordering::Relaxed)
    }

    pub fn pending_map_last_len_before(&self) -> u64 {
        self.pending_map_last_len_before.load(Ordering::Relaxed)
    }

    pub fn pending_map_last_len_after(&self) -> u64 {
        self.pending_map_last_len_after.load(Ordering::Relaxed)
    }

    pub fn pending_map_last_at_ns(&self) -> u64 {
        self.pending_map_last_at_ns.load(Ordering::Relaxed)
    }

    /// Get the time since last frame sent (None if never sent).
    pub fn last_frame_sent_ago(&self) -> Option<std::time::Duration> {
        let ms = self.last_frame_sent_ms.load(Ordering::Relaxed);
        if ms == 0 {
            return None;
        }
        let sent_at = self.created_at + std::time::Duration::from_millis(ms);
        Some(Instant::now().duration_since(sent_at))
    }

    /// Get the time since last frame received (None if never received).
    pub fn last_frame_received_ago(&self) -> Option<std::time::Duration> {
        let ms = self.last_frame_received_ms.load(Ordering::Relaxed);
        if ms == 0 {
            return None;
        }
        let received_at = self.created_at + std::time::Duration::from_millis(ms);
        Some(Instant::now().duration_since(received_at))
    }

    /// Number of in-flight requests currently tracked.
    pub fn pending_requests_outgoing(&self) -> usize {
        self.requests
            .lock()
            .map(|reqs| {
                reqs.values()
                    .filter(|req| req.direction == RequestDirection::Outgoing)
                    .count()
            })
            .unwrap_or_default()
    }

    /// Number of incoming in-flight requests currently tracked by this side.
    pub fn pending_responses(&self) -> usize {
        self.requests
            .lock()
            .map(|reqs| {
                reqs.values()
                    .filter(|req| req.direction == RequestDirection::Incoming)
                    .count()
            })
            .unwrap_or_default()
    }

    /// Number of in-flight requests currently tracked.
    pub fn pending_requests(&self) -> usize {
        self.requests
            .lock()
            .map(|reqs| reqs.len())
            .unwrap_or_default()
    }

    /// Unix timestamp for last sent frame, in nanoseconds.
    pub fn last_frame_sent_at_ns(&self) -> Option<u64> {
        let ms = self.last_frame_sent_ms.load(Ordering::Relaxed);
        if ms == 0 {
            return None;
        }
        let opened_at_ns = self.connection_identity().opened_at_ns;
        let delta_ns = ms.saturating_mul(1_000_000);
        Some(opened_at_ns.saturating_add(delta_ns))
    }

    /// Unix timestamp for last received frame, in nanoseconds.
    pub fn last_frame_received_at_ns(&self) -> Option<u64> {
        let ms = self.last_frame_received_ms.load(Ordering::Relaxed);
        if ms == 0 {
            return None;
        }
        let opened_at_ns = self.connection_identity().opened_at_ns;
        let delta_ns = ms.saturating_mul(1_000_000);
        Some(opened_at_ns.saturating_add(delta_ns))
    }

    /// Dump this connection's full diagnostic state.
    ///
    /// Always produces output — shows connection age, in-flight count (even if 0),
    /// recent completions, and open channels.
    pub fn dump(&self) -> String {
        let now = Instant::now();
        let age = now.duration_since(self.created_at);
        let total = self.total_completed.load(Ordering::Relaxed);
        let mut output = String::new();

        // Format header with optional peer name
        let peer_label = self
            .peer_name
            .try_lock()
            .ok()
            .and_then(|g| g.as_ref().map(|n| format!(" {:?}", n)));
        let _ = writeln!(
            output,
            "[{}{}] age={:.1}s total_completed={}",
            self.name,
            peer_label.as_deref().unwrap_or(""),
            age.as_secs_f64(),
            total,
        );

        // Flow control state
        let max_concurrent = self.max_concurrent_requests.load(Ordering::Relaxed);
        let credit = self.initial_credit.load(Ordering::Relaxed);
        if max_concurrent > 0 || credit > 0 {
            let _ = writeln!(
                output,
                "  Flow: max_concurrent={}, initial_credit={}",
                max_concurrent, credit,
            );
        }

        // ── Transport stats ───────────────────────────────────────
        {
            let sent = self.frames_sent.load(Ordering::Relaxed);
            let recv = self.frames_received.load(Ordering::Relaxed);
            let bytes_s = self.bytes_sent.load(Ordering::Relaxed);
            let bytes_r = self.bytes_received.load(Ordering::Relaxed);
            let last_sent = self
                .last_frame_sent_ago()
                .map(|d| format!("{:.1}s ago", d.as_secs_f64()))
                .unwrap_or_else(|| "never".to_string());
            let last_recv = self
                .last_frame_received_ago()
                .map(|d| format!("{:.1}s ago", d.as_secs_f64()))
                .unwrap_or_else(|| "never".to_string());
            let _ = writeln!(
                output,
                "  Transport: sent={sent} frames ({bytes_s} B), recv={recv} frames ({bytes_r} B)",
            );
            let _ = writeln!(output, "  Last: sent={last_sent}, recv={last_recv}",);
        }

        // ── Channel credits ──────────────────────────────────────
        if let Ok(credits) = self.channel_credits.try_lock()
            && !credits.is_empty()
        {
            let _ = writeln!(output, "  Channel credits ({}):", credits.len());
            for cc in credits.iter() {
                let _ = writeln!(
                    output,
                    "    ch#{}: in={}, out={}",
                    cc.channel_id, cc.incoming_credit, cc.outgoing_credit,
                );
            }
        }

        // ── In-flight requests ───────────────────────────────────
        if let Ok(requests) = self.requests.try_lock() {
            let mut outgoing: Vec<_> = requests
                .values()
                .filter(|r| r.direction == RequestDirection::Outgoing)
                .collect();
            let mut incoming: Vec<_> = requests
                .values()
                .filter(|r| r.direction == RequestDirection::Incoming)
                .collect();

            // Sort oldest first (most likely to be stuck)
            outgoing.sort_by_key(|r| r.started);
            incoming.sort_by_key(|r| r.started);

            let _ = writeln!(
                output,
                "  In-flight: {}⬆ {}⬇",
                outgoing.len(),
                incoming.len()
            );

            for req in &outgoing {
                let elapsed = now.duration_since(req.started);
                let method_name = req
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get(crate::PEEPS_METHOD_NAME_METADATA_KEY))
                    .cloned()
                    .unwrap_or_else(|| format!("method#0x{:x}", req.method_id));
                let _ = write!(
                    output,
                    "    ⬆#{} {} {:.3}s",
                    req.request_id,
                    method_name,
                    elapsed.as_secs_f64()
                );
                if let Some(args) = &req.args {
                    let args_str: Vec<_> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    if !args_str.is_empty() {
                        let _ = write!(output, " ({})", args_str.join(", "));
                    }
                }
                output.push('\n');
                if let Some(bt) = &req.backtrace
                    && !bt.is_empty()
                {
                    for line in bt.lines() {
                        let _ = writeln!(output, "      {}", line);
                    }
                }
            }

            for req in &incoming {
                let elapsed = now.duration_since(req.started);
                let method_name = req
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get(crate::PEEPS_METHOD_NAME_METADATA_KEY))
                    .cloned()
                    .unwrap_or_else(|| format!("method#0x{:x}", req.method_id));
                let _ = write!(
                    output,
                    "    ⬇#{} {} {:.3}s",
                    req.request_id,
                    method_name,
                    elapsed.as_secs_f64()
                );
                if let Some(args) = &req.args {
                    let args_str: Vec<_> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    if !args_str.is_empty() {
                        let _ = write!(output, " ({})", args_str.join(", "));
                    }
                }
                output.push('\n');
            }
        } else {
            let _ = writeln!(output, "  In-flight: (lock held, cannot read)");
        }

        // ── Open channels ────────────────────────────────────────
        if let Ok(channels) = self.channels.try_lock() {
            if channels.is_empty() {
                let _ = writeln!(output, "  Channels: 0");
            } else {
                let tx_count = channels
                    .values()
                    .filter(|c| c.direction == ChannelDirection::Tx)
                    .count();
                let rx_count = channels
                    .values()
                    .filter(|c| c.direction == ChannelDirection::Rx)
                    .count();
                let _ = writeln!(output, "  Channels: {tx_count}tx {rx_count}rx");
            }
        }

        // ── Recent completions ───────────────────────────────────
        if let Ok(completions) = self.recent_completions.try_lock() {
            if completions.is_empty() {
                let _ = writeln!(output, "  Recent: (none)");
            } else {
                let _ = writeln!(output, "  Recent ({}):", completions.len());
                // Show newest first
                for req in completions.iter().rev() {
                    let method_name = format!("method#0x{:x}", req.method_id);
                    let ago = now.duration_since(req.completed_at);
                    let _ = writeln!(
                        output,
                        "    {} {} took {:.3}s ({:.1}s ago)",
                        req.direction,
                        method_name,
                        req.duration.as_secs_f64(),
                        ago.as_secs_f64(),
                    );
                }
            }
        }

        // ── Custom diagnostics ───────────────────────────────────
        if let Ok(customs) = self.custom_diagnostics.try_lock() {
            for callback in customs.iter() {
                callback(&mut output);
            }
        }

        output
    }

    /// Dump with an overridden name (used for numbered connections).
    pub fn dump_with_name(&self, name: &str) -> String {
        // Clone self's dump but replace the header line's name
        let full = self.dump();
        // Replace first occurrence of [self.name] with [name]
        let old_prefix = format!("[{}", self.name);
        let new_prefix = format!("[{}", name);
        full.replacen(&old_prefix, &new_prefix, 1)
    }

    /// Legacy compat — same as `dump()` but returns `Option`.
    pub fn dump_if_nonempty(&self) -> Option<String> {
        Some(self.dump())
    }
}

/// Collect all live diagnostic states (for snapshot use).
/// Uses try_read to avoid deadlocking from signal handlers.
pub fn collect_live_states() -> Vec<Arc<DiagnosticState>> {
    let Ok(registry) = DIAGNOSTIC_REGISTRY.try_lock() else {
        return Vec::new();
    };
    registry.iter().filter_map(|weak| weak.upgrade()).collect()
}

/// Snapshot method name registry as a HashMap.
pub fn snapshot_method_names() -> std::collections::HashMap<u64, String> {
    std::collections::HashMap::new()
}
fn format_short_backtrace() -> String {
    let bt = std::backtrace::Backtrace::force_capture();
    let full = bt.to_string();
    let mut lines = Vec::new();
    for line in full.lines() {
        // Skip backtrace infrastructure, std, tokio internals
        if line.contains("std::backtrace")
            || line.contains("roam_session::diagnostic")
            || line.contains("__rust_begin_short_backtrace")
            || line.contains("__rust_end_short_backtrace")
        {
            continue;
        }
        // Keep lines that look like user code
        if line.contains("roam") || line.contains("vx_") || line.contains("vxd") {
            lines.push(line.trim().to_string());
            if lines.len() >= 5 {
                break;
            }
        }
    }
    lines.join("\n")
}

impl std::fmt::Debug for DiagnosticState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiagnosticState")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
