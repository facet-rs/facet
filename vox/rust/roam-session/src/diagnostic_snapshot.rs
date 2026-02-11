//! Structured diagnostic snapshots for JSON serialization.
//!
//! Parallel to `diagnostic.rs` but returns Facet-derivable types
//! instead of formatted strings. Used by vixen's `vx debug` to
//! write `/tmp/vx-dumps/{pid}.json`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use facet::Facet;

use crate::diagnostic::{self, ChannelDirection, RequestDirection, get_method_name};

/// Direction of an RPC request (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum Direction {
    /// We sent the request, waiting for response.
    Outgoing,
    /// We received the request, processing it.
    Incoming,
}

impl From<RequestDirection> for Direction {
    fn from(d: RequestDirection) -> Self {
        match d {
            RequestDirection::Outgoing => Direction::Outgoing,
            RequestDirection::Incoming => Direction::Incoming,
        }
    }
}

/// Direction of a channel (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ChannelDir {
    Tx,
    Rx,
}

impl From<ChannelDirection> for ChannelDir {
    fn from(d: ChannelDirection) -> Self {
        match d {
            ChannelDirection::Tx => ChannelDir::Tx,
            ChannelDirection::Rx => ChannelDir::Rx,
        }
    }
}

/// Snapshot of all roam-session diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct DiagnosticSnapshot {
    pub connections: Vec<ConnectionSnapshot>,
    pub method_names: HashMap<u64, String>,
}

/// Snapshot of a single connection's diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct ConnectionSnapshot {
    pub name: String,
    pub peer_name: Option<String>,
    pub age_secs: f64,
    pub total_completed: u64,
    pub max_concurrent_requests: u32,
    pub initial_credit: u32,
    pub in_flight: Vec<RequestSnapshot>,
    pub recent_completions: Vec<CompletionSnapshot>,
    pub channels: Vec<ChannelSnapshot>,
    pub transport: TransportStats,
    pub channel_credits: Vec<ChannelCreditSnapshot>,
}

/// Snapshot of an in-flight RPC request.
#[derive(Debug, Clone, Facet)]
pub struct RequestSnapshot {
    pub request_id: u64,
    pub method_name: Option<String>,
    pub method_id: u64,
    pub direction: Direction,
    pub elapsed_secs: f64,
    pub args: Option<HashMap<String, String>>,
    pub backtrace: Option<String>,
}

/// Snapshot of a recently completed RPC request.
#[derive(Debug, Clone, Facet)]
pub struct CompletionSnapshot {
    pub method_name: Option<String>,
    pub method_id: u64,
    pub direction: Direction,
    pub duration_secs: f64,
    pub age_secs: f64,
}

/// Snapshot of an open channel.
#[derive(Debug, Clone, Facet)]
pub struct ChannelSnapshot {
    pub channel_id: u64,
    pub direction: ChannelDir,
    pub age_secs: f64,
    pub request_id: Option<u64>,
}

/// Transport-level statistics for a connection.
#[derive(Debug, Clone, Facet)]
pub struct TransportStats {
    /// Total frames sent.
    pub frames_sent: u64,
    /// Total frames received.
    pub frames_received: u64,
    /// Total payload bytes sent.
    pub bytes_sent: u64,
    /// Total payload bytes received.
    pub bytes_received: u64,
    /// Seconds since last frame was sent (None if never sent).
    pub last_sent_ago_secs: Option<f64>,
    /// Seconds since last frame was received (None if never received).
    pub last_recv_ago_secs: Option<f64>,
}

/// Per-channel flow control credit snapshot.
#[derive(Debug, Clone, Facet)]
pub struct ChannelCreditSnapshot {
    pub channel_id: u64,
    /// Credit we granted to peer (bytes they can still send us).
    pub incoming_credit: u32,
    /// Credit peer granted us (bytes we can still send them).
    pub outgoing_credit: u32,
}

/// Take a structured snapshot of all registered diagnostic states.
///
/// Safe to call from signal handlers (uses `try_read()` on all locks).
pub fn snapshot_all_diagnostics() -> DiagnosticSnapshot {
    let now = std::time::Instant::now();

    let states = diagnostic::collect_live_states();

    let connections: Vec<ConnectionSnapshot> = states
        .iter()
        .map(|state| {
            let age_secs = now.duration_since(state.created_at).as_secs_f64();
            let total_completed = state.total_completed.load(Ordering::Relaxed);
            let max_concurrent_requests = state.max_concurrent_requests.load(Ordering::Relaxed);
            let initial_credit = state.initial_credit.load(Ordering::Relaxed);

            let peer_name = state.peer_name.try_read().ok().and_then(|g| g.clone());

            let in_flight = state
                .requests
                .try_read()
                .map(|reqs| {
                    let mut v: Vec<RequestSnapshot> = reqs
                        .values()
                        .map(|r| RequestSnapshot {
                            request_id: r.request_id,
                            method_name: get_method_name(r.method_id).map(String::from),
                            method_id: r.method_id,
                            direction: r.direction.into(),
                            elapsed_secs: now.duration_since(r.started).as_secs_f64(),
                            args: r.args.clone(),
                            backtrace: r.backtrace.clone(),
                        })
                        .collect();
                    v.sort_by(|a, b| {
                        b.elapsed_secs
                            .partial_cmp(&a.elapsed_secs)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    v
                })
                .unwrap_or_default();

            let recent_completions = state
                .recent_completions
                .try_read()
                .map(|comps| {
                    comps
                        .iter()
                        .rev()
                        .map(|c| CompletionSnapshot {
                            method_name: get_method_name(c.method_id).map(String::from),
                            method_id: c.method_id,
                            direction: c.direction.into(),
                            duration_secs: c.duration.as_secs_f64(),
                            age_secs: now.duration_since(c.completed_at).as_secs_f64(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            let channels = state
                .channels
                .try_read()
                .map(|chs| {
                    chs.values()
                        .map(|ch| ChannelSnapshot {
                            channel_id: ch.channel_id,
                            direction: ch.direction.into(),
                            age_secs: now.duration_since(ch.started).as_secs_f64(),
                            request_id: ch.request_id,
                        })
                        .collect()
                })
                .unwrap_or_default();

            let transport = TransportStats {
                frames_sent: state.frames_sent.load(Ordering::Relaxed),
                frames_received: state.frames_received.load(Ordering::Relaxed),
                bytes_sent: state.bytes_sent.load(Ordering::Relaxed),
                bytes_received: state.bytes_received.load(Ordering::Relaxed),
                last_sent_ago_secs: state.last_frame_sent_ago().map(|d| d.as_secs_f64()),
                last_recv_ago_secs: state.last_frame_received_ago().map(|d| d.as_secs_f64()),
            };

            let channel_credits = state
                .channel_credits
                .try_read()
                .map(|cc| {
                    cc.iter()
                        .map(|c| ChannelCreditSnapshot {
                            channel_id: c.channel_id,
                            incoming_credit: c.incoming_credit,
                            outgoing_credit: c.outgoing_credit,
                        })
                        .collect()
                })
                .unwrap_or_default();

            ConnectionSnapshot {
                name: state.name.clone(),
                peer_name,
                age_secs,
                total_completed,
                max_concurrent_requests,
                initial_credit,
                in_flight,
                recent_completions,
                channels,
                transport,
                channel_credits,
            }
        })
        .collect();

    let method_names = diagnostic::snapshot_method_names();

    DiagnosticSnapshot {
        connections,
        method_names,
    }
}
