//! Structured SHM diagnostic snapshots for JSON serialization.

use std::sync::Arc;

use facet::Facet;

use crate::diagnostic::{self, SHM_DIAGNOSTIC_REGISTRY};

/// Snapshot of all SHM diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct ShmSnapshot {
    pub segments: Vec<ShmSegmentSnapshot>,
    pub channels: Vec<ChannelQueueSnapshot>,
}

/// Snapshot of a single SHM segment.
#[derive(Debug, Clone, Facet)]
pub struct ShmSegmentSnapshot {
    pub segment_path: Option<String>,
    pub total_size: u64,
    pub current_size: u64,
    pub max_peers: u32,
    pub host_goodbye: bool,
    pub peers: Vec<ShmPeerSnapshot>,
    pub var_pool: Vec<VarSlotClassSnapshot>,
}

/// Snapshot of a single SHM peer.
#[derive(Debug, Clone, Facet)]
pub struct ShmPeerSnapshot {
    pub peer_id: u32,
    pub state: ShmPeerState,
    pub name: Option<String>,
    pub bipbuf_capacity: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub calls_sent: u64,
    pub calls_received: u64,
    pub time_since_heartbeat_ms: Option<u64>,
}

/// SHM peer state (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ShmPeerState {
    Empty,
    Reserved,
    Attached,
    Goodbye,
    Unknown,
}

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

/// Snapshot of a var slot pool size class.
#[derive(Debug, Clone, Facet)]
pub struct VarSlotClassSnapshot {
    pub slot_size: u32,
    pub slots_per_extent: u32,
    pub extent_count: u32,
    pub free_slots_approx: u32,
    pub total_slots: u32,
}

/// Snapshot of an auditable channel queue.
#[derive(Debug, Clone, Facet)]
pub struct ChannelQueueSnapshot {
    pub name: String,
    pub len: u64,
    pub capacity: u64,
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
