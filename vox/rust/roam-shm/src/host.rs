//! Host-side SHM implementation.
//!
//! The host creates and owns the shared memory segment. It initializes
//! the segment header, peer table, and per-guest areas.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::Ordering;

use roam_frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
use shm_primitives::{Doorbell, HeapRegion, MmapRegion, Region, SignalResult, SlotHandle};

use crate::channel::ChannelEntry;
use crate::layout::{
    CHANNEL_ENTRY_SIZE, DESC_SIZE, EXTENT_MAGIC, ExtentHeader, HEADER_SIZE, MAGIC,
    MAX_EXTENTS_PER_CLASS, SegmentConfig, SegmentHeader, SegmentLayout, VERSION,
};
use crate::peer::{PeerEntry, PeerId, PeerState};
use crate::slot_pool::SlotPool;
#[cfg(unix)]
use crate::spawn::{AddPeerOptions, DeathCallback, SpawnTicket};
#[cfg(windows)]
use crate::spawn_windows::{AddPeerOptions, DeathCallback, SpawnTicket};
use crate::var_slot_pool::{SizeClassHeader, VarSlotPool};

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
    guests: HashMap<PeerId, GuestState>,
    /// Host's slot pool
    host_slots: SlotPool,
    /// Host's local head for each guest's H→G ring
    host_to_guest_heads: HashMap<PeerId, u64>,
}

/// Host-side state for a single guest.
struct GuestState {
    /// Human-readable name for debugging
    #[allow(dead_code)]
    name: Option<String>,
    /// Last observed epoch
    last_epoch: u32,
    /// Slots we've allocated for messages to this guest
    pending_slots: Vec<SlotHandle>,
    /// Host's doorbell for this peer (if spawned via add_peer)
    doorbell: Option<Doorbell>,
    /// Death callback (if registered via add_peer)
    on_death: Option<DeathCallback>,
    /// Whether we've already notified death for this peer
    death_notified: bool,
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
        let backing = MmapRegion::create(path.as_ref(), layout.total_size as usize)?;
        let region = backing.region();

        // Initialize segment header
        // SAFETY: We just allocated this memory and it's zeroed
        unsafe {
            Self::init_header(&region, &layout);
            Self::init_peer_table(&region, &layout);
            Self::init_slot_pools(&region, &layout);
            Self::init_guest_areas(&region, &layout);
        }

        let host_slots = SlotPool::new(region, layout.host_slot_pool_offset(), &config);

        Ok(Self {
            backing: ShmBacking::Mmap(backing),
            path: Some(path.as_ref().to_path_buf()),
            region,
            layout,
            guests: HashMap::new(),
            host_slots,
            host_to_guest_heads: HashMap::new(),
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

        // Initialize segment header
        // SAFETY: We just allocated this memory and it's zeroed
        unsafe {
            Self::init_header(&region, &layout);
            Self::init_peer_table(&region, &layout);
            Self::init_slot_pools(&region, &layout);
            Self::init_guest_areas(&region, &layout);
        }

        let host_slots = SlotPool::new(region, layout.host_slot_pool_offset(), &config);

        Ok(Self {
            backing: ShmBacking::Heap(backing),
            path: None,
            region,
            layout,
            guests: HashMap::new(),
            host_slots,
            host_to_guest_heads: HashMap::new(),
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
    #[cfg(unix)]
    pub fn add_peer(&mut self, options: AddPeerOptions) -> io::Result<SpawnTicket> {
        // Must have a path for file-backed segments
        let hub_path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::other("add_peer requires a file-backed segment"))?;

        // Find and reserve an empty slot
        let peer_id = self.reserve_peer_slot()?;

        // Create doorbell pair
        let (host_doorbell, guest_fd) = Doorbell::create_pair()?;

        // Clear CLOEXEC on guest's doorbell so it's inherited by children
        // shm[impl shm.spawn.fd-inheritance]
        shm_primitives::clear_cloexec(guest_fd)?;

        // Track this peer
        self.guests.insert(
            peer_id,
            GuestState {
                name: options.peer_name.clone(),
                last_epoch: self.peer_entry(peer_id).epoch(),
                pending_slots: Vec::new(),
                doorbell: Some(host_doorbell),
                on_death: options.on_death,
                death_notified: false,
            },
        );

        Ok(SpawnTicket::new(hub_path, peer_id, guest_fd))
    }

    /// Add a new peer, returning the spawn ticket.
    ///
    /// This reserves a peer slot and creates a doorbell pair.
    /// The returned ticket should be passed to the spawned process
    /// via command-line arguments.
    ///
    /// On Windows, uses named pipes instead of socketpairs.
    ///
    /// shm[impl shm.spawn.ticket]
    #[cfg(windows)]
    pub fn add_peer(&mut self, options: AddPeerOptions) -> io::Result<SpawnTicket> {
        // Must have a path for file-backed segments
        let hub_path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::other("add_peer requires a file-backed segment"))?;

        // Find and reserve an empty slot
        let peer_id = self.reserve_peer_slot()?;

        // Create doorbell pair (returns pipe name on Windows)
        let (host_doorbell, pipe_name) = Doorbell::create_pair()?;

        // Track this peer
        self.guests.insert(
            peer_id,
            GuestState {
                name: options.peer_name.clone(),
                last_epoch: self.peer_entry(peer_id).epoch(),
                pending_slots: Vec::new(),
                doorbell: Some(host_doorbell),
                on_death: options.on_death,
                death_notified: false,
            },
        );

        Ok(SpawnTicket::new(hub_path, peer_id, pipe_name))
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

    /// Initialize the segment header.
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
        header.ring_size = layout.config.ring_size;
        header.peer_table_offset = layout.peer_table_offset;
        header.slot_region_offset = layout.slot_region_offset;
        header.slot_size = layout.config.slot_size;
        header.slots_per_guest = layout.config.slots_per_guest;
        header.max_channels = layout.config.max_channels;
        header.heartbeat_interval = layout.config.heartbeat_interval;
        header.var_slot_pool_offset = layout.var_slot_pool_offset.unwrap_or(0);
        header
            .current_size
            .store(layout.total_size, core::sync::atomic::Ordering::Release);
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

            entry.init(
                layout.guest_rings_offset(peer_id.get()),
                layout.guest_slot_pool_offset(peer_id.get()),
                layout.guest_channel_table_offset(peer_id.get()),
            );
        }
    }

    /// Initialize all slot pools (host + per-guest or shared var pool).
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_slot_pools(region: &Region, layout: &SegmentLayout) {
        if let Some(ref var_classes) = layout.config.var_slot_classes {
            // Initialize shared variable-size slot pool
            let var_pool_offset = layout.var_slot_pool_offset.unwrap();
            let mut var_pool = VarSlotPool::new(*region, var_pool_offset, var_classes.to_vec());
            unsafe { var_pool.init() };
        } else {
            // Initialize fixed-size per-guest pools
            // Host pool
            unsafe { SlotPool::init(region, layout.host_slot_pool_offset(), &layout.config) };

            // Guest pools
            for i in 0..layout.config.max_guests {
                let peer_id = PeerId::from_index(i as u8).unwrap();
                unsafe {
                    SlotPool::init(
                        region,
                        layout.guest_slot_pool_offset(peer_id.get()),
                        &layout.config,
                    )
                };
            }
        }
    }

    /// Initialize all guest areas.
    ///
    /// # Safety
    ///
    /// The region must be valid and exclusively owned.
    unsafe fn init_guest_areas(region: &Region, layout: &SegmentLayout) {
        for i in 0..layout.config.max_guests {
            let peer_id = PeerId::from_index(i as u8).unwrap();

            // Initialize rings (zeroed by HeapRegion)
            // shm[impl shm.ring.initialization]

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

    /// Poll all guest rings for incoming messages.
    ///
    /// shm[impl shm.host.poll-peers]
    ///
    /// Returns an iterator over (peer_id, frame) pairs.
    pub fn poll(&mut self) -> Vec<(PeerId, Frame)> {
        let mut messages = Vec::new();
        let mut crashed_guests = Vec::new();
        let mut goodbye_guests = Vec::new();
        let mut bad_guests = Vec::new();

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

            // Dequeue messages from Guest→Host ring
            // shm[impl shm.ordering.ring-consume]
            let ring_offset = self.layout.guest_to_host_ring_offset(peer_id.get());
            let ring_size = self.layout.config.ring_size;

            loop {
                let tail = entry.g2h_tail();
                let head = entry.g2h_head();

                if tail >= head {
                    break; // Ring empty
                }

                let slot = (tail % ring_size) as usize;
                let desc_offset = ring_offset as usize + slot * DESC_SIZE;
                let desc = unsafe { ptr::read(self.region.offset(desc_offset) as *const MsgDesc) };

                // Get payload
                match self.get_payload(&desc, peer_id) {
                    Ok(payload) => {
                        let frame = Frame { desc, payload };
                        messages.push((peer_id, frame));
                    }
                    Err(_e) => {
                        // Protocol violation / corrupted descriptor: treat as guest crash.
                        // Advance tail once to avoid stalling the poll loop, then schedule cleanup.
                        entry.g2h_advance_tail(tail.wrapping_add(1));
                        bad_guests.push(peer_id);
                        break;
                    }
                }

                // Advance tail
                entry.g2h_advance_tail(tail.wrapping_add(1));
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
                })
                .last_epoch = current_epoch;
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

        messages
    }

    /// Get payload from a descriptor and free the slot.
    fn get_payload(&self, desc: &MsgDesc, peer_id: PeerId) -> Result<Payload, RecvError> {
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
            let pool_offset = self.layout.guest_slot_pool_offset(peer_id.get());
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

            // Verify generation to detect ABA.
            if pool.generation(desc.payload_slot) != Some(desc.payload_generation) {
                return Err(RecvError::GenerationMismatch);
            }

            let Some(payload_ptr) = pool.payload_ptr(desc.payload_slot, desc.payload_offset) else {
                return Err(RecvError::SlotBoundsOutOfRange);
            };

            let payload = unsafe { std::slice::from_raw_parts(payload_ptr, len).to_vec() };

            // Free the slot (return to guest's pool)
            // shm[impl shm.slot.free]
            let handle = SlotHandle {
                index: desc.payload_slot,
                generation: desc.payload_generation,
            };
            pool.free(handle).map_err(|_| RecvError::FreeFailed)?;

            Ok(Payload::Owned(payload))
        }
    }

    /// Send a message to a specific guest.
    ///
    /// shm[impl shm.topology.hub.calls]
    pub fn send(&mut self, peer_id: PeerId, frame: Frame) -> Result<(), SendError> {
        // Check peer state and get needed values first
        let (peer_state, h2g_head, h2g_tail, current_epoch) = {
            let entry = self.peer_entry(peer_id);
            (
                entry.state(),
                entry.h2g_head(),
                entry.h2g_tail(),
                entry.epoch(),
            )
        };

        if peer_state != PeerState::Attached {
            return Err(SendError::PeerNotAttached);
        }

        // Get H→G ring info
        let ring_offset = self.layout.host_to_guest_ring_offset(peer_id.get());
        let ring_size = self.layout.config.ring_size;

        // Get or initialize local head
        let local_head = self
            .host_to_guest_heads
            .entry(peer_id)
            .or_insert_with(|| h2g_head as u64);

        // Check if ring is full
        // shm[impl shm.ring.full]
        // Ring is full when (head + 1) % ring_size == tail
        // Using head - tail comparison: full when head - tail >= ring_size - 1
        let tail = h2g_tail as u64;
        if local_head.wrapping_sub(tail) >= ring_size as u64 - 1 {
            return Err(SendError::RingFull);
        }

        // Write descriptor
        // shm[impl shm.ordering.ring-publish]
        let slot = (*local_head % ring_size as u64) as usize;
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
                    // Need slot from host pool
                    // shm[impl shm.payload.slot]
                    let Some(handle) = self.host_slots.try_alloc() else {
                        // shm[impl shm.slot.exhaustion]
                        return Err(SendError::SlotExhausted);
                    };

                    let Some(payload_ptr) = self.host_slots.payload_ptr(handle.index, 0) else {
                        return Err(SendError::PayloadTooLarge);
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(data.as_ptr(), payload_ptr, data.len());
                    }

                    desc.payload_slot = handle.index;
                    desc.payload_generation = handle.generation;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;

                    // Track for crash recovery, but prune slots already reclaimed by the guest.
                    let state = self.guests.entry(peer_id).or_insert(GuestState {
                        name: None,
                        last_epoch: current_epoch,
                        pending_slots: Vec::new(),
                        doorbell: None,
                        on_death: None,
                        death_notified: false,
                    });
                    state.pending_slots.push(handle);
                    state
                        .pending_slots
                        .retain(|h| !self.host_slots.is_reclaimed(*h));
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
                    // Need slot from host pool
                    // shm[impl shm.payload.slot]
                    let Some(handle) = self.host_slots.try_alloc() else {
                        // shm[impl shm.slot.exhaustion]
                        return Err(SendError::SlotExhausted);
                    };

                    let Some(payload_ptr) = self.host_slots.payload_ptr(handle.index, 0) else {
                        return Err(SendError::PayloadTooLarge);
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(data.as_ptr(), payload_ptr, data.len());
                    }

                    desc.payload_slot = handle.index;
                    desc.payload_generation = handle.generation;
                    desc.payload_offset = 0;
                    desc.payload_len = data.len() as u32;

                    // Track for crash recovery, but prune slots already reclaimed by the guest.
                    let state = self.guests.entry(peer_id).or_insert(GuestState {
                        name: None,
                        last_epoch: current_epoch,
                        pending_slots: Vec::new(),
                        doorbell: None,
                        on_death: None,
                        death_notified: false,
                    });
                    state.pending_slots.push(handle);
                    state
                        .pending_slots
                        .retain(|h| !self.host_slots.is_reclaimed(*h));
                }
            }
        }

        // Write descriptor with Release ordering
        unsafe {
            ptr::write(self.region.offset(desc_offset) as *mut MsgDesc, desc);
        }

        // Publish head
        let new_head = local_head.wrapping_add(1);
        *local_head = new_head;
        let _ = local_head; // Release the mutable borrow
        self.peer_entry(peer_id).h2g_publish_head(new_head as u32);

        Ok(())
    }

    /// Handle a guest crash.
    ///
    /// shm[impl shm.crash.recovery]
    /// shm[impl shm.death.callback]
    fn handle_guest_crash(&mut self, peer_id: PeerId) {
        let entry = self.peer_entry(peer_id);

        // Set state to Goodbye
        entry.set_goodbye();

        // Reset rings
        entry.reset();

        // Reset guest slot pool bitmap (any in-flight guest→host messages are discarded).
        let guest_pool_offset = self.layout.guest_slot_pool_offset(peer_id.get());
        let guest_pool = SlotPool::new(self.region, guest_pool_offset, &self.layout.config);
        unsafe { guest_pool.reset_free_bitmap() };

        // Free pending slots and invoke death callback
        if let Some(mut state) = self.guests.remove(&peer_id) {
            for handle in state.pending_slots {
                let _ = self.host_slots.free(handle);
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

        // Clear local head tracking
        self.host_to_guest_heads.remove(&peer_id);
    }

    /// Handle a guest goodbye.
    fn handle_guest_goodbye(&mut self, peer_id: PeerId) {
        // Drain remaining messages before cleanup
        // (handled by poll loop)

        // Reset guest slot pool bitmap (any in-flight guest→host messages are discarded).
        let guest_pool_offset = self.layout.guest_slot_pool_offset(peer_id.get());
        let guest_pool = SlotPool::new(self.region, guest_pool_offset, &self.layout.config);
        unsafe { guest_pool.reset_free_bitmap() };

        // Free pending slots (no death callback for graceful goodbye)
        if let Some(state) = self.guests.remove(&peer_id) {
            for handle in state.pending_slots {
                let _ = self.host_slots.free(handle);
            }
        }

        // Clear local head tracking
        self.host_to_guest_heads.remove(&peer_id);

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
    pub fn check_doorbell_deaths(&mut self) -> Vec<PeerId> {
        let mut dead_peers = Vec::new();

        for (&peer_id, state) in &self.guests {
            if state.death_notified {
                continue;
            }

            if let Some(ref doorbell) = state.doorbell {
                // Try to signal - if peer is dead, we'll find out
                if doorbell.signal() == SignalResult::PeerDead {
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
    pub fn ring_doorbell(&self, peer_id: PeerId) -> Option<SignalResult> {
        self.guests
            .get(&peer_id)
            .and_then(|state| state.doorbell.as_ref())
            .map(|doorbell| doorbell.signal())
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
    /// - `GrowError::NoVarSlotPool` - Segment was not configured with var slot pools
    /// - `GrowError::MaxExtentsReached` - Size class already has maximum extents
    /// - `GrowError::HeapBackedNotSupported` - Cannot grow heap-backed segments
    /// - `GrowError::Io` - File resize or remap failed
    ///
    /// shm[impl shm.varslot.extents]
    pub fn grow_size_class(&mut self, class_idx: usize) -> Result<u32, GrowError> {
        // Check that we have var slot pools configured
        let var_pool_offset = self
            .layout
            .var_slot_pool_offset
            .ok_or(GrowError::NoVarSlotPool)?;

        // Must be file-backed to grow
        let mmap = match &mut self.backing {
            ShmBacking::Mmap(m) => m,
            ShmBacking::Heap(_) => return Err(GrowError::HeapBackedNotSupported),
        };

        // Get size class configuration
        let var_classes = self
            .layout
            .config
            .var_slot_classes
            .as_ref()
            .ok_or(GrowError::NoVarSlotPool)?;

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

        // Update host_slots pool region (pointer may have changed)
        self.host_slots.update_region(self.region);

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
    /// Segment was not configured with variable slot pools
    NoVarSlotPool,
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
            GrowError::NoVarSlotPool => write!(f, "segment has no variable slot pools"),
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
    use crate::guest::ShmGuest;
    use crate::msg::msg_type;
    use crate::peer::PeerEntry;

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

        let messages = host.poll();
        assert!(messages.is_empty());
    }

    #[test]
    fn guest_to_host_large_payloads_reclaim_guest_slots() {
        let config = SegmentConfig {
            max_guests: 1,
            ring_size: 8,
            slot_size: 64,
            slots_per_guest: 2,
            max_payload_size: 60,
            ..SegmentConfig::default()
        };

        let mut host = ShmHost::create_heap(config.clone()).unwrap();
        let mut guest = ShmGuest::attach(host.region()).unwrap();

        let payload = vec![0xAB; 40];
        for _ in 0..16 {
            let desc = MsgDesc::new(msg_type::DATA, 1, 0);
            let frame = Frame {
                desc,
                payload: Payload::Owned(payload.clone()),
            };

            guest.send(frame).unwrap();
            let messages = host.poll();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, guest.peer_id());
            assert_eq!(messages[0].1.payload_bytes(), payload.as_slice());
        }
    }

    #[test]
    fn host_to_guest_large_payloads_reclaim_host_slots() {
        let config = SegmentConfig {
            max_guests: 1,
            ring_size: 8,
            slot_size: 64,
            slots_per_guest: 2,
            max_payload_size: 60,
            ..SegmentConfig::default()
        };

        let mut host = ShmHost::create_heap(config.clone()).unwrap();
        let mut guest = ShmGuest::attach(host.region()).unwrap();

        let payload = vec![0xCD; 40];
        for _ in 0..16 {
            let desc = MsgDesc::new(msg_type::DATA, 1, 0);
            let frame = Frame {
                desc,
                payload: Payload::Owned(payload.clone()),
            };

            host.send(guest.peer_id(), frame).unwrap();
            let recv = guest.recv().expect("frame");
            assert_eq!(recv.payload_bytes(), payload.as_slice());
        }
    }

    #[test]
    fn malformed_guest_descriptor_disconnects_guest() {
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
        let peer_id = guest.peer_id();

        // Guest sends an inline payload.
        let desc = MsgDesc::new(msg_type::DATA, 1, 0);
        let frame = Frame {
            desc,
            payload: Payload::Inline,
        };
        guest.send(frame).unwrap();

        // Corrupt the descriptor in the ring: inline payload with non-zero generation.
        let ring_offset = host.layout().guest_to_host_ring_offset(peer_id.get());
        let desc_offset = ring_offset as usize; // first slot (tail=0)
        let mut corrupt = unsafe { ptr::read(region.offset(desc_offset) as *const MsgDesc) };
        corrupt.payload_generation = 1;
        unsafe {
            ptr::write(region.offset(desc_offset) as *mut MsgDesc, corrupt);
        }

        // Host should treat this as a crash and reset the peer.
        let msgs = host.poll();
        assert!(msgs.is_empty());

        let peer_entry_offset = host.layout().peer_entry_offset(peer_id.get()) as usize;
        let entry = unsafe { &*(region.offset(peer_entry_offset) as *const PeerEntry) };
        assert_eq!(entry.state(), PeerState::Empty);
    }
}
