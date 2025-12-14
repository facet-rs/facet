//! SHM memory layout definitions.
//!
//! This module defines the `repr(C)` structures that make up the shared memory
//! segment. These are the canonical layouts from DESIGN.md.
//!
//! # Memory Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  Segment Header (64 bytes, cache-line aligned)                       │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  A→B Descriptor Ring                                                 │
//! │    - Ring header (192 bytes: visible_head, tail, capacity + padding) │
//! │    - Descriptors (capacity × 64 bytes)                               │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  B→A Descriptor Ring                                                 │
//! │    - Ring header (192 bytes)                                         │
//! │    - Descriptors (capacity × 64 bytes)                               │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Data Segment Header (64 bytes)                                      │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Slot Metadata Array (slot_count × 8 bytes)                          │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Slot Data (slot_count × slot_size bytes)                            │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use rapace_core::MsgDescHot;

/// Magic bytes identifying a rapace SHM segment.
pub const MAGIC: [u8; 8] = *b"RAPACE\0\0";

/// Current protocol version (major.minor packed into u32).
/// Major = high 16 bits, minor = low 16 bits.
pub const PROTOCOL_VERSION: u32 = 1 << 16; // v1.0

/// Default descriptor ring capacity (power of 2).
pub const DEFAULT_RING_CAPACITY: u32 = 256;

/// Default slot size in bytes (4KB).
pub const DEFAULT_SLOT_SIZE: u32 = 4096;

/// Default number of slots.
pub const DEFAULT_SLOT_COUNT: u32 = 64;

/// Sentinel value indicating end of free list.
pub const FREE_LIST_END: u32 = u32::MAX;

// =============================================================================
// Segment Header
// =============================================================================

/// Segment header at the start of the SHM region (128 bytes).
///
/// Contains version info, feature flags, configuration, peer liveness tracking,
/// and futex words for signaling.
#[repr(C, align(64))]
pub struct SegmentHeader {
    /// Magic bytes: "RAPACE\0\0".
    pub magic: [u8; 8],
    /// Protocol version (major.minor packed).
    pub version: u32,
    /// Feature flags.
    pub flags: u32,

    // Configuration (so opener can discover it from the file)
    /// Descriptor ring capacity (power of 2).
    pub ring_capacity: u32,
    /// Size of each data slot in bytes.
    pub slot_size: u32,
    /// Number of data slots.
    pub slot_count: u32,
    /// Reserved for future config fields.
    pub _config_reserved: u32,

    // Peer liveness (for crash detection)
    /// Incremented by peer A periodically.
    pub peer_a_epoch: AtomicU64,
    /// Incremented by peer B periodically.
    pub peer_b_epoch: AtomicU64,
    /// Timestamp of last peer A heartbeat (nanos since epoch).
    pub peer_a_last_seen: AtomicU64,
    /// Timestamp of last peer B heartbeat (nanos since epoch).
    pub peer_b_last_seen: AtomicU64,

    // Futex words for cross-process signaling (16 bytes)
    /// A signals after enqueue to A→B ring, B waits when ring empty.
    pub a_to_b_data_futex: AtomicU32,
    /// B signals after dequeue from A→B ring, A waits when ring full.
    pub a_to_b_space_futex: AtomicU32,
    /// B signals after enqueue to B→A ring, A waits when ring empty.
    pub b_to_a_data_futex: AtomicU32,
    /// A signals after dequeue from B→A ring, B waits when ring full.
    pub b_to_a_space_futex: AtomicU32,

    /// Padding to 128 bytes.
    pub _pad: [u8; 48],
}

const _: () = assert!(core::mem::size_of::<SegmentHeader>() == 128);

impl SegmentHeader {
    /// Initialize a new segment header with the given configuration.
    pub fn init(&mut self, ring_capacity: u32, slot_size: u32, slot_count: u32) {
        self.magic = MAGIC;
        self.version = PROTOCOL_VERSION;
        self.flags = 0;
        self.ring_capacity = ring_capacity;
        self.slot_size = slot_size;
        self.slot_count = slot_count;
        self._config_reserved = 0;
        self.peer_a_epoch = AtomicU64::new(0);
        self.peer_b_epoch = AtomicU64::new(0);
        self.peer_a_last_seen = AtomicU64::new(0);
        self.peer_b_last_seen = AtomicU64::new(0);
        // Initialize futex words to 0
        self.a_to_b_data_futex = AtomicU32::new(0);
        self.a_to_b_space_futex = AtomicU32::new(0);
        self.b_to_a_data_futex = AtomicU32::new(0);
        self.b_to_a_space_futex = AtomicU32::new(0);
        self._pad = [0; 48];
    }

    /// Validate the header and return the embedded configuration.
    pub fn validate(&self) -> Result<(), LayoutError> {
        if self.magic != MAGIC {
            return Err(LayoutError::InvalidMagic);
        }
        let major = self.version >> 16;
        let our_major = PROTOCOL_VERSION >> 16;
        if major != our_major {
            return Err(LayoutError::IncompatibleVersion {
                expected: PROTOCOL_VERSION,
                found: self.version,
            });
        }
        // Validate config fields
        if !self.ring_capacity.is_power_of_two() || self.ring_capacity == 0 {
            return Err(LayoutError::InvalidConfig("ring_capacity must be non-zero power of 2"));
        }
        if self.slot_size == 0 {
            return Err(LayoutError::InvalidConfig("slot_size must be > 0"));
        }
        if self.slot_count == 0 {
            return Err(LayoutError::InvalidConfig("slot_count must be > 0"));
        }
        Ok(())
    }

    /// Extract the configuration from a validated header.
    pub fn config(&self) -> (u32, u32, u32) {
        (self.ring_capacity, self.slot_size, self.slot_count)
    }
}

// =============================================================================
// Descriptor Ring
// =============================================================================

/// SPSC descriptor ring header.
///
/// The ring uses a single-producer single-consumer design with cache-line
/// aligned head/tail to avoid false sharing.
///
/// Layout:
/// - visible_head on its own cache line (producer publishes)
/// - tail on its own cache line (consumer advances)
/// - capacity on its own cache line (immutable after init)
/// - descriptors follow immediately after
#[repr(C)]
pub struct DescRingHeader {
    /// Producer publication index (written by producer, read by consumer).
    pub visible_head: AtomicU64,
    _pad1: [u8; 56],

    /// Consumer index (written by consumer, read by producer).
    pub tail: AtomicU64,
    _pad2: [u8; 56],

    /// Ring capacity (power of 2, immutable after init).
    pub capacity: u32,
    _pad3: [u8; 60],
}

const _: () = assert!(core::mem::size_of::<DescRingHeader>() == 192);

impl DescRingHeader {
    /// Initialize a new ring header.
    pub fn init(&mut self, capacity: u32) {
        assert!(capacity.is_power_of_two(), "capacity must be power of 2");
        self.visible_head = AtomicU64::new(0);
        self._pad1 = [0; 56];
        self.tail = AtomicU64::new(0);
        self._pad2 = [0; 56];
        self.capacity = capacity;
        self._pad3 = [0; 60];
    }

    /// Returns the mask for index wrapping.
    #[inline]
    pub fn mask(&self) -> u64 {
        self.capacity as u64 - 1
    }

    /// Check if the ring is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        tail >= head
    }

    /// Check if the ring is full (given producer's local head).
    #[inline]
    pub fn is_full(&self, local_head: u64) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        local_head.wrapping_sub(tail) >= self.capacity as u64
    }

    /// Get the number of items in the ring.
    #[inline]
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        head.saturating_sub(tail) as usize
    }
}

/// A view into a descriptor ring in SHM.
///
/// This provides safe access to the ring operations. The actual descriptors
/// are stored immediately after the header in SHM.
pub struct DescRing {
    header: *mut DescRingHeader,
    descriptors: *mut MsgDescHot,
}

// SAFETY: DescRing is Send + Sync because it points to shared memory
// that is synchronized via atomics.
unsafe impl Send for DescRing {}
unsafe impl Sync for DescRing {}

impl DescRing {
    /// Create a ring view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, initialized `DescRingHeader` in SHM.
    /// - `descriptors` must point to `header.capacity` initialized `MsgDescHot` slots.
    /// - The memory must remain valid for the lifetime of this `DescRing`.
    pub unsafe fn from_raw(header: *mut DescRingHeader, descriptors: *mut MsgDescHot) -> Self {
        Self {
            header,
            descriptors,
        }
    }

    /// Get the ring header.
    #[inline]
    fn header(&self) -> &DescRingHeader {
        // SAFETY: Caller guaranteed valid pointer in from_raw.
        unsafe { &*self.header }
    }

    /// Get a mutable reference to a descriptor slot.
    ///
    /// # Safety
    ///
    /// Index must be < capacity.
    #[inline]
    unsafe fn desc_slot(&self, index: usize) -> *mut MsgDescHot {
        // SAFETY: Caller guarantees index < capacity.
        unsafe { self.descriptors.add(index) }
    }

    /// Enqueue a descriptor (producer side).
    ///
    /// `local_head` is producer-private (stack-local, not in SHM).
    /// On success, `local_head` is incremented.
    pub fn enqueue(&self, local_head: &mut u64, desc: &MsgDescHot) -> Result<(), RingError> {
        let header = self.header();

        if header.is_full(*local_head) {
            return Err(RingError::Full);
        }

        let idx = (*local_head & header.mask()) as usize;

        // SAFETY: idx < capacity (guaranteed by mask).
        unsafe {
            std::ptr::write(self.desc_slot(idx), *desc);
        }

        *local_head += 1;

        // Publish: make the descriptor visible to consumer.
        header.visible_head.store(*local_head, Ordering::Release);

        Ok(())
    }

    /// Dequeue a descriptor (consumer side).
    pub fn dequeue(&self) -> Option<MsgDescHot> {
        let header = self.header();

        let tail = header.tail.load(Ordering::Relaxed);
        let visible = header.visible_head.load(Ordering::Acquire);

        if tail >= visible {
            return None;
        }

        let idx = (tail & header.mask()) as usize;

        // SAFETY: idx < capacity (guaranteed by mask).
        let desc = unsafe { std::ptr::read(self.desc_slot(idx)) };

        // Advance tail.
        header.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Check if the ring is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.header().is_empty()
    }

    /// Get the capacity of the ring.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.header().capacity
    }
}

// =============================================================================
// Data Segment (Slab Allocator)
// =============================================================================

/// Slot state in the data segment.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Slot is free and available for allocation.
    Free = 0,
    /// Slot is allocated, sender is writing payload.
    Allocated = 1,
    /// Descriptor enqueued, awaiting receiver.
    InFlight = 2,
}

impl SlotState {
    /// Convert from u32.
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(SlotState::Free),
            1 => Some(SlotState::Allocated),
            2 => Some(SlotState::InFlight),
            _ => None,
        }
    }
}

/// Metadata for a single slot in the data segment.
#[repr(C)]
pub struct SlotMeta {
    /// Generation counter, incremented on each allocation.
    pub generation: AtomicU32,
    /// Current state (Free / Allocated / InFlight).
    pub state: AtomicU32,
}

const _: () = assert!(core::mem::size_of::<SlotMeta>() == 8);

impl SlotMeta {
    /// Initialize a new slot metadata entry.
    pub fn init(&mut self) {
        self.generation = AtomicU32::new(0);
        self.state = AtomicU32::new(SlotState::Free as u32);
    }

    /// Get the current state.
    #[inline]
    pub fn get_state(&self) -> SlotState {
        SlotState::from_u32(self.state.load(Ordering::Acquire)).unwrap_or(SlotState::Free)
    }

    /// Get the current generation.
    #[inline]
    pub fn get_generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }
}

/// Data segment header.
#[repr(C, align(64))]
pub struct DataSegmentHeader {
    /// Size of each slot in bytes.
    pub slot_size: u32,
    /// Number of slots.
    pub slot_count: u32,
    /// Maximum frame size (must be <= slot_size).
    pub max_frame_size: u32,
    _pad: u32,

    /// Free list head: index (low 32 bits) + tag (high 32 bits) for ABA safety.
    /// Uses tagged pointer to prevent ABA problem in lock-free free list.
    pub free_head: AtomicU64,

    /// Futex for slot availability signaling.
    /// Signaled when a slot is freed, waited on when allocation fails.
    pub slot_available: AtomicU32,

    _pad2: [u8; 36],
}

const _: () = assert!(core::mem::size_of::<DataSegmentHeader>() == 64);

impl DataSegmentHeader {
    /// Initialize a new data segment header.
    ///
    /// Note: The free list must be initialized separately via `DataSegment::init_free_list()`
    /// after the slot data region is available.
    pub fn init(&mut self, slot_size: u32, slot_count: u32) {
        self.slot_size = slot_size;
        self.slot_count = slot_count;
        self.max_frame_size = slot_size;
        self._pad = 0;
        // Free list starts empty (will be populated by init_free_list).
        // Using FREE_LIST_END with tag 0.
        self.free_head = AtomicU64::new(pack_free_head(FREE_LIST_END, 0));
        self.slot_available = AtomicU32::new(0);
        self._pad2 = [0; 36];
    }
}

/// Pack a free list head from index and tag.
#[inline]
fn pack_free_head(index: u32, tag: u32) -> u64 {
    ((tag as u64) << 32) | (index as u64)
}

/// Unpack a free list head into (index, tag).
#[inline]
fn unpack_free_head(packed: u64) -> (u32, u32) {
    let index = packed as u32;
    let tag = (packed >> 32) as u32;
    (index, tag)
}

/// A view into the data segment in SHM.
pub struct DataSegment {
    header: *mut DataSegmentHeader,
    slot_meta: *mut SlotMeta,
    slot_data: *mut u8,
}

// SAFETY: DataSegment is Send + Sync because it points to shared memory
// that is synchronized via atomics.
unsafe impl Send for DataSegment {}
unsafe impl Sync for DataSegment {}

impl DataSegment {
    /// Create a data segment view from raw pointers.
    ///
    /// # Safety
    ///
    /// - All pointers must be valid and properly aligned.
    /// - The memory must remain valid for the lifetime of this `DataSegment`.
    pub unsafe fn from_raw(
        header: *mut DataSegmentHeader,
        slot_meta: *mut SlotMeta,
        slot_data: *mut u8,
    ) -> Self {
        Self {
            header,
            slot_meta,
            slot_data,
        }
    }

    /// Get the header.
    #[inline]
    fn header(&self) -> &DataSegmentHeader {
        // SAFETY: Caller guaranteed valid pointer in from_raw.
        unsafe { &*self.header }
    }

    /// Get slot metadata.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count.
    #[inline]
    unsafe fn meta(&self, index: u32) -> &SlotMeta {
        // SAFETY: Caller guarantees index < slot_count.
        unsafe { &*self.slot_meta.add(index as usize) }
    }

    /// Get slot data pointer.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count.
    #[inline]
    unsafe fn data_ptr(&self, index: u32) -> *mut u8 {
        let slot_size = self.header().slot_size as usize;
        // SAFETY: Caller guarantees index < slot_count.
        unsafe { self.slot_data.add(index as usize * slot_size) }
    }

    /// Get slot data pointer (public version for allocator).
    ///
    /// # Safety
    ///
    /// Index must be < slot_count and the caller must own the slot.
    #[inline]
    pub unsafe fn data_ptr_public(&self, index: u32) -> *mut u8 {
        unsafe { self.data_ptr(index) }
    }

    // =========================================================================
    // Lock-free free list operations
    // =========================================================================

    /// Read the next_free link stored in the first 4 bytes of a slot's data.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count and the slot must be in a free state.
    #[inline]
    unsafe fn get_slot_next_free(&self, index: u32) -> u32 {
        let ptr = unsafe { self.data_ptr(index) as *const u32 };
        // Use atomic load for cross-process visibility
        unsafe { std::ptr::read_volatile(ptr) }
    }

    /// Write the next_free link to the first 4 bytes of a slot's data.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count and the caller must own the slot.
    #[inline]
    unsafe fn set_slot_next_free(&self, index: u32, next: u32) {
        let ptr = unsafe { self.data_ptr(index) as *mut u32 };
        // Use atomic store for cross-process visibility
        unsafe { std::ptr::write_volatile(ptr, next) };
    }

    /// Initialize the free list by linking all slots together.
    ///
    /// This should be called once when creating a new SHM segment.
    /// Each slot's data region stores the index of the next free slot.
    ///
    /// # Safety
    ///
    /// Must only be called during segment initialization, before any
    /// concurrent access.
    pub unsafe fn init_free_list(&self) {
        let slot_count = self.header().slot_count;

        if slot_count == 0 {
            return;
        }

        // Link slots: 0 -> 1 -> 2 -> ... -> (n-1) -> END
        for i in 0..slot_count - 1 {
            unsafe { self.set_slot_next_free(i, i + 1) };
        }
        // Last slot points to END
        unsafe { self.set_slot_next_free(slot_count - 1, FREE_LIST_END) };

        // Set free_head to slot 0 with tag 0
        let header = unsafe { &mut *self.header };
        header.free_head.store(pack_free_head(0, 0), Ordering::Release);
    }

    /// Allocate a slot using lock-free pop from free list.
    ///
    /// Returns (slot_index, generation) on success.
    ///
    /// This is O(1) on the happy path (no contention).
    pub fn alloc(&self) -> Result<(u32, u32), SlotError> {
        let header = unsafe { &*self.header };

        loop {
            // Load current head
            let old_head = header.free_head.load(Ordering::Acquire);
            let (index, tag) = unpack_free_head(old_head);

            // Check if list is empty
            if index == FREE_LIST_END {
                return Err(SlotError::NoFreeSlots);
            }

            // SAFETY: index < slot_count (it came from the free list)
            let next = unsafe { self.get_slot_next_free(index) };

            // Try to CAS head to next, incrementing tag to prevent ABA
            let new_head = pack_free_head(next, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // Successfully popped. Now mark the slot as Allocated.
                // SAFETY: index < slot_count
                let meta = unsafe { self.meta(index) };

                // Transition Free -> Allocated
                let result = meta.state.compare_exchange(
                    SlotState::Free as u32,
                    SlotState::Allocated as u32,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );

                if result.is_err() {
                    // Slot was not in Free state - this shouldn't happen if free list is consistent.
                    // Push it back and try again.
                    self.push_to_free_list(index);
                    continue;
                }

                // Increment generation
                let generation = meta.generation.fetch_add(1, Ordering::AcqRel) + 1;
                return Ok((index, generation));
            }
            // CAS failed, retry
        }
    }

    /// Push a slot onto the free list (lock-free).
    fn push_to_free_list(&self, index: u32) {
        let header = unsafe { &*self.header };

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (old_index, tag) = unpack_free_head(old_head);

            // Store the old head as our next pointer
            // SAFETY: index < slot_count
            unsafe { self.set_slot_next_free(index, old_index) };

            // Try to CAS head to point to us
            let new_head = pack_free_head(index, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
            // CAS failed, retry
        }
    }

    /// Mark a slot as in-flight (after enqueuing descriptor).
    pub fn mark_in_flight(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        if index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        // SAFETY: index < slot_count (checked above).
        let meta = unsafe { self.meta(index) };

        // Verify generation matches.
        if meta.get_generation() != expected_gen {
            return Err(SlotError::StaleGeneration);
        }

        // Transition Allocated -> InFlight.
        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::InFlight as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        result.map(|_| ()).map_err(|_| SlotError::InvalidState)
    }

    /// Free a slot (receiver side, after processing).
    ///
    /// After transitioning to Free state, the slot is pushed back onto the free list.
    pub fn free(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        if index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        // SAFETY: index < slot_count (checked above).
        let meta = unsafe { self.meta(index) };

        // Verify generation matches.
        if meta.get_generation() != expected_gen {
            return Err(SlotError::StaleGeneration);
        }

        // Transition InFlight -> Free.
        let result = meta.state.compare_exchange(
            SlotState::InFlight as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            // Push back onto free list
            self.push_to_free_list(index);
            // Signal anyone waiting for slots
            crate::futex::futex_signal(self.slot_available_futex());
            Ok(())
        } else {
            Err(SlotError::InvalidState)
        }
    }

    /// Get the slot availability futex for backpressure signaling.
    #[inline]
    pub fn slot_available_futex(&self) -> &AtomicU32 {
        unsafe { &(*self.header).slot_available }
    }

    /// Free a slot that's still in Allocated state (never sent).
    ///
    /// This is used by the allocator when data is dropped before being sent.
    /// After transitioning to Free state, the slot is pushed back onto the free list.
    pub fn free_allocated(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        if index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        // SAFETY: index < slot_count (checked above).
        let meta = unsafe { self.meta(index) };

        // Verify generation matches.
        if meta.get_generation() != expected_gen {
            return Err(SlotError::StaleGeneration);
        }

        // Transition Allocated -> Free.
        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            // Push back onto free list
            self.push_to_free_list(index);
            // Signal anyone waiting for slots
            crate::futex::futex_signal(self.slot_available_futex());
            Ok(())
        } else {
            Err(SlotError::InvalidState)
        }
    }

    /// Copy data into a slot.
    ///
    /// # Safety
    ///
    /// Caller must own the slot (Allocated state with matching generation).
    pub unsafe fn copy_to_slot(&self, index: u32, data: &[u8]) -> Result<(), SlotError> {
        let header = self.header();

        if index >= header.slot_count {
            return Err(SlotError::InvalidIndex);
        }

        if data.len() > header.slot_size as usize {
            return Err(SlotError::PayloadTooLarge { len: data.len(), max: header.slot_size as usize });
        }

        // SAFETY: index < slot_count, data.len() <= slot_size.
        let dst = unsafe { self.data_ptr(index) };
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        }

        Ok(())
    }

    /// Read data from a slot.
    ///
    /// # Safety
    ///
    /// Caller must have read access (InFlight state with matching generation).
    pub unsafe fn read_slot(&self, index: u32, offset: u32, len: u32) -> Result<&[u8], SlotError> {
        let header = self.header();

        if index >= header.slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let end = offset.saturating_add(len);
        if end > header.slot_size {
            return Err(SlotError::PayloadTooLarge { len: end as usize, max: header.slot_size as usize });
        }

        // SAFETY: bounds checked above.
        let ptr = unsafe { self.data_ptr(index).add(offset as usize) };
        Ok(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
    }

    /// Get slot size.
    #[inline]
    pub fn slot_size(&self) -> u32 {
        self.header().slot_size
    }

    /// Get slot count.
    #[inline]
    pub fn slot_count(&self) -> u32 {
        self.header().slot_count
    }

    /// Get slot status for debugging.
    ///
    /// Returns a struct with counts of slots in each state.
    pub fn slot_status(&self) -> SlotStatus {
        let slot_count = self.header().slot_count;
        let mut free = 0u32;
        let mut allocated = 0u32;
        let mut in_flight = 0u32;
        let mut unknown = 0u32;

        for i in 0..slot_count {
            // SAFETY: i < slot_count
            let meta = unsafe { self.meta(i) };
            match meta.get_state() {
                SlotState::Free => free += 1,
                SlotState::Allocated => allocated += 1,
                SlotState::InFlight => in_flight += 1,
            }
        }

        // Count free list length to verify consistency
        let mut free_list_len = 0u32;
        let header = unsafe { &*self.header };
        let mut current = {
            let (index, _tag) = unpack_free_head(header.free_head.load(Ordering::Acquire));
            index
        };
        while current != FREE_LIST_END && free_list_len < slot_count + 1 {
            free_list_len += 1;
            // SAFETY: current should be < slot_count if free list is consistent
            if current < slot_count {
                current = unsafe { self.get_slot_next_free(current) };
            } else {
                unknown += 1;
                break;
            }
        }

        SlotStatus {
            total: slot_count,
            free,
            allocated,
            in_flight,
            unknown,
            free_list_len,
        }
    }
}

/// Slot status for debugging.
#[derive(Debug, Clone, Copy)]
pub struct SlotStatus {
    /// Total number of slots.
    pub total: u32,
    /// Slots in Free state.
    pub free: u32,
    /// Slots in Allocated state.
    pub allocated: u32,
    /// Slots in InFlight state.
    pub in_flight: u32,
    /// Slots in unknown state (should be 0).
    pub unknown: u32,
    /// Length of free list (should match `free`).
    pub free_list_len: u32,
}

impl std::fmt::Display for SlotStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "slots: {}/{} free, {} allocated, {} in_flight (free_list_len={})",
            self.free, self.total, self.allocated, self.in_flight, self.free_list_len
        )?;
        if self.unknown > 0 {
            write!(f, ", {} UNKNOWN", self.unknown)?;
        }
        if self.free != self.free_list_len {
            write!(f, " [MISMATCH: free={} != free_list={}]", self.free, self.free_list_len)?;
        }
        Ok(())
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Errors from layout validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// Invalid magic bytes.
    InvalidMagic,
    /// Incompatible protocol version.
    IncompatibleVersion { expected: u32, found: u32 },
    /// Segment too small.
    SegmentTooSmall { required: usize, found: usize },
    /// Invalid configuration in header.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "invalid magic bytes"),
            Self::IncompatibleVersion { expected, found } => {
                write!(
                    f,
                    "incompatible version: expected {}.{}, found {}.{}",
                    expected >> 16,
                    expected & 0xFFFF,
                    found >> 16,
                    found & 0xFFFF
                )
            }
            Self::SegmentTooSmall { required, found } => {
                write!(
                    f,
                    "segment too small: need {} bytes, got {}",
                    required, found
                )
            }
            Self::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Errors from ring operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    /// Ring is full.
    Full,
}

impl std::fmt::Display for RingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "ring is full"),
        }
    }
}

impl std::error::Error for RingError {}

/// Errors from slot operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    /// No free slots available.
    NoFreeSlots,
    /// Invalid slot index.
    InvalidIndex,
    /// Generation mismatch (stale reference).
    StaleGeneration,
    /// Slot in unexpected state.
    InvalidState,
    /// Payload too large for slot.
    PayloadTooLarge { len: usize, max: usize },
}

impl std::fmt::Display for SlotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFreeSlots => write!(f, "no free slots available"),
            Self::InvalidIndex => write!(f, "invalid slot index"),
            Self::StaleGeneration => write!(f, "stale generation"),
            Self::InvalidState => write!(f, "invalid slot state"),
            Self::PayloadTooLarge { len, max } => write!(f, "payload too large for slot: {} bytes, max {}", len, max),
        }
    }
}

impl std::error::Error for SlotError {}

// =============================================================================
// Layout Calculations
// =============================================================================

/// Calculate the total size needed for a SHM segment (checked).
///
/// Returns an error string describing where the overflow occurred.
pub fn calculate_segment_size_checked(
    ring_capacity: u32,
    slot_size: u32,
    slot_count: u32,
) -> Result<usize, &'static str> {
    let header_size = core::mem::size_of::<SegmentHeader>();
    let ring_header_size = core::mem::size_of::<DescRingHeader>();
    let desc_size = core::mem::size_of::<MsgDescHot>();
    let data_header_size = core::mem::size_of::<DataSegmentHeader>();
    let slot_meta_size = core::mem::size_of::<SlotMeta>();

    let ring_descs_size = (ring_capacity as usize)
        .checked_mul(desc_size)
        .ok_or("SHM size overflow (ring descs)")?;
    let ring_size = ring_header_size
        .checked_add(ring_descs_size)
        .ok_or("SHM size overflow (ring)")?;

    let slot_meta_total = slot_meta_size
        .checked_mul(slot_count as usize)
        .ok_or("SHM size overflow (slot meta)")?;
    let slot_data_total = (slot_size as usize)
        .checked_mul(slot_count as usize)
        .ok_or("SHM size overflow (slot data)")?;

    let mut total = header_size;
    total = total
        .checked_add(ring_size)
        .and_then(|v| v.checked_add(ring_size))
        .and_then(|v| v.checked_add(data_header_size))
        .and_then(|v| v.checked_add(slot_meta_total))
        .and_then(|v| v.checked_add(slot_data_total))
        .ok_or("SHM size overflow (total)")?;

    Ok(total)
}

/// Calculate the total size needed for a SHM segment.
pub fn calculate_segment_size(ring_capacity: u32, slot_size: u32, slot_count: u32) -> usize {
    calculate_segment_size_checked(ring_capacity, slot_size, slot_count)
        .expect("SHM segment size overflow")
}

/// Offsets within the SHM segment.
#[derive(Debug, Clone, Copy)]
pub struct SegmentOffsets {
    pub header: usize,
    pub ring_a_to_b_header: usize,
    pub ring_a_to_b_descs: usize,
    pub ring_b_to_a_header: usize,
    pub ring_b_to_a_descs: usize,
    pub data_header: usize,
    pub slot_meta: usize,
    pub slot_data: usize,
}

impl SegmentOffsets {
    /// Calculate offsets for given parameters.
    pub fn calculate(ring_capacity: u32, slot_count: u32) -> Self {
        Self::calculate_checked(ring_capacity, slot_count).expect("SHM offset overflow")
    }

    /// Calculate offsets for given parameters (checked).
    ///
    /// Returns an error string describing where the overflow occurred.
    pub fn calculate_checked(ring_capacity: u32, slot_count: u32) -> Result<Self, &'static str> {
        let header_size = core::mem::size_of::<SegmentHeader>();
        let ring_header_size = core::mem::size_of::<DescRingHeader>();
        let desc_size = core::mem::size_of::<MsgDescHot>();
        let data_header_size = core::mem::size_of::<DataSegmentHeader>();
        let slot_meta_size = core::mem::size_of::<SlotMeta>();

        let ring_descs_size = (ring_capacity as usize)
            .checked_mul(desc_size)
            .ok_or("SHM offset overflow (ring descs)")?;
        let slot_meta_total = slot_meta_size
            .checked_mul(slot_count as usize)
            .ok_or("SHM offset overflow (slot meta)")?;

        let header = 0usize;
        let ring_a_to_b_header = header
            .checked_add(header_size)
            .ok_or("SHM offset overflow (ring A->B header)")?;
        let ring_a_to_b_descs = ring_a_to_b_header
            .checked_add(ring_header_size)
            .ok_or("SHM offset overflow (ring A->B descs)")?;
        let ring_b_to_a_header = ring_a_to_b_descs
            .checked_add(ring_descs_size)
            .ok_or("SHM offset overflow (ring B->A header)")?;
        let ring_b_to_a_descs = ring_b_to_a_header
            .checked_add(ring_header_size)
            .ok_or("SHM offset overflow (ring B->A descs)")?;
        let data_header = ring_b_to_a_descs
            .checked_add(ring_descs_size)
            .ok_or("SHM offset overflow (data header)")?;
        let slot_meta = data_header
            .checked_add(data_header_size)
            .ok_or("SHM offset overflow (slot meta)")?;
        let slot_data = slot_meta
            .checked_add(slot_meta_total)
            .ok_or("SHM offset overflow (slot data)")?;

        Ok(Self {
            header,
            ring_a_to_b_header,
            ring_a_to_b_descs,
            ring_b_to_a_header,
            ring_b_to_a_descs,
            data_header,
            slot_meta,
            slot_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_header_size() {
        assert_eq!(core::mem::size_of::<SegmentHeader>(), 128);
    }

    #[test]
    fn test_desc_ring_header_size() {
        assert_eq!(core::mem::size_of::<DescRingHeader>(), 192);
    }

    #[test]
    fn test_slot_meta_size() {
        assert_eq!(core::mem::size_of::<SlotMeta>(), 8);
    }

    #[test]
    fn test_data_segment_header_size() {
        assert_eq!(core::mem::size_of::<DataSegmentHeader>(), 64);
    }

    #[test]
    fn test_calculate_segment_size() {
        let size =
            calculate_segment_size(DEFAULT_RING_CAPACITY, DEFAULT_SLOT_SIZE, DEFAULT_SLOT_COUNT);
        // Rough sanity check
        assert!(size > 0);
        // Header (64) + 2 rings (2 * (192 + 256*64)) + data header (64) + meta (64*8) + data (64*4096)
        // = 64 + 2*(192 + 16384) + 64 + 512 + 262144
        // = 64 + 33152 + 64 + 512 + 262144 = 295936
        assert_eq!(size, 295936);
    }

    #[test]
    fn test_segment_offsets() {
        let offsets = SegmentOffsets::calculate(DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT);

        assert_eq!(offsets.header, 0);
        assert_eq!(offsets.ring_a_to_b_header, 64);
        assert_eq!(offsets.ring_a_to_b_descs, 64 + 192);
        // ring_a_to_b_descs + 256*64 = 256 + 16384 = 16640
        assert_eq!(offsets.ring_b_to_a_header, 256 + 16384);
        // etc.
    }

    #[test]
    fn test_segment_header_validate() {
        let mut header = unsafe { std::mem::zeroed::<SegmentHeader>() };
        header.init();
        assert!(header.validate().is_ok());

        header.magic[0] = b'X';
        assert!(matches!(header.validate(), Err(LayoutError::InvalidMagic)));
    }
}
