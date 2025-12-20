use core::mem::{align_of, size_of};
use core::ptr;

use crate::region::Region;
use crate::sync::{AtomicU64, Ordering};

/// SPSC ring header (192 bytes, cache-line aligned fields).
#[repr(C)]
pub struct SpscRingHeader {
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

#[cfg(not(feature = "loom"))]
const _: () = assert!(core::mem::size_of::<SpscRingHeader>() == 192);

impl SpscRingHeader {
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

    #[inline]
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        tail >= head
    }

    #[inline]
    pub fn mask(&self) -> u64 {
        self.capacity as u64 - 1
    }

    #[inline]
    pub fn is_full(&self, local_head: u64) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        local_head.wrapping_sub(tail) >= self.capacity as u64
    }

    #[inline]
    pub fn len(&self) -> u64 {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        head.saturating_sub(tail)
    }
}

/// A wait-free SPSC ring buffer in a shared memory region.
///
/// This is a convenience wrapper around `SpscRingRaw<T>` that manages
/// memory through a `Region`. All operations delegate to the raw implementation.
pub struct SpscRing<T> {
    /// We hold the region to keep the backing memory alive.
    /// All operations go through `inner` which holds raw pointers into this region.
    #[allow(dead_code)]
    region: Region,
    inner: SpscRingRaw<T>,
}

unsafe impl<T: Send> Send for SpscRing<T> {}
unsafe impl<T: Send> Sync for SpscRing<T> {}

impl<T: Copy> SpscRing<T> {
    /// Initialize a new ring in the region.
    ///
    /// # Safety
    ///
    /// The region must be writable and exclusively owned during initialization.
    pub unsafe fn init(region: Region, header_offset: usize, capacity: u32) -> Self {
        assert!(
            capacity.is_power_of_two() && capacity > 0,
            "capacity must be power of 2"
        );
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );
        assert!(align_of::<T>() <= 64, "entry alignment must be <= 64");

        let entries_offset = header_offset + size_of::<SpscRingHeader>();
        let required = entries_offset + (capacity as usize * size_of::<T>());
        assert!(required <= region.len(), "region too small for ring");
        assert!(
            entries_offset.is_multiple_of(align_of::<T>()),
            "entries misaligned"
        );

        let header_ptr = region.offset(header_offset) as *mut SpscRingHeader;
        let entries_ptr = region.offset(entries_offset) as *mut T;

        // Initialize the header
        unsafe { (*header_ptr).init(capacity) };

        // Create the inner raw ring
        let inner = unsafe { SpscRingRaw::from_raw(header_ptr, entries_ptr) };

        Self { region, inner }
    }

    /// Attach to an existing ring in the region.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized ring header.
    pub unsafe fn attach(region: Region, header_offset: usize) -> Self {
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );
        assert!(align_of::<T>() <= 64, "entry alignment must be <= 64");

        let entries_offset = header_offset + size_of::<SpscRingHeader>();
        let header_ptr = region.offset(header_offset) as *mut SpscRingHeader;
        let capacity = unsafe { (*header_ptr).capacity };

        assert!(
            capacity.is_power_of_two() && capacity > 0,
            "invalid ring capacity"
        );
        let required = entries_offset + (capacity as usize * size_of::<T>());
        assert!(required <= region.len(), "region too small for ring");
        assert!(
            entries_offset.is_multiple_of(align_of::<T>()),
            "entries misaligned"
        );

        let entries_ptr = region.offset(entries_offset) as *mut T;
        let inner = unsafe { SpscRingRaw::from_raw(header_ptr, entries_ptr) };

        Self { region, inner }
    }

    /// Get a reference to the inner raw ring.
    #[inline]
    pub fn inner(&self) -> &SpscRingRaw<T> {
        &self.inner
    }

    /// Split into producer and consumer handles.
    pub fn split(&self) -> (SpscProducer<'_, T>, SpscConsumer<'_, T>) {
        let head = self.inner.status().visible_head;
        (
            SpscProducer {
                ring: self,
                local_head: head,
            },
            SpscConsumer { ring: self },
        )
    }

    /// Returns the ring capacity.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.inner.capacity()
    }

    /// Returns true if the ring appears empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns a status snapshot of head/tail.
    pub fn status(&self) -> RingStatus {
        self.inner.status()
    }
}

/// Producer handle for the ring.
pub struct SpscProducer<'a, T> {
    pub(crate) ring: &'a SpscRing<T>,
    pub(crate) local_head: u64,
}

/// Consumer handle for the ring.
pub struct SpscConsumer<'a, T> {
    pub(crate) ring: &'a SpscRing<T>,
}

/// Result of a push attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    Ok,
    WouldBlock,
}

impl PushResult {
    #[inline]
    pub fn is_would_block(self) -> bool {
        matches!(self, PushResult::WouldBlock)
    }
}

impl<'a, T: Copy> SpscProducer<'a, T> {
    /// Try to push an entry to the ring.
    ///
    /// Delegates to `SpscRingRaw::enqueue`.
    pub fn try_push(&mut self, entry: T) -> PushResult {
        match self.ring.inner.enqueue(&mut self.local_head, &entry) {
            Ok(()) => PushResult::Ok,
            Err(RingFull) => PushResult::WouldBlock,
        }
    }

    /// Returns true if the ring appears full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.ring.inner.is_full(self.local_head)
    }

    /// Returns the number of entries that can be pushed (approximate).
    #[inline]
    pub fn available_capacity(&self) -> u64 {
        let capacity = self.ring.inner.capacity() as u64;
        let tail = self.ring.inner.status().tail;
        capacity.saturating_sub(self.local_head.wrapping_sub(tail))
    }
}

impl<'a, T: Copy> SpscConsumer<'a, T> {
    /// Try to pop an entry from the ring.
    ///
    /// Delegates to `SpscRingRaw::dequeue`.
    pub fn try_pop(&mut self) -> Option<T> {
        self.ring.inner.dequeue()
    }

    /// Returns true if the ring appears empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// Returns the number of entries available to pop (approximate).
    #[inline]
    pub fn len(&self) -> u64 {
        self.ring.inner.status().len as u64
    }
}

/// Status snapshot of a ring.
#[derive(Debug, Clone, Copy)]
pub struct RingStatus {
    pub visible_head: u64,
    pub tail: u64,
    pub capacity: u32,
    pub len: u32,
}

// =============================================================================
// SpscRingRaw - Raw pointer version for rapace-core compatibility
// =============================================================================

/// Error returned when the ring is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingFull;

/// A wait-free SPSC ring buffer operating on raw pointers.
///
/// This is the "raw" API that matches rapace-core's `DescRing` interface:
/// - Constructed from raw pointers to header and entries
/// - `enqueue` takes `&mut local_head` (caller-managed producer state)
/// - `dequeue` is stateless on the caller side
///
/// Use this when you need to integrate with existing SHM layouts or when
/// the `Region` abstraction doesn't fit your use case.
pub struct SpscRingRaw<T> {
    header: *mut SpscRingHeader,
    entries: *mut T,
}

unsafe impl<T: Send> Send for SpscRingRaw<T> {}
unsafe impl<T: Send> Sync for SpscRingRaw<T> {}

impl<T: Copy> SpscRingRaw<T> {
    /// Create a ring view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, initialized `SpscRingHeader`
    /// - `entries` must point to `header.capacity` initialized `T` slots
    /// - The memory must remain valid for the lifetime of this ring
    /// - `entries` must be properly aligned for `T`
    #[inline]
    pub unsafe fn from_raw(header: *mut SpscRingHeader, entries: *mut T) -> Self {
        Self { header, entries }
    }

    /// Get the ring header.
    #[inline]
    fn header(&self) -> &SpscRingHeader {
        unsafe { &*self.header }
    }

    /// Get a pointer to an entry slot.
    #[inline]
    unsafe fn entry_ptr(&self, slot: usize) -> *mut T {
        unsafe { self.entries.add(slot) }
    }

    /// Enqueue an entry (producer side).
    ///
    /// `local_head` is producer-private (stack-local, not in SHM).
    /// On success, `local_head` is incremented.
    ///
    /// This matches rapace-core's `DescRing::enqueue` signature.
    pub fn enqueue(&self, local_head: &mut u64, entry: &T) -> Result<(), RingFull> {
        let header = self.header();
        let capacity = header.capacity as u64;
        let mask = header.mask();

        let tail = header.tail.load(Ordering::Acquire);
        if local_head.wrapping_sub(tail) >= capacity {
            return Err(RingFull);
        }

        let slot = (*local_head & mask) as usize;
        unsafe {
            ptr::write(self.entry_ptr(slot), *entry);
        }

        *local_head = local_head.wrapping_add(1);
        header.visible_head.store(*local_head, Ordering::Release);

        Ok(())
    }

    /// Dequeue an entry (consumer side).
    ///
    /// This matches rapace-core's `DescRing::dequeue` signature.
    pub fn dequeue(&self) -> Option<T> {
        let header = self.header();

        let tail = header.tail.load(Ordering::Relaxed);
        let visible = header.visible_head.load(Ordering::Acquire);

        if tail >= visible {
            return None;
        }

        let mask = header.mask();
        let slot = (tail & mask) as usize;
        let entry = unsafe { ptr::read(self.entry_ptr(slot)) };

        header.tail.store(tail.wrapping_add(1), Ordering::Release);

        Some(entry)
    }

    /// Check if the ring is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.header().is_empty()
    }

    /// Check if the ring is full (given producer's local head).
    #[inline]
    pub fn is_full(&self, local_head: u64) -> bool {
        self.header().is_full(local_head)
    }

    /// Get the capacity of the ring.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.header().capacity
    }

    /// Get the ring status (for diagnostics).
    pub fn status(&self) -> RingStatus {
        let header = self.header();
        let visible_head = header.visible_head.load(Ordering::Acquire);
        let tail = header.tail.load(Ordering::Acquire);
        let capacity = header.capacity;
        let len = visible_head.saturating_sub(tail) as u32;

        RingStatus {
            visible_head,
            tail,
            capacity,
            len,
        }
    }
}
