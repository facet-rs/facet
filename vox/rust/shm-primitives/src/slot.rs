use crate::sync::{AtomicU32, Ordering};

/// Slot states for generational slots.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Free = 0,
    Allocated = 1,
    InFlight = 2,
}

impl SlotState {
    #[inline]
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(SlotState::Free),
            1 => Some(SlotState::Allocated),
            2 => Some(SlotState::InFlight),
            _ => None,
        }
    }
}

/// Metadata for a single slot.
#[repr(C)]
pub struct SlotMeta {
    pub generation: AtomicU32,
    pub state: AtomicU32,
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<SlotMeta>() == 8);

/// Metadata for a variable-size slot with ownership tracking.
///
/// Used by shared variable-size slot pools where we need to track
/// which peer allocated each slot for crash recovery.
///
/// shm[impl shm.varslot.ownership]
#[repr(C)]
pub struct VarSlotMeta {
    /// ABA counter, incremented on allocation.
    pub generation: AtomicU32,
    /// Slot state: Free=0, Allocated=1, InFlight=2.
    pub state: AtomicU32,
    /// Peer ID that allocated this slot (0 = host, 1-255 = guest).
    /// Used for crash recovery: when a peer dies, its slots are reclaimed.
    pub owner_peer: AtomicU32,
    /// Free list link (next free slot index, or u32::MAX for end).
    pub next_free: AtomicU32,
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<VarSlotMeta>() == 16);

impl VarSlotMeta {
    /// Initialize a new variable slot metadata entry.
    #[inline]
    pub fn init(&mut self) {
        self.generation = AtomicU32::new(0);
        self.state = AtomicU32::new(SlotState::Free as u32);
        self.owner_peer = AtomicU32::new(0);
        self.next_free = AtomicU32::new(u32::MAX);
    }

    /// Read the current generation.
    #[inline]
    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// Read the current state.
    #[inline]
    pub fn state(&self) -> SlotState {
        SlotState::from_u32(self.state.load(Ordering::Acquire)).unwrap_or(SlotState::Free)
    }

    /// Read the owner peer ID.
    #[inline]
    pub fn owner(&self) -> u8 {
        self.owner_peer.load(Ordering::Acquire) as u8
    }

    /// Read the next free slot index.
    #[inline]
    pub fn next_free(&self) -> u32 {
        self.next_free.load(Ordering::Acquire)
    }

    /// Check if the slot matches the expected generation.
    #[inline]
    pub fn check_generation(&self, expected: u32) -> bool {
        self.generation.load(Ordering::Acquire) == expected
    }
}

impl SlotMeta {
    /// Initialize a new slot metadata entry.
    #[inline]
    pub fn init(&mut self) {
        self.generation = AtomicU32::new(0);
        self.state = AtomicU32::new(SlotState::Free as u32);
    }

    /// Attempt to transition state.
    ///
    /// Returns `Ok(generation)` on success, `Err(actual_state)` on failure.
    #[inline]
    pub fn try_transition(&self, expected: SlotState, new: SlotState) -> Result<u32, SlotState> {
        match self.state.compare_exchange(
            expected as u32,
            new as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(self.generation.load(Ordering::Acquire)),
            Err(actual) => Err(SlotState::from_u32(actual).unwrap_or(SlotState::Free)),
        }
    }

    /// Check if the slot matches the expected generation.
    #[inline]
    pub fn check_generation(&self, expected: u32) -> bool {
        self.generation.load(Ordering::Acquire) == expected
    }

    /// Read the current generation.
    #[inline]
    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// Read the current state.
    #[inline]
    pub fn state(&self) -> SlotState {
        SlotState::from_u32(self.state.load(Ordering::Acquire)).unwrap_or(SlotState::Free)
    }
}
