// Integration tests for rapace RPC system using real shared memory
//
// These tests exercise the full stack of rapace modules with actual shared
// memory segments, simulating two peers communicating in a single process.

use rapace::alloc::DataSegment;
use rapace::doorbell::Doorbell;
use rapace::frame::{FrameBuilder, RawDescriptor, DescriptorLimits, FrameFlags};
use rapace::layout::{SegmentHeader, DescRingHeader, MsgDescHot, SlotMeta, MAGIC};
use rapace::ring::Ring;
use rapace::shm::{SharedMemory, calculate_segment_size};
use rapace::types::{ChannelId, MethodId, MsgId, ByteLen};
use std::ptr::NonNull;
use std::sync::atomic::Ordering;

// Helper to set up a basic rapace segment layout in shared memory
struct TestSegment {
    shm: SharedMemory,
    ring_capacity: u32,
    slot_count: u32,
    slot_size: u32,
}

impl TestSegment {
    fn new(ring_capacity: u32, slot_count: u32, slot_size: u32) -> Self {
        // Generate unique name to avoid conflicts between concurrent tests
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let name = format!("test-{}-{}", std::process::id(), unique_id);
        let size = calculate_segment_size(ring_capacity, slot_count, slot_size);
        let shm = SharedMemory::create(&name, size).unwrap();

        // Initialize the segment header
        unsafe {
            let header = shm.as_ptr() as *mut SegmentHeader;
            (*header).magic = MAGIC;
            (*header).version = 2;
            (*header).flags = 0;
        }

        TestSegment {
            shm,
            ring_capacity,
            slot_count,
            slot_size,
        }
    }

    // Get pointer to segment header
    fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.shm.as_ptr() as *const SegmentHeader) }
    }

    // Get pointer to A->B ring header
    fn ring_a_to_b_header(&self) -> NonNull<DescRingHeader> {
        let offset = 64; // After SegmentHeader
        unsafe { NonNull::new((self.shm.as_ptr().add(offset)) as *mut DescRingHeader).unwrap() }
    }

    // Get pointer to B->A ring header
    fn ring_b_to_a_header(&self) -> NonNull<DescRingHeader> {
        let ring_header_size = std::mem::size_of::<DescRingHeader>();
        let desc_size = std::mem::size_of::<MsgDescHot>();
        let ring_size = ring_header_size + (desc_size * self.ring_capacity as usize);
        let offset = 64 + ring_size;
        unsafe { NonNull::new((self.shm.as_ptr().add(offset)) as *mut DescRingHeader).unwrap() }
    }

    // Get pointers for data segment
    fn data_segment_pointers(&self) -> (NonNull<SlotMeta>, NonNull<u8>) {
        let ring_header_size = std::mem::size_of::<DescRingHeader>();
        let desc_size = std::mem::size_of::<MsgDescHot>();
        let ring_size = ring_header_size + (desc_size * self.ring_capacity as usize);
        let total_rings = ring_size * 2;
        let slot_meta_size = std::mem::size_of::<SlotMeta>();
        let meta_size = slot_meta_size * self.slot_count as usize;

        // Align to 64 bytes
        let align = |x: usize| (x + 63) & !63;

        let meta_offset = align(64 + total_rings);
        let data_offset = align(meta_offset + meta_size);

        unsafe {
            let meta_ptr = NonNull::new(self.shm.as_ptr().add(meta_offset) as *mut SlotMeta).unwrap();
            let data_ptr = NonNull::new(self.shm.as_ptr().add(data_offset)).unwrap();
            (meta_ptr, data_ptr)
        }
    }

    // Initialize ring headers
    fn init_ring(&self, header: NonNull<DescRingHeader>) {
        unsafe {
            let h = header.as_ptr();
            (*h).capacity = self.ring_capacity;
        }
    }

    // Map a duplicate fd to simulate a second peer
    fn duplicate(&self) -> SharedMemory {
        let fd = self.shm.try_clone_fd().unwrap();
        let size = self.shm.len();
        SharedMemory::from_fd(fd, size).unwrap()
    }
}

// ===== TEST 1: SHM Segment Creation and Mapping =====

#[test]
fn test_shm_segment_creation_and_mapping() {
    // Create a shared memory segment
    let seg = TestSegment::new(64, 16, 4096);

    // Verify we can read the header
    let header = seg.header();
    assert_eq!(header.magic, MAGIC, "Magic number should match");
    assert_eq!(header.version, 2, "Version should be 2");

    // Simulate a second peer by mapping the same fd
    let peer2 = seg.duplicate();

    // Verify peer2 can see the same header
    unsafe {
        let header2 = &*(peer2.as_ptr() as *const SegmentHeader);
        assert_eq!(header2.magic, MAGIC, "Peer 2 should see same magic number");
        assert_eq!(header2.version, 2, "Peer 2 should see same version");
    }

    // Modify epoch from peer1
    unsafe {
        let header = &*(seg.shm.as_ptr() as *const SegmentHeader);
        header.peer_a_epoch.store(42, Ordering::Release);
    }

    // Verify peer2 sees the change
    unsafe {
        let header2 = &*(peer2.as_ptr() as *const SegmentHeader);
        let epoch = header2.peer_a_epoch.load(Ordering::Acquire);
        assert_eq!(epoch, 42, "Peer 2 should see epoch update from peer 1");
    }
}

// ===== TEST 2: Ring Operations Through SHM =====

#[test]
fn test_ring_operations_through_shm() {
    // Create segment and initialize ring
    let seg = TestSegment::new(16, 8, 4096);
    let ring_header = seg.ring_a_to_b_header();
    seg.init_ring(ring_header);

    // Create a ring from the shared memory
    let mut ring = unsafe { Ring::from_raw(ring_header, seg.ring_capacity) };
    let (mut producer, mut consumer) = ring.split();

    // Enqueue descriptors from producer side
    for i in 0..5 {
        let mut desc = MsgDescHot::default();
        desc.msg_id = i;
        desc.channel_id = 1;
        desc.method_id = 100 + i as u32;

        producer.try_enqueue(desc).unwrap();
    }

    // Verify consumer can dequeue in FIFO order
    for i in 0..5 {
        let desc = consumer.try_dequeue().expect("Should have descriptor");
        assert_eq!(desc.msg_id, i, "FIFO ordering should be preserved");
        assert_eq!(desc.channel_id, 1);
        assert_eq!(desc.method_id, 100 + i as u32);
    }

    // Ring should be empty now
    assert!(consumer.is_empty(), "Ring should be empty after dequeuing all items");
}

#[test]
fn test_ring_operations_across_mappings() {
    // Create segment and initialize ring
    let seg = TestSegment::new(32, 8, 4096);
    let ring_header = seg.ring_a_to_b_header();
    seg.init_ring(ring_header);

    // Create peer1's ring (producer)
    let mut ring1 = unsafe { Ring::from_raw(ring_header, seg.ring_capacity) };
    let (mut producer, _) = ring1.split();

    // Simulate peer2 by getting a second mapping
    let peer2_shm = seg.duplicate();
    let ring_header_offset = 64;
    let peer2_ring_header = unsafe {
        NonNull::new((peer2_shm.as_ptr().add(ring_header_offset)) as *mut DescRingHeader).unwrap()
    };

    // Create peer2's ring (consumer)
    let mut ring2 = unsafe { Ring::from_raw(peer2_ring_header, seg.ring_capacity) };
    let (_, mut consumer) = ring2.split();

    // Producer enqueues on peer1's mapping
    for i in 0..10 {
        let mut desc = MsgDescHot::default();
        desc.msg_id = i * 100;
        producer.try_enqueue(desc).unwrap();
    }

    // Consumer dequeues on peer2's mapping
    for i in 0..10 {
        let desc = consumer.try_dequeue().expect("Should dequeue from peer2");
        assert_eq!(desc.msg_id, i * 100);
    }
}

// ===== TEST 3: Data Segment Allocation =====

#[test]
fn test_data_segment_allocation() {
    // Create segment with data slots
    let seg = TestSegment::new(16, 8, 1024);
    let (meta_ptr, data_ptr) = seg.data_segment_pointers();

    // Create data segment
    let data_seg = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };

    // Initialize free list
    unsafe {
        data_seg.init_free_list();
    }

    // Allocate a slot
    let mut slot = data_seg.alloc().expect("Should allocate slot");
    let slot_idx = slot.slot();
    let slot_gen = slot.generation();

    // Write data to the slot
    let payload = b"Hello from rapace!";
    let buf = slot.as_mut_bytes();
    buf[..payload.len()].copy_from_slice(payload);

    // Commit the slot
    let committed = slot.commit(ByteLen::new(payload.len() as u32, seg.slot_size).unwrap());
    assert_eq!(committed.len(), payload.len() as u32);
    assert_eq!(committed.slot(), slot_idx);
    assert_eq!(committed.generation(), slot_gen);

    // Verify we can read the data back
    let read_data = data_seg.slot_data(slot_idx, 0, payload.len() as u32);
    assert_eq!(read_data, payload, "Should read back the same data");

    // Free the slot
    data_seg.free(slot_idx, slot_gen);

    // Should be able to allocate again
    let _slot2 = data_seg.alloc().expect("Should be able to allocate after free");
}

#[test]
fn test_data_segment_across_mappings() {
    // Create segment
    let seg = TestSegment::new(16, 4, 2048);
    let (meta_ptr, data_ptr) = seg.data_segment_pointers();

    // Peer 1's data segment
    let data_seg1 = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };
    unsafe { data_seg1.init_free_list(); }

    // Peer 2's mapping
    let peer2_shm = seg.duplicate();
    let ring_header_size = std::mem::size_of::<DescRingHeader>();
    let desc_size = std::mem::size_of::<MsgDescHot>();
    let ring_size = ring_header_size + (desc_size * seg.ring_capacity as usize);
    let total_rings = ring_size * 2;
    let slot_meta_size = std::mem::size_of::<SlotMeta>();
    let meta_size = slot_meta_size * seg.slot_count as usize;
    let align = |x: usize| (x + 63) & !63;
    let meta_offset = align(64 + total_rings);
    let data_offset = align(meta_offset + meta_size);

    let (meta_ptr2, data_ptr2) = unsafe {
        (
            NonNull::new(peer2_shm.as_ptr().add(meta_offset) as *mut SlotMeta).unwrap(),
            NonNull::new(peer2_shm.as_ptr().add(data_offset)).unwrap(),
        )
    };

    let data_seg2 = unsafe {
        DataSegment::from_raw(meta_ptr2, data_ptr2, seg.slot_size, seg.slot_count)
    };

    // Allocate and write from peer1
    let mut slot = data_seg1.alloc().unwrap();
    let slot_idx = slot.slot();
    let slot_gen = slot.generation();
    let payload = b"Cross-peer payload!";
    slot.as_mut_bytes()[..payload.len()].copy_from_slice(payload);
    let committed = slot.commit(ByteLen::new(payload.len() as u32, seg.slot_size).unwrap());

    // Read from peer2
    let read_data = data_seg2.slot_data(committed.slot(), 0, committed.len());
    assert_eq!(read_data, payload, "Peer 2 should see peer 1's data");

    // Peer 2 frees the slot
    data_seg2.free(slot_idx, slot_gen);
}

// ===== TEST 4: Doorbell Notifications =====

#[test]
fn test_doorbell_notifications() {
    // Create a doorbell
    let doorbell = Doorbell::new().unwrap();

    // Initially no notifications
    assert_eq!(doorbell.try_consume().unwrap(), 0, "Should have no notifications initially");

    // Ring the doorbell
    doorbell.ring().unwrap();

    // Should see notification
    let count = doorbell.try_consume().unwrap();
    assert!(count > 0, "Should have notification after ringing");

    // Should be empty again
    assert_eq!(doorbell.try_consume().unwrap(), 0, "Should be empty after consuming");
}

#[test]
fn test_doorbell_split_and_notify() {
    // Create and split doorbell
    let doorbell = Doorbell::new().unwrap();
    let (sender, receiver) = doorbell.split();

    // Initially nothing
    assert_eq!(receiver.try_consume().unwrap(), 0);

    // Sender rings
    sender.ring().unwrap();

    // Receiver sees it
    let count = receiver.try_consume().unwrap();
    assert!(count > 0, "Receiver should see notification from sender");

    // Ring multiple times
    for _ in 0..3 {
        sender.ring().unwrap();
    }

    // Should accumulate
    let count = receiver.try_consume().unwrap();
    assert!(count >= 1, "Should have accumulated notifications");
}

// ===== TEST 5: Full Message Flow =====

#[test]
fn test_full_message_flow() {
    // Set up a complete rapace segment
    let seg = TestSegment::new(32, 8, 4096);

    // Initialize both rings (A->B and B->A)
    let ring_a_to_b = seg.ring_a_to_b_header();
    let ring_b_to_a = seg.ring_b_to_a_header();
    seg.init_ring(ring_a_to_b);
    seg.init_ring(ring_b_to_a);

    // Initialize data segment
    let (meta_ptr, data_ptr) = seg.data_segment_pointers();
    let data_seg = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };
    unsafe { data_seg.init_free_list(); }

    // Create doorbell for A->B direction
    let doorbell = Doorbell::new().unwrap();
    let (sender, receiver) = doorbell.split();

    // Set up producer/consumer for A->B ring
    let mut ring = unsafe { Ring::from_raw(ring_a_to_b, seg.ring_capacity) };
    let (mut producer, mut consumer) = ring.split();

    // === SENDER SIDE (Peer A) ===

    // 1. Allocate payload slot
    let mut outbound_slot = data_seg.alloc().expect("Should allocate slot");
    let payload = b"This is a test message through the full rapace stack!";

    // 2. Write payload data
    let buf = outbound_slot.as_mut_bytes();
    buf[..payload.len()].copy_from_slice(payload);

    // 3. Commit the slot
    let committed = outbound_slot.commit(ByteLen::new(payload.len() as u32, seg.slot_size).unwrap());

    // 4. Build frame with FrameBuilder
    let channel = ChannelId::new(1).unwrap();
    let method = MethodId::new(42);
    let msg_id = MsgId::new(1001);

    let desc = FrameBuilder::data(channel, method, msg_id)
        .slot_payload(committed)
        .eos() // Mark as end-of-stream
        .build();

    // 5. Enqueue descriptor
    producer.try_enqueue(desc).expect("Should enqueue descriptor");

    // 6. Ring doorbell
    sender.ring().expect("Should ring doorbell");

    // === RECEIVER SIDE (Peer B) ===

    // 7. Wait for doorbell notification
    let notification = receiver.try_consume().expect("Should get notification");
    assert!(notification > 0, "Should have doorbell notification");

    // 8. Dequeue descriptor
    let received_desc = consumer.try_dequeue().expect("Should dequeue descriptor");

    // 9. Validate descriptor
    let raw = RawDescriptor::new(received_desc);
    let limits = DescriptorLimits::default();
    let valid = raw.validate(&data_seg, &limits).expect("Descriptor should be valid");

    // 10. Verify descriptor fields
    assert_eq!(valid.channel_id(), Some(channel), "Channel ID should match");
    assert_eq!(valid.method_id(), method, "Method ID should match");
    assert_eq!(valid.msg_id(), msg_id, "Message ID should match");
    assert!(valid.is_eos(), "EOS flag should be set");
    assert!(!valid.is_inline(), "Should not be inline payload");

    // 11. Read payload
    let (slot, _gen, offset, len) = valid.slot_info().expect("Should have slot info");
    let received_payload = data_seg.slot_data(slot, offset, len);
    assert_eq!(received_payload, payload, "Payload should match");

    // 12. Free slot (simulated by InboundPayload drop)
    let inbound = valid.into_inbound_payload(&data_seg);
    drop(inbound); // Frees the slot automatically

    // Verify slot is freed by allocating again
    let _new_slot = data_seg.alloc().expect("Slot should be available after free");
}

#[test]
fn test_full_message_flow_inline_payload() {
    // Test with inline payload (small message)
    let seg = TestSegment::new(32, 8, 4096);

    // Initialize ring
    let ring_header = seg.ring_a_to_b_header();
    seg.init_ring(ring_header);

    // Initialize data segment (needed for validation)
    let (meta_ptr, data_ptr) = seg.data_segment_pointers();
    let data_seg = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };
    unsafe { data_seg.init_free_list(); }

    // Set up ring
    let mut ring = unsafe { Ring::from_raw(ring_header, seg.ring_capacity) };
    let (mut producer, mut consumer) = ring.split();

    // Create doorbell
    let doorbell = Doorbell::new().unwrap();
    let (sender, receiver) = doorbell.split();

    // === SENDER SIDE ===

    // Small payload that fits inline (max 24 bytes)
    let payload = b"Short message";

    // Build frame with inline payload
    let channel = ChannelId::new(1).unwrap();
    let method = MethodId::new(100);
    let msg_id = MsgId::new(42);

    let desc = FrameBuilder::data(channel, method, msg_id)
        .inline_payload(payload)
        .expect("Payload should fit inline")
        .with_flags(FrameFlags::DATA)
        .build();

    // Enqueue and ring
    producer.try_enqueue(desc).unwrap();
    sender.ring().unwrap();

    // === RECEIVER SIDE ===

    // Wait for notification
    assert!(receiver.try_consume().unwrap() > 0);

    // Dequeue
    let received_desc = consumer.try_dequeue().unwrap();

    // Validate
    let raw = RawDescriptor::new(received_desc);
    let limits = DescriptorLimits::default();
    let valid = raw.validate(&data_seg, &limits).expect("Should validate inline payload");

    // Verify inline payload
    assert!(valid.is_inline(), "Should be inline payload");
    assert_eq!(valid.inline_payload(), payload, "Inline payload should match");
    assert_eq!(valid.channel_id(), Some(channel));
    assert_eq!(valid.method_id(), method);
    assert_eq!(valid.msg_id(), msg_id);

    // For inline payloads, InboundPayload doesn't free anything
    let inbound = valid.into_inbound_payload(&data_seg);
    assert!(inbound.is_inline(), "Should be recognized as inline");
    drop(inbound);
}

#[test]
fn test_bidirectional_communication() {
    // Test both A->B and B->A rings working together
    let seg = TestSegment::new(16, 4, 2048);

    // Initialize both rings
    let ring_a_to_b = seg.ring_a_to_b_header();
    let ring_b_to_a = seg.ring_b_to_a_header();
    seg.init_ring(ring_a_to_b);
    seg.init_ring(ring_b_to_a);

    // Create rings
    let mut ring_ab = unsafe { Ring::from_raw(ring_a_to_b, seg.ring_capacity) };
    let (mut prod_ab, mut cons_ab) = ring_ab.split();

    let mut ring_ba = unsafe { Ring::from_raw(ring_b_to_a, seg.ring_capacity) };
    let (mut prod_ba, mut cons_ba) = ring_ba.split();

    // A sends to B
    let mut desc_a_to_b = MsgDescHot::default();
    desc_a_to_b.msg_id = 100;
    desc_a_to_b.channel_id = 1;
    prod_ab.try_enqueue(desc_a_to_b).unwrap();

    // B receives from A
    let received_ab = cons_ab.try_dequeue().unwrap();
    assert_eq!(received_ab.msg_id, 100);

    // B sends response to A
    let mut desc_b_to_a = MsgDescHot::default();
    desc_b_to_a.msg_id = 200;
    desc_b_to_a.channel_id = 1;
    prod_ba.try_enqueue(desc_b_to_a).unwrap();

    // A receives response from B
    let received_ba = cons_ba.try_dequeue().unwrap();
    assert_eq!(received_ba.msg_id, 200);
}

#[test]
fn test_multiple_messages_flow() {
    // Test sending multiple messages in sequence
    let seg = TestSegment::new(64, 16, 4096);

    let ring_header = seg.ring_a_to_b_header();
    seg.init_ring(ring_header);

    let (meta_ptr, data_ptr) = seg.data_segment_pointers();
    let data_seg = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };
    unsafe { data_seg.init_free_list(); }

    let mut ring = unsafe { Ring::from_raw(ring_header, seg.ring_capacity) };
    let (mut producer, mut consumer) = ring.split();

    let doorbell = Doorbell::new().unwrap();
    let (sender, receiver) = doorbell.split();

    // Send 10 messages
    let num_messages = 10;
    for i in 0..num_messages {
        // Allocate slot
        let mut slot = data_seg.alloc().expect("Should allocate");
        let msg = format!("Message number {}", i);
        let bytes = msg.as_bytes();

        // Write payload
        slot.as_mut_bytes()[..bytes.len()].copy_from_slice(bytes);
        let committed = slot.commit(ByteLen::new(bytes.len() as u32, seg.slot_size).unwrap());

        // Build and enqueue
        let desc = FrameBuilder::data(
            ChannelId::new(1).unwrap(),
            MethodId::new(1000),
            MsgId::new(i as u64),
        )
        .slot_payload(committed)
        .build();

        producer.try_enqueue(desc).unwrap();
    }

    // Ring once for all messages
    sender.ring().unwrap();

    // Receive notification
    assert!(receiver.try_consume().unwrap() > 0);

    // Receive and validate all messages
    for i in 0..num_messages {
        let desc = consumer.try_dequeue().expect("Should dequeue");
        let raw = RawDescriptor::new(desc);
        let valid = raw.validate(&data_seg, &DescriptorLimits::default()).unwrap();

        assert_eq!(valid.msg_id(), MsgId::new(i as u64));

        // Read and verify payload
        let (slot, gen, offset, len) = valid.slot_info().unwrap();
        let payload = data_seg.slot_data(slot, offset, len);
        let expected = format!("Message number {}", i);
        assert_eq!(payload, expected.as_bytes());

        // Free slot
        data_seg.free(slot, gen);
    }

    // All messages processed
    assert!(consumer.is_empty());
}

#[test]
fn test_ring_wraparound_with_payload() {
    // Test that ring properly wraps around with actual payload transfers
    let seg = TestSegment::new(4, 2, 1024); // Small ring and slot count

    let ring_header = seg.ring_a_to_b_header();
    seg.init_ring(ring_header);

    let (meta_ptr, data_ptr) = seg.data_segment_pointers();
    let data_seg = unsafe {
        DataSegment::from_raw(meta_ptr, data_ptr, seg.slot_size, seg.slot_count)
    };
    unsafe { data_seg.init_free_list(); }

    let mut ring = unsafe { Ring::from_raw(ring_header, seg.ring_capacity) };
    let (mut producer, mut consumer) = ring.split();

    // Send multiple rounds to test wraparound
    for round in 0..3 {
        for i in 0..2 {
            let mut slot = data_seg.alloc().unwrap();
            let msg = format!("Round {} Message {}", round, i);
            slot.as_mut_bytes()[..msg.len()].copy_from_slice(msg.as_bytes());
            let committed = slot.commit(ByteLen::new(msg.len() as u32, seg.slot_size).unwrap());

            let desc = FrameBuilder::data(
                ChannelId::new(1).unwrap(),
                MethodId::new(1),
                MsgId::new((round * 10 + i) as u64),
            )
            .slot_payload(committed)
            .build();

            producer.try_enqueue(desc).unwrap();
        }

        // Consume this round
        for i in 0..2 {
            let desc = consumer.try_dequeue().unwrap();
            let raw = RawDescriptor::new(desc);
            let valid = raw.validate(&data_seg, &DescriptorLimits::default()).unwrap();

            let expected_msg_id = (round * 10 + i) as u64;
            assert_eq!(valid.msg_id(), MsgId::new(expected_msg_id));

            // Free the slot for reuse
            let (slot, gen, _, _) = valid.slot_info().unwrap();
            data_seg.free(slot, gen);
        }
    }
}
