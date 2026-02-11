//! Structured diagnostic snapshots for JSON serialization.
//!
//! Uses types from peeps-types and registers as a diagnostics source
//! via inventory so peeps can collect roam-session diagnostics.

use std::sync::atomic::Ordering;

use peeps_types::{
    ChannelCreditSnapshot, ChannelDir, ChannelSnapshot, CompletionSnapshot, ConnectionSnapshot,
    Direction, RequestSnapshot, SessionSnapshot, TransportStats,
};
#[cfg(feature = "diagnostics")]
use peeps_types::{Diagnostics, DiagnosticsSource};

use crate::diagnostic::{self, ChannelDirection, RequestDirection, get_method_name};

impl From<RequestDirection> for Direction {
    fn from(d: RequestDirection) -> Self {
        match d {
            RequestDirection::Outgoing => Direction::Outgoing,
            RequestDirection::Incoming => Direction::Incoming,
        }
    }
}

impl From<ChannelDirection> for ChannelDir {
    fn from(d: ChannelDirection) -> Self {
        match d {
            ChannelDirection::Tx => ChannelDir::Tx,
            ChannelDirection::Rx => ChannelDir::Rx,
        }
    }
}

// Register with peeps diagnostics inventory
#[cfg(feature = "diagnostics")]
inventory::submit! {
    DiagnosticsSource {
        collect: || Diagnostics::RoamSession(snapshot_all_diagnostics()),
    }
}

/// Take a structured snapshot of all registered diagnostic states.
///
/// Safe to call from signal handlers (uses `try_read()` on all locks).
pub fn snapshot_all_diagnostics() -> SessionSnapshot {
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

    SessionSnapshot {
        connections,
        method_names,
    }
}
