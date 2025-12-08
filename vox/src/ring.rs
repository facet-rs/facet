// src/ring.rs

use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use crate::layout::{DescRingHeader, MsgDescHot};

/// Shared ring state. Not directly usable—must split into Producer/Consumer.
pub struct Ring {
    header: NonNull<DescRingHeader>,
    capacity: u64,
}

// Safety: Ring can be sent between threads as long as only one thread
// uses Producer and one uses Consumer
unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

/// Producer half of the ring. Only one exists per ring.
pub struct Producer<'ring> {
    ring: &'ring Ring,
    local_head: u64,  // Private to producer, not in SHM
}

/// Consumer half of the ring. Only one exists per ring.
pub struct Consumer<'ring> {
    ring: &'ring Ring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingFull;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingEmpty;

impl Ring {
    /// # Safety
    /// - `header` must point to a valid, mapped DescRingHeader that outlives the Ring
    /// - The memory after the header must contain `capacity` MsgDescHot entries
    /// - `capacity` must be a power of 2
    pub unsafe fn from_raw(header: NonNull<DescRingHeader>, capacity: u32) -> Self {
        debug_assert!(capacity.is_power_of_two(), "capacity must be power of 2");
        Ring {
            header,
            capacity: capacity as u64,
        }
    }

    /// Split into producer and consumer halves.
    ///
    /// This enforces SPSC at the type level: you get exactly one of each,
    /// and they borrow the ring so it can't be split again.
    pub fn split(&mut self) -> (Producer<'_>, Consumer<'_>) {
        let visible_head = self.header().visible_head.load(Ordering::Acquire);
        (
            Producer { ring: self, local_head: visible_head },
            Consumer { ring: self },
        )
    }

    fn header(&self) -> &DescRingHeader {
        unsafe { self.header.as_ref() }
    }

    /// Get pointer to descriptor at index
    ///
    /// # Safety
    /// Index must be < capacity
    unsafe fn desc_ptr(&self, idx: usize) -> *mut MsgDescHot {
        let base = self.header.as_ptr().add(1) as *mut MsgDescHot;
        base.add(idx)
    }
}

impl<'ring> Producer<'ring> {
    /// Try to enqueue a descriptor. Returns Err if ring is full.
    pub fn try_enqueue(&mut self, desc: MsgDescHot) -> Result<(), RingFull> {
        let header = self.ring.header();
        let tail = header.tail.load(Ordering::Acquire);

        // Check if full: (head - tail) >= capacity
        if self.local_head.wrapping_sub(tail) >= self.ring.capacity {
            return Err(RingFull);
        }

        let idx = (self.local_head & (self.ring.capacity - 1)) as usize;

        // Write descriptor (we own this slot)
        unsafe {
            let slot = self.ring.desc_ptr(idx);
            std::ptr::write(slot, desc);
        }

        self.local_head += 1;

        // Publish: make descriptor visible to consumer
        // Release ordering ensures desc write completes before consumer sees new head
        header.visible_head.store(self.local_head, Ordering::Release);

        Ok(())
    }

    /// Check how many slots are available for writing
    pub fn available(&self) -> u64 {
        let tail = self.ring.header().tail.load(Ordering::Acquire);
        self.ring.capacity - self.local_head.wrapping_sub(tail)
    }

    /// Check if the ring is full
    pub fn is_full(&self) -> bool {
        self.available() == 0
    }
}

impl<'ring> Consumer<'ring> {
    /// Try to dequeue a descriptor. Returns None if ring is empty.
    pub fn try_dequeue(&mut self) -> Option<MsgDescHot> {
        let header = self.ring.header();
        let tail = header.tail.load(Ordering::Relaxed);
        let visible = header.visible_head.load(Ordering::Acquire);

        // Check if empty
        if tail >= visible {
            return None;
        }

        let idx = (tail & (self.ring.capacity - 1)) as usize;

        // Read descriptor
        let desc = unsafe {
            let slot = self.ring.desc_ptr(idx);
            std::ptr::read(slot)
        };

        // Advance tail
        // Release ordering ensures desc read completes before producer sees freed slot
        header.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Drain up to `max` descriptors.
    pub fn drain(&mut self, max: usize) -> impl Iterator<Item = MsgDescHot> + '_ + use<'_, 'ring> {
        std::iter::from_fn(move || self.try_dequeue()).take(max)
    }

    /// Check how many items are available for reading
    pub fn len(&self) -> u64 {
        let header = self.ring.header();
        let tail = header.tail.load(Ordering::Relaxed);
        let visible = header.visible_head.load(Ordering::Acquire);
        visible.saturating_sub(tail)
    }

    /// Check if the ring is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{alloc_zeroed, dealloc, Layout};

    /// Test helper: allocate aligned memory for ring + descriptors
    struct TestRing {
        ptr: *mut u8,
        layout: Layout,
        capacity: u32,
    }

    impl TestRing {
        fn new(capacity: u32) -> Self {
            assert!(capacity.is_power_of_two());
            let header_size = std::mem::size_of::<DescRingHeader>();
            let descs_size = std::mem::size_of::<MsgDescHot>() * capacity as usize;
            let total_size = header_size + descs_size;
            let layout = Layout::from_size_align(total_size, 64).unwrap();

            let ptr = unsafe { alloc_zeroed(layout) };
            assert!(!ptr.is_null());

            // Initialize header
            unsafe {
                let header = ptr as *mut DescRingHeader;
                (*header).capacity = capacity;
            }

            TestRing { ptr, layout, capacity }
        }

        fn as_ring(&self) -> Ring {
            unsafe {
                Ring::from_raw(
                    NonNull::new(self.ptr as *mut DescRingHeader).unwrap(),
                    self.capacity,
                )
            }
        }
    }

    impl Drop for TestRing {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) }
        }
    }

    #[test]
    fn empty_ring_dequeue_returns_none() {
        let test_ring = TestRing::new(4);
        let mut ring = test_ring.as_ring();
        let (_, mut consumer) = ring.split();

        assert!(consumer.try_dequeue().is_none());
        assert!(consumer.is_empty());
    }

    #[test]
    fn enqueue_dequeue_single() {
        let test_ring = TestRing::new(4);
        let mut ring = test_ring.as_ring();
        let (mut producer, mut consumer) = ring.split();

        let mut desc = MsgDescHot::default();
        desc.msg_id = 42;
        desc.channel_id = 1;

        producer.try_enqueue(desc.clone()).unwrap();

        let received = consumer.try_dequeue().unwrap();
        assert_eq!(received.msg_id, 42);
        assert_eq!(received.channel_id, 1);
    }

    #[test]
    fn ring_full_returns_error() {
        let test_ring = TestRing::new(2);
        let mut ring = test_ring.as_ring();
        let (mut producer, _consumer) = ring.split();

        producer.try_enqueue(MsgDescHot::default()).unwrap();
        producer.try_enqueue(MsgDescHot::default()).unwrap();

        assert_eq!(producer.try_enqueue(MsgDescHot::default()), Err(RingFull));
        assert!(producer.is_full());
    }

    #[test]
    fn fifo_ordering() {
        let test_ring = TestRing::new(8);
        let mut ring = test_ring.as_ring();
        let (mut producer, mut consumer) = ring.split();

        for i in 0..5 {
            let mut desc = MsgDescHot::default();
            desc.msg_id = i;
            producer.try_enqueue(desc).unwrap();
        }

        for i in 0..5 {
            let desc = consumer.try_dequeue().unwrap();
            assert_eq!(desc.msg_id, i);
        }
    }

    #[test]
    fn wraparound() {
        let test_ring = TestRing::new(4);
        let mut ring = test_ring.as_ring();
        let (mut producer, mut consumer) = ring.split();

        // Fill and drain multiple times to test wraparound
        for round in 0..3 {
            for i in 0..3 {
                let mut desc = MsgDescHot::default();
                desc.msg_id = round * 10 + i;
                producer.try_enqueue(desc).unwrap();
            }

            for i in 0..3 {
                let desc = consumer.try_dequeue().unwrap();
                assert_eq!(desc.msg_id, round * 10 + i);
            }
        }
    }

    #[test]
    fn drain_iterator() {
        let test_ring = TestRing::new(8);
        let mut ring = test_ring.as_ring();
        let (mut producer, mut consumer) = ring.split();

        for i in 0..5 {
            let mut desc = MsgDescHot::default();
            desc.msg_id = i;
            producer.try_enqueue(desc).unwrap();
        }

        let drained: Vec<_> = consumer.drain(10).collect();
        assert_eq!(drained.len(), 5);
        assert!(consumer.is_empty());
    }
}
