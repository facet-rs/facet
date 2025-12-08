//! Echo server example for rapace.
//!
//! This example demonstrates how to create a server that:
//! - Creates a shared memory segment (PeerA role)
//! - Initializes rings and data segments
//! - Listens for messages from a client
//! - Echoes back received messages
//!
//! Run this before running the echo_client example.

use std::ptr::NonNull;
use std::time::Duration;

use rapace::alloc::DataSegment;
use rapace::frame::{FrameBuilder, RawDescriptor, DescriptorLimits, ValidDescriptor};
use rapace::layout::{DescRingHeader, MsgDescHot, SegmentHeader, SlotMeta, MAGIC};
use rapace::ring::Ring;
use rapace::session::{PeerA, Session, SessionConfig};
use rapace::shm::{calculate_segment_size, SharedMemory};
use rapace::types::{ByteLen, ChannelId, MethodId, MsgId};

/// Server configuration
const RING_CAPACITY: u32 = 64;  // Must be power of 2
const SLOT_COUNT: u32 = 16;
const SLOT_SIZE: u32 = 4096;

/// Simple echo method ID
const ECHO_METHOD: u32 = 1000;

fn main() {
    println!("=== Rapace Echo Server ===");
    println!("Creating shared memory segment...");

    // Calculate required size for the segment
    let segment_size = calculate_segment_size(RING_CAPACITY, SLOT_COUNT, SLOT_SIZE);
    println!("Segment size: {} bytes", segment_size);

    // Create shared memory segment (PeerA creates it)
    let mut shm = SharedMemory::create("echo-server", segment_size)
        .expect("Failed to create shared memory");

    // Initialize the segment layout
    unsafe {
        initialize_segment(&mut shm);
    }

    // Get file descriptor for passing to client (in real app, would pass via socket)
    let fd = shm.as_raw_fd();
    println!("Shared memory FD: {} (client should open this)", fd);
    println!("\nWaiting for client to connect...");

    // Create session as PeerA
    let mut session = unsafe {
        create_peer_a_session(&shm)
    };

    println!("Session created. Starting echo loop...");

    // Message counter
    let mut msg_id_counter = 1u64;
    let mut messages_echoed = 0usize;

    // Main echo loop
    loop {
        // Update our heartbeat so client knows we're alive
        session.heartbeat();

        // Check if client is alive
        if !session.is_peer_alive() {
            println!("Client appears to be dead. Waiting for reconnect...");
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }

        // Try to receive messages from client
        loop {
            let desc = match session.inbound_consumer().try_dequeue() {
                Some(d) => d,
                None => break,
            };

            // Validate the descriptor
            let raw_desc = RawDescriptor::new(desc);
            let limits = DescriptorLimits::default();

            let valid_desc = match raw_desc.validate(session.inbound_segment(), &limits) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Invalid descriptor: {:?}", e);
                    continue;
                }
            };

            // Process the message
            handle_message(&mut session, valid_desc, &mut msg_id_counter, &mut messages_echoed);
        }

        // Small delay to avoid busy-waiting
        std::thread::sleep(Duration::from_micros(100));

        // Exit after 100 messages for demo purposes
        if messages_echoed >= 100 {
            println!("\nProcessed 100 messages. Shutting down...");
            break;
        }
    }

    println!("Echo server terminated.");
}

/// Handle an incoming message and echo it back
fn handle_message(
    session: &mut Session<PeerA>,
    desc: ValidDescriptor,
    msg_id_counter: &mut u64,
    messages_echoed: &mut usize,
) {
    // Extract message info
    let msg_id = desc.msg_id();
    let method_id = desc.method_id();
    let channel_id = desc.channel_id();

    // Read the payload and prepare to free slot
    let payload_data = if desc.is_inline() {
        let data = desc.inline_payload().to_vec();
        // Free inline payload (no-op since there's no slot)
        let _inbound = desc.into_inbound_payload(session.inbound_segment());
        data
    } else {
        // For slot-based payload, we need to copy before the slot is freed
        let (slot_idx, _gen, offset, len) = desc.slot_info().unwrap();
        let data = session.inbound_segment()
            .slot_data(slot_idx, offset, len)
            .to_vec();

        // Now free the slot
        let _inbound = desc.into_inbound_payload(session.inbound_segment());
        data
    };

    // Print what we received
    if let Ok(text) = std::str::from_utf8(&payload_data) {
        println!("[Server] Received msg #{}: \"{}\" on channel {:?}, method {}",
                 msg_id.get(), text, channel_id, method_id.get());
    } else {
        println!("[Server] Received msg #{}: {} bytes on channel {:?}, method {}",
                 msg_id.get(), payload_data.len(), channel_id, method_id.get());
    }

    // Echo the message back
    if let Some(channel) = channel_id {
        send_echo(session, channel, &payload_data, msg_id_counter);
        *messages_echoed += 1;
    } else {
        println!("[Server] Ignoring control channel message");
    }
}

/// Send an echo response back to the client
fn send_echo(
    session: &mut Session<PeerA>,
    channel: ChannelId,
    payload: &[u8],
    msg_id_counter: &mut u64,
) {
    let msg_id = MsgId::new(*msg_id_counter);
    *msg_id_counter += 1;

    // Try inline first if payload is small
    if payload.len() <= 24 {
        // Use inline payload
        let frame = FrameBuilder::data(channel, MethodId::new(ECHO_METHOD), msg_id)
            .inline_payload(payload)
            .expect("Payload too large for inline")
            .build();

        // Enqueue the frame
        match session.outbound_producer().try_enqueue(frame) {
            Ok(()) => {
                if let Ok(text) = std::str::from_utf8(payload) {
                    println!("[Server] Echoed (inline): \"{}\"", text);
                }
            }
            Err(_) => {
                eprintln!("[Server] Failed to send echo: ring full");
            }
        }
    } else {
        // Use slot-based payload for larger messages
        let mut slot = match session.outbound_segment().alloc() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[Server] Failed to allocate slot for echo");
                return;
            }
        };

        // Copy payload to slot
        let buf = slot.as_mut_bytes();
        buf[..payload.len()].copy_from_slice(payload);

        // Commit the slot
        let byte_len = ByteLen::new(payload.len() as u32, SLOT_SIZE)
            .expect("Payload too large");
        let committed = slot.commit(byte_len);

        // Build frame with slot reference
        let frame = FrameBuilder::data(channel, MethodId::new(ECHO_METHOD), msg_id)
            .slot_payload(committed)
            .build();

        // Enqueue the frame
        match session.outbound_producer().try_enqueue(frame) {
            Ok(()) => {
                println!("[Server] Echoed (slot): {} bytes", payload.len());
            }
            Err(_) => {
                eprintln!("[Server] Failed to send echo: ring full");
            }
        }
    }
}

/// Initialize the shared memory segment layout.
///
/// Layout:
/// - SegmentHeader (64 bytes, aligned)
/// - A→B Ring (header + descriptors)
/// - B→A Ring (header + descriptors)
/// - Slot metadata array
/// - Slot data array
unsafe fn initialize_segment(shm: &mut SharedMemory) {
    let base = shm.as_ptr();
    let mut offset = 0usize;

    // Helper to align offset
    let align = |off: usize| (off + 63) & !63;

    // 1. Initialize segment header
    let header = base.add(offset) as *mut SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    std::ptr::write(header, SegmentHeader {
        magic: MAGIC,
        version: 1,
        flags: 0,
        peer_a_epoch: std::sync::atomic::AtomicU64::new(0),
        peer_b_epoch: std::sync::atomic::AtomicU64::new(0),
        peer_a_last_seen: std::sync::atomic::AtomicU64::new(0),
        peer_b_last_seen: std::sync::atomic::AtomicU64::new(0),
    });

    // 2. Initialize A→B ring (server sends to client)
    let a_to_b_ring = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    std::ptr::write(a_to_b_ring, DescRingHeader {
        visible_head: std::sync::atomic::AtomicU64::new(0),
        _pad1: [0; 56],
        tail: std::sync::atomic::AtomicU64::new(0),
        _pad2: [0; 56],
        capacity: RING_CAPACITY,
        _pad3: [0; 60],
    });

    // 3. Initialize B→A ring (client sends to server)
    let b_to_a_ring = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    std::ptr::write(b_to_a_ring, DescRingHeader {
        visible_head: std::sync::atomic::AtomicU64::new(0),
        _pad1: [0; 56],
        tail: std::sync::atomic::AtomicU64::new(0),
        _pad2: [0; 56],
        capacity: RING_CAPACITY,
        _pad3: [0; 60],
    });

    // 4. Initialize slot metadata
    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    for i in 0..SLOT_COUNT {
        std::ptr::write(slot_meta_base.add(i as usize), SlotMeta {
            generation: std::sync::atomic::AtomicU32::new(0),
            state: std::sync::atomic::AtomicU32::new(0), // Free
        });
    }

    // 5. Slot data region (just zero it)
    let _slot_data_base = base.add(offset);
    // Already zeroed by SharedMemory creation

    println!("Segment initialized successfully");
}

/// Create a PeerA session from the initialized shared memory.
unsafe fn create_peer_a_session(shm: &SharedMemory) -> Session<PeerA> {
    let base = shm.as_ptr();
    let mut offset = 0usize;

    let align = |off: usize| (off + 63) & !63;

    // Get pointers to all components
    let header = base as *const SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    // A→B ring (our outbound)
    let a_to_b_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    // B→A ring (our inbound)
    let b_to_a_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    // Slot metadata
    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    // Slot data
    let slot_data_base = base.add(offset);

    // Create rings and leak them to get 'static lifetime
    // (In a real application, these would be properly managed with the Session lifetime)
    let a_to_b_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(a_to_b_ring_header).unwrap(),
        RING_CAPACITY,
    )));
    let b_to_a_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(b_to_a_ring_header).unwrap(),
        RING_CAPACITY,
    )));

    // Split rings into producer/consumer
    let (outbound_producer, _outbound_consumer) = a_to_b_ring.split();
    let (_inbound_producer, inbound_consumer) = b_to_a_ring.split();

    // Create data segments
    let outbound_segment = DataSegment::from_raw(
        NonNull::new(slot_meta_base).unwrap(),
        NonNull::new(slot_data_base).unwrap(),
        SLOT_SIZE,
        SLOT_COUNT,
    );
    outbound_segment.init_free_list();

    let inbound_segment = DataSegment::from_raw(
        NonNull::new(slot_meta_base).unwrap(),
        NonNull::new(slot_data_base).unwrap(),
        SLOT_SIZE,
        SLOT_COUNT,
    );
    // Note: Same free list, shared between peers

    // Create session
    let session = Session::new(
        header,
        outbound_producer,
        inbound_consumer,
        outbound_segment,
        inbound_segment,
        SessionConfig::default(),
    );

    // Initialize our heartbeat
    session.heartbeat();

    session
}
