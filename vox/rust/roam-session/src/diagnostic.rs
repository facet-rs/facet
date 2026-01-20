//! Diagnostic state tracking for SIGUSR1 dumps.
//!
//! Tracks in-flight RPC requests and open channels to help debug hung connections.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, LazyLock, RwLock, Weak};
use std::time::Instant;

/// Global registry of all diagnostic states.
/// Each connection/driver registers its state here.
static DIAGNOSTIC_REGISTRY: LazyLock<RwLock<Vec<Weak<DiagnosticState>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Method name registry - maps method_id to human-readable names.
static METHOD_NAMES: LazyLock<RwLock<HashMap<u64, &'static str>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Whether to record extra debug info (checked once at startup).
/// Set ROAM_DEBUG=1 to enable.
static DEBUG_ENABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var("ROAM_DEBUG").is_ok());

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
pub fn dump_all_diagnostics() -> String {
    let mut output = String::new();

    let states: Vec<Arc<DiagnosticState>> = {
        // Use try_read to avoid deadlocking if called from signal handler
        let Ok(registry) = DIAGNOSTIC_REGISTRY.try_read() else {
            return "ERROR: Could not acquire diagnostic registry lock (held by another thread)\n".to_string();
        };
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    };

    if states.is_empty() {
        return String::new();
    }

    for state in states {
        // Only include states that have something to report
        if let Some(content) = state.dump_if_nonempty() {
            let _ = writeln!(output, "[{}] {}", state.name, content);
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

/// An in-flight RPC request.
#[derive(Debug, Clone)]
pub struct InFlightRequest {
    pub request_id: u64,
    pub method_id: u64,
    pub started: Instant,
    pub direction: RequestDirection,
    /// Optional structured arguments (only recorded when ROAM_DEBUG_ARGS is set).
    pub args: Option<HashMap<String, String>>,
}

/// Direction of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDirection {
    /// We're sending on this channel
    Tx,
    /// We're receiving on this channel
    Rx,
}

/// An open streaming channel.
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
    /// Human-readable name for this connection (e.g., "cell-http", "host→markdown")
    pub name: String,

    /// In-flight requests
    requests: RwLock<HashMap<u64, InFlightRequest>>,

    /// Open channels
    channels: RwLock<HashMap<u64, OpenChannel>>,

    /// Custom diagnostic callbacks
    custom_diagnostics: RwLock<Vec<Box<dyn Fn(&mut String) + Send + Sync>>>,
}

impl DiagnosticState {
    /// Create a new diagnostic state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            requests: RwLock::new(HashMap::new()),
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

    /// Mark a request as completed.
    pub fn complete_request(&self, request_id: u64) {
        if let Ok(mut requests) = self.requests.write() {
            requests.remove(&request_id);
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

    /// Dump this state if non-empty, returning None if there's nothing to report.
    /// Uses try_read() to avoid deadlocking when called from signal handlers.
    /// Output is compact: single line for summary, one line per request.
    pub fn dump_if_nonempty(&self) -> Option<String> {
        let now = Instant::now();
        let mut parts = Vec::new();
        let mut details = Vec::new();

        // Check requests
        if let Ok(requests) = self.requests.try_read() {
            let mut outgoing: Vec<_> = requests
                .values()
                .filter(|r| r.direction == RequestDirection::Outgoing)
                .collect();
            let mut incoming: Vec<_> = requests
                .values()
                .filter(|r| r.direction == RequestDirection::Incoming)
                .collect();

            outgoing.sort_by_key(|r| std::cmp::Reverse(r.started));
            incoming.sort_by_key(|r| std::cmp::Reverse(r.started));

            if !outgoing.is_empty() {
                parts.push(format!("{}⬆", outgoing.len()));
                for req in outgoing {
                    let elapsed = now.duration_since(req.started);
                    let method_name = get_method_name(req.method_id).unwrap_or("?");
                    let mut line = format!(
                        "  ⬆#{} {} {:.1}s",
                        req.request_id, method_name, elapsed.as_secs_f64()
                    );
                    if let Some(args) = &req.args {
                        let args_str: Vec<_> = args.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        if !args_str.is_empty() {
                            let _ = write!(line, " ({})", args_str.join(", "));
                        }
                    }
                    details.push(line);
                }
            }

            if !incoming.is_empty() {
                parts.push(format!("{}⬇", incoming.len()));
                for req in incoming {
                    let elapsed = now.duration_since(req.started);
                    let method_name = get_method_name(req.method_id).unwrap_or("?");
                    let mut line = format!(
                        "  ⬇#{} {} {:.1}s",
                        req.request_id, method_name, elapsed.as_secs_f64()
                    );
                    if let Some(args) = &req.args {
                        let args_str: Vec<_> = args.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        if !args_str.is_empty() {
                            let _ = write!(line, " ({})", args_str.join(", "));
                        }
                    }
                    details.push(line);
                }
            }
        }

        // Check channels
        if let Ok(channels) = self.channels.try_read() {
            if !channels.is_empty() {
                let tx_count = channels.values().filter(|c| c.direction == ChannelDirection::Tx).count();
                let rx_count = channels.values().filter(|c| c.direction == ChannelDirection::Rx).count();
                if tx_count > 0 {
                    parts.push(format!("{}tx", tx_count));
                }
                if rx_count > 0 {
                    parts.push(format!("{}rx", rx_count));
                }
            }
        }

        if parts.is_empty() {
            return None;
        }

        let mut output = parts.join(" ");
        for detail in details {
            let _ = write!(output, "\n{}", detail);
        }
        Some(output)
    }
}

impl std::fmt::Debug for DiagnosticState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiagnosticState")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
