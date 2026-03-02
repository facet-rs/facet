use crate::sync::{AtomicU32, AtomicU64, Ordering};

/// States a peer table slot can be in.
///
/// r[impl shm.peer-table.states]
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Slot available for a new guest.
    Empty = 0,
    /// Guest is active.
    Attached = 1,
    /// Guest is shutting down or has crashed.
    Goodbye = 2,
    /// Host has reserved this slot; guest not yet attached.
    Reserved = 3,
}

impl PeerState {
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

/// One 64-byte entry in the peer table.
///
/// r[impl shm.peer-table]
#[repr(C)]
pub struct PeerEntry {
    /// Current peer state (Empty / Attached / Goodbye / Reserved).
    pub state: AtomicU32,
    /// Incremented on each attach; used as an ABA counter for crash recovery.
    pub epoch: AtomicU32,
    /// Monotonic clock reading (nanoseconds) of the last heartbeat.
    pub last_heartbeat: AtomicU64,
    /// Byte offset from the start of the segment to this guest's BipBuffer pair.
    pub ring_offset: u64,
    pub _reserved: [u8; 40],
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<PeerEntry>() == 64);

impl PeerEntry {
    /// Write initial values for a new peer entry.
    ///
    /// # Safety
    ///
    /// `self` must point into exclusively-owned, zeroed memory.
    pub unsafe fn init(&mut self, ring_offset: u64) {
        self.state = AtomicU32::new(PeerState::Empty as u32);
        self.epoch = AtomicU32::new(0);
        self.last_heartbeat = AtomicU64::new(0);
        self.ring_offset = ring_offset;
        self._reserved = [0u8; 40];
    }

    /// Read the current peer state.
    #[inline]
    pub fn state(&self) -> PeerState {
        PeerState::from_u32(self.state.load(Ordering::Acquire)).unwrap_or(PeerState::Empty)
    }

    /// Read the current epoch.
    #[inline]
    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Attempt to attach: CAS `Empty → Attached`, increment epoch.
    ///
    /// Returns `Ok(new_epoch)` on success, `Err(actual)` if the slot is not Empty.
    pub fn try_attach(&self) -> Result<u32, PeerState> {
        match self.state.compare_exchange(
            PeerState::Empty as u32,
            PeerState::Attached as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(self.epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1)),
            Err(actual) => Err(PeerState::from_u32(actual).unwrap_or(PeerState::Empty)),
        }
    }

    /// Reserve this slot before spawning a guest: CAS `Empty → Reserved`, increment epoch.
    ///
    /// Returns `Ok(new_epoch)` on success, `Err(actual)` if the slot is not Empty.
    pub fn try_reserve(&self) -> Result<u32, PeerState> {
        match self.state.compare_exchange(
            PeerState::Empty as u32,
            PeerState::Reserved as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(self.epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1)),
            Err(actual) => Err(PeerState::from_u32(actual).unwrap_or(PeerState::Empty)),
        }
    }

    /// Claim a reserved slot: CAS `Reserved → Attached`.
    ///
    /// Called by a spawned guest to complete the attach handshake.
    /// Returns `Ok(())` on success, `Err(actual)` if the slot is not Reserved.
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

    /// Release a reserved slot back to Empty (called by host if spawn fails).
    pub fn release_reserved(&self) {
        let _ = self.state.compare_exchange(
            PeerState::Reserved as u32,
            PeerState::Empty as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Mark this slot as Goodbye (orderly detach or crash detected by host).
    #[inline]
    pub fn set_goodbye(&self) {
        self.state
            .store(PeerState::Goodbye as u32, Ordering::Release);
    }

    /// Reset to Empty (crash recovery — called by host after reclaiming slots).
    #[inline]
    pub fn reset(&self) {
        self.last_heartbeat.store(0, Ordering::Release);
        self.state.store(PeerState::Empty as u32, Ordering::Release);
    }

    /// Write the current heartbeat timestamp.
    ///
    /// `timestamp_ns` must be a monotonic clock reading in nanoseconds.
    ///
    /// r[impl shm.crash.heartbeat-clock]
    #[inline]
    pub fn update_heartbeat(&self, timestamp_ns: u64) {
        self.last_heartbeat.store(timestamp_ns, Ordering::Release);
    }

    /// Read the last recorded heartbeat timestamp.
    #[inline]
    pub fn last_heartbeat(&self) -> u64 {
        self.last_heartbeat.load(Ordering::Acquire)
    }

    /// Return true if the heartbeat is stale (peer likely crashed).
    ///
    /// A heartbeat is stale when `current_time_ns - last_heartbeat > 2 * interval_ns`.
    /// Always returns false when `interval_ns == 0` (heartbeats disabled).
    #[inline]
    pub fn is_heartbeat_stale(&self, current_time_ns: u64, interval_ns: u64) -> bool {
        if interval_ns == 0 {
            return false;
        }
        current_time_ns.saturating_sub(self.last_heartbeat()) > 2 * interval_ns
    }
}

// ── PeerId ────────────────────────────────────────────────────────────────────

/// A peer ID in the range 1–255 (host is 0, guests are 1–255).
///
/// r[impl shm.topology.peer-id]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(u8);

impl PeerId {
    /// Construct from a 0-based table index.  Returns `None` if index ≥ 255.
    #[inline]
    pub fn from_index(index: u8) -> Option<Self> {
        if index < 255 {
            Some(Self(index + 1))
        } else {
            None
        }
    }

    /// Construct from the raw peer ID value (1–255).  Returns `None` if 0.
    #[inline]
    pub fn new(value: u8) -> Option<Self> {
        if value >= 1 { Some(Self(value)) } else { None }
    }

    /// Raw peer ID value (1–255).
    #[inline]
    pub fn get(self) -> u8 {
        self.0
    }

    /// 0-based index into the peer table.
    #[inline]
    pub fn index(self) -> u8 {
        self.0 - 1
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    fn make_entry(ring_offset: u64) -> PeerEntry {
        // Safety: zeroed stack value is valid for init.
        let mut entry: PeerEntry = unsafe { core::mem::zeroed() };
        unsafe { entry.init(ring_offset) };
        entry
    }

    #[test]
    fn initial_state_is_empty() {
        let e = make_entry(0);
        assert_eq!(e.state(), PeerState::Empty);
        assert_eq!(e.epoch(), 0);
    }

    #[test]
    fn try_attach_transitions_and_bumps_epoch() {
        let e = make_entry(0);
        let epoch = e.try_attach().unwrap();
        assert_eq!(epoch, 1);
        assert_eq!(e.state(), PeerState::Attached);
    }

    #[test]
    fn try_attach_fails_when_not_empty() {
        let e = make_entry(0);
        e.try_attach().unwrap();
        assert!(e.try_attach().is_err());
    }

    #[test]
    fn reserve_then_claim() {
        let e = make_entry(0);
        let epoch = e.try_reserve().unwrap();
        assert_eq!(epoch, 1);
        assert_eq!(e.state(), PeerState::Reserved);
        e.try_claim_reserved().unwrap();
        assert_eq!(e.state(), PeerState::Attached);
        // epoch does not increment on claim
        assert_eq!(e.epoch(), 1);
    }

    #[test]
    fn release_reserved_returns_to_empty() {
        let e = make_entry(0);
        e.try_reserve().unwrap();
        e.release_reserved();
        assert_eq!(e.state(), PeerState::Empty);
    }

    #[test]
    fn set_goodbye() {
        let e = make_entry(0);
        e.try_attach().unwrap();
        e.set_goodbye();
        assert_eq!(e.state(), PeerState::Goodbye);
    }

    #[test]
    fn reset_clears_to_empty() {
        let e = make_entry(0);
        e.try_attach().unwrap();
        e.update_heartbeat(999_999);
        e.reset();
        assert_eq!(e.state(), PeerState::Empty);
        assert_eq!(e.last_heartbeat(), 0);
        // epoch is not reset — it keeps incrementing across crashes
        assert_eq!(e.epoch(), 1);
    }

    #[test]
    fn heartbeat_stale_detection() {
        let e = make_entry(0);
        let interval = 1_000_000_000u64; // 1 s
        e.update_heartbeat(0);
        // 2.5 s elapsed → stale
        assert!(e.is_heartbeat_stale(2_500_000_000, interval));
        // 1.9 s elapsed → not stale
        assert!(!e.is_heartbeat_stale(1_900_000_000, interval));
    }

    #[test]
    fn heartbeat_disabled_never_stale() {
        let e = make_entry(0);
        e.update_heartbeat(0);
        assert!(!e.is_heartbeat_stale(u64::MAX, 0));
    }

    #[test]
    fn peer_id_roundtrip() {
        let id = PeerId::from_index(0).unwrap();
        assert_eq!(id.get(), 1);
        assert_eq!(id.index(), 0);

        let id = PeerId::new(5).unwrap();
        assert_eq!(id.get(), 5);
        assert_eq!(id.index(), 4);
    }

    #[test]
    fn peer_id_bounds() {
        assert!(PeerId::from_index(254).is_some()); // max index → peer 255
        assert!(PeerId::from_index(255).is_none()); // overflow
        assert!(PeerId::new(0).is_none()); // host id not valid
    }
}
