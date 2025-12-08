//! Echo client example for rapace.
//!
//! This example demonstrates how to create a client that:
//! - Opens an existing shared memory segment (PeerB role)
//! - Sends messages to the server
//! - Receives echo responses
//!
//! Run echo_server first, then run this client.
//!
//! NOTE: In this simplified example, the client connects to the same process's
//! shared memory via file descriptor. In a real application, you would pass the
//! FD via Unix domain socket or similar IPC mechanism.

use std::env;
use std::os::fd::{FromRawFd, OwnedFd};
use std::ptr::NonNull;
use std::time::Duration;

use rapace::alloc::DataSegment;
use rapace::frame::{FrameBuilder, RawDescriptor, DescriptorLimits};
use rapace::layout::{DescRingHeader, MsgDescHot, SegmentHeader, SlotMeta};
use rapace::ring::Ring;
use rapace::session::{PeerB, Session, SessionConfig};
use rapace::shm::{calculate_segment_size, SharedMemory};
use rapace::types::{ByteLen, ChannelId, MethodId, MsgId};

/// Client configuration (must match server)
const RING_CAPACITY: u32 = 64;
const SLOT_COUNT: u32 = 16;
const SLOT_SIZE: u32 = 4096;

/// Echo service method ID
const ECHO_METHOD: u32 = 1000;

/// Echo channel ID
const ECHO_CHANNEL: u32 = 1;

fn main() {
    println!("=== Rapace Echo Client ===");

    // In a real application, you would receive the FD via socket
    // For this demo, we'll use a command-line argument or create our own segment
    let shm = if let Some(fd_str) = env::args().nth(1) {
        println!("Connecting to shared memory FD: {}", fd_str);
        let fd: i32 = fd_str.parse().expect("Invalid FD number");
        let segment_size = calculate_segment_size(RING_CAPACITY, SLOT_COUNT, SLOT_SIZE);

        unsafe {
            let owned_fd = OwnedFd::from_raw_fd(fd);
            SharedMemory::from_fd(owned_fd, segment_size)
                .expect("Failed to open shared memory")
        }
    } else {
        println!("No FD provided. Creating shared segment for single-process demo...");
        println!("(In production, client would receive FD from server via socket)");

        // For demo purposes, create and initialize our own segment
        let segment_size = calculate_segment_size(RING_CAPACITY, SLOT_COUNT, SLOT_SIZE);
        let mut shm = SharedMemory::create("echo-client-demo", segment_size)
            .expect("Failed to create shared memory");

        unsafe {
            initialize_segment(&mut shm);
        }

        shm
    };

    println!("Connected to shared memory segment");

    // Create session as PeerB
    let mut session = unsafe {
        create_peer_b_session(&shm)
    };

    println!("Session created. Sending messages...");

    // Message counter
    let mut msg_id_counter = 1u64;
    let channel = ChannelId::new(ECHO_CHANNEL).expect("Invalid channel ID");

    // Send some test messages
    let test_messages = [
        "Hello, server!",
        "How are you?",
        "This is a test message",
        "Echo echo echo...",
        "Rapace shared memory RPC",
        "Short",
        "A much longer message that won't fit in the inline payload buffer, so it will need to use a slot instead",
        "Final message",
    ];

    for (i, msg) in test_messages.iter().enumerate() {
        // Update our heartbeat
        session.heartbeat();

        // Check if server is alive
        if !session.is_peer_alive() {
            eprintln!("Server appears to be dead. Exiting...");
            break;
        }

        println!("\n[Client] Sending message {}: \"{}\"", i + 1, msg);
        send_message(&mut session, channel, msg.as_bytes(), &mut msg_id_counter);

        // Wait for echo response
        let mut retries = 0;
        let max_retries = 100; // 10 seconds total (100ms * 100)

        loop {
            session.heartbeat();

            let consumer = session.inbound_consumer();
            if let Some(desc) = consumer.try_dequeue() {
                // Validate and process the response
                let raw_desc = RawDescriptor::new(desc);
                let limits = DescriptorLimits::default();

                match raw_desc.validate(session.inbound_segment(), &limits) {
                    Ok(valid_desc) => {
                        // Read the echo response
                        let payload_data = if valid_desc.is_inline() {
                            valid_desc.inline_payload().to_vec()
                        } else {
                            let (slot_idx, _gen, offset, len) = valid_desc.slot_info().unwrap();
                            session.inbound_segment()
                                .slot_data(slot_idx, offset, len)
                                .to_vec()
                        };

                        // Free the slot
                        let _inbound = valid_desc.into_inbound_payload(session.inbound_segment());

                        if let Ok(text) = std::str::from_utf8(&payload_data) {
                            println!("[Client] Received echo: \"{}\"", text);
                        } else {
                            println!("[Client] Received echo: {} bytes", payload_data.len());
                        }

                        // Verify the echo matches what we sent
                        if payload_data == msg.as_bytes() {
                            println!("[Client] ✓ Echo verified!");
                        } else {
                            println!("[Client] ✗ Echo mismatch!");
                        }

                        break;
                    }
                    Err(e) => {
                        eprintln!("[Client] Invalid descriptor: {:?}", e);
                        break;
                    }
                }
            }

            retries += 1;
            if retries >= max_retries {
                eprintln!("[Client] Timeout waiting for echo response");
                break;
            }

            std::thread::sleep(Duration::from_millis(100));
        }

        // Small delay between messages
        std::thread::sleep(Duration::from_millis(200));
    }

    println!("\n[Client] All messages sent. Exiting...");
}

/// Send a message to the server
fn send_message(
    session: &mut Session<PeerB>,
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
                println!("[Client] Sent (inline): {} bytes", payload.len());
            }
            Err(_) => {
                eprintln!("[Client] Failed to send: ring full");
            }
        }
    } else {
        // Use slot-based payload for larger messages
        let mut slot = match session.outbound_segment().alloc() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[Client] Failed to allocate slot");
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
                println!("[Client] Sent (slot): {} bytes", payload.len());
            }
            Err(_) => {
                eprintln!("[Client] Failed to send: ring full");
            }
        }
    }
}

/// Initialize segment layout (for single-process demo only).
/// In production, only the server (PeerA) would initialize the segment.
unsafe fn initialize_segment(shm: &mut SharedMemory) {
    use rapace::layout::MAGIC;

    let base = shm.as_ptr();
    let mut offset = 0usize;

    let align = |off: usize| (off + 63) & !63;

    // 1. Segment header
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

    // 2. A→B ring
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

    // 3. B→A ring
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

    // 4. Slot metadata
    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    for i in 0..SLOT_COUNT {
        std::ptr::write(slot_meta_base.add(i as usize), SlotMeta {
            generation: std::sync::atomic::AtomicU32::new(0),
            state: std::sync::atomic::AtomicU32::new(0),
        });
    }
}

/// Create a PeerB session from the shared memory segment.
unsafe fn create_peer_b_session(shm: &SharedMemory) -> Session<PeerB> {
    let base = shm.as_ptr();
    let mut offset = 0usize;

    let align = |off: usize| (off + 63) & !63;

    // Get pointers to all components
    let header = base as *const SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    // A→B ring (our inbound - server sends to us)
    let a_to_b_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    // B→A ring (our outbound - we send to server)
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
    // For PeerB: A→B is inbound, B→A is outbound
    let (_inbound_producer, inbound_consumer) = a_to_b_ring.split();
    let (outbound_producer, _outbound_consumer) = b_to_a_ring.split();

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
