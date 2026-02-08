//! SHM diagnostic utilities for SIGUSR1 dumps.
//!
//! Provides functions to dump the state of shared memory segments,
//! slot pools, BipBuffers, and peer connections.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use shm_primitives::Region;

use crate::host::ShmHost;
use crate::layout::SegmentLayout;
use crate::peer::{PeerId, PeerState};
use crate::var_slot_pool::VarSlotPool;

/// Diagnostic stats for a variable slot pool (all size classes).
#[derive(Debug, Clone)]
pub struct VarSlotPoolStats {
    pub classes: Vec<VarSlotClassStats>,
}

/// Stats for a single size class in a variable slot pool.
#[derive(Debug, Clone)]
pub struct VarSlotClassStats {
    pub slot_size: u32,
    pub slots_per_extent: u32,
    pub extent_count: u32,
    pub free_slots_approx: u32,
    pub total_slots: u32,
}

/// Diagnostic stats for a BipBuffer.
#[derive(Debug, Clone)]
pub struct BipBufStats {
    pub capacity: u32,
}

/// Diagnostic stats for a single peer slot.
#[derive(Debug, Clone)]
pub struct PeerSlotStats {
    pub peer_id: PeerId,
    pub state: PeerState,
    pub epoch: u32,
    pub last_heartbeat_ns: u64,
    pub time_since_heartbeat_ms: Option<u64>,
    pub host_to_guest_bipbuf: BipBufStats,
    pub guest_to_host_bipbuf: BipBufStats,
    /// Only present for tracked guests (those we've communicated with)
    pub tracked_state: Option<TrackedGuestStats>,
}

/// Stats for a guest we're actively tracking.
#[derive(Debug, Clone)]
pub struct TrackedGuestStats {
    pub name: Option<String>,
    pub pending_slots: usize,
    pub has_doorbell: bool,
    pub death_notified: bool,
    /// Cumulative call statistics
    pub calls_sent: u64,
    pub calls_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// Full diagnostic snapshot of an SHM segment (v2).
#[derive(Debug, Clone)]
pub struct ShmDiagnostics {
    pub segment_path: Option<String>,
    pub total_size: u64,
    pub current_size: u64,
    pub max_peers: u32,
    pub var_pool: VarSlotPoolStats,
    pub peer_slots: Vec<PeerSlotStats>,
    pub host_goodbye: bool,
}

impl ShmDiagnostics {
    /// Format the diagnostics as a compact human-readable string.
    /// Only shows non-empty peers with activity.
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Count active peers
        let attached = self
            .peer_slots
            .iter()
            .filter(|p| p.state == PeerState::Attached)
            .count();
        if attached == 0 {
            return String::new(); // Nothing to show
        }

        let _ = write!(output, "[SHM] {} peers", attached);
        if self.host_goodbye {
            let _ = write!(output, " GOODBYE");
        }
        let _ = writeln!(output);

        // Only show attached peers
        for peer in &self.peer_slots {
            if peer.state != PeerState::Attached {
                continue;
            }

            // Get name if available, otherwise use peer_id
            let name = peer
                .tracked_state
                .as_ref()
                .and_then(|t| t.name.as_ref())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("peer#{}", peer.peer_id.get()));

            let _ = write!(
                output,
                "  [{}] bipbuf_cap:{}",
                name, peer.host_to_guest_bipbuf.capacity
            );

            // Show byte stats if available
            if let Some(ref tracked) = peer.tracked_state
                && (tracked.bytes_sent > 0 || tracked.bytes_received > 0)
            {
                let _ = write!(
                    output,
                    " ({}up {}down)",
                    format_bytes(tracked.bytes_sent),
                    format_bytes(tracked.bytes_received)
                );
            }

            let _ = writeln!(output);
        }

        output
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// A read-only view of an SHM segment for diagnostic purposes.
///
/// This can be extracted from an `ShmHost` before it's consumed by the driver,
/// allowing diagnostics to be collected from signal handlers without needing
/// to access the driver's internal state.
///
/// # Safety
///
/// The diagnostic view holds a copy of the Region pointer. The caller must ensure
/// that the underlying SHM segment remains valid for the lifetime of this view.
/// In practice, this is safe because the driver keeps the ShmHost alive.
pub struct ShmDiagnosticView {
    region: Region,
    layout: SegmentLayout,
    segment_path: Option<PathBuf>,
}

// SAFETY: The Region contains a pointer to memory-mapped shared memory.
// This is safe to send across threads because:
// 1. The memory region is backed by a file that remains mapped
// 2. All reads use atomic operations where needed
// 3. This view is read-only and doesn't modify the segment
unsafe impl Send for ShmDiagnosticView {}
unsafe impl Sync for ShmDiagnosticView {}

impl ShmDiagnosticView {
    /// Create a diagnostic view from an ShmHost.
    ///
    /// This should be called before the host is passed to the driver.
    pub fn from_host(host: &ShmHost) -> Self {
        Self {
            region: host.region(),
            layout: host.layout().clone(),
            segment_path: host.path().map(|p| p.to_path_buf()),
        }
    }

    /// Get a diagnostic snapshot of the SHM segment.
    ///
    /// This reads directly from shared memory and can be called from
    /// signal handlers (no async operations, no locks that could deadlock).
    pub fn diagnostics(&self) -> ShmDiagnostics {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        // Read current size from header
        let header = unsafe { &*(self.region.as_ptr() as *const crate::layout::SegmentHeader) };
        let current_size = header.current_size.load(Ordering::Acquire);
        let host_goodbye = header.host_goodbye.load(Ordering::Acquire) != 0;

        // Get variable slot pool stats
        let var_pool = {
            let offset = self.layout.var_slot_pool_offset;
            let classes = &self.layout.config.var_slot_classes;
            let pool = VarSlotPool::new(self.region, offset, classes.to_vec());
            pool.stats()
        };

        // Get peer slot diagnostics
        let bipbuf_capacity = self.layout.config.bipbuf_capacity;
        let mut peer_slots = Vec::with_capacity(self.layout.config.max_guests as usize);
        for i in 0..self.layout.config.max_guests {
            let Some(peer_id) = PeerId::from_index(i as u8) else {
                continue;
            };

            let offset = self.layout.peer_entry_offset(peer_id.get()) as usize;
            let entry = unsafe { &*(self.region.offset(offset) as *const crate::peer::PeerEntry) };

            let state = entry.state();
            let epoch = entry.epoch();
            let last_heartbeat_ns = entry.last_heartbeat();

            let time_since_heartbeat_ms = if last_heartbeat_ns > 0 && now_ns > last_heartbeat_ns {
                Some((now_ns - last_heartbeat_ns) / 1_000_000)
            } else {
                None
            };

            peer_slots.push(PeerSlotStats {
                peer_id,
                state,
                epoch,
                last_heartbeat_ns,
                time_since_heartbeat_ms,
                host_to_guest_bipbuf: BipBufStats {
                    capacity: bipbuf_capacity,
                },
                guest_to_host_bipbuf: BipBufStats {
                    capacity: bipbuf_capacity,
                },
                // No tracked state available from the view (that's in the driver)
                tracked_state: None,
            });
        }

        ShmDiagnostics {
            segment_path: self.segment_path.as_ref().map(|p| p.display().to_string()),
            total_size: self.layout.total_size,
            current_size,
            max_peers: self.layout.config.max_guests,
            var_pool,
            peer_slots,
            host_goodbye,
        }
    }
}

impl VarSlotPool {
    /// Get diagnostic stats for this variable slot pool.
    pub fn stats(&self) -> VarSlotPoolStats {
        let mut classes = Vec::with_capacity(self.class_count());
        for (i, class) in self.classes().iter().enumerate() {
            let extent_count = self.extent_count(i);
            let free_approx = self.free_count_approx(i);
            let total = class.count * extent_count;
            classes.push(VarSlotClassStats {
                slot_size: class.slot_size,
                slots_per_extent: class.count,
                extent_count,
                free_slots_approx: free_approx,
                total_slots: total,
            });
        }
        VarSlotPoolStats { classes }
    }
}

/// Call statistics tracker for a peer.
///
/// This is stored in GuestState and updated on each send/receive.
#[derive(Debug, Default)]
pub struct PeerCallStats {
    pub calls_sent: AtomicU64,
    pub calls_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
}

impl PeerCallStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_send(&self, bytes: u64) {
        self.calls_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_receive(&self, bytes: u64) {
        self.calls_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.calls_sent.load(Ordering::Relaxed),
            self.calls_received.load(Ordering::Relaxed),
            self.bytes_sent.load(Ordering::Relaxed),
            self.bytes_received.load(Ordering::Relaxed),
        )
    }
}

impl Clone for PeerCallStats {
    fn clone(&self) -> Self {
        Self {
            calls_sent: AtomicU64::new(self.calls_sent.load(Ordering::Relaxed)),
            calls_received: AtomicU64::new(self.calls_received.load(Ordering::Relaxed)),
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
        }
    }
}

impl ShmHost {
    /// Get full diagnostic snapshot.
    pub fn diagnostics(&self) -> ShmDiagnostics {
        let layout = self.layout();
        ShmDiagnostics {
            segment_path: self.path().map(|p| p.display().to_string()),
            total_size: layout.total_size,
            current_size: self.current_size_for_diagnostic(),
            max_peers: layout.config.max_guests,
            var_pool: self.var_pool_stats_for_diagnostic(),
            peer_slots: self.all_peer_diagnostics(),
            host_goodbye: self.is_goodbye(),
        }
    }

    /// Get diagnostic stats for the variable slot pool.
    fn var_pool_stats_for_diagnostic(&self) -> VarSlotPoolStats {
        let layout = self.layout();
        let var_pool_offset = layout.var_slot_pool_offset();
        let var_classes = &layout.config.var_slot_classes;
        let var_pool = VarSlotPool::new(self.region(), var_pool_offset, var_classes.to_vec());
        var_pool.stats()
    }

    /// Get diagnostic stats for ALL peer slots (not just connected ones).
    fn all_peer_diagnostics(&self) -> Vec<PeerSlotStats> {
        let layout = self.layout();
        let mut stats = Vec::with_capacity(layout.config.max_guests as usize);
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let bipbuf_capacity = layout.config.bipbuf_capacity;

        for i in 0..layout.config.max_guests {
            let Some(peer_id) = PeerId::from_index(i as u8) else {
                continue;
            };

            let entry = self.peer_entry_for_diagnostic(peer_id);
            let state = entry.state();
            let epoch = entry.epoch();
            let last_heartbeat_ns = entry.last_heartbeat();

            // Calculate time since heartbeat
            let time_since_heartbeat_ms = if last_heartbeat_ns > 0 && now_ns > last_heartbeat_ns {
                Some((now_ns - last_heartbeat_ns) / 1_000_000)
            } else {
                None
            };

            // Get tracked state if we have it
            let tracked_state = self.guests.get(&peer_id).map(|guest| {
                let (calls_sent, calls_received, bytes_sent, bytes_received) =
                    guest.stats.snapshot();
                TrackedGuestStats {
                    name: guest.name.clone(),
                    pending_slots: guest.pending_slots.len(),
                    has_doorbell: guest.doorbell.is_some(),
                    death_notified: guest.death_notified,
                    calls_sent,
                    calls_received,
                    bytes_sent,
                    bytes_received,
                }
            });

            stats.push(PeerSlotStats {
                peer_id,
                state,
                epoch,
                last_heartbeat_ns,
                time_since_heartbeat_ms,
                host_to_guest_bipbuf: BipBufStats {
                    capacity: bipbuf_capacity,
                },
                guest_to_host_bipbuf: BipBufStats {
                    capacity: bipbuf_capacity,
                },
                tracked_state,
            });
        }

        stats
    }

    /// Get a peer entry (for diagnostics only - avoids name conflict with existing peer_entry).
    fn peer_entry_for_diagnostic(&self, peer_id: PeerId) -> &crate::peer::PeerEntry {
        let layout = self.layout();
        let region = self.region();
        let offset = layout.peer_entry_offset(peer_id.get()) as usize;
        unsafe { &*(region.offset(offset) as *const crate::peer::PeerEntry) }
    }

    /// Get the current segment size from the header.
    fn current_size_for_diagnostic(&self) -> u64 {
        let region = self.region();
        let header = unsafe { &*(region.as_ptr() as *const crate::layout::SegmentHeader) };
        header.current_size.load(Ordering::Acquire)
    }
}
