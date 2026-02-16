//! Diagnostic state tracking for SIGUSR1 dumps.
//!
//! Tracks in-flight RPC requests, open channels, and recent completed operations
//! to help debug hung connections.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Instant;

/// A callback that appends diagnostic info to a string.
pub type DiagnosticCallback = Box<dyn Fn(&mut String) + Send + Sync>;

/// Global registry of all diagnostic states.
/// Each connection/driver registers its state here.
static DIAGNOSTIC_REGISTRY: LazyLock<Mutex<Vec<Weak<DiagnosticState>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Method name registry - maps method_id to human-readable names.
static METHOD_NAMES: LazyLock<Mutex<HashMap<u64, &'static str>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether to record extra debug info (checked once at startup).
/// Set ROAM_DEBUG=1 to enable.
static DEBUG_ENABLED: LazyLock<bool> = LazyLock::new(|| std::env::var("ROAM_DEBUG").is_ok());

/// Maximum number of recent completions to keep per connection.
const MAX_RECENT_COMPLETIONS: usize = 16;

/// Check if debug recording is enabled.
pub fn debug_enabled() -> bool {
    *DEBUG_ENABLED
}

/// Register a method name for diagnostic display.
pub fn register_method_name(method_id: u64, name: &'static str) {
    if let Ok(mut names) = METHOD_NAMES.lock() {
        names.insert(method_id, name);
    }
}

/// Look up a method name by ID.
pub fn get_method_name(method_id: u64) -> Option<&'static str> {
    METHOD_NAMES.lock().ok()?.get(&method_id).copied()
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

    // Dump registered method names for reference
    if let Ok(names) = METHOD_NAMES.try_lock()
        && !names.is_empty()
    {
        let _ = writeln!(output, "Registered methods:");
        let mut sorted: Vec<_> = names.iter().collect();
        sorted.sort_by_key(|(id, _)| *id);
        for (id, name) in sorted {
            let _ = writeln!(output, "  0x{id:x} = {name}");
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
        Self {
            name: name.into(),
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
        }
    }

    /// Set the peer's name (from Hello V6 metadata).
    pub fn set_peer_name(&self, name: String) {
        if let Ok(mut peer_name) = self.peer_name.lock() {
            *peer_name = Some(name);
        }
    }

    /// Set negotiated flow control parameters.
    pub fn set_negotiated_params(&self, max_concurrent_requests: u32, initial_credit: u32) {
        self.max_concurrent_requests
            .store(max_concurrent_requests, Ordering::Relaxed);
        self.initial_credit.store(initial_credit, Ordering::Relaxed);
    }

    /// Record an outgoing request (we're calling remote).
    pub fn record_outgoing_request(
        &self,
        conn_id: u64,
        request_id: u64,
        method_id: u64,
        metadata: Option<&roam_wire::Metadata>,
        task_id: Option<u64>,
        task_name: Option<String>,
        args: Option<HashMap<String, String>>,
    ) {
        let backtrace = Some(format_short_backtrace());
        let metadata = Self::metadata_to_debug_map(metadata);
        if let Ok(mut requests) = self.requests.lock() {
            requests.insert(
                (conn_id, request_id),
                InFlightRequest {
                    request_id,
                    method_id,
                    started: Instant::now(),
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
    }

    /// Record an incoming request (remote is calling us).
    pub fn record_incoming_request(
        &self,
        conn_id: u64,
        request_id: u64,
        method_id: u64,
        metadata: Option<&roam_wire::Metadata>,
        task_id: Option<u64>,
        task_name: Option<String>,
        args: Option<HashMap<String, String>>,
    ) {
        let metadata = Self::metadata_to_debug_map(metadata);
        // Task tracking APIs removed — set to None
        let server_task_id = None;
        let server_task_name = None;
        if let Ok(mut requests) = self.requests.lock() {
            requests.insert(
                (conn_id, request_id),
                InFlightRequest {
                    request_id,
                    method_id,
                    started: Instant::now(),
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
        if let Ok(mut channels) = self.channels.lock() {
            if let Some(ch) = channels.get_mut(&channel_id) {
                ch.closed = true;
            }
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
    }

    /// Record a frame being received (call after successful transport recv).
    pub fn record_frame_received(&self, payload_bytes: usize) {
        self.frames_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(payload_bytes as u64, Ordering::Relaxed);
        let ms = self.created_at.elapsed().as_millis() as u64;
        self.last_frame_received_ms.store(ms, Ordering::Relaxed);
    }

    /// Update the per-channel credit snapshot.
    pub fn update_channel_credits(&self, credits: Vec<ChannelCreditInfo>) {
        if let Ok(mut cc) = self.channel_credits.lock() {
            *cc = credits;
        }
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
                let method_name = get_method_name(req.method_id).unwrap_or("?");
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
                let method_name = get_method_name(req.method_id).unwrap_or("?");
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
                    let method_name = get_method_name(req.method_id).unwrap_or("?");
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
    let Ok(names) = METHOD_NAMES.try_lock() else {
        return std::collections::HashMap::new();
    };
    names
        .iter()
        .map(|(&id, &name)| (id, name.to_string()))
        .collect()
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
