//! Guest-side SHM implementation (v2 BipBuffer transport).
//!
//! Guests attach to an existing shared memory segment created by the host.
//! Each guest gets a unique peer ID and communicates through its dedicated
//! BipBuffer pair and the shared VarSlotPool.

use std::io;
use std::path::Path;
use std::ptr;

use roam_frame::{
    Frame, MsgDesc, SHM_FRAME_HEADER_SIZE, SLOT_REF_FRAME_SIZE, ShmFrameHeader, SlotRef,
    encode_inline_frame, encode_slot_ref_frame, inline_frame_size, should_inline,
};
use shm_primitives::{BipBufRaw, HeapRegion, MmapRegion, Region};

use crate::channel::ChannelEntry;
use crate::layout::{
    BIPBUF_HEADER_SIZE, CHANNEL_ENTRY_SIZE, HEADER_SIZE, MAGIC, SegmentConfig, SegmentHeader,
    SegmentLayout, VERSION, VERSION_V1,
};
use crate::peer::{PeerEntry, PeerId};
use crate::spawn::SpawnArgs;
use crate::var_slot_pool::{VarSlotHandle, VarSlotPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum RecvError {
    /// Frame header was truncated or missing.
    TruncatedHeader,
    /// Frame's total_len exceeds the readable region.
    TruncatedFrame,
    /// total_len is smaller than the header itself.
    InvalidTotalLen,
    /// Slot reference was truncated.
    TruncatedSlotRef,
    /// Payload length exceeds max_payload_size.
    PayloadTooLarge,
    /// Slot reference points to an invalid class.
    InvalidSlotClass,
    /// VarSlotPool free failed (generation mismatch or double-free).
    FreeFailed,
    /// Payload ptr from VarSlotPool was null / invalid.
    InvalidSlotPtr,
}

/// Backing memory for a guest's SHM view.
///
/// The backing is kept alive for the lifetime of the guest, ensuring
/// the memory mapping remains valid.
#[allow(dead_code)]
enum GuestBacking {
    /// No owned backing (external region)
    None,
    /// Heap-allocated memory (for testing)
    Heap(HeapRegion),
    /// File-backed mmap (for production cross-process IPC)
    Mmap(MmapRegion),
}

/// Guest-side handle for a SHM segment.
///
/// shm[impl shm.topology.hub]
pub struct ShmGuest {
    /// Backing memory (heap, mmap, or external)
    #[allow(dead_code)]
    backing: GuestBacking,
    /// Region view into backing memory
    region: Region,
    /// Our peer ID
    peer_id: PeerId,
    /// Computed layout (reconstructed from header)
    layout: SegmentLayout,
    /// Guest-to-Host BipBuffer (we are the producer)
    g2h_buf: BipBufRaw,
    /// Host-to-Guest BipBuffer (we are the consumer)
    h2g_buf: BipBufRaw,
    /// Shared variable-size slot pool
    var_pool: VarSlotPool,
    /// Set when we observe a protocol violation / memory corruption.
    fatal_error: bool,
}

/// Errors when attaching to a SHM segment.
#[derive(Debug)]
pub enum AttachError {
    /// Invalid magic bytes
    InvalidMagic,
    /// Unsupported version
    UnsupportedVersion,
    /// Segment is v1 (requires upgrade)
    V1Segment,
    /// No available peer slots
    NoPeerSlots,
    /// Host has signaled goodbye
    HostGoodbye,
    /// Slot was not in Reserved state (for spawned guests)
    SlotNotReserved,
    /// Peer ID is out of range for this segment
    InvalidPeerId,
    /// Segment header validation failed
    InvalidHeader(&'static str),
    /// I/O error
    Io(io::Error),
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::InvalidMagic => write!(f, "invalid magic bytes"),
            AttachError::UnsupportedVersion => write!(f, "unsupported segment version"),
            AttachError::V1Segment => write!(f, "v1 segment (upgrade required)"),
            AttachError::NoPeerSlots => write!(f, "no available peer slots"),
            AttachError::HostGoodbye => write!(f, "host has signaled goodbye"),
            AttachError::SlotNotReserved => write!(f, "slot was not reserved for this guest"),
            AttachError::InvalidPeerId => write!(f, "peer ID is out of range for this segment"),
            AttachError::InvalidHeader(msg) => write!(f, "invalid segment header: {}", msg),
            AttachError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for AttachError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AttachError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl ShmGuest {
    /// Validate the segment header for v2.
    ///
    /// Returns the header reference on success.
    fn validate_header(region: &Region) -> Result<&SegmentHeader, AttachError> {
        let header = unsafe { &*(region.as_ptr() as *const SegmentHeader) };

        if header.magic != MAGIC {
            return Err(AttachError::InvalidMagic);
        }
        if header.version == VERSION_V1 {
            return Err(AttachError::V1Segment);
        }
        if header.version != VERSION || header.header_size != HEADER_SIZE as u32 {
            return Err(AttachError::UnsupportedVersion);
        }
        if header.is_host_goodbye() {
            return Err(AttachError::HostGoodbye);
        }
        // v2: slot_size must be 0 (fixed per-guest pools eliminated)
        if header.slot_size != 0 {
            return Err(AttachError::InvalidHeader(
                "v2 segment must have slot_size = 0",
            ));
        }
        // v2: var_slot_pool_offset must be non-zero
        if header.var_slot_pool_offset == 0 {
            return Err(AttachError::InvalidHeader(
                "v2 segment must have non-zero var_slot_pool_offset",
            ));
        }
        Ok(header)
    }

    /// Reconstruct SegmentConfig from a v2 header by reading size class
    /// headers from shared memory.
    ///
    /// In v2:
    /// - `header.ring_size` -> `bipbuf_capacity`
    /// - `header.slots_per_guest` -> `inline_threshold` (0 = default 256)
    fn config_from_header(region: &Region, header: &SegmentHeader) -> SegmentConfig {
        // Read size class headers from the VarSlotPool region to reconstruct
        // the actual var_slot_classes. This is needed so the SegmentLayout
        // computes the correct guest_areas_offset.
        let num_classes = header.num_var_slot_classes as usize;
        let pool_offset = header.var_slot_pool_offset as usize;

        let mut var_slot_classes = Vec::with_capacity(num_classes);
        for i in 0..num_classes {
            let class_header_offset = pool_offset + i * 64;
            let class_header = unsafe {
                &*(region.offset(class_header_offset)
                    as *const crate::var_slot_pool::SizeClassHeader)
            };
            var_slot_classes.push(crate::layout::SizeClass {
                slot_size: class_header.slot_size,
                count: class_header.slots_per_extent,
            });
        }

        // Fallback to defaults if header predates the num_var_slot_classes field
        if var_slot_classes.is_empty() {
            var_slot_classes = SegmentConfig::default_size_classes();
        }

        SegmentConfig {
            max_payload_size: header.max_payload_size,
            initial_credit: header.initial_credit,
            max_guests: header.max_guests,
            bipbuf_capacity: header.ring_size,
            inline_threshold: header.slots_per_guest,
            max_channels: header.max_channels,
            heartbeat_interval: header.heartbeat_interval,
            var_slot_classes,
            file_cleanup: shm_primitives::FileCleanup::Auto,
        }
    }

    /// Create BipBufRaw views for a guest's G2H and H2G buffers.
    ///
    /// # Safety
    ///
    /// The region must contain valid, initialized BipBuffer headers at the
    /// computed offsets.
    unsafe fn create_bipbufs(
        region: &Region,
        layout: &SegmentLayout,
        peer_id: PeerId,
    ) -> (BipBufRaw, BipBufRaw) {
        let g2h_header_offset = layout.guest_to_host_bipbuf_offset(peer_id.get()) as usize;
        let h2g_header_offset = layout.host_to_guest_bipbuf_offset(peer_id.get()) as usize;

        let g2h_header_ptr = region.offset(g2h_header_offset) as *mut shm_primitives::BipBufHeader;
        let g2h_data_ptr = region.offset(g2h_header_offset + BIPBUF_HEADER_SIZE);
        let g2h_buf = unsafe { BipBufRaw::from_raw(g2h_header_ptr, g2h_data_ptr) };

        let h2g_header_ptr = region.offset(h2g_header_offset) as *mut shm_primitives::BipBufHeader;
        let h2g_data_ptr = region.offset(h2g_header_offset + BIPBUF_HEADER_SIZE);
        let h2g_buf = unsafe { BipBufRaw::from_raw(h2g_header_ptr, h2g_data_ptr) };

        (g2h_buf, h2g_buf)
    }

    /// Create a VarSlotPool view from the segment.
    fn create_var_pool(region: &Region, layout: &SegmentLayout) -> VarSlotPool {
        let offset = layout.var_slot_pool_offset();
        VarSlotPool::new(*region, offset, layout.config.var_slot_classes.clone())
    }

    /// Initialize channel table entries for a peer.
    fn init_channel_table(
        region: &Region,
        layout: &SegmentLayout,
        config: &SegmentConfig,
        peer_id: PeerId,
    ) {
        let channel_table_offset = layout.guest_channel_table_offset(peer_id.get());
        for i in 0..config.max_channels {
            let entry_offset = channel_table_offset as usize + i as usize * CHANNEL_ENTRY_SIZE;
            let entry = unsafe { &mut *(region.offset(entry_offset) as *mut ChannelEntry) };
            entry.init();
        }
    }

    /// Attach to an existing SHM segment by path.
    ///
    /// This opens the file, maps it into memory, finds an empty peer slot,
    /// atomically claims it, and initializes the guest's local state.
    ///
    /// shm[impl shm.file.attach]
    /// shm[impl shm.guest.attach]
    pub fn attach_path<P: AsRef<Path>>(path: P) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(path.as_ref()).map_err(AttachError::Io)?;
        let region = backing.region();

        let mut guest = Self::attach_region(region)?;
        guest.backing = GuestBacking::Mmap(backing);
        Ok(guest)
    }

    /// Attach to a segment using a spawn ticket's arguments.
    ///
    /// This is for spawned guest processes that have a pre-reserved slot.
    /// The host has already reserved a slot via `add_peer()`, and the guest
    /// claims it via CAS transition from Reserved -> Attached.
    ///
    /// shm[impl shm.spawn.guest-init]
    pub fn attach_with_ticket(args: &SpawnArgs) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(&args.hub_path).map_err(AttachError::Io)?;
        let region = backing.region();

        let header = Self::validate_header(&region)?;

        let config = Self::config_from_header(&region, header);
        let layout = config.layout().map_err(AttachError::InvalidHeader)?;

        // Validate peer ID is within range
        let peer_id = args.peer_id;
        if peer_id.get() < 1 || peer_id.get() > header.max_guests as u8 {
            return Err(AttachError::InvalidPeerId);
        }

        // Claim our reserved slot
        let offset = layout.peer_entry_offset(peer_id.get());
        let entry = unsafe { &*(region.offset(offset as usize) as *const PeerEntry) };

        // CAS: Reserved -> Attached
        entry
            .try_claim_reserved()
            .map_err(|_| AttachError::SlotNotReserved)?;

        let (g2h_buf, h2g_buf) = unsafe { Self::create_bipbufs(&region, &layout, peer_id) };
        let var_pool = Self::create_var_pool(&region, &layout);

        Self::init_channel_table(&region, &layout, &config, peer_id);

        Ok(Self {
            backing: GuestBacking::Mmap(backing),
            region,
            peer_id,
            layout,
            g2h_buf,
            h2g_buf,
            var_pool,
            fatal_error: false,
        })
    }

    /// Attach to an existing SHM segment via a Region.
    ///
    /// This finds an empty peer slot, atomically claims it, and initializes
    /// the guest's local state.
    ///
    /// This is the low-level attach that works with any Region source.
    /// For file-backed segments, prefer `attach_path`.
    ///
    /// shm[impl shm.guest.attach]
    pub fn attach(region: Region) -> Result<Self, AttachError> {
        Self::attach_region(region)
    }

    /// Internal attach implementation.
    fn attach_region(region: Region) -> Result<Self, AttachError> {
        let header = Self::validate_header(&region)?;

        let config = Self::config_from_header(&region, header);
        let layout = config.layout().map_err(AttachError::InvalidHeader)?;

        // Find and claim an empty peer slot
        // shm[impl shm.guest.attach]
        let mut peer_id = None;
        for i in 1..=header.max_guests as u8 {
            let Some(id) = PeerId::from_index(i - 1) else {
                continue;
            };
            let offset = layout.peer_entry_offset(i);
            let entry = unsafe { &*(region.offset(offset as usize) as *const PeerEntry) };

            // Try to claim this slot with CAS
            if entry.try_attach().is_ok() {
                peer_id = Some(id);
                break;
            }
        }

        let peer_id = peer_id.ok_or(AttachError::NoPeerSlots)?;

        let (g2h_buf, h2g_buf) = unsafe { Self::create_bipbufs(&region, &layout, peer_id) };
        let var_pool = Self::create_var_pool(&region, &layout);

        Self::init_channel_table(&region, &layout, &config, peer_id);

        Ok(Self {
            backing: GuestBacking::None,
            region,
            peer_id,
            layout,
            g2h_buf,
            h2g_buf,
            var_pool,
            fatal_error: false,
        })
    }

    /// Attach to a segment using a HeapRegion (for testing).
    #[cfg(test)]
    pub fn attach_heap(backing: HeapRegion) -> Result<Self, AttachError> {
        let region = backing.region();
        let mut guest = Self::attach_region(region)?;
        guest.backing = GuestBacking::Heap(backing);
        Ok(guest)
    }

    /// Get our peer ID.
    #[inline]
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Get the segment configuration.
    ///
    /// This returns the config that was read from the segment header when
    /// attaching. Useful for getting max_payload_size, initial_credit, etc.
    #[inline]
    pub fn config(&self) -> &SegmentConfig {
        &self.layout.config
    }

    /// Get the segment header.
    fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.region.as_ptr() as *const SegmentHeader) }
    }

    /// Get our peer entry.
    fn peer_entry(&self) -> &PeerEntry {
        let offset = self.layout.peer_entry_offset(self.peer_id.get()) as usize;
        unsafe { &*(self.region.offset(offset) as *const PeerEntry) }
    }

    /// Check if the host has signaled goodbye.
    ///
    /// shm[impl shm.goodbye.host]
    #[inline]
    pub fn is_host_goodbye(&self) -> bool {
        self.fatal_error || self.header().is_host_goodbye()
    }

    /// Send a message to the host.
    ///
    /// The frame is encoded into the G2H BipBuffer as an SHM frame. Small
    /// payloads go inline; large payloads are placed in the shared VarSlotPool
    /// and referenced via a SlotRef in the frame.
    ///
    /// shm[impl shm.topology.hub.calls]
    pub fn send(&mut self, frame: Frame) -> Result<(), SendError> {
        if self.is_host_goodbye() {
            return Err(SendError::HostGoodbye);
        }

        let msg_type = frame.desc.msg_type;
        let id = frame.desc.id;
        let method_id = frame.desc.method_id;

        // Get the payload bytes
        let payload_data: &[u8] = frame.payload.as_slice(&frame.desc);
        let payload_len = payload_data.len() as u32;

        if payload_len > self.layout.config.max_payload_size {
            return Err(SendError::PayloadTooLarge);
        }

        let threshold = self.layout.config.effective_inline_threshold();

        if should_inline(payload_len, threshold) {
            // Inline path: header + payload in the BipBuffer
            let total_len = inline_frame_size(payload_len);
            let Some(grant) = self.g2h_buf.try_grant(total_len) else {
                return Err(SendError::RingFull);
            };

            encode_inline_frame(msg_type, id, method_id, payload_data, grant);
            self.g2h_buf.commit(total_len);
        } else {
            // Slot-ref path: allocate from VarSlotPool, copy payload, write ref frame
            // shm[impl shm.payload.slot]
            let Some(handle) = self.var_pool.alloc(payload_len, self.peer_id.get()) else {
                // shm[impl shm.slot.exhaustion]
                warn!(
                    payload_len,
                    "slot exhaustion: VarSlotPool has no class large enough or all exhausted"
                );
                return Err(SendError::SlotExhausted);
            };

            // Copy payload into the slot
            let Some(slot_ptr) = self.var_pool.payload_ptr(handle) else {
                // Failed to get pointer - free the slot and report error
                let _ = self.var_pool.free_allocated(handle);
                return Err(SendError::PayloadTooLarge);
            };
            unsafe {
                ptr::copy_nonoverlapping(payload_data.as_ptr(), slot_ptr, payload_data.len());
            }

            // Mark slot as in-flight before writing the frame
            if self.var_pool.mark_in_flight(handle).is_err() {
                let _ = self.var_pool.free_allocated(handle);
                return Err(SendError::SlotExhausted);
            }

            let slot_ref = SlotRef {
                class_idx: handle.class_idx,
                extent_idx: handle.extent_idx,
                slot_idx: handle.slot_idx,
                slot_generation: handle.generation,
            };

            let frame_size = SLOT_REF_FRAME_SIZE as u32;
            let Some(grant) = self.g2h_buf.try_grant(frame_size) else {
                // BipBuffer is full - free the slot
                let _ = self.var_pool.free(handle);
                return Err(SendError::RingFull);
            };

            encode_slot_ref_frame(msg_type, id, method_id, payload_len, &slot_ref, grant);
            self.g2h_buf.commit(frame_size);
        }

        Ok(())
    }

    /// Receive a message from the host.
    ///
    /// Reads and parses one frame from the H2G BipBuffer. If the buffer
    /// contains multiple frames, only the first is returned; call `recv`
    /// again for the next.
    ///
    /// shm[impl shm.ordering.ring-consume]
    pub fn recv(&mut self) -> Option<Frame> {
        if self.is_host_goodbye() {
            return None;
        }

        let readable = self.h2g_buf.try_read()?;
        if readable.len() < SHM_FRAME_HEADER_SIZE {
            // Not enough data for even a header - should not happen in a
            // well-behaved system, but don't panic.
            return None;
        }

        // Parse the first frame header
        let shm_header = ShmFrameHeader::read_from(readable)?;

        let total_len = shm_header.total_len as usize;
        if total_len < SHM_FRAME_HEADER_SIZE {
            // Corrupt frame
            self.fatal_error = true;
            return None;
        }
        if total_len > readable.len() {
            // Frame is truncated - should not happen with correct host
            self.fatal_error = true;
            return None;
        }

        // Parse the frame content
        let frame_bytes = &readable[..total_len];
        match self.parse_frame(&shm_header, frame_bytes) {
            Ok(frame) => {
                self.h2g_buf.release(total_len as u32);
                Some(frame)
            }
            Err(_e) => {
                self.fatal_error = true;
                None
            }
        }
    }

    /// Parse a single frame from a byte slice.
    fn parse_frame(
        &self,
        shm_header: &ShmFrameHeader,
        frame_bytes: &[u8],
    ) -> Result<Frame, RecvError> {
        let msg_type = shm_header.msg_type;
        let id = shm_header.id;
        let method_id = shm_header.method_id;
        let payload_len = shm_header.payload_len as usize;

        if shm_header.has_slot_ref() {
            // Slot-ref frame: payload is in VarSlotPool
            if frame_bytes.len() < SHM_FRAME_HEADER_SIZE + roam_frame::SLOT_REF_SIZE {
                return Err(RecvError::TruncatedSlotRef);
            }

            let slot_ref = SlotRef::read_from(&frame_bytes[SHM_FRAME_HEADER_SIZE..])
                .ok_or(RecvError::TruncatedSlotRef)?;

            let handle = VarSlotHandle {
                class_idx: slot_ref.class_idx,
                extent_idx: slot_ref.extent_idx,
                slot_idx: slot_ref.slot_idx,
                generation: slot_ref.slot_generation,
            };

            if payload_len > self.layout.config.max_payload_size as usize {
                return Err(RecvError::PayloadTooLarge);
            }

            // Validate payload_len fits within the slot's size class
            if let Some(slot_size) = self.var_pool.slot_size(slot_ref.class_idx) {
                if payload_len > slot_size as usize {
                    return Err(RecvError::PayloadTooLarge);
                }
            } else {
                return Err(RecvError::InvalidSlotPtr);
            }

            let slot_ptr = self
                .var_pool
                .payload_ptr(handle)
                .ok_or(RecvError::InvalidSlotPtr)?;

            // Copy payload out of the slot
            let payload_data =
                unsafe { std::slice::from_raw_parts(slot_ptr, payload_len) }.to_vec();

            // Free the slot (return to shared pool)
            // shm[impl shm.slot.free]
            self.var_pool
                .free(handle)
                .map_err(|_| RecvError::FreeFailed)?;

            let desc = MsgDesc::new(msg_type, id, method_id);
            Ok(Frame::with_owned_payload(desc, payload_data))
        } else {
            // Inline frame: payload follows the header in the BipBuffer
            let payload_start = SHM_FRAME_HEADER_SIZE;
            let payload_end = payload_start + payload_len;
            if payload_end > frame_bytes.len() {
                return Err(RecvError::TruncatedFrame);
            }

            let desc = MsgDesc::new(msg_type, id, method_id);
            if payload_len == 0 {
                Ok(Frame::new(desc))
            } else {
                let payload_data = frame_bytes[payload_start..payload_end].to_vec();
                Ok(Frame::with_owned_payload(desc, payload_data))
            }
        }
    }

    /// Update heartbeat.
    ///
    /// shm[impl shm.crash.heartbeat]
    pub fn heartbeat(&self) {
        let entry = self.peer_entry();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        entry.update_heartbeat(now);
    }

    /// Check if the segment has grown and remap if necessary.
    ///
    /// The host may grow the segment by adding extents to variable-size slot
    /// pools. Guests detect this by comparing `header.current_size` against
    /// their mapped size and remapping when it differs.
    ///
    /// Returns `true` if a remap occurred.
    ///
    /// # Errors
    ///
    /// Returns an error if the remap fails (e.g., mmap error).
    ///
    /// shm[impl shm.varslot.extents]
    pub fn check_remap(&mut self) -> io::Result<bool> {
        use std::sync::atomic::Ordering;

        // Only mmap-backed guests can remap
        let mmap = match &mut self.backing {
            GuestBacking::Mmap(m) => m,
            _ => return Ok(false),
        };

        // Check current_size in header
        let header = unsafe { &*(self.region.as_ptr() as *const SegmentHeader) };
        let current_size = header.current_size.load(Ordering::Acquire) as usize;
        let mapped_size = mmap.len();

        if current_size <= mapped_size {
            return Ok(false); // No growth
        }

        // Segment has grown - remap
        mmap.check_and_remap()?;

        // Update our region view
        self.region = mmap.region();

        // Update BipBuffer views (pointers may have changed after remap)
        let (g2h_buf, h2g_buf) =
            unsafe { Self::create_bipbufs(&self.region, &self.layout, self.peer_id) };
        self.g2h_buf = g2h_buf;
        self.h2g_buf = h2g_buf;

        // Update VarSlotPool region (pointer may have changed)
        self.var_pool.update_region(self.region);

        Ok(true)
    }

    /// Initiate graceful detach.
    ///
    /// shm[impl shm.guest.detach]
    pub fn detach(&mut self) {
        let entry = self.peer_entry();

        // Set state to Goodbye
        entry.set_goodbye();

        // Drain any pending received messages
        while self.recv().is_some() {}
    }
}

impl Drop for ShmGuest {
    fn drop(&mut self) {
        // Ensure graceful detach
        self.detach();
    }
}

/// Errors from send operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// Host has signaled goodbye
    HostGoodbye,
    /// BipBuffer is full (backpressure)
    ///
    /// shm[impl shm.bipbuf.full]
    RingFull,
    /// Payload is too large for configured limits
    PayloadTooLarge,
    /// No slots available in VarSlotPool (backpressure)
    SlotExhausted,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::HostGoodbye => write!(f, "host goodbye"),
            SendError::RingFull => write!(f, "ring full"),
            SendError::PayloadTooLarge => write!(f, "payload too large"),
            SendError::SlotExhausted => write!(f, "slot exhausted"),
        }
    }
}

impl std::error::Error for SendError {}

// Guest unit tests live in tests/roundtrip.rs — they need a cooperating
// v2 host (ShmHost) to create and initialize the segment.
