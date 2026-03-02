//! VarSlotPool — lock-free shared-memory allocator for large payloads.
//!
//! The pool is partitioned into size classes. Each class maintains a
//! Treiber stack free list over a contiguous array of slots. Slots are
//! shared across all peers; ownership is tracked via `owner_peer` for
//! crash recovery.
//!
//! Memory layout (relative to the pool's base offset in the segment):
//!
//! ```text
//! [SizeClassHeader × num_classes]          (num_classes × 64 bytes, 64-byte aligned)
//! For each class i, in order:
//!   [VarSlotMeta × slot_count[i]]          (16 × slot_count bytes, 64-byte aligned)
//!   [u8 × slot_size[i] × slot_count[i]]    (data region, 64-byte aligned)
//! ```
//!
//! The same layout is reconstructed on both host and guest side from
//! the `SizeClassConfig` slice stored in the segment header.

use core::mem::size_of;

use crate::sync::{AtomicU32, AtomicU64, Ordering};
use crate::{Region, SlotState, VarSlotMeta};

// ── helpers ──────────────────────────────────────────────────────────────────

const fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

/// Sentinel value in the free list meaning "end of list / class exhausted".
const EMPTY: u32 = u32::MAX;

/// Pack a (slot_idx, aba_gen) pair into a single u64 for the Treiber head.
#[inline]
fn pack(slot_idx: u32, aba_gen: u32) -> u64 {
    ((aba_gen as u64) << 32) | (slot_idx as u64)
}

/// Unpack a Treiber head back to (slot_idx, aba_gen).
#[inline]
fn unpack(v: u64) -> (u32, u32) {
    (v as u32, (v >> 32) as u32)
}

// ── public types ─────────────────────────────────────────────────────────────

/// Configuration for one size class, supplied at pool creation and at attach.
///
/// r[impl shm.varslot.classes]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeClassConfig {
    /// Size of each slot's data region in bytes.
    pub slot_size: u32,
    /// Number of slots in this class's initial extent.
    pub slot_count: u32,
}

/// Reference to an allocated slot — returned by [`VarSlotPool::allocate`].
///
/// This is the value that goes into a `framing` slot-ref entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotRef {
    /// Index into the pool's class array.
    pub class_idx: u8,
    /// Extent index within the class (always 0 until dynamic growth is wired up).
    pub extent_idx: u8,
    /// Slot index within the extent.
    pub slot_idx: u32,
    /// Generation counter at the time of allocation (for double-free detection).
    pub generation: u32,
}

/// Returned by [`VarSlotPool::free`] when the generation check fails.
#[derive(Debug)]
pub struct DoubleFreeError {
    pub slot: SlotRef,
}

// ── shared-memory header ──────────────────────────────────────────────────────

/// Per-class header that lives inside the shared memory region.
///
/// r[impl shm.varslot.freelist]
#[repr(C, align(64))]
pub struct SizeClassHeader {
    /// Size of each slot's data region in bytes.
    pub slot_size: u32,
    /// Number of slots in this class (across all current extents).
    pub slot_count: u32,
    /// Treiber stack head: packed `(slot_idx: u32, aba_gen: u32)`.
    /// `slot_idx == EMPTY` means the free list is empty.
    pub free_head: AtomicU64,
    _pad: [u8; 48],
}

#[cfg(not(loom))]
const _: () = assert!(size_of::<SizeClassHeader>() == 64);

// ── VarSlotPool ──────────────────────────────────────────────────────────────

/// Per-class runtime view (Rust-side, not in shared memory).
struct ClassView {
    header: *mut SizeClassHeader,
    meta: *mut VarSlotMeta,
    data: *mut u8,
    slot_count: u32,
    slot_size: u32,
}

unsafe impl Send for ClassView {}
unsafe impl Sync for ClassView {}

/// Lock-free slot allocator operating on a shared memory `Region`.
///
/// r[impl shm.varslot]
pub struct VarSlotPool {
    classes: Vec<ClassView>,
}

unsafe impl Send for VarSlotPool {}
unsafe impl Sync for VarSlotPool {}

impl VarSlotPool {
    /// Compute the byte offsets of each class's metadata and data arrays,
    /// plus the total size required.
    ///
    /// Both `init` and `attach` use this to reconstruct the layout from the
    /// same `configs` slice.
    pub fn layout(configs: &[SizeClassConfig]) -> PoolLayout {
        let headers_size = align_up(configs.len() * size_of::<SizeClassHeader>(), 64);
        let mut offset = headers_size;
        let mut class_offsets = Vec::with_capacity(configs.len());

        for cfg in configs {
            let meta_offset = offset;
            offset += align_up(size_of::<VarSlotMeta>() * cfg.slot_count as usize, 64);
            let data_offset = offset;
            offset += align_up(cfg.slot_size as usize * cfg.slot_count as usize, 64);
            class_offsets.push(ClassOffsets {
                meta_offset,
                data_offset,
            });
        }

        PoolLayout {
            total_size: offset,
            class_offsets,
        }
    }

    /// Total bytes this pool needs in the region.
    pub fn required_size(configs: &[SizeClassConfig]) -> usize {
        Self::layout(configs).total_size
    }

    /// Discover size-class configs from the in-segment class headers.
    ///
    /// # Safety
    ///
    /// `region` must point to a valid mapped segment region and `base_offset`
    /// must be the var-slot pool base from the segment header.
    pub unsafe fn discover_configs(
        region: Region,
        base_offset: usize,
        num_classes: u32,
    ) -> Result<Vec<SizeClassConfig>, &'static str> {
        if num_classes == 0 {
            return Err("segment missing var-slot classes");
        }

        let headers_size = num_classes as usize * size_of::<SizeClassHeader>();
        if base_offset
            .checked_add(headers_size)
            .is_none_or(|end| end > region.len())
        {
            return Err("var-slot class header table out of bounds");
        }

        let mut configs = Vec::with_capacity(num_classes as usize);
        for class_idx in 0..num_classes as usize {
            let header_off = base_offset + class_idx * size_of::<SizeClassHeader>();
            let header = unsafe { region.get::<SizeClassHeader>(header_off) };
            if header.slot_size == 0 || header.slot_count == 0 {
                return Err("invalid var-slot class config in segment");
            }
            configs.push(SizeClassConfig {
                slot_size: header.slot_size,
                slot_count: header.slot_count,
            });
        }

        Ok(configs)
    }

    /// Initialize a new pool in `region` at `base_offset`.
    ///
    /// Writes all headers and builds the initial free lists. The region bytes
    /// must already be zeroed (segment creation zeros the mmap).
    ///
    /// # Safety
    ///
    /// `region` must be exclusively owned (no concurrent readers/writers).
    /// `base_offset` must be 64-byte aligned.
    ///
    /// r[impl shm.varslot.slot-meta]
    pub unsafe fn init(region: Region, base_offset: usize, configs: &[SizeClassConfig]) -> Self {
        assert!(
            base_offset.is_multiple_of(64),
            "base_offset must be 64-byte aligned"
        );

        let layout = Self::layout(configs);
        assert!(
            base_offset + layout.total_size <= region.len(),
            "region too small for VarSlotPool"
        );

        let mut classes = Vec::with_capacity(configs.len());

        for (i, (cfg, offsets)) in configs.iter().zip(layout.class_offsets.iter()).enumerate() {
            // Write SizeClassHeader
            let hdr_off = base_offset + i * size_of::<SizeClassHeader>();
            let header: *mut SizeClassHeader =
                unsafe { region.get_mut::<SizeClassHeader>(hdr_off) };

            unsafe {
                (*header).slot_size = cfg.slot_size;
                (*header).slot_count = cfg.slot_count;
                // Free list starts at slot 0
                (*header).free_head = AtomicU64::new(if cfg.slot_count > 0 {
                    pack(0, 0)
                } else {
                    pack(EMPTY, 0)
                });
                (*header)._pad = [0u8; 48];
            }

            // Init VarSlotMeta array — build a linked free list: 0 → 1 → … → n-1 → EMPTY
            let meta_ptr = region.offset(base_offset + offsets.meta_offset) as *mut VarSlotMeta;
            let data_ptr = region.offset(base_offset + offsets.data_offset);

            for slot in 0..cfg.slot_count {
                let m = unsafe { &mut *meta_ptr.add(slot as usize) };
                m.generation = AtomicU32::new(0);
                m.state = AtomicU32::new(SlotState::Free as u32);
                m.owner_peer = AtomicU32::new(0);
                m.next_free = AtomicU32::new(if slot + 1 < cfg.slot_count {
                    slot + 1
                } else {
                    EMPTY
                });
            }

            classes.push(ClassView {
                header,
                meta: meta_ptr,
                data: data_ptr,
                slot_count: cfg.slot_count,
                slot_size: cfg.slot_size,
            });
        }

        Self { classes }
    }

    /// Attach to an existing, already-initialized pool.
    ///
    /// # Safety
    ///
    /// The pool at `base_offset` must have been initialized with the same `configs`.
    pub unsafe fn attach(region: Region, base_offset: usize, configs: &[SizeClassConfig]) -> Self {
        assert!(
            base_offset.is_multiple_of(64),
            "base_offset must be 64-byte aligned"
        );

        let layout = Self::layout(configs);
        assert!(
            base_offset + layout.total_size <= region.len(),
            "region too small for VarSlotPool"
        );

        let mut classes = Vec::with_capacity(configs.len());

        for (i, (cfg, offsets)) in configs.iter().zip(layout.class_offsets.iter()).enumerate() {
            let hdr_off = base_offset + i * size_of::<SizeClassHeader>();
            let header: *mut SizeClassHeader =
                unsafe { region.get_mut::<SizeClassHeader>(hdr_off) };
            let meta_ptr = region.offset(base_offset + offsets.meta_offset) as *mut VarSlotMeta;
            let data_ptr = region.offset(base_offset + offsets.data_offset);

            classes.push(ClassView {
                header,
                meta: meta_ptr,
                data: data_ptr,
                slot_count: cfg.slot_count,
                slot_size: cfg.slot_size,
            });
        }

        Self { classes }
    }

    /// Allocate a slot for a payload of `size` bytes.
    ///
    /// Finds the smallest size class where `slot_size >= size` and pops from
    /// its free list. If that class is exhausted, tries the next larger one.
    /// Returns `None` if all suitable classes are exhausted (backpressure).
    ///
    /// r[impl shm.varslot.selection]
    /// r[impl shm.varslot.allocate]
    pub fn allocate(&self, size: u32, owner_peer: u8) -> Option<SlotRef> {
        // Find the first class whose slot_size >= size.
        let start = self.classes.iter().position(|c| c.slot_size >= size)?;

        for (class_idx, view) in self.classes[start..].iter().enumerate() {
            let class_idx = (start + class_idx) as u8;
            if let Some(slot_ref) = self.try_alloc_from(class_idx, view, owner_peer) {
                return Some(slot_ref);
            }
        }
        None
    }

    fn try_alloc_from(&self, class_idx: u8, view: &ClassView, owner_peer: u8) -> Option<SlotRef> {
        let header = unsafe { &*view.header };
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            let (slot_idx, aba_gen) = unpack(head);

            if slot_idx == EMPTY {
                return None; // class exhausted
            }

            let meta = unsafe { &*view.meta.add(slot_idx as usize) };
            let next = meta.next_free.load(Ordering::Acquire);

            // CAS the free head from (slot_idx, aba_gen) to (next, aba_gen+1)
            let new_head = pack(next, aba_gen.wrapping_add(1));
            if header
                .free_head
                .compare_exchange(head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // Won the slot — update metadata.
                let new_gen = meta
                    .generation
                    .fetch_add(1, Ordering::AcqRel)
                    .wrapping_add(1);
                meta.state
                    .store(SlotState::Allocated as u32, Ordering::Release);
                meta.owner_peer.store(owner_peer as u32, Ordering::Release);

                return Some(SlotRef {
                    class_idx,
                    extent_idx: 0,
                    slot_idx,
                    generation: new_gen,
                });
            }
            // CAS failed — retry
        }
    }

    /// Free a previously allocated slot.
    ///
    /// Verifies the generation matches to detect double-frees, then pushes
    /// the slot back onto the class's Treiber free list.
    ///
    /// r[impl shm.varslot.free]
    pub fn free(&self, slot_ref: SlotRef) -> Result<(), DoubleFreeError> {
        let view = &self.classes[slot_ref.class_idx as usize];
        let meta = unsafe { &*view.meta.add(slot_ref.slot_idx as usize) };

        // Detect double-free: slot must still be Allocated and generation must match.
        // State check catches immediate double-free; generation check catches stale handles
        // after the slot has been recycled by another allocator.
        if meta.state.load(Ordering::Acquire) != SlotState::Allocated as u32
            || meta.generation.load(Ordering::Acquire) != slot_ref.generation
        {
            return Err(DoubleFreeError { slot: slot_ref });
        }

        meta.state.store(SlotState::Free as u32, Ordering::Release);
        meta.owner_peer.store(0, Ordering::Release);

        let header = unsafe { &*view.header };
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            let (head_idx, aba_gen) = unpack(head);

            meta.next_free.store(head_idx, Ordering::Release);

            let new_head = pack(slot_ref.slot_idx, aba_gen.wrapping_add(1));
            if header
                .free_head
                .compare_exchange(head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
            // CAS failed — retry
        }
    }

    /// Return a mutable slice into the slot's data area.
    ///
    /// # Safety
    ///
    /// The slot must be currently allocated and the caller must ensure
    /// no concurrent access to the same slot's data.
    pub unsafe fn slot_data_mut<'a>(&self, slot_ref: &SlotRef) -> &'a mut [u8] {
        let view = &self.classes[slot_ref.class_idx as usize];
        let offset = slot_ref.slot_idx as usize * view.slot_size as usize;
        unsafe { core::slice::from_raw_parts_mut(view.data.add(offset), view.slot_size as usize) }
    }

    /// Return an immutable slice into the slot's data area.
    ///
    /// # Safety
    ///
    /// The slot must be currently allocated and contain readable payload bytes.
    pub unsafe fn slot_data<'a>(&self, slot_ref: &SlotRef) -> &'a [u8] {
        let view = &self.classes[slot_ref.class_idx as usize];
        let offset = slot_ref.slot_idx as usize * view.slot_size as usize;
        unsafe { core::slice::from_raw_parts(view.data.add(offset), view.slot_size as usize) }
    }

    /// Return the number of size classes.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Return the slot size for a given class.
    pub fn slot_size(&self, class_idx: usize) -> u32 {
        self.classes[class_idx].slot_size
    }

    /// Crash recovery: reclaim all slots owned by `peer_id` back to their
    /// respective free lists.
    ///
    /// Must be called only by the host after confirming the peer is dead.
    ///
    /// r[impl shm.varslot.crash-recovery]
    pub fn reclaim_peer_slots(&self, peer_id: u8) {
        for (class_idx, view) in self.classes.iter().enumerate() {
            for slot_idx in 0..view.slot_count {
                let meta = unsafe { &*view.meta.add(slot_idx as usize) };
                let owner = meta.owner_peer.load(Ordering::Acquire);
                if owner != peer_id as u32 {
                    continue;
                }
                let state = meta.state.load(Ordering::Acquire);
                if state != SlotState::Allocated as u32 {
                    continue;
                }

                let slot_gen = meta.generation.load(Ordering::Acquire);
                let _ = self.free(SlotRef {
                    class_idx: class_idx as u8,
                    extent_idx: 0,
                    slot_idx,
                    generation: slot_gen,
                });
            }
        }
    }
}

// ── layout helper ─────────────────────────────────────────────────────────────

/// Byte offsets of a single class's metadata and data arrays.
pub struct ClassOffsets {
    pub meta_offset: usize,
    pub data_offset: usize,
}

/// Result of [`VarSlotPool::layout`].
pub struct PoolLayout {
    /// Total bytes needed in the region.
    pub total_size: usize,
    /// Per-class offsets (relative to pool base).
    pub class_offsets: Vec<ClassOffsets>,
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::HeapRegion;

    const CLASSES: &[SizeClassConfig] = &[
        SizeClassConfig {
            slot_size: 1024,
            slot_count: 8,
        },
        SizeClassConfig {
            slot_size: 16384,
            slot_count: 4,
        },
        SizeClassConfig {
            slot_size: 262144,
            slot_count: 2,
        },
    ];

    fn make_pool() -> (HeapRegion, VarSlotPool) {
        let size = VarSlotPool::required_size(CLASSES);
        let region = HeapRegion::new_zeroed(size);
        let pool = unsafe { VarSlotPool::init(region.region(), 0, CLASSES) };
        (region, pool)
    }

    #[test]
    fn alloc_and_free_basic() {
        let (_region, pool) = make_pool();

        let slot = pool.allocate(512, 1).expect("should allocate from class 0");
        assert_eq!(slot.class_idx, 0);
        assert_eq!(slot.slot_idx, 0);
        assert_eq!(slot.generation, 1);

        pool.free(slot).expect("free should succeed");
    }

    #[test]
    fn alloc_fills_smallest_fitting_class() {
        let (_region, pool) = make_pool();

        // 2000 bytes doesn't fit in class 0 (1024), goes to class 1 (16384)
        let slot = pool
            .allocate(2000, 0)
            .expect("should allocate from class 1");
        assert_eq!(slot.class_idx, 1);
    }

    #[test]
    fn alloc_exhausts_class_falls_through() {
        let (_region, pool) = make_pool();

        // Exhaust all 8 slots in class 0
        let mut slots = Vec::new();
        for _ in 0..8 {
            slots.push(pool.allocate(1, 0).expect("should allocate"));
        }
        assert!(slots.iter().all(|s| s.class_idx == 0));

        // Class 0 exhausted; should fall through to class 1
        let overflow = pool.allocate(1, 0).expect("should fall through to class 1");
        assert_eq!(overflow.class_idx, 1);

        for s in slots {
            pool.free(s).unwrap();
        }
        pool.free(overflow).unwrap();
    }

    #[test]
    fn all_classes_exhausted_returns_none() {
        let (_region, pool) = make_pool();

        // Drain everything: 8 + 4 + 2 = 14 slots
        let mut slots = Vec::new();
        while let Some(s) = pool.allocate(1, 0) {
            slots.push(s);
        }
        assert_eq!(slots.len(), 14);
        assert!(pool.allocate(1, 0).is_none());

        for s in slots {
            pool.free(s).unwrap();
        }
    }

    #[test]
    fn free_recycles_slot() {
        let (_region, pool) = make_pool();

        let s1 = pool.allocate(1, 0).unwrap();
        pool.free(s1).unwrap();

        let s2 = pool.allocate(1, 0).unwrap();
        // Same physical slot (LIFO Treiber stack), but generation bumped
        assert_eq!(s2.slot_idx, s1.slot_idx);
        assert_eq!(s2.generation, s1.generation + 1);
        pool.free(s2).unwrap();
    }

    #[test]
    fn double_free_detected() {
        let (_region, pool) = make_pool();

        let s = pool.allocate(1, 0).unwrap();
        pool.free(s).unwrap();

        // Second free with same generation → error
        assert!(pool.free(s).is_err());
    }

    #[test]
    fn slot_data_write_read() {
        let (_region, pool) = make_pool();

        let s = pool.allocate(100, 0).unwrap();
        unsafe {
            let data = pool.slot_data_mut(&s);
            data[..5].copy_from_slice(b"hello");
        }
        unsafe {
            let data = pool.slot_data_mut(&s);
            assert_eq!(&data[..5], b"hello");
        }
        pool.free(s).unwrap();
    }

    #[test]
    fn size_too_large_returns_none() {
        let (_region, pool) = make_pool();

        // Largest class is 262144; asking for more → None
        assert!(pool.allocate(300_000, 0).is_none());
    }

    #[test]
    fn reclaim_peer_slots() {
        let (_region, pool) = make_pool();

        // Allocate some slots as peer 7
        let _s1 = pool.allocate(1, 7).unwrap();
        let _s2 = pool.allocate(1, 7).unwrap();
        let s3 = pool.allocate(1, 2).unwrap(); // different peer

        pool.reclaim_peer_slots(7);

        // s3 (peer 2) still allocated; 7 free slots remain in class 0.
        // Collect all free slots without re-freeing during the loop.
        let mut freed = Vec::new();
        while let Some(s) = pool.allocate(1, 0) {
            freed.push(s);
        }
        // class 0: 8 - 1 (s3) = 7 free; class 1: 4 free; class 2: 2 free → 13 total
        assert_eq!(freed.len(), 13);

        for s in freed {
            pool.free(s).unwrap();
        }
        pool.free(s3).unwrap();
    }

    #[test]
    fn layout_is_deterministic() {
        let l1 = VarSlotPool::layout(CLASSES);
        let l2 = VarSlotPool::layout(CLASSES);
        assert_eq!(l1.total_size, l2.total_size);
        for (a, b) in l1.class_offsets.iter().zip(l2.class_offsets.iter()) {
            assert_eq!(a.meta_offset, b.meta_offset);
            assert_eq!(a.data_offset, b.data_offset);
        }
    }

    #[test]
    fn owner_peer_tracked() {
        let (_region, pool) = make_pool();

        let s = pool.allocate(1, 42).unwrap();
        let view = &pool.classes[s.class_idx as usize];
        let meta = unsafe { &*view.meta.add(s.slot_idx as usize) };
        assert_eq!(meta.owner_peer.load(Ordering::Acquire), 42);

        pool.free(s).unwrap();
        assert_eq!(meta.owner_peer.load(Ordering::Acquire), 0);
    }
}

// ── loom tests ────────────────────────────────────────────────────────────────

#[cfg(loom)]
#[allow(dead_code)]
mod loom_tests {
    use super::*;
    use crate::HeapRegion;
    use loom::sync::Arc;

    // Tiny pool: 1 class, 2 slots of 64 bytes. Enough to exercise races without
    // blowing up loom's state-space budget.
    const LOOM_CLASSES: &[SizeClassConfig] = &[SizeClassConfig {
        slot_size: 64,
        slot_count: 2,
    }];

    fn loom_pool() -> (HeapRegion, Arc<VarSlotPool>) {
        let size = VarSlotPool::required_size(LOOM_CLASSES);
        let region = HeapRegion::new_zeroed(size);
        let pool = unsafe { VarSlotPool::init(region.region(), 0, LOOM_CLASSES) };
        (region, Arc::new(pool))
    }

    /// Two threads concurrently allocate from a 2-slot pool.
    /// They must get distinct slots (no aliasing).
    #[test]
    fn concurrent_alloc_no_aliasing() {
        loom::model(|| {
            let (_region, pool) = loom_pool();
            let pool1 = pool.clone();
            let pool2 = pool.clone();

            let t1 = loom::thread::spawn(move || pool1.allocate(1, 1));
            let t2 = loom::thread::spawn(move || pool2.allocate(1, 2));

            let s1 = t1.join().unwrap().expect("thread 1 must get a slot");
            let s2 = t2.join().unwrap().expect("thread 2 must get a slot");

            // Both threads must have won different physical slots.
            assert_ne!(s1.slot_idx, s2.slot_idx, "threads must not alias slots");

            pool.free(s1).unwrap();
            pool.free(s2).unwrap();
        });
    }

    /// One thread allocates a slot, then a second thread frees it.
    /// The generation must be consistent across the handoff.
    #[test]
    fn alloc_then_free_cross_thread() {
        loom::model(|| {
            let (_region, pool) = loom_pool();

            let slot = pool.allocate(1, 0).expect("must allocate");

            let pool2 = pool.clone();
            let t = loom::thread::spawn(move || pool2.free(slot));

            t.join().unwrap().expect("cross-thread free must succeed");
        });
    }

    /// Concurrent alloc and free on a 2-slot pool: one thread allocates then
    /// immediately frees in a loop; the other just allocates. Exercises the
    /// Treiber stack CAS paths under all interleavings.
    #[test]
    fn concurrent_alloc_and_free() {
        loom::model(|| {
            let (_region, pool) = loom_pool();
            let pool_alloc = pool.clone();
            let pool_free = pool.clone();

            // Pre-allocate one slot so the free thread always has something.
            let initial = pool.allocate(1, 0).expect("initial alloc");

            let t_free = loom::thread::spawn(move || {
                pool_free.free(initial).expect("free must succeed");
            });

            let t_alloc = loom::thread::spawn(move || pool_alloc.allocate(1, 0));

            t_free.join().unwrap();
            let maybe_slot = t_alloc.join().unwrap();

            // After the free, the pool has at least 1 slot available.
            // The alloc thread may have run before or after the free; either way
            // it should have found a slot (the pool had 2 to begin with, 1 pre-allocated).
            if let Some(s) = maybe_slot {
                pool.free(s).unwrap();
            }
        });
    }
}
