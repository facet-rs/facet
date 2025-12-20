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

#[cfg(not(feature = "loom"))]
const _: () = assert!(core::mem::size_of::<SlotMeta>() == 8);

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
