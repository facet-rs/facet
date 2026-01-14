//! Guest-side SHM implementation.
//!
//! Guests attach to an existing shared memory segment created by the host.
//! Each guest gets a unique peer ID and communicates through its dedicated
//! rings and slot pool.

use std::io;
use std::path::Path;
use std::ptr;

use roam_frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
use shm_primitives::{HeapRegion, MmapRegion, Region, SlotHandle};

use crate::channel::ChannelEntry;
use crate::layout::{
    CHANNEL_ENTRY_SIZE, DESC_SIZE, HEADER_SIZE, MAGIC, SegmentConfig, SegmentHeader, SegmentLayout,
    VERSION,
};
use crate::peer::{PeerEntry, PeerId};
use crate::slot_pool::SlotPool;
#[cfg(unix)]
use crate::spawn::SpawnArgs;
#[cfg(windows)]
use crate::spawn_windows::SpawnArgs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecvError {
    MalformedInlineFields,
    InlineLenTooLarge,
    PayloadTooLarge,
    SlotIndexOutOfRange,
    SlotBoundsOutOfRange,
    GenerationMismatch,
    FreeFailed,
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
    /// Our slot pool
    slots: SlotPool,
    /// Local tail for our G→H ring (what we've published)
    g2h_local_head: u64,
    /// Local head for our H→G ring (what we've consumed)
    h2g_local_tail: u64,
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
    /// No available peer slots
    NoPeerSlots,
    /// Host has signaled goodbye
    HostGoodbye,
    /// Slot was not in Reserved state (for spawned guests)
    SlotNotReserved,
    /// Peer ID is out of range for this segment
    InvalidPeerId,
    /// Segment uses variable-size slot pools which this guest doesn't support
    VarSlotPoolNotSupported,
    /// I/O error
    Io(io::Error),
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::InvalidMagic => write!(f, "invalid magic bytes"),
            AttachError::UnsupportedVersion => write!(f, "unsupported segment version"),
            AttachError::NoPeerSlots => write!(f, "no available peer slots"),
            AttachError::HostGoodbye => write!(f, "host has signaled goodbye"),
            AttachError::SlotNotReserved => write!(f, "slot was not reserved for this guest"),
            AttachError::InvalidPeerId => write!(f, "peer ID is out of range for this segment"),
            AttachError::VarSlotPoolNotSupported => {
                write!(f, "segment uses variable-size slot pools (not supported)")
            }
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
    /// claims it via CAS transition from Reserved → Attached.
    ///
    /// shm[impl shm.spawn.guest-init]
    pub fn attach_with_ticket(args: &SpawnArgs) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(&args.hub_path).map_err(AttachError::Io)?;
        let region = backing.region();

        // Validate header
        let header = unsafe { &*(region.as_ptr() as *const crate::layout::SegmentHeader) };

        if header.magic != crate::layout::MAGIC {
            return Err(AttachError::InvalidMagic);
        }
        if header.version != crate::layout::VERSION
            || header.header_size != crate::layout::HEADER_SIZE as u32
        {
            return Err(AttachError::UnsupportedVersion);
        }
        if header.is_host_goodbye() {
            return Err(AttachError::HostGoodbye);
        }

        // Check if this segment uses variable-size slot pools
        if header.var_slot_pool_offset != 0 {
            return Err(AttachError::VarSlotPoolNotSupported);
        }

        // Reconstruct layout from header (fixed-size per-guest pools only)
        let config = crate::layout::SegmentConfig {
            max_payload_size: header.max_payload_size,
            initial_credit: header.initial_credit,
            max_guests: header.max_guests,
            ring_size: header.ring_size,
            slot_size: header.slot_size,
            slots_per_guest: header.slots_per_guest,
            max_channels: header.max_channels,
            heartbeat_interval: header.heartbeat_interval,
            var_slot_classes: None,
            file_cleanup: shm_primitives::FileCleanup::Auto,
        };
        let layout = config
            .layout()
            .map_err(|_| AttachError::UnsupportedVersion)?;

        // Validate peer ID is within range
        let peer_id = args.peer_id;
        if peer_id.get() < 1 || peer_id.get() > header.max_guests as u8 {
            return Err(AttachError::InvalidPeerId);
        }

        // Claim our reserved slot
        let offset = layout.peer_entry_offset(peer_id.get());
        let entry = unsafe { &*(region.offset(offset as usize) as *const PeerEntry) };

        // CAS: Reserved → Attached
        entry
            .try_claim_reserved()
            .map_err(|_| AttachError::SlotNotReserved)?;

        let slots = SlotPool::new(
            region,
            layout.guest_slot_pool_offset(peer_id.get()),
            &config,
        );

        // Initialize channel table entries to Free
        let channel_table_offset = layout.guest_channel_table_offset(peer_id.get());
        for i in 0..config.max_channels {
            let entry_offset =
                channel_table_offset as usize + i as usize * crate::layout::CHANNEL_ENTRY_SIZE;
            let channel_entry = unsafe { &mut *(region.offset(entry_offset) as *mut ChannelEntry) };
            channel_entry.init();
        }

        Ok(Self {
            backing: GuestBacking::Mmap(backing),
            region,
            peer_id,
            layout,
            slots,
            g2h_local_head: 0,
            h2g_local_tail: 0,
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
        // Validate header
        let header = unsafe { &*(region.as_ptr() as *const SegmentHeader) };

        if header.magic != MAGIC {
            return Err(AttachError::InvalidMagic);
        }
        if header.version != VERSION || header.header_size != HEADER_SIZE as u32 {
            return Err(AttachError::UnsupportedVersion);
        }
        if header.is_host_goodbye() {
            return Err(AttachError::HostGoodbye);
        }

        // Check if this segment uses variable-size slot pools
        if header.var_slot_pool_offset != 0 {
            return Err(AttachError::VarSlotPoolNotSupported);
        }

        // Reconstruct layout from header (fixed-size per-guest pools only)
        let config = crate::layout::SegmentConfig {
            max_payload_size: header.max_payload_size,
            initial_credit: header.initial_credit,
            max_guests: header.max_guests,
            ring_size: header.ring_size,
            slot_size: header.slot_size,
            slots_per_guest: header.slots_per_guest,
            max_channels: header.max_channels,
            heartbeat_interval: header.heartbeat_interval,
            var_slot_classes: None,
            file_cleanup: shm_primitives::FileCleanup::Auto,
        };
        let layout = config
            .layout()
            .map_err(|_| AttachError::UnsupportedVersion)?;

        // Find and claim an empty peer slot
        // shm[impl shm.guest.attach.cas]
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

        let slots = SlotPool::new(
            region,
            layout.guest_slot_pool_offset(peer_id.get()),
            &config,
        );

        // Initialize channel table entries to Free
        let channel_table_offset = layout.guest_channel_table_offset(peer_id.get());
        for i in 0..config.max_channels {
            let entry_offset = channel_table_offset as usize + i as usize * CHANNEL_ENTRY_SIZE;
            let entry = unsafe { &mut *(region.offset(entry_offset) as *mut ChannelEntry) };
            entry.init();
        }

        Ok(Self {
            backing: GuestBacking::None,
            region,
            peer_id,
            layout,
            slots,
            g2h_local_head: 0,
            h2g_local_tail: 0,
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
    /// shm[impl shm.topology.hub.calls]
    pub fn send(&mut self, frame: Frame) -> Result<(), SendError> {
        if self.is_host_goodbye() {
            return Err(SendError::HostGoodbye);
        }

        // Get G→H ring info
        let ring_offset = self.layout.guest_to_host_ring_offset(self.peer_id.get());
        let ring_size = self.layout.config.ring_size as u64;

        // Check if ring is full
        // shm[impl shm.ring.full]
        // Ring is full when (head + 1) % ring_size == tail
        // Using head - tail comparison: full when head - tail >= ring_size - 1
        let tail = self.peer_entry().g2h_tail() as u64;
        if self.g2h_local_head.wrapping_sub(tail) >= ring_size - 1 {
            return Err(SendError::RingFull);
        }

        // Write descriptor
        // shm[impl shm.ordering.ring-publish]
        let slot = (self.g2h_local_head % ring_size) as usize;
        let desc_offset = ring_offset as usize + slot * DESC_SIZE;

        // Handle payload - extract data slice
        let mut desc = frame.desc;

        match &frame.payload {
            Payload::Inline => {
                if desc.payload_len as usize > INLINE_PAYLOAD_LEN {
                    return Err(SendError::PayloadTooLarge);
                }
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
            }
            Payload::Owned(data) => {
                let data = data.as_slice();
                if data.len() <= INLINE_PAYLOAD_LEN {
                    // Can inline
                    desc.payload_slot = INLINE_PAYLOAD_SLOT;
                    desc.payload_generation = 0;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;
                    desc.inline_payload[..data.len()].copy_from_slice(data);
                } else {
                    if data.len() > self.layout.config.max_payload_size as usize {
                        return Err(SendError::PayloadTooLarge);
                    }
                    // Need slot from our pool
                    // shm[impl shm.payload.slot]
                    let Some(handle) = self.slots.try_alloc() else {
                        // shm[impl shm.slot.exhaustion]
                        return Err(SendError::SlotExhausted);
                    };

                    let Some(payload_ptr) = self.slots.payload_ptr(handle.index, 0) else {
                        return Err(SendError::PayloadTooLarge);
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(data.as_ptr(), payload_ptr, data.len());
                    }

                    desc.payload_slot = handle.index;
                    desc.payload_generation = handle.generation;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;
                }
            }
            Payload::Bytes(data) => {
                let data = data.as_ref();
                if data.len() <= INLINE_PAYLOAD_LEN {
                    // Can inline
                    desc.payload_slot = INLINE_PAYLOAD_SLOT;
                    desc.payload_generation = 0;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;
                    desc.inline_payload[..data.len()].copy_from_slice(data);
                } else {
                    if data.len() > self.layout.config.max_payload_size as usize {
                        return Err(SendError::PayloadTooLarge);
                    }
                    // Need slot from our pool
                    // shm[impl shm.payload.slot]
                    let Some(handle) = self.slots.try_alloc() else {
                        // shm[impl shm.slot.exhaustion]
                        return Err(SendError::SlotExhausted);
                    };

                    let Some(payload_ptr) = self.slots.payload_ptr(handle.index, 0) else {
                        return Err(SendError::PayloadTooLarge);
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(data.as_ptr(), payload_ptr, data.len());
                    }

                    desc.payload_slot = handle.index;
                    desc.payload_generation = handle.generation;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;
                }
            }
        }

        // Write descriptor
        unsafe {
            ptr::write(self.region.offset(desc_offset) as *mut MsgDesc, desc);
        }

        // Publish head
        self.g2h_local_head = self.g2h_local_head.wrapping_add(1);
        self.peer_entry()
            .g2h_publish_head(self.g2h_local_head as u32);

        Ok(())
    }

    /// Receive a message from the host.
    ///
    /// shm[impl shm.ordering.ring-consume]
    pub fn recv(&mut self) -> Option<Frame> {
        if self.is_host_goodbye() {
            return None;
        }

        // Get H→G ring info
        let ring_offset = self.layout.host_to_guest_ring_offset(self.peer_id.get());
        let ring_size = self.layout.config.ring_size as u64;

        // Check for messages
        let head = self.peer_entry().h2g_head() as u64;
        if self.h2g_local_tail >= head {
            return None; // Ring empty
        }

        let slot = (self.h2g_local_tail % ring_size) as usize;
        let desc_offset = ring_offset as usize + slot * DESC_SIZE;
        let desc = unsafe { ptr::read(self.region.offset(desc_offset) as *const MsgDesc) };

        // Get payload
        let payload = match self.get_payload(&desc) {
            Ok(payload) => payload,
            Err(_e) => {
                self.fatal_error = true;
                return None;
            }
        };
        let frame = Frame { desc, payload };

        // Advance tail
        self.h2g_local_tail = self.h2g_local_tail.wrapping_add(1);
        self.peer_entry()
            .h2g_advance_tail(self.h2g_local_tail as u32);

        Some(frame)
    }

    /// Get payload from a descriptor and free the slot.
    fn get_payload(&self, desc: &MsgDesc) -> Result<Payload, RecvError> {
        if desc.payload_slot == INLINE_PAYLOAD_SLOT {
            // r[shm.desc.inline-fields]
            if desc.payload_generation != 0 || desc.payload_offset != 0 {
                return Err(RecvError::MalformedInlineFields);
            }
            if desc.payload_len as usize > INLINE_PAYLOAD_LEN {
                return Err(RecvError::InlineLenTooLarge);
            }
            Ok(Payload::Inline)
        } else {
            let pool_offset = self.layout.host_slot_pool_offset();
            let pool = SlotPool::new(self.region, pool_offset, &self.layout.config);

            let usable = self.layout.config.slot_size as usize - 4;
            let len = desc.payload_len as usize;
            let off = desc.payload_offset as usize;
            if len > self.layout.config.max_payload_size as usize {
                return Err(RecvError::PayloadTooLarge);
            }
            if desc.payload_slot >= self.layout.config.slots_per_guest {
                return Err(RecvError::SlotIndexOutOfRange);
            }
            if off > usable || off + len > usable {
                return Err(RecvError::SlotBoundsOutOfRange);
            }

            if pool.generation(desc.payload_slot) != Some(desc.payload_generation) {
                return Err(RecvError::GenerationMismatch);
            }

            let Some(payload_ptr) = pool.payload_ptr(desc.payload_slot, desc.payload_offset) else {
                return Err(RecvError::SlotBoundsOutOfRange);
            };

            let payload = unsafe { std::slice::from_raw_parts(payload_ptr, len).to_vec() };

            // Free the slot (return to host's pool)
            // shm[impl shm.slot.free]
            let handle = SlotHandle {
                index: desc.payload_slot,
                generation: desc.payload_generation,
            };
            pool.free(handle).map_err(|_| RecvError::FreeFailed)?;

            Ok(Payload::Owned(payload))
        }
    }

    /// Update heartbeat.
    ///
    /// shm[impl shm.heartbeat]
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

        // Update our slot pool region (pointer may have changed)
        self.slots.update_region(self.region);

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
    /// Ring is full (backpressure)
    RingFull,
    /// Payload is too large for configured limits
    PayloadTooLarge,
    /// No slots available (backpressure)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ShmHost;
    use crate::layout::SegmentConfig;
    use crate::msg::msg_type;

    #[test]
    fn attach_to_segment() {
        let config = SegmentConfig::default();
        let host = ShmHost::create_heap(config).unwrap();
        let region = host.region();

        let guest = ShmGuest::attach(region).unwrap();
        assert_eq!(guest.peer_id().get(), 1);
    }

    #[test]
    fn multiple_guests_get_different_ids() {
        let config = SegmentConfig::default();
        let host = ShmHost::create_heap(config).unwrap();
        let region = host.region();

        let guest1 = ShmGuest::attach(region).unwrap();
        let guest2 = ShmGuest::attach(region).unwrap();

        assert_eq!(guest1.peer_id().get(), 1);
        assert_eq!(guest2.peer_id().get(), 2);
    }

    #[test]
    fn malformed_host_descriptor_trips_fatal_error() {
        let config = SegmentConfig {
            max_guests: 1,
            ring_size: 8,
            slot_size: 64,
            slots_per_guest: 2,
            max_payload_size: 60,
            ..SegmentConfig::default()
        };

        let mut host = ShmHost::create_heap(config).unwrap();
        let region = host.region();
        let mut guest = ShmGuest::attach(region).unwrap();

        let desc = MsgDesc::new(msg_type::DATA, 1, 0);
        let frame = Frame {
            desc,
            payload: Payload::Inline,
        };
        host.send(guest.peer_id(), frame).unwrap();

        // Corrupt the host->guest descriptor: inline payload with non-zero generation.
        let ring_offset = host
            .layout()
            .host_to_guest_ring_offset(guest.peer_id().get());
        let desc_offset = ring_offset as usize; // first slot (tail=0)
        let mut corrupt = unsafe { ptr::read(region.offset(desc_offset) as *const MsgDesc) };
        corrupt.payload_generation = 1;
        unsafe {
            ptr::write(region.offset(desc_offset) as *mut MsgDesc, corrupt);
        }

        assert!(guest.recv().is_none());
        assert!(guest.is_host_goodbye());
    }
}
