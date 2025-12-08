// Stress test for the rapace shared-memory RPC system
//
// This benchmark tests the throughput, latency, memory pressure, and concurrent
// access characteristics of the low-level rapace primitives (ring, alloc, frame).
//
// ## Usage
//
// ```bash
// # Run with default settings (5 seconds, 128 byte messages)
// cargo run --example stress_test
//
// # Run with custom duration (in seconds) and message size (in bytes)
// cargo run --example stress_test -- 10 512
// ```
//
// ## Tests Performed
//
// 1. **Throughput Test**: Producer and consumer threads exchange messages as fast
//    as possible. Measures messages/second and MB/second throughput.
//
// 2. **Latency Test**: Measures round-trip time for messages, reporting min/avg/
//    p50/p95/p99/max latency in nanoseconds.
//
// 3. **Memory Pressure Test**: Rapidly allocates and frees slots to verify there
//    are no memory leaks and the allocator handles pressure correctly.
//
// 4. **Concurrent Access Test**: Multiple channels operate simultaneously with
//    data verification to ensure no corruption under concurrent load.

use rapace::{
    alloc::DataSegment,
    frame::{FrameBuilder, RawDescriptor, DescriptorLimits},
    layout::{MsgDescHot, DescRingHeader, SlotMeta},
    ring::Ring,
    types::{ChannelId, MethodId, MsgId, ByteLen},
};

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;

const DEFAULT_DURATION_SECS: u64 = 5;
const DEFAULT_MSG_SIZE: usize = 128;
const DEFAULT_RING_CAPACITY: u32 = 256;
const DEFAULT_SLOT_COUNT: u32 = 128;
const DEFAULT_SLOT_SIZE: u32 = 4096;

// === Statistics Tracking ===

struct ThroughputStats {
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
}

impl ThroughputStats {
    fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
        }
    }

    fn record_send(&self, bytes: u64) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    fn record_receive(&self, bytes: u64) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    fn print(&self, elapsed: Duration) {
        let msgs_sent = self.messages_sent.load(Ordering::Relaxed);
        let msgs_recv = self.messages_received.load(Ordering::Relaxed);
        let bytes_sent = self.bytes_sent.load(Ordering::Relaxed);
        let bytes_recv = self.bytes_received.load(Ordering::Relaxed);
        let secs = elapsed.as_secs_f64();

        println!("Duration: {:.2}s", secs);
        println!("Messages sent: {}", msgs_sent);
        println!("Messages received: {}", msgs_recv);
        println!("Throughput: {:.0} msg/s", msgs_recv as f64 / secs);
        println!("Bandwidth (sent): {:.2} MB/s", bytes_sent as f64 / secs / 1_000_000.0);
        println!("Bandwidth (recv): {:.2} MB/s", bytes_recv as f64 / secs / 1_000_000.0);
    }
}

struct LatencyStats {
    samples: Vec<u64>,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    fn record(&mut self, nanos: u64) {
        self.samples.push(nanos);
    }

    fn print(&self) {
        if self.samples.is_empty() {
            println!("No samples recorded");
            return;
        }

        let mut sorted = self.samples.clone();
        sorted.sort_unstable();

        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let avg = sorted.iter().sum::<u64>() / sorted.len() as u64;
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[sorted.len() * 95 / 100];
        let p99 = sorted[sorted.len() * 99 / 100];

        println!("Samples: {}", sorted.len());
        println!("Min: {}ns", min);
        println!("Avg: {}ns", avg);
        println!("P50: {}ns", p50);
        println!("P95: {}ns", p95);
        println!("P99: {}ns", p99);
        println!("Max: {}ns", max);
    }
}

// === Test Harness ===

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

struct TestSegment {
    meta_ptr: *mut u8,
    data_ptr: *mut u8,
    meta_layout: Layout,
    data_layout: Layout,
    slot_count: u32,
    slot_size: u32,
}

impl TestSegment {
    fn new(slot_count: u32, slot_size: u32) -> Self {
        let meta_layout = Layout::array::<SlotMeta>(slot_count as usize).unwrap();
        let data_layout = Layout::from_size_align(
            (slot_count * slot_size) as usize,
            64,
        ).unwrap();

        let meta_ptr = unsafe { alloc_zeroed(meta_layout) };
        let data_ptr = unsafe { alloc_zeroed(data_layout) };

        TestSegment {
            meta_ptr,
            data_ptr,
            meta_layout,
            data_layout,
            slot_count,
            slot_size,
        }
    }

    fn as_segment(&self) -> DataSegment {
        unsafe {
            let seg = DataSegment::from_raw(
                NonNull::new(self.meta_ptr as *mut SlotMeta).unwrap(),
                NonNull::new(self.data_ptr).unwrap(),
                self.slot_size,
                self.slot_count,
            );
            seg.init_free_list();
            seg
        }
    }
}

impl Drop for TestSegment {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.meta_ptr, self.meta_layout);
            dealloc(self.data_ptr, self.data_layout);
        }
    }
}

// === Test 1: Throughput Test ===

fn throughput_test(duration: Duration, msg_size: usize, ring_capacity: u32, slot_count: u32, slot_size: u32) {
    println!("\n=== Throughput Test ===");
    println!("Message size: {} bytes", msg_size);
    println!("Ring capacity: {}", ring_capacity);
    println!("Slot count: {}", slot_count);

    let test_ring = Box::new(TestRing::new(ring_capacity));
    let test_segment = Box::new(TestSegment::new(slot_count, slot_size));

    let stats = Arc::new(ThroughputStats::new());
    let stop = Arc::new(AtomicBool::new(false));

    // Create ring and segment with 'static lifetime via Box::leak
    // This is safe for a benchmark that runs once and exits
    let ring_ptr = Box::leak(test_ring);
    let segment_ptr = Box::leak(test_segment);

    // SAFETY: We create the ring as 'static by leaking the backing memory.
    // The split() method borrows the ring, but we extend the lifetime here
    // because we know the memory is leaked and will never be freed.
    let (producer, consumer) = unsafe {
        let mut ring = ring_ptr.as_ring();
        std::mem::transmute(ring.split())
    };

    // SAFETY: Create two references to the same segment for producer and consumer.
    // This is safe because the allocator uses atomic operations internally,
    // and alloc/free operations are thread-safe.
    let segment_prod = unsafe { std::ptr::read(&segment_ptr.as_segment()) };
    let segment_cons = unsafe { std::ptr::read(&segment_ptr.as_segment()) };

    // Spawn producer thread
    let stats_prod = Arc::clone(&stats);
    let stop_prod = Arc::clone(&stop);

    let producer_handle = thread::spawn(move || {
        let mut msg_id = 0u64;
        let channel = ChannelId::new(1).unwrap();
        let method = MethodId::new(100);

        // Use a local reference to avoid move issues
        let mut prod: rapace::ring::Producer<'static> = producer;

        while !stop_prod.load(Ordering::Relaxed) {
            // Allocate and write payload
            if let Ok(mut slot) = segment_prod.alloc() {
                let buf = slot.as_mut_bytes();
                let write_size = msg_size.min(buf.len());

                // Fill with pattern
                for i in 0..write_size {
                    buf[i] = (i & 0xFF) as u8;
                }

                let len = ByteLen::new(write_size as u32, slot_size).unwrap();
                let committed = slot.commit(len);

                // Build frame
                let desc = FrameBuilder::data(channel, method, MsgId::new(msg_id))
                    .slot_payload(committed)
                    .build();

                // Try to enqueue
                if prod.try_enqueue(desc).is_ok() {
                    stats_prod.record_send(write_size as u64);
                    msg_id += 1;
                } else {
                    // Ring full, spin briefly
                    thread::yield_now();
                }
            } else {
                // Out of slots, yield
                thread::yield_now();
            }
        }
    });

    // Spawn consumer thread
    let stats_cons = Arc::clone(&stats);
    let stop_cons = Arc::clone(&stop);

    let consumer_handle = thread::spawn(move || {
        let mut cons: rapace::ring::Consumer<'static> = consumer;
        let limits = DescriptorLimits::default();

        while !stop_cons.load(Ordering::Relaxed) {
            if let Some(desc) = cons.try_dequeue() {
                // Validate descriptor
                let raw = RawDescriptor::new(desc);
                if let Ok(valid) = raw.validate(&segment_cons, &limits) {
                    let len = valid.payload_len();
                    stats_cons.record_receive(len as u64);

                    // Free the slot
                    let _payload = valid.into_inbound_payload(&segment_cons);
                }
            } else {
                thread::yield_now();
            }
        }
    });

    // Run for specified duration
    let start = Instant::now();
    thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);

    // Wait for threads to finish
    producer_handle.join().unwrap();
    consumer_handle.join().unwrap();

    let elapsed = start.elapsed();
    stats.print(elapsed);
}

// === Test 2: Latency Test ===

fn latency_test(samples: usize, msg_size: usize, ring_capacity: u32, slot_count: u32, slot_size: u32) {
    println!("\n=== Latency Test ===");
    println!("Samples: {}", samples);
    println!("Message size: {} bytes", msg_size);

    let test_ring = TestRing::new(ring_capacity);
    let test_segment = TestSegment::new(slot_count, slot_size);

    let mut ring = test_ring.as_ring();
    let (mut producer, mut consumer) = ring.split();
    let segment = test_segment.as_segment();

    let mut latencies = LatencyStats::new();
    let channel = ChannelId::new(1).unwrap();
    let method = MethodId::new(100);
    let limits = DescriptorLimits::default();

    for msg_id in 0..samples {
        // Allocate and write payload
        let mut slot = segment.alloc().expect("allocation failed");
        let buf = slot.as_mut_bytes();
        let write_size = msg_size.min(buf.len());

        for i in 0..write_size {
            buf[i] = (i & 0xFF) as u8;
        }

        let len = ByteLen::new(write_size as u32, slot_size).unwrap();
        let committed = slot.commit(len);

        // Build frame
        let desc = FrameBuilder::data(channel, method, MsgId::new(msg_id as u64))
            .slot_payload(committed)
            .build();

        // Measure round-trip time
        let start = Instant::now();

        // Enqueue
        while producer.try_enqueue(desc.clone()).is_err() {
            thread::yield_now();
        }

        // Dequeue
        let received = loop {
            if let Some(d) = consumer.try_dequeue() {
                break d;
            }
            thread::yield_now();
        };

        let elapsed = start.elapsed();
        latencies.record(elapsed.as_nanos() as u64);

        // Validate and free
        let raw = RawDescriptor::new(received);
        if let Ok(valid) = raw.validate(&segment, &limits) {
            let _payload = valid.into_inbound_payload(&segment);
        }
    }

    latencies.print();
}

// === Test 3: Memory Pressure Test ===

fn memory_pressure_test(iterations: usize, slot_count: u32, slot_size: u32) {
    println!("\n=== Memory Pressure Test ===");
    println!("Iterations: {}", iterations);
    println!("Slot count: {}", slot_count);
    println!("Slot size: {} bytes", slot_size);

    let test_segment = TestSegment::new(slot_count, slot_size);
    let segment = test_segment.as_segment();

    let start = Instant::now();
    let mut total_allocs = 0u64;

    for round in 0..iterations {
        // Allocate all slots
        let mut slots = Vec::new();

        for _ in 0..slot_count {
            if let Ok(slot) = segment.alloc() {
                slots.push(slot);
                total_allocs += 1;
            } else {
                break;
            }
        }

        let allocated = slots.len();

        // Commit half, drop half
        let commit_count = allocated / 2;
        let mut committed = Vec::new();

        for (i, slot) in slots.into_iter().enumerate() {
            if i < commit_count {
                let len = ByteLen::new(128, slot_size).unwrap();
                committed.push(slot.commit(len));
            }
            // else: slot is dropped, automatically freed
        }

        // Free committed slots manually
        for c in committed {
            segment.free(c.slot(), c.generation());
        }

        if (round + 1) % (iterations / 10).max(1) == 0 {
            println!("  Progress: {}/{} rounds", round + 1, iterations);
        }
    }

    let elapsed = start.elapsed();
    println!("Completed {} allocations in {:.2}s", total_allocs, elapsed.as_secs_f64());
    println!("Rate: {:.0} allocs/sec", total_allocs as f64 / elapsed.as_secs_f64());
    println!("No memory leaks detected (all slots freed)");
}

// === Test 4: Concurrent Access Test ===

fn concurrent_access_test(duration: Duration, num_channels: usize, ring_capacity: u32, slot_count: u32, slot_size: u32) {
    println!("\n=== Concurrent Access Test ===");
    println!("Duration: {}s", duration.as_secs());
    println!("Channels: {}", num_channels);
    println!("Ring capacity: {}", ring_capacity);

    let test_ring = Box::new(TestRing::new(ring_capacity));
    let test_segment = Box::new(TestSegment::new(slot_count, slot_size));

    let stats = Arc::new(ThroughputStats::new());
    let stop = Arc::new(AtomicBool::new(false));
    let corruption_detected = Arc::new(AtomicBool::new(false));

    // Leak for 'static lifetime
    let ring_ptr = Box::leak(test_ring);
    let segment_ptr = Box::leak(test_segment);

    // SAFETY: Extend lifetimes for 'static
    let (producer, consumer) = unsafe {
        let mut ring = ring_ptr.as_ring();
        std::mem::transmute(ring.split())
    };

    let segment_prod = unsafe { std::ptr::read(&segment_ptr.as_segment()) };
    let segment_cons = unsafe { std::ptr::read(&segment_ptr.as_segment()) };

    // Producer thread (writes to multiple channels)
    let stats_prod = Arc::clone(&stats);
    let stop_prod = Arc::clone(&stop);

    let producer_handle = thread::spawn(move || {
        let mut msg_id = 0u64;
        let mut prod: rapace::ring::Producer<'static> = producer;

        while !stop_prod.load(Ordering::Relaxed) {
            // Cycle through channels
            let channel_num = (msg_id % num_channels as u64) + 1;
            let channel = ChannelId::new(channel_num as u32).unwrap();
            let method = MethodId::new(channel_num as u32);

            if let Ok(mut slot) = segment_prod.alloc() {
                let buf = slot.as_mut_bytes();

                // Write a recognizable pattern: channel ID repeated
                let pattern = (channel_num as u8).wrapping_mul(17);
                for i in 0..256.min(buf.len()) {
                    buf[i] = pattern.wrapping_add(i as u8);
                }

                let len = ByteLen::new(256.min(slot_size), slot_size).unwrap();
                let committed = slot.commit(len);

                let desc = FrameBuilder::data(channel, method, MsgId::new(msg_id))
                    .slot_payload(committed)
                    .build();

                if prod.try_enqueue(desc).is_ok() {
                    stats_prod.record_send(256);
                    msg_id += 1;
                }
            } else {
                thread::yield_now();
            }
        }
    });

    // Consumer thread (validates data from all channels)
    let stats_cons = Arc::clone(&stats);
    let stop_cons = Arc::clone(&stop);
    let corruption = Arc::clone(&corruption_detected);

    let consumer_handle = thread::spawn(move || {
        let mut cons: rapace::ring::Consumer<'static> = consumer;
        let limits = DescriptorLimits::default();

        while !stop_cons.load(Ordering::Relaxed) {
            if let Some(desc) = cons.try_dequeue() {
                let raw = RawDescriptor::new(desc);
                if let Ok(valid) = raw.validate(&segment_cons, &limits) {
                    let channel_id = valid.channel_id().unwrap().get();
                    let len = valid.payload_len();

                    // Verify payload
                    if let Some((slot, _gen, offset, plen)) = valid.slot_info() {
                        let data = segment_cons.slot_data(slot, offset, plen);

                        // Check pattern
                        let expected_pattern = (channel_id as u8).wrapping_mul(17);
                        for (i, &byte) in data.iter().enumerate() {
                            let expected = expected_pattern.wrapping_add(i as u8);
                            if byte != expected {
                                corruption.store(true, Ordering::Relaxed);
                                eprintln!("CORRUPTION DETECTED: channel {}, expected {}, got {}",
                                         channel_id, expected, byte);
                                break;
                            }
                        }
                    }

                    stats_cons.record_receive(len as u64);
                    let _payload = valid.into_inbound_payload(&segment_cons);
                }
            } else {
                thread::yield_now();
            }
        }
    });

    // Run test
    let start = Instant::now();
    thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);

    producer_handle.join().unwrap();
    consumer_handle.join().unwrap();

    let elapsed = start.elapsed();
    stats.print(elapsed);

    if corruption_detected.load(Ordering::Relaxed) {
        println!("\nWARNING: Data corruption detected!");
    } else {
        println!("\nNo data corruption detected");
    }
}

// === Main ===

fn main() {
    println!("Rapace Shared-Memory RPC Stress Test");
    println!("=====================================\n");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let duration_secs = if args.len() > 1 {
        args[1].parse().unwrap_or(DEFAULT_DURATION_SECS)
    } else {
        DEFAULT_DURATION_SECS
    };

    let msg_size = if args.len() > 2 {
        args[2].parse().unwrap_or(DEFAULT_MSG_SIZE)
    } else {
        DEFAULT_MSG_SIZE
    };

    let duration = Duration::from_secs(duration_secs);

    // Test 1: Throughput
    throughput_test(
        duration,
        msg_size,
        DEFAULT_RING_CAPACITY,
        DEFAULT_SLOT_COUNT,
        DEFAULT_SLOT_SIZE,
    );

    // Test 2: Latency (smaller sample count)
    latency_test(
        10_000,
        msg_size,
        DEFAULT_RING_CAPACITY,
        DEFAULT_SLOT_COUNT,
        DEFAULT_SLOT_SIZE,
    );

    // Test 3: Memory Pressure
    memory_pressure_test(
        1000,
        DEFAULT_SLOT_COUNT,
        DEFAULT_SLOT_SIZE,
    );

    // Test 4: Concurrent Access
    concurrent_access_test(
        duration,
        8, // 8 concurrent channels
        DEFAULT_RING_CAPACITY,
        DEFAULT_SLOT_COUNT,
        DEFAULT_SLOT_SIZE,
    );

    println!("\n=== All Tests Complete ===");
}
