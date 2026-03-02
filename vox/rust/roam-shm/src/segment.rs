//! High-level `Segment` — the top-level handle for a roam SHM segment.
//!
//! Combines the mmap region, segment header, peer table, and VarSlotPool
//! into a single owned type for both host and guest use.
//!
//! r[impl shm]
//! r[impl shm.architecture]

use std::io;
use std::path::Path;

use shm_primitives::{
    BipBuf, FileCleanup, MmapRegion, PeerId, PeerState, SEGMENT_HEADER_SIZE, SegmentHeader,
    SegmentHeaderInit,
};

use crate::framing::{self, OwnedFrame};
use crate::peer_table::{PeerTable, bipbuf_pair_size};
use crate::varslot::{SizeClassConfig, VarSlotPool};

// ── layout ─────────────────────────────────────────────────────────────────

const fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

/// Computed byte offsets for a segment's sub-structures.
pub struct SegmentLayout {
    /// Byte offset of the peer table (= SEGMENT_HEADER_SIZE = 128).
    pub peer_table_offset: usize,
    /// Byte offset of the BipBuffer pairs within the segment.
    pub ring_base_offset: usize,
    /// Byte offset of the VarSlotPool.
    pub var_pool_offset: usize,
    /// Total segment size in bytes.
    pub total_size: usize,
}

impl SegmentLayout {
    pub fn compute(max_guests: u8, bipbuf_capacity: u32, size_classes: &[SizeClassConfig]) -> Self {
        let peer_table_offset = SEGMENT_HEADER_SIZE; // 128, already 64-byte aligned
        let peer_entries_size = max_guests as usize * 64; // PeerEntry is 64 bytes
        let ring_base_offset = peer_table_offset + peer_entries_size;
        // ring_base is 64-byte aligned: 128 + N*64 is always a multiple of 64
        let rings_size = max_guests as usize * bipbuf_pair_size(bipbuf_capacity);
        let var_pool_offset = align_up(ring_base_offset + rings_size, 64);
        let pool_size = VarSlotPool::required_size(size_classes);
        let total_size = var_pool_offset + pool_size;

        Self {
            peer_table_offset,
            ring_base_offset,
            var_pool_offset,
            total_size,
        }
    }
}

// ── SegmentConfig ──────────────────────────────────────────────────────────

/// Parameters for creating a new segment (host side).
pub struct SegmentConfig<'a> {
    /// Maximum number of concurrent guest processes.
    pub max_guests: u8,
    /// BipBuffer data capacity per direction, per guest, in bytes.
    pub bipbuf_capacity: u32,
    /// Maximum total payload size (written into header for guest reference).
    pub max_payload_size: u32,
    /// Inline threshold (0 = default 256).
    pub inline_threshold: u32,
    /// Heartbeat interval in nanoseconds; 0 = heartbeats disabled.
    pub heartbeat_interval: u64,
    /// VarSlotPool size classes.
    pub size_classes: &'a [SizeClassConfig],
}

// ── AttachError ────────────────────────────────────────────────────────────

/// Errors that can occur when attaching to an existing segment.
#[derive(Debug)]
pub enum AttachError {
    Io(io::Error),
    BadHeader(&'static str),
}

impl From<io::Error> for AttachError {
    fn from(e: io::Error) -> Self {
        AttachError::Io(e)
    }
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::Io(e) => write!(f, "I/O error: {e}"),
            AttachError::BadHeader(msg) => write!(f, "bad segment header: {msg}"),
        }
    }
}

impl std::error::Error for AttachError {}

// ── Segment ────────────────────────────────────────────────────────────────

/// A roam SHM segment — the top-level handle tying together all sub-structures.
///
/// On the host side, create with [`Segment::create`].
/// On the guest side, attach with [`Segment::attach`].
pub struct Segment {
    mmap: MmapRegion,
    header: *mut SegmentHeader,
    #[allow(dead_code)]
    max_guests: u8,
    bipbuf_capacity: u32,
    peer_table: PeerTable,
    var_pool: VarSlotPool,
    #[allow(dead_code)]
    size_classes: Vec<SizeClassConfig>,
}

unsafe impl Send for Segment {}
unsafe impl Sync for Segment {}

impl Segment {
    fn refresh_views_after_remap(&mut self) {
        let region = self.mmap.region();
        self.header = unsafe { region.get_mut::<SegmentHeader>(0) };

        let peer_table_offset = self.header().peer_table_offset as usize;
        let var_pool_offset = self.header().var_pool_offset as usize;
        self.bipbuf_capacity = self.header().bipbuf_capacity;

        self.peer_table = unsafe { PeerTable::attach(region, peer_table_offset, self.max_guests) };
        self.var_pool = unsafe { VarSlotPool::attach(region, var_pool_offset, &self.size_classes) };
    }

    /// Create a new segment file at `path` and initialize all sub-structures.
    ///
    /// r[impl shm.segment]
    pub fn create(
        path: &Path,
        config: SegmentConfig<'_>,
        cleanup: FileCleanup,
    ) -> io::Result<Self> {
        let layout = SegmentLayout::compute(
            config.max_guests,
            config.bipbuf_capacity,
            config.size_classes,
        );

        let mut mmap = MmapRegion::create(path, layout.total_size, cleanup)?;
        let region = mmap.region();

        // Initialize segment header.
        let header: *mut SegmentHeader = unsafe { region.get_mut::<SegmentHeader>(0) };
        unsafe {
            (*header).init(SegmentHeaderInit {
                total_size: layout.total_size as u64,
                max_payload_size: config.max_payload_size,
                inline_threshold: config.inline_threshold,
                max_guests: config.max_guests as u32,
                bipbuf_capacity: config.bipbuf_capacity,
                peer_table_offset: layout.peer_table_offset as u64,
                var_pool_offset: layout.var_pool_offset as u64,
                heartbeat_interval: config.heartbeat_interval,
                num_var_slot_classes: config.size_classes.len() as u32,
            });
        }

        // Initialize peer table and BipBuffer pairs.
        let peer_table = unsafe {
            PeerTable::init(
                region,
                layout.peer_table_offset,
                config.max_guests,
                layout.ring_base_offset,
                config.bipbuf_capacity,
            )
        };

        // Initialize VarSlotPool.
        let var_pool =
            unsafe { VarSlotPool::init(region, layout.var_pool_offset, config.size_classes) };

        // If Auto cleanup, ownership transferred to mmap; keep it live.
        // For Manual, we own the file.
        if cleanup == FileCleanup::Manual {
            mmap.take_ownership();
        }

        Ok(Self {
            mmap,
            header,
            max_guests: config.max_guests,
            bipbuf_capacity: config.bipbuf_capacity,
            peer_table,
            var_pool,
            size_classes: config.size_classes.to_vec(),
        })
    }

    /// Attach to an existing segment file at `path`.
    ///
    /// r[impl shm.guest.attach]
    pub fn attach(path: &Path) -> Result<Self, AttachError> {
        let mmap = MmapRegion::attach(path)?;
        let region = mmap.region();

        // Validate the header.
        let header: *mut SegmentHeader = unsafe { region.get_mut::<SegmentHeader>(0) };
        unsafe { &*header }
            .validate()
            .map_err(AttachError::BadHeader)?;

        let max_guests = unsafe { (*header).max_guests as u8 };
        let bipbuf_capacity = unsafe { (*header).bipbuf_capacity };
        let peer_table_offset = unsafe { (*header).peer_table_offset as usize };
        let var_pool_offset = unsafe { (*header).var_pool_offset as usize };
        let num_var_slot_classes = unsafe { (*header).num_var_slot_classes };

        let size_classes =
            unsafe { VarSlotPool::discover_configs(region, var_pool_offset, num_var_slot_classes) }
                .map_err(AttachError::BadHeader)?;

        let peer_table = unsafe { PeerTable::attach(region, peer_table_offset, max_guests) };
        let var_pool = unsafe { VarSlotPool::attach(region, var_pool_offset, &size_classes) };

        Ok(Self {
            mmap,
            header,
            max_guests,
            bipbuf_capacity,
            peer_table,
            var_pool,
            size_classes,
        })
    }

    /// Access the segment header.
    #[inline]
    pub fn header(&self) -> &SegmentHeader {
        unsafe { &*self.header }
    }

    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.mmap.as_raw_fd()
    }

    pub fn path(&self) -> &Path {
        self.mmap.path()
    }

    /// Access the peer table.
    #[inline]
    pub fn peer_table(&self) -> &PeerTable {
        &self.peer_table
    }

    /// Access the VarSlotPool.
    #[inline]
    pub fn var_pool(&self) -> &VarSlotPool {
        &self.var_pool
    }

    // ── peer lifecycle ──────────────────────────────────────────────────────

    /// Reserve an empty peer table slot for an about-to-be-spawned guest.
    ///
    /// Returns the assigned `PeerId`, or `None` if all slots are occupied.
    ///
    /// r[impl shm.spawn]
    pub fn reserve_peer(&self) -> Option<PeerId> {
        let peer_id = self.peer_table.find_empty()?;
        self.peer_table.entry(peer_id).try_reserve().ok()?;
        Some(peer_id)
    }

    /// Release a reserved slot if the spawn fails before the guest could claim it.
    pub fn release_reserved_peer(&self, peer_id: PeerId) {
        self.peer_table.entry(peer_id).release_reserved();
    }

    /// Claim a Reserved slot from the guest side (called by the spawned guest).
    ///
    /// Returns `Err(actual)` if the slot is not in the Reserved state.
    ///
    /// r[impl shm.guest.attach]
    pub fn claim_peer(&self, peer_id: PeerId) -> Result<(), PeerState> {
        self.peer_table.entry(peer_id).try_claim_reserved()
    }

    /// Attach to any empty slot (walk-in guest, no prior reservation).
    ///
    /// Returns the assigned `PeerId`, or `None` if no empty slot exists.
    ///
    /// r[impl shm.guest.attach]
    /// r[impl shm.guest.attach-failure]
    pub fn attach_peer(&self) -> Option<PeerId> {
        let peer_id = self.peer_table.find_empty()?;
        self.peer_table.entry(peer_id).try_attach().ok()?;
        Some(peer_id)
    }

    /// Mark a peer as detaching (graceful detach — step 1).
    ///
    /// r[impl shm.guest.detach]
    pub fn detach_peer(&self, peer_id: PeerId) {
        self.peer_table.entry(peer_id).set_goodbye();
    }

    /// Signal host shutdown to all guests by setting `host_goodbye`.
    ///
    /// r[impl shm.host.goodbye]
    pub fn set_host_goodbye(&self) {
        self.header()
            .host_goodbye
            .store(1, shm_primitives::sync::Ordering::Release);
    }

    // ── crash detection helpers ─────────────────────────────────────────────

    /// Return true if the peer's heartbeat is stale.
    ///
    /// r[impl shm.crash.detection]
    pub fn is_peer_heartbeat_stale(&self, peer_id: PeerId, current_ns: u64) -> bool {
        let interval = self.header().heartbeat_interval;
        self.peer_table
            .entry(peer_id)
            .is_heartbeat_stale(current_ns, interval)
    }

    // ── crash recovery ──────────────────────────────────────────────────────

    /// Perform all crash recovery steps for a dead guest.
    ///
    /// Must only be called by the host after confirming the peer has crashed.
    ///
    /// r[impl shm.crash.recovery]
    pub fn recover_crashed_peer(&self, peer_id: PeerId) {
        let entry = self.peer_table.entry(peer_id);

        // Step 1: mark as Goodbye.
        entry.set_goodbye();

        // Step 2: scan H2G BipBuffer for SLOT_REF/MMAP_REF frames; free slots.
        // This MUST happen before step 3 (resetting the buffer destroys content).
        // r[impl shm.mmap.crash-recovery]
        {
            let h2g = self.h2g_bipbuf(peer_id);
            let (_, mut consumer) = h2g.split();
            while let Some(frame) = framing::read_frame(&mut consumer) {
                match frame {
                    OwnedFrame::SlotRef(slot_ref) => {
                        let _ = self.var_pool.free(slot_ref);
                    }
                    OwnedFrame::MmapRef(_) => {
                        // MmapRef frames reference regions allocated by the crashed peer.
                        // Those regions are gone with the process — just drain the frame.
                        // Per-link mmap leases are cleaned up when the ShmLink is dropped.
                    }
                    OwnedFrame::Inline(_) => {}
                }
            }
            // consumer dropped here — borrow of h2g ends.
            h2g.reset();
        }

        // Step 3 (continued): reset G2H BipBuffer.
        self.g2h_bipbuf(peer_id).reset();

        // Step 4: reclaim all VarSlotPool slots owned by this peer.
        self.var_pool.reclaim_peer_slots(peer_id.get());

        // Step 5: return slot to Empty so a new guest can attach.
        entry.reset();
    }

    // ── extent growth ───────────────────────────────────────────────────────

    /// Grow the segment to `new_size` bytes and publish the new size.
    ///
    /// After this returns, the caller MUST signal every attached guest's
    /// doorbell so they remap and see the new extent.
    ///
    /// r[impl shm.varslot.extents.notification]
    pub fn grow_segment(&mut self, new_size: usize) -> io::Result<()> {
        // Step 1: truncate/grow the backing file and remap.
        self.mmap.resize(new_size)?;
        self.refresh_views_after_remap();

        // Step 2: publish new size with Release ordering.
        self.header()
            .current_size
            .store(new_size as u64, shm_primitives::sync::Ordering::Release);

        Ok(())
    }

    /// Check if the backing file has grown and remap if needed (guest side).
    ///
    /// Returns `true` if the mapping was extended.
    ///
    /// r[impl shm.varslot.extents.notification]
    pub fn check_and_remap(&mut self) -> io::Result<bool> {
        let published = self
            .header()
            .current_size
            .load(shm_primitives::sync::Ordering::Acquire);
        let mapped = self.mmap.len();
        if published as usize > mapped {
            self.mmap.resize(published as usize)?;
            self.refresh_views_after_remap();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ── BipBuffer access ────────────────────────────────────────────────────

    /// Guest-to-host BipBuffer for the given peer.
    ///
    /// The guest writes into this; the host reads from it.
    pub fn g2h_bipbuf(&self, peer_id: PeerId) -> BipBuf {
        let entry = self.peer_table.entry(peer_id);
        let g2h_offset = entry.ring_offset as usize;
        let region = self.mmap.region();
        unsafe { BipBuf::attach(region, g2h_offset) }
    }

    /// Host-to-guest BipBuffer for the given peer.
    ///
    /// The host writes into this; the guest reads from it.
    pub fn h2g_bipbuf(&self, peer_id: PeerId) -> BipBuf {
        let entry = self.peer_table.entry(peer_id);
        let g2h_offset = entry.ring_offset as usize;
        let h2g_offset = g2h_offset + crate::peer_table::bipbuf_single_stride(self.bipbuf_capacity);
        let region = self.mmap.region();
        unsafe { BipBuf::attach(region, h2g_offset) }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::path::PathBuf;

    use shm_primitives::{FileCleanup, MAGIC, MmapRegion, PeerState};

    use super::{AttachError, Segment, SegmentConfig, SegmentLayout};
    use crate::varslot::SizeClassConfig;

    fn test_size_classes() -> [SizeClassConfig; 2] {
        [
            SizeClassConfig {
                slot_size: 1024,
                slot_count: 8,
            },
            SizeClassConfig {
                slot_size: 16384,
                slot_count: 4,
            },
        ]
    }

    fn test_path(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join(name);
        (dir, path)
    }

    fn make_config<'a>(size_classes: &'a [SizeClassConfig]) -> SegmentConfig<'a> {
        SegmentConfig {
            max_guests: 4,
            bipbuf_capacity: 4096,
            max_payload_size: 64 * 1024,
            inline_threshold: 0,
            heartbeat_interval: 1_000_000,
            size_classes,
        }
    }

    #[test]
    fn layout_compute_produces_aligned_monotonic_offsets() {
        let size_classes = test_size_classes();
        let layout = SegmentLayout::compute(4, 4096, &size_classes);

        assert_eq!(
            layout.peer_table_offset,
            shm_primitives::SEGMENT_HEADER_SIZE
        );
        assert!(layout.ring_base_offset >= layout.peer_table_offset);
        assert!(layout.var_pool_offset >= layout.ring_base_offset);
        assert!(layout.var_pool_offset.is_multiple_of(64));
        assert!(layout.total_size > layout.var_pool_offset);
    }

    #[test]
    fn create_then_attach_roundtrips_header_and_offsets() {
        let size_classes = test_size_classes();
        let (_tmp, path) = test_path("roundtrip.segment");

        let host = Segment::create(&path, make_config(&size_classes), FileCleanup::Manual)
            .expect("create segment");
        let guest = Segment::attach(&path).expect("attach segment");

        assert_eq!(host.header().magic, MAGIC);
        assert_eq!(guest.header().magic, MAGIC);
        assert_eq!(host.header().max_guests, 4);
        assert_eq!(guest.header().max_guests, 4);
        assert_eq!(host.header().bipbuf_capacity, 4096);
        assert_eq!(guest.header().bipbuf_capacity, 4096);
        assert_eq!(
            host.header().peer_table_offset,
            guest.header().peer_table_offset
        );
        assert_eq!(
            host.header().var_pool_offset,
            guest.header().var_pool_offset
        );
    }

    #[test]
    fn attach_rejects_corrupted_header_magic() {
        let size_classes = test_size_classes();
        let (_tmp, path) = test_path("corrupt.segment");
        let _segment = Segment::create(&path, make_config(&size_classes), FileCleanup::Manual)
            .expect("create segment");

        let mmap = MmapRegion::attach(&path).expect("attach raw mmap");
        let region = mmap.region();
        let header = unsafe { region.get_mut::<shm_primitives::SegmentHeader>(0) };
        header.magic[0] ^= 0xFF;
        drop(mmap);

        let err = match Segment::attach(&path) {
            Ok(_) => panic!("corrupted header must fail attach"),
            Err(err) => err,
        };
        assert!(
            matches!(err, AttachError::BadHeader(_)),
            "unexpected err: {err:?}"
        );
    }

    #[test]
    fn peer_lifecycle_reserve_release_claim_detach_and_attach() {
        let size_classes = test_size_classes();
        let (_tmp, path) = test_path("peer-lifecycle.segment");
        let segment = Segment::create(&path, make_config(&size_classes), FileCleanup::Manual)
            .expect("create segment");

        let reserved = segment.reserve_peer().expect("reserve peer");
        assert_eq!(
            segment.peer_table().entry(reserved).state(),
            PeerState::Reserved
        );
        segment.release_reserved_peer(reserved);
        assert_eq!(
            segment.peer_table().entry(reserved).state(),
            PeerState::Empty
        );

        let reserved_again = segment.reserve_peer().expect("reserve peer again");
        segment
            .claim_peer(reserved_again)
            .expect("claim reserved peer");
        assert_eq!(
            segment.peer_table().entry(reserved_again).state(),
            PeerState::Attached
        );
        segment.detach_peer(reserved_again);
        assert_eq!(
            segment.peer_table().entry(reserved_again).state(),
            PeerState::Goodbye
        );

        let attached = segment.attach_peer().expect("attach walk-in peer");
        assert_eq!(
            segment.peer_table().entry(attached).state(),
            PeerState::Attached
        );
    }

    #[test]
    fn grow_segment_publishes_size_and_guest_remaps() {
        let size_classes = test_size_classes();
        let (_tmp, path) = test_path("grow.segment");
        let mut host = Segment::create(&path, make_config(&size_classes), FileCleanup::Manual)
            .expect("create segment");
        let mut guest = Segment::attach(&path).expect("attach guest");

        let old_size = host.header().current_size() as usize;
        let new_size = old_size + 64 * 1024;

        host.grow_segment(new_size).expect("grow segment");
        assert_eq!(host.header().current_size() as usize, new_size);

        let remapped = guest.check_and_remap().expect("guest remap check");
        assert!(remapped, "guest should remap after host growth");

        let remapped_again = guest.check_and_remap().expect("guest remap check again");
        assert!(!remapped_again, "no additional growth should produce false");
    }
}
