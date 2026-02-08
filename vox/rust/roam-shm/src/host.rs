//! Host-side SHM implementation (v2: BipBuffer transport).
//!
//! The host creates and owns the shared memory segment. It initializes
//! the segment header, peer table, BipBuffers, and shared VarSlotPool.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::Ordering;

use roam_frame::shm_frame::{
    self, SHM_FRAME_HEADER_SIZE, SLOT_REF_FRAME_SIZE, ShmFrameHeader, SlotRef,
};

use crate::msg::ShmMsg;
use shm_primitives::{
    BipBufHeader, BipBufRaw, Doorbell, HeapRegion, MmapRegion, Region, SignalResult,
};

use crate::channel::ChannelEntry;
use crate::layout::{
    BIPBUF_HEADER_SIZE, CHANNEL_ENTRY_SIZE, EXTENT_MAGIC, ExtentHeader, HEADER_SIZE, MAGIC,
    MAX_EXTENTS_PER_CLASS, SegmentConfig, SegmentHeader, SegmentLayout, VERSION,
};
use crate::peer::{PeerEntry, PeerId, PeerState};
use crate::spawn::{AddPeerOptions, DeathCallback, SpawnTicket};
use crate::var_slot_pool::{SizeClassHeader, VarSlotHandle, VarSlotPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecvError {
    /// Frame header too small or total_len nonsensical
    MalformedFrame,
    /// SlotRef data could not be parsed
    MalformedSlotRef,
    /// Payload exceeds max_payload_size
    PayloadTooLarge,
    /// VarSlotPool free failed
    FreeFailed,
    /// VarSlotPool alloc returned a bad pointer
    SlotPtrInvalid,
}

/// Result of polling the host for incoming messages.
#[derive(Debug, Default)]
pub struct PollResult {
    /// Messages received from guests, as (peer_id, shm_msg) pairs.
    pub messages: Vec<(PeerId, ShmMsg)>,

    /// Peers whose slots were freed during this poll.
    ///
    /// The caller should ring the doorbell for each peer in this list to wake up
    /// guests that may be waiting for slots to become available (backpressure).
    pub slots_freed_for: Vec<PeerId>,
}

/// Backing memory for a SHM segment.
///
/// The backing is kept alive for the lifetime of the host, ensuring
/// the memory mapping remains valid.
#[allow(dead_code)]
enum ShmBacking {
    /// Heap-allocated memory (for testing)
    Heap(HeapRegion),
    /// File-backed mmap (for production cross-process IPC)
    Mmap(MmapRegion),
}

/// Host-side handle for a SHM segment.
///
/// shm[impl shm.topology.hub]
pub struct ShmHost {
    /// Backing memory (heap or mmap)
    #[allow(dead_code)]
    backing: ShmBacking,
    /// Path to segment file (for cross-process use)
    path: Option<PathBuf>,
    /// Region view into backing memory
    region: Region,
    /// Computed layout
    layout: SegmentLayout,
    /// Per-guest state tracked by the host
    pub(crate) guests: HashMap<PeerId, GuestState>,
}

/// Host-side state for a single guest.
pub(crate) struct GuestState {
    /// Human-readable name for debugging
    pub(crate) name: Option<String>,
    /// Last observed epoch
    pub(crate) last_epoch: u32,
    /// VarSlotPool handles we've allocated for messages to this guest
    pub(crate) pending_slots: Vec<VarSlotHandle>,
    /// Host's doorbell for this peer (if spawned via add_peer)
    pub(crate) doorbell: Option<Doorbell>,
    /// Death callback (if registered via add_peer)
    pub(crate) on_death: Option<DeathCallback>,
    /// Whether we've already notified death for this peer
    pub(crate) death_notified: bool,
    /// Call statistics for diagnostics
    pub(crate) stats: crate::diagnostic::PeerCallStats,
}

impl ShmHost {
    /// Create a new file-backed SHM segment at the given path.
    ///
    /// This creates a file, maps it into memory, and initializes all data structures.
    /// The file will be deleted when the ShmHost is dropped.
    ///
    /// shm[impl shm.file.create]
    pub fn create<P: AsRef<Path>>(path: P, config: SegmentConfig) -> io::Result<Self> {
        let layout = config
            .layout()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        // Create file-backed memory
        let backing = MmapRegion::create(
            path.as_ref(),
            layout.total_size as usize,
            config.file_cleanup,
        )?;
        let region = backing.region();

        // Initialize segment
        // SAFETY: We just allocated this memory and it's zeroed
        unsafe {
            Self::init_header(&region, &layout);
            Self::init_peer_table(&region, &layout);
            Self::init_var_slot_pool(&region, &layout);
            Self::init_guest_areas(&region, &layout);
        }

        Ok(Self {
            backing: ShmBacking::Mmap(backing),
            path: Some(path.as_ref().to_path_buf()),
            region,
            layout,
            guests: HashMap::new(),
        })
    }

    /// Create a new heap-backed SHM segment (for testing).
    ///
    /// This allocates memory on the heap and initializes all data structures.
    /// Useful for unit tests that don't need cross-process IPC.
    pub fn create_heap(config: SegmentConfig) -> io::Result<Self> {
        let layout = config
            .layout()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        // Allocate backing memory
        let backing = HeapRegion::new_zeroed(layout.total_size as usize);
        let region = backing.region();

        // Initialize segment
        // SAFETY: We just allocated this memory and it's zeroed
        unsafe {
            Self::init_header(&region, &layout);
            Self::init_peer_table(&region, &layout);
            Self::init_var_slot_pool(&region, &layout);
            Self::init_guest_areas(&region, &layout);
        }

        Ok(Self {
            backing: ShmBacking::Heap(backing),
            path: None,
            region,
            layout,
            guests: HashMap::new(),
        })
    }

    /// Get the path to the segment file, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the segment configuration.
    ///
    /// Returns the config that was used to create this segment.
    #[inline]
    pub fn config(&self) -> &SegmentConfig {
        &self.layout.config
    }

    /// Add a new peer, returning the spawn ticket.
    ///
    /// This reserves a peer slot and creates a doorbell pair.
    /// The returned ticket should be passed to the spawned process
    /// via command-line arguments.
    ///
    /// shm[impl shm.spawn.ticket]
    /// shm[impl shm.doorbell.socketpair]
    pub fn add_peer(&mut self, options: AddPeerOptions) -> io::Result<SpawnTicket> {
        // Must have a path for file-backed segments
        let hub_path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::other("add_peer requires a file-backed segment"))?;

        // Find and reserve an empty slot
        let peer_id = self.reserve_peer_slot()?;

        // Create doorbell pair
        let (host_doorbell, guest_handle) = Doorbell::create_pair()?;

        // On Unix, clear CLOEXEC on guest's doorbell so it's inherited by children
        // shm[impl shm.spawn.fd-inheritance]
        #[cfg(unix)]
        shm_primitives::clear_cloexec(guest_handle.as_raw_fd())?;

        // Track this peer
        tracing::debug!("add_peer: storing doorbell for {:?}", peer_id);
        self.guests.insert(
            peer_id,
            GuestState {
                name: options.peer_name.clone(),
                last_epoch: self.peer_entry(peer_id).epoch(),
                pending_slots: Vec::new(),
                doorbell: Some(host_doorbell),
                on_death: options.on_death,
                death_notified: false,
                stats: crate::diagnostic::PeerCallStats::new(),
            },
        );
        tracing::debug!("add_peer: doorbell stored for {:?}", peer_id);

        Ok(SpawnTicket::new(hub_path, peer_id, guest_handle))
    }

    /// Reserve a peer slot, returning its ID.
    fn reserve_peer_slot(&self) -> io::Result<PeerId> {
        for i in 1..=self.layout.config.max_guests as u8 {
            let Some(peer_id) = PeerId::from_index(i - 1) else {
                continue;
            };
            let entry = self.peer_entry(peer_id);

            if entry.try_reserve().is_ok() {
                return Ok(peer_id);
            }
        }

        Err(io::Error::other("no available peer slots"))
    }

    /// Release a reserved peer slot (if spawn fails).
    ///
    /// Call this if `Command::spawn()` fails after calling `add_peer()`.
    pub fn release_peer(&mut self, peer_id: PeerId) {
        let entry = self.peer_entry(peer_id);
        entry.release_reserved();
        self.guests.remove(&peer_id);
    }

    /// Initialize the segment header (v2).
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_header(region: &Region, layout: &SegmentLayout) {
        let header = unsafe { &mut *(region.as_ptr() as *mut SegmentHeader) };
        header.magic = MAGIC;
        header.version = VERSION;
        header.header_size = HEADER_SIZE as u32;
        header.total_size = layout.total_size;
        header.max_payload_size = layout.config.max_payload_size;
        header.initial_credit = layout.config.initial_credit;
        header.max_guests = layout.config.max_guests;
        // v2: ring_size field holds bipbuf_capacity
        header.ring_size = layout.config.bipbuf_capacity;
        header.peer_table_offset = layout.peer_table_offset;
        header.slot_region_offset = 0; // unused in v2
        // v2: slot_size must be 0 (fixed pools eliminated)
        header.slot_size = 0;
        // v2: slots_per_guest field holds inline_threshold
        header.slots_per_guest = layout.config.inline_threshold;
        header.max_channels = layout.config.max_channels;
        header.heartbeat_interval = layout.config.heartbeat_interval;
        // v2: var_slot_pool_offset is mandatory
        header.var_slot_pool_offset = layout.var_slot_pool_offset;
        header
            .current_size
            .store(layout.total_size, core::sync::atomic::Ordering::Release);
        header.guest_areas_offset = layout.guest_areas_offset;
        header.num_var_slot_classes = layout.config.var_slot_classes.len() as u32;
        // host_goodbye and reserved are already zeroed
    }

    /// Initialize the peer table.
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_peer_table(region: &Region, layout: &SegmentLayout) {
        for i in 0..layout.config.max_guests {
            let peer_id = PeerId::from_index(i as u8).unwrap();
            let offset = layout.peer_entry_offset(peer_id.get()) as usize;
            let entry = unsafe { &mut *(region.offset(offset) as *mut PeerEntry) };

            // In v2, ring_offset points to the G2H bipbuf, slot_pool_offset is unused (shared pool)
            entry.init(
                layout.guest_to_host_bipbuf_offset(peer_id.get()),
                0, // no per-guest slot pool in v2
                layout.guest_channel_table_offset(peer_id.get()),
            );
        }
    }

    /// Initialize the shared VarSlotPool.
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_var_slot_pool(region: &Region, layout: &SegmentLayout) {
        let var_pool_offset = layout.var_slot_pool_offset;
        let var_classes = &layout.config.var_slot_classes;
        let mut var_pool = VarSlotPool::new(*region, var_pool_offset, var_classes.to_vec());
        unsafe { var_pool.init() };
    }

    /// Initialize all guest areas (BipBuffers + channel tables).
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_guest_areas(region: &Region, layout: &SegmentLayout) {
        for i in 0..layout.config.max_guests {
            let peer_id = PeerId::from_index(i as u8).unwrap();

            // Initialize G2H BipBuffer header
            let g2h_offset = layout.guest_to_host_bipbuf_offset(peer_id.get()) as usize;
            let g2h_header = unsafe { &mut *(region.offset(g2h_offset) as *mut BipBufHeader) };
            g2h_header.init(layout.config.bipbuf_capacity);

            // Initialize H2G BipBuffer header
            let h2g_offset = layout.host_to_guest_bipbuf_offset(peer_id.get()) as usize;
            let h2g_header = unsafe { &mut *(region.offset(h2g_offset) as *mut BipBufHeader) };
            h2g_header.init(layout.config.bipbuf_capacity);

            // Initialize channel table
            // SAFETY: caller guarantees region is valid
            unsafe {
                Self::init_channel_table(
                    region,
                    layout.guest_channel_table_offset(peer_id.get()),
                    &layout.config,
                )
            };
        }
    }

    /// Initialize a channel table at the given offset.
    ///
    /// # Safety
    ///
    /// The region must be valid and the offset must be correct.
    unsafe fn init_channel_table(region: &Region, offset: u64, config: &SegmentConfig) {
        for i in 0..config.max_channels {
            let entry_offset = offset as usize + i as usize * CHANNEL_ENTRY_SIZE;
            let entry = unsafe { &mut *(region.offset(entry_offset) as *mut ChannelEntry) };
            entry.init();
        }
    }

    /// Get the segment header.
    fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.region.as_ptr() as *const SegmentHeader) }
    }

    /// Get a peer entry.
    fn peer_entry(&self, peer_id: PeerId) -> &PeerEntry {
        let offset = self.layout.peer_entry_offset(peer_id.get()) as usize;
        unsafe { &*(self.region.offset(offset) as *const PeerEntry) }
    }

    /// Create a BipBufRaw view for a guest's G2H (guest-to-host) buffer.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized BipBuffer at the computed offset.
    unsafe fn g2h_bipbuf(&self, peer_id: PeerId) -> BipBufRaw {
        let header_offset = self.layout.guest_to_host_bipbuf_offset(peer_id.get()) as usize;
        let data_offset = header_offset + BIPBUF_HEADER_SIZE;
        let header_ptr = self.region.offset(header_offset) as *mut BipBufHeader;
        let data_ptr = self.region.offset(data_offset);
        unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) }
    }

    /// Create a BipBufRaw view for a guest's H2G (host-to-guest) buffer.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized BipBuffer at the computed offset.
    unsafe fn h2g_bipbuf(&self, peer_id: PeerId) -> BipBufRaw {
        let header_offset = self.layout.host_to_guest_bipbuf_offset(peer_id.get()) as usize;
        let data_offset = header_offset + BIPBUF_HEADER_SIZE;
        let header_ptr = self.region.offset(header_offset) as *mut BipBufHeader;
        let data_ptr = self.region.offset(data_offset);
        unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) }
    }

    /// Create a VarSlotPool view for the shared pool.
    fn var_slot_pool(&self) -> VarSlotPool {
        let offset = self.layout.var_slot_pool_offset;
        let classes = self.layout.config.var_slot_classes.clone();
        VarSlotPool::new(self.region, offset, classes)
    }

    /// Poll all guest BipBuffers for incoming messages.
    ///
    /// shm[impl shm.host.poll-peers]
    pub fn poll(&mut self) -> PollResult {
        let mut result = PollResult::default();
        let mut crashed_guests = Vec::new();
        let mut goodbye_guests = Vec::new();
        let mut bad_guests = Vec::new();

        let inline_threshold = self.layout.config.effective_inline_threshold();

        for i in 0..self.layout.config.max_guests {
            let Some(peer_id) = PeerId::from_index(i as u8) else {
                continue;
            };
            let entry = self.peer_entry(peer_id);

            // Check peer state
            let state = entry.state();
            if state == PeerState::Empty {
                continue;
            }

            // Check for epoch change (crash detection)
            // shm[impl shm.crash.epoch]
            let current_epoch = entry.epoch();
            if let Some(guest_state) = self.guests.get(&peer_id)
                && guest_state.last_epoch != current_epoch
                && guest_state.last_epoch != 0
            {
                // Epoch changed unexpectedly - previous guest crashed
                crashed_guests.push(peer_id);
                continue;
            }

            if state == PeerState::Goodbye {
                // Guest is shutting down
                goodbye_guests.push(peer_id);
                continue;
            }

            // Read frames from the G2H BipBuffer
            // shm[impl shm.ordering.ring-consume]
            let g2h = unsafe { self.g2h_bipbuf(peer_id) };

            loop {
                let Some(readable) = g2h.try_read() else {
                    break; // Buffer empty
                };

                // We need at least SHM_FRAME_HEADER_SIZE bytes to parse a frame header
                if readable.len() < SHM_FRAME_HEADER_SIZE {
                    // Partial frame header -- should not happen in a well-behaved guest.
                    // Treat as protocol violation.
                    bad_guests.push(peer_id);
                    break;
                }

                let Some(frame_header) = ShmFrameHeader::read_from(readable) else {
                    bad_guests.push(peer_id);
                    break;
                };

                let total_len = frame_header.total_len as usize;
                if total_len < SHM_FRAME_HEADER_SIZE || total_len > readable.len() {
                    // Frame claims to be larger than what's available, or impossibly small.
                    // For the "larger" case this could be a partial write; treat as corruption.
                    bad_guests.push(peer_id);
                    break;
                }

                // Extract payload
                match self.extract_payload(
                    &frame_header,
                    &readable[..total_len],
                    peer_id,
                    inline_threshold,
                ) {
                    Ok(payload) => {
                        // Track if we freed a slot (non-inline payload)
                        if frame_header.has_slot_ref() && !result.slots_freed_for.contains(&peer_id)
                        {
                            result.slots_freed_for.push(peer_id);
                        }

                        let shm_msg = ShmMsg {
                            msg_type: frame_header.msg_type,
                            id: frame_header.id,
                            method_id: frame_header.method_id,
                            payload,
                        };
                        result.messages.push((peer_id, shm_msg));
                    }
                    Err(_e) => {
                        // Protocol violation: release this frame and schedule cleanup.
                        g2h.release(total_len as u32);
                        bad_guests.push(peer_id);
                        break;
                    }
                }

                // Release the consumed frame bytes
                g2h.release(total_len as u32);
            }

            // Update guest state
            self.guests
                .entry(peer_id)
                .or_insert(GuestState {
                    name: None,
                    last_epoch: current_epoch,
                    pending_slots: Vec::new(),
                    doorbell: None,
                    on_death: None,
                    death_notified: false,
                    stats: crate::diagnostic::PeerCallStats::new(),
                })
                .last_epoch = current_epoch;
        }

        // Prune pending_slots that guests have already freed
        {
            let var_pool = self.var_slot_pool();
            for state in self.guests.values_mut() {
                state.pending_slots.retain(|h| !var_pool.is_slot_free(h));
            }
        }

        // Handle crashed and goodbye guests after the loop
        for peer_id in crashed_guests {
            self.handle_guest_crash(peer_id);
        }
        for peer_id in goodbye_guests {
            self.handle_guest_goodbye(peer_id);
        }
        for peer_id in bad_guests {
            self.handle_guest_crash(peer_id);
        }

        result
    }

    /// Extract payload bytes from a received SHM frame.
    fn extract_payload(
        &self,
        header: &ShmFrameHeader,
        frame_bytes: &[u8],
        _peer_id: PeerId,
        _inline_threshold: u32,
    ) -> Result<Vec<u8>, RecvError> {
        if header.has_slot_ref() {
            // Payload is in VarSlotPool, referenced by SlotRef after header
            if frame_bytes.len() < SLOT_REF_FRAME_SIZE {
                return Err(RecvError::MalformedSlotRef);
            }
            let slot_ref = SlotRef::read_from(&frame_bytes[SHM_FRAME_HEADER_SIZE..])
                .ok_or(RecvError::MalformedSlotRef)?;

            let payload_len = header.payload_len as usize;
            if payload_len > self.layout.config.max_payload_size as usize {
                return Err(RecvError::PayloadTooLarge);
            }

            let var_pool = self.var_slot_pool();
            let handle = VarSlotHandle {
                class_idx: slot_ref.class_idx,
                extent_idx: slot_ref.extent_idx,
                slot_idx: slot_ref.slot_idx,
                generation: slot_ref.slot_generation,
            };

            // Validate payload_len fits within the slot's size class
            if let Some(slot_size) = var_pool.slot_size(slot_ref.class_idx) {
                if payload_len > slot_size as usize {
                    return Err(RecvError::PayloadTooLarge);
                }
            } else {
                return Err(RecvError::SlotPtrInvalid);
            }

            let payload_ptr = var_pool
                .payload_ptr(handle)
                .ok_or(RecvError::SlotPtrInvalid)?;
            let payload = unsafe { std::slice::from_raw_parts(payload_ptr, payload_len).to_vec() };

            // Free the slot
            var_pool.free(handle).map_err(|_| RecvError::FreeFailed)?;

            Ok(payload)
        } else {
            // Inline payload: bytes are in the frame after the header
            let payload_len = header.payload_len as usize;
            if payload_len > self.layout.config.max_payload_size as usize {
                return Err(RecvError::PayloadTooLarge);
            }

            let payload_start = SHM_FRAME_HEADER_SIZE;
            let payload_end = payload_start + payload_len;
            if payload_end > frame_bytes.len() {
                return Err(RecvError::MalformedFrame);
            }

            if payload_len == 0 {
                Ok(Vec::new())
            } else {
                Ok(frame_bytes[payload_start..payload_end].to_vec())
            }
        }
    }

    /// Send a message to a specific guest.
    ///
    /// shm[impl shm.topology.hub.calls]
    pub fn send(&mut self, peer_id: PeerId, msg: &ShmMsg) -> Result<(), SendError> {
        // Check peer state
        let (peer_state, current_epoch) = {
            let entry = self.peer_entry(peer_id);
            (entry.state(), entry.epoch())
        };

        if peer_state != PeerState::Attached {
            return Err(SendError::PeerNotAttached);
        }

        let inline_threshold = self.layout.config.effective_inline_threshold();

        let payload_data: &[u8] = &msg.payload;
        let payload_len = payload_data.len() as u32;

        if payload_data.len() > self.layout.config.max_payload_size as usize {
            return Err(SendError::PayloadTooLarge);
        }

        let h2g = unsafe { self.h2g_bipbuf(peer_id) };

        if shm_frame::should_inline(payload_len, inline_threshold) {
            // Inline frame: header + payload, padded to 4 bytes
            let total_len = shm_frame::inline_frame_size(payload_len);

            let Some(grant) = h2g.try_grant(total_len) else {
                return Err(SendError::RingFull);
            };

            shm_frame::encode_inline_frame(
                msg.msg_type,
                msg.id,
                msg.method_id,
                payload_data,
                grant,
            );

            h2g.commit(total_len);
        } else {
            // Slot-referenced frame: alloc from VarSlotPool, write payload there
            let var_pool = self.var_slot_pool();
            // The host is peer 0 (convention for ownership tracking)
            let Some(handle) = var_pool.alloc(payload_len, 0) else {
                // shm[impl shm.slot.exhaustion]
                warn!(
                    payload_len,
                    ?peer_id,
                    "slot exhaustion: VarSlotPool has no capacity for this payload"
                );
                return Err(SendError::SlotExhausted);
            };

            // Copy payload into the slot
            let Some(slot_ptr) = var_pool.payload_ptr(handle) else {
                let _ = var_pool.free_allocated(handle);
                return Err(SendError::PayloadTooLarge);
            };
            unsafe {
                ptr::copy_nonoverlapping(payload_data.as_ptr(), slot_ptr, payload_data.len());
            }

            // Mark as in-flight
            if var_pool.mark_in_flight(handle).is_err() {
                let _ = var_pool.free_allocated(handle);
                return Err(SendError::SlotExhausted);
            }

            // Write slot-ref frame into BipBuffer
            let slot_ref = SlotRef {
                class_idx: handle.class_idx,
                extent_idx: handle.extent_idx,
                slot_idx: handle.slot_idx,
                slot_generation: handle.generation,
            };

            let total_len = SLOT_REF_FRAME_SIZE as u32;
            let Some(grant) = h2g.try_grant(total_len) else {
                let _ = var_pool.free(handle);
                return Err(SendError::RingFull);
            };

            shm_frame::encode_slot_ref_frame(
                msg.msg_type,
                msg.id,
                msg.method_id,
                payload_len,
                &slot_ref,
                grant,
            );

            h2g.commit(total_len);

            // Track for crash recovery
            let state = self.guests.entry(peer_id).or_insert(GuestState {
                name: None,
                last_epoch: current_epoch,
                pending_slots: Vec::new(),
                doorbell: None,
                on_death: None,
                death_notified: false,
                stats: crate::diagnostic::PeerCallStats::new(),
            });
            state.pending_slots.push(handle);
        }

        Ok(())
    }

    /// Handle a guest crash.
    ///
    /// shm[impl shm.crash.recovery]
    /// shm[impl shm.death.callback]
    fn handle_guest_crash(&mut self, peer_id: PeerId) {
        // Set state to Goodbye
        {
            let entry = self.peer_entry(peer_id);
            entry.set_goodbye();
        }

        // Reset BipBuffer headers
        let g2h_offset = self.layout.guest_to_host_bipbuf_offset(peer_id.get()) as usize;
        let g2h_header = unsafe { &mut *(self.region.offset(g2h_offset) as *mut BipBufHeader) };
        g2h_header.reset();

        let h2g_offset = self.layout.host_to_guest_bipbuf_offset(peer_id.get()) as usize;
        let h2g_header = unsafe { &mut *(self.region.offset(h2g_offset) as *mut BipBufHeader) };
        h2g_header.reset();

        // Recover VarSlotPool slots owned by this peer
        let var_pool = self.var_slot_pool();
        var_pool.recover_peer(peer_id.get());

        // Free pending host-allocated slots and invoke death callback
        if let Some(mut state) = self.guests.remove(&peer_id) {
            for handle in state.pending_slots.drain(..) {
                // These are host-allocated (owner=0), recover_peer(peer_id) won't catch them.
                // Try to free them directly.
                let _ = var_pool.free(handle);
            }

            // Invoke death callback if registered and not already notified
            // shm[impl shm.death.callback]
            // shm[impl shm.death.callback-context]
            if !state.death_notified {
                state.death_notified = true;
                if let Some(ref callback) = state.on_death {
                    callback(peer_id);
                }
            }
        }

        // Reset channel table entries
        let channel_table_offset = self.layout.guest_channel_table_offset(peer_id.get());
        for i in 0..self.layout.config.max_channels {
            let entry_offset = channel_table_offset as usize + i as usize * CHANNEL_ENTRY_SIZE;
            let channel_entry =
                unsafe { &*(self.region.offset(entry_offset) as *const ChannelEntry) };
            channel_entry.reset_to_free();
        }

        // Reset peer entry to Empty
        self.peer_entry(peer_id).reset();
    }

    /// Handle a guest goodbye.
    fn handle_guest_goodbye(&mut self, peer_id: PeerId) {
        // Reset BipBuffer headers
        let g2h_offset = self.layout.guest_to_host_bipbuf_offset(peer_id.get()) as usize;
        let g2h_header = unsafe { &mut *(self.region.offset(g2h_offset) as *mut BipBufHeader) };
        g2h_header.reset();

        let h2g_offset = self.layout.host_to_guest_bipbuf_offset(peer_id.get()) as usize;
        let h2g_header = unsafe { &mut *(self.region.offset(h2g_offset) as *mut BipBufHeader) };
        h2g_header.reset();

        // Recover VarSlotPool slots owned by this peer
        let var_pool = self.var_slot_pool();
        var_pool.recover_peer(peer_id.get());

        // Free pending host-allocated slots
        if let Some(state) = self.guests.remove(&peer_id) {
            for handle in state.pending_slots {
                let _ = var_pool.free(handle);
            }
        }

        // Reset peer entry to Empty so slot can be reused
        self.peer_entry(peer_id).reset();
    }

    /// Check all peer doorbells for death events.
    ///
    /// Returns a list of peers that have died (doorbell shows peer disconnected).
    /// This should be called periodically to detect crashed guests.
    ///
    /// shm[impl shm.doorbell.death]
    /// shm[impl shm.death.detection-methods]
    pub async fn check_doorbell_deaths(&mut self) -> Vec<PeerId> {
        let mut dead_peers = Vec::new();

        for (&peer_id, state) in &self.guests {
            if state.death_notified {
                continue;
            }

            if let Some(ref doorbell) = state.doorbell {
                // Try to signal - if peer is dead, we'll find out
                if doorbell.signal().await == SignalResult::PeerDead {
                    dead_peers.push(peer_id);
                }
            }
        }

        // Handle deaths
        for peer_id in &dead_peers {
            self.handle_guest_crash(*peer_id);
        }

        dead_peers
    }

    /// Signal a guest's doorbell after sending a message.
    ///
    /// This wakes up a guest that might be waiting for messages.
    ///
    /// shm[impl shm.doorbell.ring-integration]
    pub async fn ring_doorbell(&self, peer_id: PeerId) -> Option<SignalResult> {
        if let Some(doorbell) = self
            .guests
            .get(&peer_id)
            .and_then(|state| state.doorbell.as_ref())
        {
            Some(doorbell.signal().await)
        } else {
            None
        }
    }

    /// Take ownership of a peer's doorbell (for async waiting in driver).
    ///
    /// This removes the doorbell from the host's tracking, transferring ownership
    /// to the caller (typically the driver). After this call, `ring_doorbell()` and
    /// `check_doorbell_deaths()` will no longer have access to this peer's doorbell.
    ///
    /// Returns `None` if the peer doesn't exist or has no doorbell.
    pub fn take_doorbell(&mut self, peer_id: PeerId) -> Option<Doorbell> {
        self.guests
            .get_mut(&peer_id)
            .and_then(|state| state.doorbell.take())
    }

    /// Initiate host goodbye (graceful shutdown).
    ///
    /// shm[impl shm.goodbye.host]
    pub fn goodbye(&self, _reason: &str) {
        self.header().set_host_goodbye(1);
    }

    /// Check if host has said goodbye.
    pub fn is_goodbye(&self) -> bool {
        self.header().is_host_goodbye()
    }

    /// Get the number of attached guests.
    pub fn attached_guest_count(&self) -> usize {
        (0..self.layout.config.max_guests)
            .filter(|&i| {
                PeerId::from_index(i as u8)
                    .map(|id| self.peer_entry(id).state() == PeerState::Attached)
                    .unwrap_or(false)
            })
            .count()
    }

    /// Get the segment layout.
    pub fn layout(&self) -> &SegmentLayout {
        &self.layout
    }

    /// Get a Region view of the segment.
    ///
    /// This is needed for guests to attach to the segment.
    pub fn region(&self) -> Region {
        self.region
    }

    /// Grow a variable-size slot pool size class by adding a new extent.
    ///
    /// This appends a new extent to the segment file, initializes it with
    /// free slots, and atomically updates the extent count. Guests will
    /// detect the size change via `current_size` in the segment header
    /// and remap accordingly.
    ///
    /// Returns the new extent index (1 or 2) on success.
    ///
    /// # Errors
    ///
    /// - `GrowError::MaxExtentsReached` - Size class already has maximum extents
    /// - `GrowError::HeapBackedNotSupported` - Cannot grow heap-backed segments
    /// - `GrowError::Io` - File resize or remap failed
    ///
    /// shm[impl shm.varslot.extents]
    pub fn grow_size_class(&mut self, class_idx: usize) -> Result<u32, GrowError> {
        let var_pool_offset = self.layout.var_slot_pool_offset;

        // Must be file-backed to grow
        let mmap = match &mut self.backing {
            ShmBacking::Mmap(m) => m,
            ShmBacking::Heap(_) => return Err(GrowError::HeapBackedNotSupported),
        };

        // Get size class configuration
        let var_classes = &self.layout.config.var_slot_classes;

        if class_idx >= var_classes.len() {
            return Err(GrowError::InvalidClassIndex);
        }

        let class = &var_classes[class_idx];
        let slot_size = class.slot_size;
        let slots_per_extent = class.count;

        // Read current extent count from header (before resize)
        let class_header_offset = var_pool_offset as usize + class_idx * 64;
        let current_extent_count = {
            let class_header =
                unsafe { &*(self.region.offset(class_header_offset) as *const SizeClassHeader) };
            class_header.extent_count.load(Ordering::Acquire)
        };

        if current_extent_count >= MAX_EXTENTS_PER_CLASS as u32 {
            return Err(GrowError::MaxExtentsReached);
        }

        let new_extent_idx = current_extent_count;

        // Calculate extent size and new segment size
        let extent_size = ExtentHeader::extent_size(slot_size, slots_per_extent);
        let current_size = mmap.len();
        let new_size = current_size + extent_size as usize;

        // Resize the backing file and remap
        mmap.resize(new_size)?;

        // Update our region view (pointer may have changed after remap)
        self.region = mmap.region();

        // Calculate extent offset (at end of old size)
        let extent_offset = current_size as u64;

        // Initialize extent header (using fresh pointer after remap)
        let extent_header =
            unsafe { &mut *(self.region.offset(extent_offset as usize) as *mut ExtentHeader) };
        extent_header.magic = EXTENT_MAGIC;
        extent_header.class_idx = class_idx as u32;
        extent_header.extent_idx = new_extent_idx;
        extent_header.slot_count = slots_per_extent;
        extent_header.slot_size = slot_size;
        extent_header._reserved = [0; 40];

        // Reacquire class header pointer after remap (old pointer is stale)
        let class_header =
            unsafe { &*(self.region.offset(class_header_offset) as *const SizeClassHeader) };

        // Build free list for the new extent
        // Construct a temporary VarSlotPool view and init the extent
        let var_classes_vec: Vec<_> = var_classes.to_vec();
        let mut var_pool = VarSlotPool::new(self.region, var_pool_offset, var_classes_vec);

        // Store the extent offset in the class header
        class_header.extent_offsets[new_extent_idx as usize - 1]
            .store(extent_offset, Ordering::Release);

        // Initialize the extent's slots and free list
        // SAFETY: We have exclusive access during growth
        unsafe {
            var_pool.init_extent_slots(class_idx, new_extent_idx as usize);
        }

        // Atomically increment extent count (makes extent visible to allocators)
        class_header
            .extent_count
            .store(new_extent_idx + 1, Ordering::Release);

        // Update segment header's current_size
        let header = self.header();
        header
            .current_size
            .store(new_size as u64, Ordering::Release);

        Ok(new_extent_idx)
    }
}

/// Errors from send operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// Peer is not attached
    PeerNotAttached,
    /// BipBuffer is full (backpressure)
    RingFull,
    /// Payload is too large for configured limits
    PayloadTooLarge,
    /// No slots available (backpressure)
    SlotExhausted,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::PeerNotAttached => write!(f, "peer not attached"),
            SendError::RingFull => write!(f, "ring full"),
            SendError::PayloadTooLarge => write!(f, "payload too large"),
            SendError::SlotExhausted => write!(f, "slot exhausted"),
        }
    }
}

impl std::error::Error for SendError {}

/// Errors from grow operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrowError {
    /// Size class already has maximum number of extents
    MaxExtentsReached,
    /// Invalid size class index
    InvalidClassIndex,
    /// Cannot grow heap-backed segments (only file-backed)
    HeapBackedNotSupported,
    /// I/O error during resize or remap
    Io(String),
}

impl std::fmt::Display for GrowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrowError::MaxExtentsReached => write!(f, "size class has maximum extents"),
            GrowError::InvalidClassIndex => write!(f, "invalid size class index"),
            GrowError::HeapBackedNotSupported => {
                write!(f, "cannot grow heap-backed segments")
            }
            GrowError::Io(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for GrowError {}

impl From<io::Error> for GrowError {
    fn from(err: io::Error) -> Self {
        GrowError::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_host() {
        let config = SegmentConfig::default();
        let host = ShmHost::create_heap(config).unwrap();

        // Verify header
        let header = host.header();
        assert_eq!(header.magic, MAGIC);
        assert_eq!(header.version, VERSION);
        assert!(!host.is_goodbye());
    }

    #[test]
    fn host_goodbye() {
        let config = SegmentConfig::default();
        let host = ShmHost::create_heap(config).unwrap();

        assert!(!host.is_goodbye());
        host.goodbye("test shutdown");
        assert!(host.is_goodbye());
    }

    #[test]
    fn poll_empty() {
        let config = SegmentConfig::default();
        let mut host = ShmHost::create_heap(config).unwrap();

        let result = host.poll();
        assert!(result.messages.is_empty());
    }
}
