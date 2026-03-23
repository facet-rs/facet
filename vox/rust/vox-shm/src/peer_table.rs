//! PeerTable — the array of peer entries in the shared memory segment.
//!
//! Sits at `segment_header.peer_table_offset` and contains one 64-byte
//! [`PeerEntry`] per potential guest (indices 0 .. max_guests-1, corresponding
//! to peer IDs 1 .. max_guests).
//!
//! The host initialises the table on segment creation; guests attach to an
//! already-initialised table.

use core::mem::size_of;

use shm_primitives::{BipBuf, PeerEntry, PeerId, Region};

/// Size of one peer entry in shared memory.
pub const PEER_ENTRY_SIZE: usize = size_of::<PeerEntry>();

/// Aligned size of a single BipBuffer (header + data), rounded up to 64-byte
/// alignment so that the *next* BipBuffer header starts 64-byte aligned.
#[inline]
pub fn bipbuf_single_stride(bipbuf_capacity: u32) -> usize {
    let raw = shm_primitives::BIPBUF_HEADER_SIZE + bipbuf_capacity as usize;
    (raw + 63) & !63
}

/// Size of the BipBuffer pair (G→H + H→G) for one guest.
///
/// r[impl shm.bipbuf.layout]
#[inline]
pub fn bipbuf_pair_size(bipbuf_capacity: u32) -> usize {
    2 * bipbuf_single_stride(bipbuf_capacity)
}

/// In-process view of the peer table.
///
/// r[impl shm.peer-table]
/// r[impl shm.topology]
/// r[impl shm.topology.bidirectional]
/// r[impl shm.topology.communication]
/// r[impl shm.topology.max-guests]
pub struct PeerTable {
    base: *mut PeerEntry,
    max_guests: u8,
}

unsafe impl Send for PeerTable {}
unsafe impl Sync for PeerTable {}

impl PeerTable {
    /// Initialise a new peer table in `region`.
    ///
    /// Writes `max_guests` peer entries starting at `base_offset`.  Each
    /// entry's `ring_offset` is set to `ring_base_offset + i * ring_stride`,
    /// where `i` is the 0-based guest index and `ring_stride` is
    /// `bipbuf_pair_size(bipbuf_capacity)`.  Both BipBuffers in each pair are
    /// also initialised.
    ///
    /// # Safety
    ///
    /// `region` must be exclusively owned and large enough.
    /// `base_offset` must be 64-byte aligned.
    ///
    /// r[impl shm.bipbuf.init]
    pub unsafe fn init(
        region: Region,
        base_offset: usize,
        max_guests: u8,
        ring_base_offset: usize,
        bipbuf_capacity: u32,
    ) -> Self {
        assert!(
            base_offset.is_multiple_of(64),
            "peer table base_offset must be 64-byte aligned"
        );

        let stride = bipbuf_pair_size(bipbuf_capacity);

        let base: *mut PeerEntry = unsafe { region.get_mut::<PeerEntry>(base_offset) };

        for i in 0..max_guests as usize {
            let ring_offset = (ring_base_offset + i * stride) as u64;

            // Init peer entry
            let entry = unsafe { &mut *base.add(i) };
            unsafe { entry.init(ring_offset) };

            // Init BipBuffer pair: G→H then H→G
            let g2h_offset = ring_base_offset + i * stride;
            let h2g_offset = g2h_offset + bipbuf_single_stride(bipbuf_capacity);

            unsafe {
                BipBuf::init(region, g2h_offset, bipbuf_capacity);
                BipBuf::init(region, h2g_offset, bipbuf_capacity);
            }
        }

        Self { base, max_guests }
    }

    /// Attach to an already-initialised peer table.
    ///
    /// # Safety
    ///
    /// The table at `base_offset` must have been initialised with the same
    /// `max_guests`.
    pub unsafe fn attach(region: Region, base_offset: usize, max_guests: u8) -> Self {
        let base: *mut PeerEntry = unsafe { region.get_mut::<PeerEntry>(base_offset) };
        Self { base, max_guests }
    }

    /// Number of guest slots in this table.
    #[inline]
    pub fn max_guests(&self) -> u8 {
        self.max_guests
    }

    /// Borrow a peer entry by [`PeerId`].
    ///
    /// # Panics
    ///
    /// Panics if `peer_id.index() >= max_guests`.
    #[inline]
    pub fn entry(&self, peer_id: PeerId) -> &PeerEntry {
        assert!(peer_id.index() < self.max_guests, "peer_id out of range");
        unsafe { &*self.base.add(peer_id.index() as usize) }
    }

    /// Find the first slot in the `Empty` state, returning its [`PeerId`].
    pub fn find_empty(&self) -> Option<PeerId> {
        for i in 0..self.max_guests {
            let entry = unsafe { &*self.base.add(i as usize) };
            if entry.state() == shm_primitives::PeerState::Empty {
                return PeerId::from_index(i);
            }
        }
        None
    }

    /// Iterate over all entries together with their peer IDs.
    pub fn iter(&self) -> impl Iterator<Item = (PeerId, &PeerEntry)> {
        (0..self.max_guests).filter_map(|i| {
            let id = PeerId::from_index(i)?;
            Some((id, unsafe { &*self.base.add(i as usize) }))
        })
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shm_primitives::{HeapRegion, PeerState};

    const MAX_GUESTS: u8 = 4;
    const BIPBUF_CAP: u32 = 1024;

    fn make_table() -> (HeapRegion, PeerTable) {
        let table_size = MAX_GUESTS as usize * PEER_ENTRY_SIZE;
        let rings_size = MAX_GUESTS as usize * bipbuf_pair_size(BIPBUF_CAP);
        // Add 64 bytes of alignment padding between regions
        let total = 64 + table_size + rings_size;
        let region = HeapRegion::new_zeroed(total);
        let table =
            unsafe { PeerTable::init(region.region(), 0, MAX_GUESTS, table_size, BIPBUF_CAP) };
        (region, table)
    }

    #[test]
    fn all_entries_start_empty() {
        let (_r, table) = make_table();
        for i in 0..MAX_GUESTS {
            let id = PeerId::from_index(i).unwrap();
            assert_eq!(table.entry(id).state(), PeerState::Empty);
        }
    }

    #[test]
    fn find_empty_returns_first_slot() {
        let (_r, table) = make_table();
        let id = table.find_empty().unwrap();
        assert_eq!(id.get(), 1);
    }

    #[test]
    fn attach_marks_slot_occupied() {
        let (_r, table) = make_table();
        let id = table.find_empty().unwrap();
        table.entry(id).try_attach().unwrap();
        assert_eq!(table.entry(id).state(), PeerState::Attached);
    }

    #[test]
    fn find_empty_skips_occupied_slots() {
        let (_r, table) = make_table();
        let id1 = table.find_empty().unwrap();
        table.entry(id1).try_attach().unwrap();
        let id2 = table.find_empty().unwrap();
        assert_ne!(id1.get(), id2.get());
    }

    #[test]
    fn find_empty_returns_none_when_full() {
        let (_r, table) = make_table();
        for i in 0..MAX_GUESTS {
            table
                .entry(PeerId::from_index(i).unwrap())
                .try_attach()
                .unwrap();
        }
        assert!(table.find_empty().is_none());
    }

    #[test]
    fn attach_via_region_matches_init() {
        let table_size = MAX_GUESTS as usize * PEER_ENTRY_SIZE;
        let rings_size = MAX_GUESTS as usize * bipbuf_pair_size(BIPBUF_CAP);
        let total = 64 + table_size + rings_size;
        let region = HeapRegion::new_zeroed(total);

        // Init
        let table =
            unsafe { PeerTable::init(region.region(), 0, MAX_GUESTS, table_size, BIPBUF_CAP) };
        let id = table.find_empty().unwrap();
        table.entry(id).try_attach().unwrap();

        // Attach (same region)
        let table2 = unsafe { PeerTable::attach(region.region(), 0, MAX_GUESTS) };
        assert_eq!(table2.entry(id).state(), PeerState::Attached);
    }

    #[test]
    fn ring_offset_is_set() {
        let table_size = MAX_GUESTS as usize * PEER_ENTRY_SIZE;
        let (_r, table) = make_table();
        let id = PeerId::from_index(0).unwrap();
        // ring_offset for guest 0 should be table_size (ring_base_offset + 0 * stride)
        assert_eq!(table.entry(id).ring_offset, table_size as u64);
    }

    #[test]
    fn iter_visits_all_entries() {
        let (_r, table) = make_table();
        let ids: Vec<_> = table.iter().map(|(id, _)| id.get()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }
}
