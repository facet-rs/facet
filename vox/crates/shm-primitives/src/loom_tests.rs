#![cfg(all(test, feature = "loom"))]

use crate::region::HeapRegion;
use crate::spsc::{SpscRing, SpscRingHeader, SpscRingRaw};
use crate::sync::{AtomicU32, Ordering, thread};
use crate::treiber::{AllocResult, SlotHandle, TreiberSlab, TreiberSlabHeader, TreiberSlabRaw};
use crate::{SlotMeta, SlotState};
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use loom::sync::Arc;

#[test]
fn spsc_ring_concurrent() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let ring: SpscRing<u64> = unsafe { SpscRing::init(region, 0, 4) };
        let ring = Arc::new(ring);

        let producer_ring = ring.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_ring.split();
            for i in 0..3u64 {
                while producer.try_push(i).is_would_block() {
                    thread::yield_now();
                }
            }
        });

        let consumer_ring = ring.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_ring.split();
            let mut received = alloc::vec::Vec::new();
            while received.len() < 3 {
                if let Some(v) = consumer.try_pop() {
                    received.push(v);
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received, vec![0, 1, 2]);
    });
}

#[test]
fn treiber_concurrent_alloc_free() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 4, 64) };
        let slab = Arc::new(slab);

        let t1_slab = slab.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if let AllocResult::Ok(handle) = t1_slab.try_alloc() {
                t1_slab.free_allocated(handle).unwrap();
            }
        });

        let t2_slab = slab.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if let AllocResult::Ok(handle) = t2_slab.try_alloc() {
                t2_slab.free_allocated(handle).unwrap();
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}

#[test]
fn treiber_no_double_alloc() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 2, 64) };
        let slab = Arc::new(slab);
        let counter = Arc::new(AtomicU32::new(0));

        let run = |slab: Arc<TreiberSlab>, counter: Arc<AtomicU32>, owner: Arc<HeapRegion>| {
            let _keep = owner;
            for _ in 0..2 {
                if let AllocResult::Ok(_handle) = slab.try_alloc() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        };

        let t1 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        let t2 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        t1.join().unwrap();
        t2.join().unwrap();

        assert!(counter.load(Ordering::SeqCst) <= 2);
    });
}

#[test]
fn slot_state_transitions() {
    loom::model(|| {
        let meta = Arc::new(SlotMeta {
            generation: AtomicU32::new(0),
            state: AtomicU32::new(SlotState::Free as u32),
        });

        let t1 = thread::spawn({
            let meta = meta.clone();
            move || meta.try_transition(SlotState::Free, SlotState::Allocated)
        });

        let t2 = thread::spawn({
            let meta = meta.clone();
            move || meta.try_transition(SlotState::Free, SlotState::Allocated)
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();
        assert!(r1.is_ok() != r2.is_ok());
    });
}

#[test]
fn alloc_enqueue_dequeue_free_cycle() {
    loom::model(|| {
        let slab_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let slab_region = slab_owner.region();
        let slab = unsafe { TreiberSlab::init(slab_region, 0, 4, 64) };
        let slab = Arc::new(slab);

        let ring_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let ring_region = ring_owner.region();
        let ring: SpscRing<SlotHandle> = unsafe { SpscRing::init(ring_region, 0, 4) };
        let ring = Arc::new(ring);

        let producer_slab = slab.clone();
        let producer_ring = ring.clone();
        let producer_owner = (slab_owner.clone(), ring_owner.clone());
        let producer = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_ring.split();
            let handle = match producer_slab.try_alloc() {
                AllocResult::Ok(handle) => handle,
                AllocResult::WouldBlock => return,
            };
            unsafe {
                let ptr = producer_slab.slot_data_ptr(handle);
                core::ptr::write_bytes(ptr, 0xAB, 16);
            }
            producer_slab.mark_in_flight(handle).unwrap();
            while producer.try_push(handle).is_would_block() {
                thread::yield_now();
            }
        });

        let consumer_slab = slab.clone();
        let consumer_ring = ring.clone();
        let consumer_owner = (slab_owner.clone(), ring_owner.clone());
        let consumer = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_ring.split();
            loop {
                if let Some(handle) = consumer.try_pop() {
                    consumer_slab.free(handle).unwrap();
                    break;
                }
                thread::yield_now();
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    });
}

/// Test ABA scenario: Thread 1 allocs, pauses, frees.
/// Thread 2 allocs same slot, allocs another, frees both.
/// Without proper tagging, Thread 1's CAS could succeed incorrectly.
#[test]
fn treiber_aba_stress() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        // 3 slots to allow the ABA pattern
        let slab = unsafe { TreiberSlab::init(region, 0, 3, 64) };
        let slab = Arc::new(slab);

        // Thread 1: alloc slot, then free it
        let t1_slab = slab.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if let AllocResult::Ok(handle) = t1_slab.try_alloc() {
                // Simulate some work before freeing
                thread::yield_now();
                t1_slab.free_allocated(handle).unwrap();
            }
        });

        // Thread 2: alloc two slots, free them in reverse order
        // This creates the classic ABA: slot A freed, then back at head
        let t2_slab = slab.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            let mut handles = Vec::new();
            for _ in 0..2 {
                if let AllocResult::Ok(handle) = t2_slab.try_alloc() {
                    handles.push(handle);
                }
            }
            // Free in reverse order
            for handle in handles.into_iter().rev() {
                t2_slab.free_allocated(handle).unwrap();
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Verify free list integrity: should have 3 free slots
        assert_eq!(slab.free_count_approx(), 3);
    });
}

/// Test that generation counters increment correctly per-slot.
/// When the same slot is reused, its generation must be higher.
#[test]
fn treiber_generation_increments() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        // Single slot forces reuse
        let slab = unsafe { TreiberSlab::init(region, 0, 1, 64) };

        let mut prev_gen_for_slot: Option<u32> = None;
        for _ in 0..4 {
            if let AllocResult::Ok(handle) = slab.try_alloc() {
                if let Some(prev) = prev_gen_for_slot {
                    assert!(
                        handle.generation > prev,
                        "generation must increase: {} > {}",
                        handle.generation,
                        prev
                    );
                }
                prev_gen_for_slot = Some(handle.generation);
                slab.free_allocated(handle).unwrap();
            }
        }
    });
}

/// Test SPSC ring wraparound deterministically (no scheduling explosion).
/// Uses a small ring and pushes more than capacity total items.
/// Note: We use capacity 4 and push 5 items to trigger one wrap.
#[test]
fn spsc_ring_wraparound() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        // Capacity 4, push 5 items to wrap once
        let ring: SpscRing<u64> = unsafe { SpscRing::init(region, 0, 4) };
        let (mut producer, mut consumer) = ring.split();

        for i in 0..4u64 {
            assert_eq!(producer.try_push(i), crate::spsc::PushResult::Ok);
        }
        let mut received = Vec::new();
        received.push(consumer.try_pop().unwrap());

        assert_eq!(producer.try_push(4), crate::spsc::PushResult::Ok);
        while let Some(v) = consumer.try_pop() {
            received.push(v);
        }

        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    });
}

/// Test concurrent exhaustion and freeing:
/// One thread tries to exhaust the slab while another frees slots.
#[test]
fn treiber_exhaust_while_freeing() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 2, 64) };
        let slab = Arc::new(slab);

        // Pre-allocate one slot that we'll free from another thread
        let initial_handle = match slab.try_alloc() {
            AllocResult::Ok(h) => h,
            AllocResult::WouldBlock => panic!("should have slots"),
        };

        let t1_slab = slab.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            // Try to allocate - may or may not succeed depending on timing
            let mut allocated = Vec::new();
            for _ in 0..3 {
                if let AllocResult::Ok(handle) = t1_slab.try_alloc() {
                    allocated.push(handle);
                }
            }
            // Free everything we got
            for handle in allocated {
                t1_slab.free_allocated(handle).unwrap();
            }
        });

        let t2_slab = slab.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            // Free the pre-allocated slot, making room for t1
            t2_slab.free_allocated(initial_handle).unwrap();
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // All slots should be free at the end
        assert_eq!(slab.free_count_approx(), 2);
    });
}

/// Test mark_in_flight under contention:
/// Multiple threads race to mark the same slot as in-flight.
/// Only one should succeed.
#[test]
fn treiber_mark_in_flight_race() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 4, 64) };
        let slab = Arc::new(slab);

        // Allocate a slot
        let handle = match slab.try_alloc() {
            AllocResult::Ok(h) => h,
            AllocResult::WouldBlock => panic!("should have slots"),
        };
        let handle = Arc::new(handle);
        let success_count = Arc::new(AtomicU32::new(0));

        // Two threads race to mark_in_flight
        let t1_slab = slab.clone();
        let t1_handle = handle.clone();
        let t1_count = success_count.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if t1_slab.mark_in_flight(*t1_handle).is_ok() {
                t1_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        let t2_slab = slab.clone();
        let t2_handle = handle.clone();
        let t2_count = success_count.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if t2_slab.mark_in_flight(*t2_handle).is_ok() {
                t2_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Exactly one thread should succeed
        assert_eq!(success_count.load(Ordering::SeqCst), 1);
    });
}

/// Test the full free() path (InFlight -> Free) under contention.
/// Only one thread should be able to free a slot.
#[test]
fn treiber_free_race() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 4, 64) };
        let slab = Arc::new(slab);

        // Allocate and mark in-flight
        let handle = match slab.try_alloc() {
            AllocResult::Ok(h) => h,
            AllocResult::WouldBlock => panic!("should have slots"),
        };
        slab.mark_in_flight(handle).unwrap();

        let handle = Arc::new(handle);
        let success_count = Arc::new(AtomicU32::new(0));

        // Two threads race to free
        let t1_slab = slab.clone();
        let t1_handle = handle.clone();
        let t1_count = success_count.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if t1_slab.free(*t1_handle).is_ok() {
                t1_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        let t2_slab = slab.clone();
        let t2_handle = handle.clone();
        let t2_count = success_count.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if t2_slab.free(*t2_handle).is_ok() {
                t2_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Exactly one thread should succeed
        assert_eq!(success_count.load(Ordering::SeqCst), 1);
    });
}

// =============================================================================
// Raw API Tests - These exercise the same code paths Rapace uses
// =============================================================================

/// Test SpscRingRaw with concurrent producer and consumer.
/// This is the code path Rapace's DescRing uses.
#[test]
fn spsc_ring_raw_concurrent() {
    loom::model(|| {
        const CAPACITY: u32 = 4;
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));

        // Initialize header manually (like rapace-core does)
        let header_ptr = region_owner.region().as_ptr() as *mut SpscRingHeader;
        unsafe { (*header_ptr).init(CAPACITY) };

        let entries_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<SpscRingHeader>()) as *mut u64
        };

        // Create the raw ring
        let ring: SpscRingRaw<u64> = unsafe { SpscRingRaw::from_raw(header_ptr, entries_ptr) };
        let ring = Arc::new(ring);

        let producer_ring = ring.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            let mut local_head = 0u64;
            for i in 0..3u64 {
                while producer_ring.enqueue(&mut local_head, &i).is_err() {
                    thread::yield_now();
                }
            }
        });

        let consumer_ring = ring.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            let mut received = Vec::new();
            while received.len() < 3 {
                if let Some(v) = consumer_ring.dequeue() {
                    received.push(v);
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received, vec![0, 1, 2]);
    });
}

/// Test SpscRingRaw wraparound behavior.
#[test]
fn spsc_ring_raw_wraparound() {
    loom::model(|| {
        const CAPACITY: u32 = 4;
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));

        let header_ptr = region_owner.region().as_ptr() as *mut SpscRingHeader;
        unsafe { (*header_ptr).init(CAPACITY) };

        let entries_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<SpscRingHeader>()) as *mut u64
        };

        let ring: SpscRingRaw<u64> = unsafe { SpscRingRaw::from_raw(header_ptr, entries_ptr) };
        let mut local_head = 0u64;

        // Fill the ring
        for i in 0..4u64 {
            assert!(ring.enqueue(&mut local_head, &i).is_ok());
        }

        // Pop one to make room
        let mut received = Vec::new();
        received.push(ring.dequeue().unwrap());

        // Push one more (wraps around)
        assert!(ring.enqueue(&mut local_head, &4).is_ok());

        // Drain the rest
        while let Some(v) = ring.dequeue() {
            received.push(v);
        }

        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    });
}

/// Test TreiberSlabRaw with concurrent alloc/free.
/// This is the code path Rapace's DataSegment uses.
#[test]
fn treiber_slab_raw_concurrent_alloc_free() {
    loom::model(|| {
        const SLOT_COUNT: u32 = 4;
        const SLOT_SIZE: u32 = 64;
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));

        // Calculate offsets like rapace-core does
        let header_ptr = region_owner.region().as_ptr() as *mut TreiberSlabHeader;
        let meta_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>()) as *mut SlotMeta
        };
        let data_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>())
                .add(SLOT_COUNT as usize * size_of::<SlotMeta>())
        };

        // Initialize header
        unsafe { (*header_ptr).init(SLOT_SIZE, SLOT_COUNT) };

        // Initialize slot metadata
        for i in 0..SLOT_COUNT {
            unsafe { (*meta_ptr.add(i as usize)).init() };
        }

        // Create the raw slab and initialize free list
        let slab = unsafe { TreiberSlabRaw::from_raw(header_ptr, meta_ptr, data_ptr) };
        unsafe { slab.init_free_list() };
        let slab = Arc::new(slab);

        let t1_slab = slab.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if let AllocResult::Ok(handle) = t1_slab.try_alloc() {
                t1_slab.free_allocated(handle).unwrap();
            }
        });

        let t2_slab = slab.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if let AllocResult::Ok(handle) = t2_slab.try_alloc() {
                t2_slab.free_allocated(handle).unwrap();
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}

/// Test TreiberSlabRaw: no double allocation (2 threads, 2 slots).
#[test]
fn treiber_slab_raw_no_double_alloc() {
    loom::model(|| {
        const SLOT_COUNT: u32 = 2;
        const SLOT_SIZE: u32 = 64;
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));

        let header_ptr = region_owner.region().as_ptr() as *mut TreiberSlabHeader;
        let meta_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>()) as *mut SlotMeta
        };
        let data_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>())
                .add(SLOT_COUNT as usize * size_of::<SlotMeta>())
        };

        unsafe { (*header_ptr).init(SLOT_SIZE, SLOT_COUNT) };
        for i in 0..SLOT_COUNT {
            unsafe { (*meta_ptr.add(i as usize)).init() };
        }

        let slab = unsafe { TreiberSlabRaw::from_raw(header_ptr, meta_ptr, data_ptr) };
        unsafe { slab.init_free_list() };
        let slab = Arc::new(slab);
        let counter = Arc::new(AtomicU32::new(0));

        let run = |slab: Arc<TreiberSlabRaw>, counter: Arc<AtomicU32>, owner: Arc<HeapRegion>| {
            let _keep = owner;
            for _ in 0..2 {
                if let AllocResult::Ok(_handle) = slab.try_alloc() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        };

        let t1 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        let t2 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // At most 2 allocations should succeed (we have 2 slots)
        assert!(counter.load(Ordering::SeqCst) <= 2);
    });
}

/// Test the full alloc → enqueue → dequeue → free cycle with Raw APIs.
/// This mirrors exactly what Rapace does: DataSegment + DescRing.
#[test]
fn raw_alloc_enqueue_dequeue_free_cycle() {
    loom::model(|| {
        const SLOT_COUNT: u32 = 4;
        const SLOT_SIZE: u32 = 64;
        const RING_CAPACITY: u32 = 4;

        // Set up slab (DataSegment equivalent)
        let slab_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let slab_header = slab_owner.region().as_ptr() as *mut TreiberSlabHeader;
        let slab_meta = unsafe {
            slab_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>()) as *mut SlotMeta
        };
        let slab_data = unsafe {
            slab_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>())
                .add(SLOT_COUNT as usize * size_of::<SlotMeta>())
        };

        unsafe { (*slab_header).init(SLOT_SIZE, SLOT_COUNT) };
        for i in 0..SLOT_COUNT {
            unsafe { (*slab_meta.add(i as usize)).init() };
        }
        let slab = unsafe { TreiberSlabRaw::from_raw(slab_header, slab_meta, slab_data) };
        unsafe { slab.init_free_list() };
        let slab = Arc::new(slab);

        // Set up ring (DescRing equivalent)
        let ring_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let ring_header = ring_owner.region().as_ptr() as *mut SpscRingHeader;
        unsafe { (*ring_header).init(RING_CAPACITY) };
        let ring_entries = unsafe {
            ring_owner
                .region()
                .as_ptr()
                .add(size_of::<SpscRingHeader>()) as *mut SlotHandle
        };
        let ring: SpscRingRaw<SlotHandle> =
            unsafe { SpscRingRaw::from_raw(ring_header, ring_entries) };
        let ring = Arc::new(ring);

        // Producer: alloc → mark_in_flight → enqueue
        let producer_slab = slab.clone();
        let producer_ring = ring.clone();
        let producer_owner = (slab_owner.clone(), ring_owner.clone());
        let producer = thread::spawn(move || {
            let _keep = producer_owner;
            let mut local_head = 0u64;
            let handle = match producer_slab.try_alloc() {
                AllocResult::Ok(handle) => handle,
                AllocResult::WouldBlock => return,
            };
            producer_slab.mark_in_flight(handle).unwrap();
            while producer_ring.enqueue(&mut local_head, &handle).is_err() {
                thread::yield_now();
            }
        });

        // Consumer: dequeue → free
        let consumer_slab = slab.clone();
        let consumer_ring = ring.clone();
        let consumer_owner = (slab_owner.clone(), ring_owner.clone());
        let consumer = thread::spawn(move || {
            let _keep = consumer_owner;
            loop {
                if let Some(handle) = consumer_ring.dequeue() {
                    consumer_slab.free(handle).unwrap();
                    break;
                }
                thread::yield_now();
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    });
}

/// Test TreiberSlabRaw mark_in_flight race (only one should succeed).
#[test]
fn treiber_slab_raw_mark_in_flight_race() {
    loom::model(|| {
        const SLOT_COUNT: u32 = 4;
        const SLOT_SIZE: u32 = 64;
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));

        let header_ptr = region_owner.region().as_ptr() as *mut TreiberSlabHeader;
        let meta_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>()) as *mut SlotMeta
        };
        let data_ptr = unsafe {
            region_owner
                .region()
                .as_ptr()
                .add(size_of::<TreiberSlabHeader>())
                .add(SLOT_COUNT as usize * size_of::<SlotMeta>())
        };

        unsafe { (*header_ptr).init(SLOT_SIZE, SLOT_COUNT) };
        for i in 0..SLOT_COUNT {
            unsafe { (*meta_ptr.add(i as usize)).init() };
        }

        let slab = unsafe { TreiberSlabRaw::from_raw(header_ptr, meta_ptr, data_ptr) };
        unsafe { slab.init_free_list() };
        let slab = Arc::new(slab);

        // Allocate a slot
        let handle = match slab.try_alloc() {
            AllocResult::Ok(h) => h,
            AllocResult::WouldBlock => panic!("should have slots"),
        };
        let handle = Arc::new(handle);
        let success_count = Arc::new(AtomicU32::new(0));

        // Two threads race to mark_in_flight
        let t1_slab = slab.clone();
        let t1_handle = handle.clone();
        let t1_count = success_count.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if t1_slab.mark_in_flight(*t1_handle).is_ok() {
                t1_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        let t2_slab = slab.clone();
        let t2_handle = handle.clone();
        let t2_count = success_count.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if t2_slab.mark_in_flight(*t2_handle).is_ok() {
                t2_count.fetch_add(1, Ordering::SeqCst);
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Exactly one thread should succeed
        assert_eq!(success_count.load(Ordering::SeqCst), 1);
    });
}
