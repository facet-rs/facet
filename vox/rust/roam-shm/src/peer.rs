//! Peer table types.
//!
//! Defines the peer entry structure and peer state machine.

use core::mem::size_of;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Peer states.
///
/// shm[impl shm.segment.peer-state]
/// shm[impl shm.spawn.reserved-state]
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Slot available for a new guest
    Empty = 0,
    /// Guest is active
    Attached = 1,
    /// Guest is shutting down or has crashed
    Goodbye = 2,
    /// Host has reserved this slot for a spawned guest
    ///
    /// shm[impl shm.spawn.reserved-state]
    Reserved = 3,
}

impl PeerState {
    /// Convert from u32, returning None for invalid values.
    #[inline]
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(PeerState::Empty),
            1 => Some(PeerState::Attached),
            2 => Some(PeerState::Goodbye),
            3 => Some(PeerState::Reserved),
            _ => None,
        }
    }
}

/// Peer table entry (64 bytes).
///
/// shm[impl shm.segment.peer-table]
#[repr(C)]
pub struct PeerEntry {
    /// Peer state (Empty, Attached, Goodbye)
    pub state: AtomicU32,
    /// Epoch counter, incremented on attach
    ///
    /// shm[impl shm.crash.epoch]
    pub epoch: AtomicU32,
    /// Guest→Host ring head (guest writes)
    pub guest_to_host_head: AtomicU32,
    /// Guest→Host ring tail (host reads)
    pub guest_to_host_tail: AtomicU32,
    /// Host→Guest ring head (host writes)
    pub host_to_guest_head: AtomicU32,
    /// Host→Guest ring tail (guest reads)
    pub host_to_guest_tail: AtomicU32,
    /// Last heartbeat monotonic timestamp
    ///
    /// shm[impl shm.crash.heartbeat]
    /// shm[impl shm.crash.heartbeat-clock]
    pub last_heartbeat: AtomicU64,
    /// Offset to this guest's descriptor rings
    pub ring_offset: u64,
    /// Offset to this guest's slot pool
    pub slot_pool_offset: u64,
    /// Offset to this guest's channel table
    pub channel_table_offset: u64,
    /// Reserved (zero)
    pub reserved: [u8; 8],
}

const _: () = assert!(size_of::<PeerEntry>() == 64);

impl PeerEntry {
    /// Initialize a peer entry for a specific guest.
    ///
    /// Sets state to Empty and all indices to zero.
    pub fn init(&mut self, ring_offset: u64, slot_pool_offset: u64, channel_table_offset: u64) {
        self.state = AtomicU32::new(PeerState::Empty as u32);
        self.epoch = AtomicU32::new(0);
        self.guest_to_host_head = AtomicU32::new(0);
        self.guest_to_host_tail = AtomicU32::new(0);
        self.host_to_guest_head = AtomicU32::new(0);
        self.host_to_guest_tail = AtomicU32::new(0);
        self.last_heartbeat = AtomicU64::new(0);
        self.ring_offset = ring_offset;
        self.slot_pool_offset = slot_pool_offset;
        self.channel_table_offset = channel_table_offset;
        self.reserved = [0; 8];
    }

    /// Get the current peer state.
    #[inline]
    pub fn state(&self) -> PeerState {
        PeerState::from_u32(self.state.load(Ordering::Acquire)).unwrap_or(PeerState::Empty)
    }

    /// Get the current epoch.
    #[inline]
    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Attempt to transition from Empty to Attached.
    ///
    /// shm[impl shm.guest.attach]
    ///
    /// Returns `Ok(new_epoch)` on success, `Err(actual_state)` on failure.
    pub fn try_attach(&self) -> Result<u32, PeerState> {
        match self.state.compare_exchange(
            PeerState::Empty as u32,
            PeerState::Attached as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // Increment epoch
                let new_epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
                Ok(new_epoch)
            }
            Err(actual) => Err(PeerState::from_u32(actual).unwrap_or(PeerState::Empty)),
        }
    }

    /// Reserve this slot for a spawned guest.
    ///
    /// Called by the host before spawning a guest process.
    /// Returns `Ok(new_epoch)` if successful, `Err(actual_state)` if slot not empty.
    ///
    /// shm[impl shm.spawn.reserved-state]
    pub fn try_reserve(&self) -> Result<u32, PeerState> {
        match self.state.compare_exchange(
            PeerState::Empty as u32,
            PeerState::Reserved as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // Increment epoch
                let new_epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
                Ok(new_epoch)
            }
            Err(actual) => Err(PeerState::from_u32(actual).unwrap_or(PeerState::Empty)),
        }
    }

    /// Claim a reserved slot (transition Reserved -> Attached).
    ///
    /// Called by a spawned guest to claim its pre-assigned slot.
    /// Returns `Ok(())` if successful, `Err(actual_state)` if not reserved.
    ///
    /// shm[impl shm.spawn.guest-init]
    pub fn try_claim_reserved(&self) -> Result<(), PeerState> {
        match self.state.compare_exchange(
            PeerState::Reserved as u32,
            PeerState::Attached as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(actual) => Err(PeerState::from_u32(actual).unwrap_or(PeerState::Empty)),
        }
    }

    /// Release a reserved slot back to Empty.
    ///
    /// Called by the host if spawn fails.
    pub fn release_reserved(&self) {
        // Only release if still reserved (avoid racing with other transitions)
        let _ = self.state.compare_exchange(
            PeerState::Reserved as u32,
            PeerState::Empty as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Set state to Goodbye.
    ///
    /// shm[impl shm.guest.detach]
    /// shm[impl shm.goodbye.guest]
    #[inline]
    pub fn set_goodbye(&self) {
        self.state
            .store(PeerState::Goodbye as u32, Ordering::Release);
    }

    /// Reset to Empty state (for crash recovery).
    ///
    /// shm[impl shm.crash.recovery]
    pub fn reset(&self) {
        self.guest_to_host_head.store(0, Ordering::Release);
        self.guest_to_host_tail.store(0, Ordering::Release);
        self.host_to_guest_head.store(0, Ordering::Release);
        self.host_to_guest_tail.store(0, Ordering::Release);
        self.last_heartbeat.store(0, Ordering::Release);
        self.state.store(PeerState::Empty as u32, Ordering::Release);
    }

    /// Update the heartbeat timestamp.
    #[inline]
    pub fn update_heartbeat(&self, timestamp_ns: u64) {
        self.last_heartbeat.store(timestamp_ns, Ordering::Release);
    }

    /// Get the last heartbeat timestamp.
    #[inline]
    pub fn last_heartbeat(&self) -> u64 {
        self.last_heartbeat.load(Ordering::Acquire)
    }

    /// Check if this peer appears crashed based on heartbeat.
    ///
    /// shm[impl shm.crash.heartbeat]
    ///
    /// Returns true if heartbeat is stale by more than 2 * heartbeat_interval.
    #[inline]
    pub fn is_heartbeat_stale(&self, current_time_ns: u64, heartbeat_interval_ns: u64) -> bool {
        if heartbeat_interval_ns == 0 {
            return false; // Heartbeat disabled
        }
        let last = self.last_heartbeat();
        current_time_ns.saturating_sub(last) > 2 * heartbeat_interval_ns
    }

    // Ring index accessors for the host side

    /// Guest→Host ring: get tail (host reads from here)
    #[inline]
    pub fn g2h_tail(&self) -> u32 {
        self.guest_to_host_tail.load(Ordering::Acquire)
    }

    /// Guest→Host ring: get visible head (guest has written up to here)
    #[inline]
    pub fn g2h_head(&self) -> u32 {
        self.guest_to_host_head.load(Ordering::Acquire)
    }

    /// Guest→Host ring: advance tail (host consumed a message)
    ///
    /// shm[impl shm.ordering.ring-consume]
    #[inline]
    pub fn g2h_advance_tail(&self, new_tail: u32) {
        self.guest_to_host_tail.store(new_tail, Ordering::Release);
    }

    /// Host→Guest ring: get head (host writes here)
    #[inline]
    pub fn h2g_head(&self) -> u32 {
        self.host_to_guest_head.load(Ordering::Relaxed)
    }

    /// Host→Guest ring: get tail (guest has consumed up to here)
    #[inline]
    pub fn h2g_tail(&self) -> u32 {
        self.host_to_guest_tail.load(Ordering::Acquire)
    }

    /// Host→Guest ring: publish head (host wrote a message)
    ///
    /// shm[impl shm.ordering.ring-publish]
    #[inline]
    pub fn h2g_publish_head(&self, new_head: u32) {
        self.host_to_guest_head.store(new_head, Ordering::Release);
    }

    // Ring index accessors for the guest side

    /// Guest→Host ring: get local head for guest (guest writes here)
    /// Note: Guest maintains local_head on stack, this is for initialization
    #[inline]
    pub fn g2h_local_head(&self) -> u32 {
        self.guest_to_host_head.load(Ordering::Relaxed)
    }

    /// Guest→Host ring: publish head (guest wrote a message)
    ///
    /// shm[impl shm.ordering.ring-publish]
    #[inline]
    pub fn g2h_publish_head(&self, new_head: u32) {
        self.guest_to_host_head.store(new_head, Ordering::Release);
    }

    /// Host→Guest ring: advance tail (guest consumed a message)
    ///
    /// shm[impl shm.ordering.ring-consume]
    #[inline]
    pub fn h2g_advance_tail(&self, new_tail: u32) {
        self.host_to_guest_tail.store(new_tail, Ordering::Release);
    }
}

/// A peer ID (1-255).
///
/// shm[impl shm.topology.peer-id]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(u8);

impl PeerId {
    /// Create a new peer ID from an index (0-based).
    ///
    /// Returns None if index >= 255.
    #[inline]
    pub fn from_index(index: u8) -> Option<Self> {
        if index < 255 {
            Some(Self(index + 1))
        } else {
            None
        }
    }

    /// Create a peer ID from the raw value (1-255).
    ///
    /// Returns None if value is 0 or would overflow.
    #[inline]
    pub fn new(value: u8) -> Option<Self> {
        if value >= 1 { Some(Self(value)) } else { None }
    }

    /// Get the raw peer ID value (1-255).
    #[inline]
    pub fn get(self) -> u8 {
        self.0
    }

    /// Get the index (0-based) for this peer ID.
    #[inline]
    pub fn index(self) -> u8 {
        self.0 - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_entry_is_64_bytes() {
        assert_eq!(size_of::<PeerEntry>(), 64);
    }

    #[test]
    fn peer_state_roundtrip() {
        assert_eq!(PeerState::from_u32(0), Some(PeerState::Empty));
        assert_eq!(PeerState::from_u32(1), Some(PeerState::Attached));
        assert_eq!(PeerState::from_u32(2), Some(PeerState::Goodbye));
        assert_eq!(PeerState::from_u32(3), Some(PeerState::Reserved));
        assert_eq!(PeerState::from_u32(4), None);
    }

    #[test]
    fn peer_id_conversion() {
        let id = PeerId::from_index(0).unwrap();
        assert_eq!(id.get(), 1);
        assert_eq!(id.index(), 0);

        let id = PeerId::from_index(254).unwrap();
        assert_eq!(id.get(), 255);
        assert_eq!(id.index(), 254);

        assert!(PeerId::from_index(255).is_none());
        assert!(PeerId::new(0).is_none());
    }
}
