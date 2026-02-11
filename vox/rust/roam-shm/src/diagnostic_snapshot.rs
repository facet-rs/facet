//! Structured SHM diagnostic snapshots for JSON serialization.
//!
//! Uses types from peeps-types and registers as a diagnostics source
//! via inventory so peeps can collect roam-shm diagnostics.

use std::sync::Arc;

use peeps_types::{
    ChannelQueueSnapshot, ShmPeerSnapshot, ShmPeerState, ShmSegmentSnapshot, ShmSnapshot,
    VarSlotClassSnapshot,
};
#[cfg(feature = "diagnostics")]
use peeps_types::{Diagnostics, DiagnosticsSource};

use crate::diagnostic::{self, SHM_DIAGNOSTIC_REGISTRY};

impl From<crate::peer::PeerState> for ShmPeerState {
    fn from(s: crate::peer::PeerState) -> Self {
        match s {
            crate::peer::PeerState::Empty => ShmPeerState::Empty,
            crate::peer::PeerState::Reserved => ShmPeerState::Reserved,
            crate::peer::PeerState::Attached => ShmPeerState::Attached,
            crate::peer::PeerState::Goodbye => ShmPeerState::Goodbye,
        }
    }
}

// Register with peeps diagnostics inventory
#[cfg(feature = "diagnostics")]
inventory::submit! {
    DiagnosticsSource {
        collect: || Diagnostics::RoamShm(snapshot_all_shm()),
    }
}

/// Take a structured snapshot of all SHM state.
///
/// Safe to call from signal handlers (uses `try_read()` on all locks).
pub fn snapshot_all_shm() -> ShmSnapshot {
    let segments = snapshot_segments();
    let channels = snapshot_channels();
    ShmSnapshot { segments, channels }
}

fn snapshot_segments() -> Vec<ShmSegmentSnapshot> {
    let views: Vec<Arc<diagnostic::ShmDiagnosticView>> = {
        let Ok(registry) = SHM_DIAGNOSTIC_REGISTRY.try_read() else {
            return Vec::new();
        };
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    };

    views
        .iter()
        .map(|view| {
            let diag = view.diagnostics();

            let peers: Vec<ShmPeerSnapshot> = diag
                .peer_slots
                .iter()
                .filter(|p| p.state == crate::peer::PeerState::Attached)
                .map(|p| {
                    let (name, bytes_sent, bytes_received, calls_sent, calls_received) =
                        match &p.tracked_state {
                            Some(t) => (
                                t.name.clone(),
                                t.bytes_sent,
                                t.bytes_received,
                                t.calls_sent,
                                t.calls_received,
                            ),
                            None => (None, 0, 0, 0, 0),
                        };
                    ShmPeerSnapshot {
                        peer_id: p.peer_id.get() as u32,
                        state: p.state.into(),
                        name,
                        bipbuf_capacity: p.host_to_guest_bipbuf.capacity,
                        bytes_sent,
                        bytes_received,
                        calls_sent,
                        calls_received,
                        time_since_heartbeat_ms: p.time_since_heartbeat_ms,
                    }
                })
                .collect();

            let var_pool: Vec<VarSlotClassSnapshot> = diag
                .var_pool
                .classes
                .iter()
                .map(|c| VarSlotClassSnapshot {
                    slot_size: c.slot_size,
                    slots_per_extent: c.slots_per_extent,
                    extent_count: c.extent_count,
                    free_slots_approx: c.free_slots_approx,
                    total_slots: c.total_slots,
                })
                .collect();

            ShmSegmentSnapshot {
                segment_path: diag.segment_path,
                total_size: diag.total_size,
                current_size: diag.current_size,
                max_peers: diag.max_peers,
                host_goodbye: diag.host_goodbye,
                peers,
                var_pool,
            }
        })
        .collect()
}

fn snapshot_channels() -> Vec<ChannelQueueSnapshot> {
    let channels = crate::auditable::collect_live_channels();
    channels
        .iter()
        .map(|ch| ChannelQueueSnapshot {
            name: ch.name().to_string(),
            len: ch.len() as u64,
            capacity: ch.capacity() as u64,
        })
        .collect()
}
