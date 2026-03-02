use crate::sync::{AtomicU32, AtomicU64, Ordering};

/// Magic bytes that identify a v7 roam SHM segment.
///
/// r[impl shm.segment.magic.v7]
pub const MAGIC: [u8; 8] = *b"ROAMHUB\x07";

/// Current segment format version.
pub const SEGMENT_VERSION: u32 = 7;

/// Fixed size of the segment header in bytes.
pub const SEGMENT_HEADER_SIZE: usize = 128;

/// Parameters for initializing a fresh segment header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHeaderInit {
    pub total_size: u64,
    pub max_payload_size: u32,
    pub inline_threshold: u32,
    pub max_guests: u32,
    pub bipbuf_capacity: u32,
    pub peer_table_offset: u64,
    pub var_pool_offset: u64,
    pub heartbeat_interval: u64,
    pub num_var_slot_classes: u32,
}

/// The segment header lives at offset 0 of every roam SHM segment.
///
/// All fields are set by the host at creation time and treated as read-only
/// by guests after attach — except `host_goodbye` and `current_size`, which
/// the host may update at runtime.
///
/// r[impl shm.segment]
/// r[impl shm.segment.header]
/// r[impl shm.segment.config]
#[repr(C)]
pub struct SegmentHeader {
    /// "ROAMHUB\x07" — identifies a v7 roam SHM segment.
    pub magic: [u8; 8],
    /// Segment format version (currently 7).
    pub version: u32,
    /// Always 128 — allows future extension without breaking older readers.
    pub header_size: u32,
    /// Total size of the segment in bytes (set at creation).
    pub total_size: u64,
    /// Maximum payload size in bytes.
    pub max_payload_size: u32,
    /// Inline threshold: payloads ≤ this go inline; larger ones use a slot ref.
    /// 0 means use the default (256 bytes).
    pub inline_threshold: u32,
    /// Maximum number of guests (≤ 255).
    pub max_guests: u32,
    /// BipBuffer data region size per direction, in bytes.
    pub bipbuf_capacity: u32,
    /// Byte offset of the peer table from the start of the segment.
    pub peer_table_offset: u64,
    /// Byte offset of the shared VarSlotPool from the start of the segment.
    pub var_pool_offset: u64,
    /// Heartbeat interval in nanoseconds; 0 = heartbeats disabled.
    pub heartbeat_interval: u64,
    /// Set to non-zero by the host during orderly shutdown.
    pub host_goodbye: AtomicU32,
    /// Number of var-slot size classes described at `var_pool_offset`.
    pub num_var_slot_classes: u32,
    /// Current segment size in bytes. May grow if extents are appended.
    pub current_size: AtomicU64,
    _reserved: [u8; 48],
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<SegmentHeader>() == SEGMENT_HEADER_SIZE);

impl SegmentHeader {
    /// Write initial values into a zeroed header.
    ///
    /// # Safety
    ///
    /// `self` must point into exclusively-owned, zeroed memory.
    pub unsafe fn init(&mut self, init: SegmentHeaderInit) {
        self.magic = MAGIC;
        self.version = SEGMENT_VERSION;
        self.header_size = SEGMENT_HEADER_SIZE as u32;
        self.total_size = init.total_size;
        self.max_payload_size = init.max_payload_size;
        self.inline_threshold = init.inline_threshold;
        self.max_guests = init.max_guests;
        self.bipbuf_capacity = init.bipbuf_capacity;
        self.peer_table_offset = init.peer_table_offset;
        self.var_pool_offset = init.var_pool_offset;
        self.heartbeat_interval = init.heartbeat_interval;
        self.host_goodbye = AtomicU32::new(0);
        self.num_var_slot_classes = init.num_var_slot_classes;
        self.current_size = AtomicU64::new(init.total_size);
        self._reserved = [0u8; 48];
    }

    /// Validate that the header looks like a v7 roam segment.
    ///
    /// Returns `Err` with a description if validation fails.
    ///
    /// r[impl shm.segment.magic.v7]
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.magic != MAGIC {
            return Err("bad magic: not a roam v7 segment");
        }
        if self.version != SEGMENT_VERSION {
            return Err("unsupported segment version");
        }
        if self.header_size != SEGMENT_HEADER_SIZE as u32 {
            return Err("unexpected header_size");
        }
        if self.num_var_slot_classes == 0 {
            return Err("segment missing var-slot classes");
        }
        Ok(())
    }

    /// Read the effective inline threshold (substituting the default if 0).
    #[inline]
    pub fn effective_inline_threshold(&self) -> u32 {
        if self.inline_threshold == 0 {
            256
        } else {
            self.inline_threshold
        }
    }

    /// Read the current segment size.
    #[inline]
    pub fn current_size(&self) -> u64 {
        self.current_size.load(Ordering::Acquire)
    }

    /// Check whether the host has raised the goodbye flag.
    #[inline]
    pub fn host_goodbye(&self) -> bool {
        self.host_goodbye.load(Ordering::Acquire) != 0
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::region::HeapRegion;

    fn make_header() -> (HeapRegion, *mut SegmentHeader) {
        let region = HeapRegion::new_zeroed(SEGMENT_HEADER_SIZE);
        let r = region.region();
        let hdr: *mut SegmentHeader = unsafe { r.get_mut::<SegmentHeader>(0) };
        unsafe {
            (*hdr).init(SegmentHeaderInit {
                total_size: 65536,
                max_payload_size: 65536,
                inline_threshold: 0,
                max_guests: 4,
                bipbuf_capacity: 16384,
                peer_table_offset: 128,
                var_pool_offset: 4096,
                heartbeat_interval: 0,
                num_var_slot_classes: 1,
            });
        }
        (region, hdr)
    }

    #[test]
    fn roundtrip() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &*hdr };

        assert_eq!(hdr.magic, MAGIC);
        assert_eq!(hdr.version, SEGMENT_VERSION);
        assert_eq!(hdr.header_size, 128);
        assert_eq!(hdr.total_size, 65536);
        assert_eq!(hdr.max_guests, 4);
        assert_eq!(hdr.bipbuf_capacity, 16384);
        assert_eq!(hdr.peer_table_offset, 128);
        assert_eq!(hdr.var_pool_offset, 4096);
        assert_eq!(hdr.num_var_slot_classes, 1);
        assert_eq!(hdr.current_size(), 65536);
    }

    #[test]
    fn validate_ok() {
        let (_region, hdr) = make_header();
        unsafe { &*hdr }.validate().expect("valid header");
    }

    #[test]
    fn validate_bad_magic() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &mut *hdr };
        hdr.magic[7] = 0x01; // corrupt version byte in magic
        assert!(hdr.validate().is_err());
    }

    #[test]
    fn validate_bad_version() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &mut *hdr };
        hdr.version = 99;
        assert!(hdr.validate().is_err());
    }

    #[test]
    fn inline_threshold_default() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &*hdr };
        // inline_threshold was 0 → should return 256
        assert_eq!(hdr.effective_inline_threshold(), 256);
    }

    #[test]
    fn host_goodbye_flag() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &*hdr };
        assert!(!hdr.host_goodbye());
        hdr.host_goodbye.store(1, Ordering::Release);
        assert!(hdr.host_goodbye());
    }

    #[test]
    fn current_size_matches_total() {
        let (_region, hdr) = make_header();
        let hdr = unsafe { &*hdr };
        assert_eq!(hdr.current_size(), hdr.total_size);
    }
}
