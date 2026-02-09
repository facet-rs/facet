//! Diagnostic state tracking for SIGUSR1 dumps.
//!
//! Tracks in-flight RPC requests, open channels, and recent completed operations
//! to help debug hung connections.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock, Weak};
use std::time::Instant;

/// A callback that appends diagnostic info to a string.
pub type DiagnosticCallback = Box<dyn Fn(&mut String) + Send + Sync>;

/// Global registry of all diagnostic states.
/// Each connection/driver registers its state here.
static DIAGNOSTIC_REGISTRY: LazyLock<RwLock<Vec<Weak<DiagnosticState>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Method name registry - maps method_id to human-readable names.
static METHOD_NAMES: LazyLock<RwLock<HashMap<u64, &'static str>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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
    if let Ok(mut names) = METHOD_NAMES.write() {
        names.insert(method_id, name);
    }
}

/// Look up a method name by ID.
pub fn get_method_name(method_id: u64) -> Option<&'static str> {
    METHOD_NAMES.read().ok()?.get(&method_id).copied()
}

/// Register a diagnostic state for SIGUSR1 dumps.
pub fn register_diagnostic_state(state: &Arc<DiagnosticState>) {
    if let Ok(mut registry) = DIAGNOSTIC_REGISTRY.write() {
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
        let Ok(registry) = DIAGNOSTIC_REGISTRY.try_read() else {
            return "ERROR: Could not acquire diagnostic registry lock (held by another thread)\n"
                .to_string();
        };
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    };

    if states.is_empty() {
        output.push_str("(no roam connections registered)\n");
        return output;
    }

    for state in &states {
        let _ = writeln!(output, "{}", state.dump());
    }

    // Dump registered method names for reference
    if let Ok(names) = METHOD_NAMES.try_read()
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
    /// Optional structured arguments (only recorded when ROAM_DEBUG is set).
    pub args: Option<HashMap<String, String>>,
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
}

/// Diagnostic state for a single connection.
pub struct DiagnosticState {
    /// Human-readable name for this connection (e.g., "client", "server")
    pub name: String,

    /// When this connection was established
    created_at: Instant,

    /// Total requests completed over the lifetime of this connection
    total_completed: AtomicU64,

    /// In-flight requests
    requests: RwLock<HashMap<u64, InFlightRequest>>,

    /// Recently completed requests (ring buffer, newest last)
    recent_completions: RwLock<VecDeque<CompletedRequest>>,

    /// Open channels
    channels: RwLock<HashMap<u64, OpenChannel>>,

    /// Custom diagnostic callbacks
    custom_diagnostics: RwLock<Vec<DiagnosticCallback>>,
}

impl DiagnosticState {
    /// Create a new diagnostic state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            created_at: Instant::now(),
            total_completed: AtomicU64::new(0),
            requests: RwLock::new(HashMap::new()),
            recent_completions: RwLock::new(VecDeque::with_capacity(MAX_RECENT_COMPLETIONS)),
            channels: RwLock::new(HashMap::new()),
            custom_diagnostics: RwLock::new(Vec::new()),
        }
    }

    /// Record an outgoing request (we're calling remote).
    pub fn record_outgoing_request(
        &self,
        request_id: u64,
        method_id: u64,
        args: Option<HashMap<String, String>>,
    ) {
        if let Ok(mut requests) = self.requests.write() {
            requests.insert(
                request_id,
                InFlightRequest {
                    request_id,
                    method_id,
                    started: Instant::now(),
                    direction: RequestDirection::Outgoing,
                    args,
                },
            );
        }
    }

    /// Record an incoming request (remote is calling us).
    pub fn record_incoming_request(
        &self,
        request_id: u64,
        method_id: u64,
        args: Option<HashMap<String, String>>,
    ) {
        if let Ok(mut requests) = self.requests.write() {
            requests.insert(
                request_id,
                InFlightRequest {
                    request_id,
                    method_id,
                    started: Instant::now(),
                    direction: RequestDirection::Incoming,
                    args,
                },
            );
        }
    }

    /// Mark a request as completed and record it in the recent completions ring buffer.
    pub fn complete_request(&self, request_id: u64) {
        let completed = if let Ok(mut requests) = self.requests.write() {
            requests.remove(&request_id)
        } else {
            None
        };

        if let Some(req) = completed {
            self.total_completed.fetch_add(1, Ordering::Relaxed);
            let duration = req.started.elapsed();
            if let Ok(mut completions) = self.recent_completions.write() {
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
        if let Ok(mut channels) = self.channels.write() {
            channels.insert(
                channel_id,
                OpenChannel {
                    channel_id,
                    started: Instant::now(),
                    direction,
                    request_id,
                },
            );
        }
    }

    /// Record a channel being closed.
    pub fn record_channel_close(&self, channel_id: u64) {
        if let Ok(mut channels) = self.channels.write() {
            channels.remove(&channel_id);
        }
    }

    /// Associate channels with a request (called after channels are opened but before request is sent).
    pub fn associate_channels_with_request(&self, channel_ids: &[u64], request_id: u64) {
        if let Ok(mut channels) = self.channels.write() {
            for &channel_id in channel_ids {
                if let Some(channel) = channels.get_mut(&channel_id) {
                    channel.request_id = Some(request_id);
                }
            }
        }
    }

    /// Add a custom diagnostic callback.
    pub fn add_custom_diagnostic<F>(&self, callback: F)
    where
        F: Fn(&mut String) + Send + Sync + 'static,
    {
        if let Ok(mut diagnostics) = self.custom_diagnostics.write() {
            diagnostics.push(Box::new(callback));
        }
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

        let _ = writeln!(
            output,
            "[{}] age={:.1}s total_completed={}",
            self.name,
            age.as_secs_f64(),
            total,
        );

        // ── In-flight requests ───────────────────────────────────
        if let Ok(requests) = self.requests.try_read() {
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
        if let Ok(channels) = self.channels.try_read() {
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
        if let Ok(completions) = self.recent_completions.try_read() {
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
        if let Ok(customs) = self.custom_diagnostics.try_read() {
            for callback in customs.iter() {
                callback(&mut output);
            }
        }

        output
    }

    /// Legacy compat — same as `dump()` but returns `Option`.
    pub fn dump_if_nonempty(&self) -> Option<String> {
        Some(self.dump())
    }
}

impl std::fmt::Debug for DiagnosticState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiagnosticState")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
