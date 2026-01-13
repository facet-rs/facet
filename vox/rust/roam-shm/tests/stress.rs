//! Stress tests for SHM implementation.
//!
//! These tests exercise the implementation under load to catch
//! race conditions, memory corruption, and edge cases.
//!
//! shm[verify shm.host.poll-peers]
//! shm[verify shm.topology.max-guests]

use roam_frame::{Frame, MsgDesc, Payload};
use roam_shm::guest::ShmGuest;
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use roam_shm::msg_type;

fn make_data_frame(seq: u32, payload: Vec<u8>) -> Frame {
    let desc = MsgDesc::new(msg_type::DATA, seq, 0);
    Frame {
        desc,
        payload: Payload::Owned(payload),
    }
}

#[test]
fn stress_single_guest_high_throughput() {
    let config = SegmentConfig {
        ring_size: 256,
        slots_per_guest: 32,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    const TOTAL_MESSAGES: u32 = 10_000;
    const BATCH_SIZE: u32 = 100;

    let mut sent = 0u32;
    let mut received = 0u32;

    while received < TOTAL_MESSAGES {
        // Send a batch (as many as the ring allows)
        while sent < TOTAL_MESSAGES && sent - received < BATCH_SIZE {
            let payload = vec![(sent % 256) as u8; 16];
            match guest.send(make_data_frame(sent, payload)) {
                Ok(()) => sent += 1,
                Err(roam_shm::guest::SendError::RingFull) => break,
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        // Receive what's available
        let messages = host.poll();
        for (_, frame) in messages {
            assert_eq!(frame.desc.id, received);
            received += 1;
        }
    }

    assert_eq!(sent, TOTAL_MESSAGES);
    assert_eq!(received, TOTAL_MESSAGES);
}

#[test]
fn stress_multiple_guests_interleaved() {
    let config = SegmentConfig {
        max_guests: 8,
        ring_size: 64,
        slots_per_guest: 16,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    const NUM_GUESTS: usize = 4;
    const MESSAGES_PER_GUEST: u32 = 500;

    let mut guests: Vec<_> = (0..NUM_GUESTS)
        .map(|_| ShmGuest::attach(region).unwrap())
        .collect();

    let mut sent_per_guest = [0u32; NUM_GUESTS];
    let mut received_per_guest = [0u32; NUM_GUESTS];

    let total_expected = NUM_GUESTS as u32 * MESSAGES_PER_GUEST;
    let mut total_received = 0u32;

    while total_received < total_expected {
        // Each guest tries to send
        for (i, guest) in guests.iter_mut().enumerate() {
            if sent_per_guest[i] < MESSAGES_PER_GUEST {
                let seq = sent_per_guest[i];
                let payload = vec![i as u8; 8];
                match guest.send(make_data_frame(seq, payload)) {
                    Ok(()) => sent_per_guest[i] += 1,
                    Err(roam_shm::guest::SendError::RingFull) => {}
                    Err(e) => panic!("Guest {} error: {:?}", i, e),
                }
            }
        }

        // Host receives from all
        let messages = host.poll();
        for (peer_id, frame) in messages {
            let guest_idx = peer_id.index() as usize;
            assert_eq!(frame.desc.id, received_per_guest[guest_idx]);
            received_per_guest[guest_idx] += 1;
            total_received += 1;
        }
    }

    // Verify all guests sent/received correctly
    for i in 0..NUM_GUESTS {
        assert_eq!(sent_per_guest[i], MESSAGES_PER_GUEST);
        assert_eq!(received_per_guest[i], MESSAGES_PER_GUEST);
    }
}

#[test]
fn stress_bidirectional_ping_pong() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    const ITERATIONS: u32 = 1000;

    for i in 0..ITERATIONS {
        // Guest sends ping
        let ping = make_data_frame(i, vec![0xAA; 4]);
        guest.send(ping).unwrap();

        // Host receives and sends pong
        let messages = host.poll();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1.desc.id, i);

        let pong = make_data_frame(i, vec![0xBB; 4]);
        host.send(peer_id, pong).unwrap();

        // Guest receives pong
        let frame = guest.recv().unwrap();
        assert_eq!(frame.desc.id, i);
    }
}

#[test]
fn stress_varying_payload_sizes() {
    let config = SegmentConfig {
        slot_size: 4096,
        max_payload_size: 4092,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Test various payload sizes including edge cases
    let sizes = [
        0, 1, 31, 32, 33, 63, 64, 65, 100, 255, 256, 257, 512, 1000, 1024, 2048, 3000,
    ];

    for (i, &size) in sizes.iter().enumerate() {
        let payload: Vec<u8> = (0..size).map(|j| ((i + j) % 256) as u8).collect();
        let frame = make_data_frame(i as u32, payload.clone());
        guest.send(frame).unwrap();

        let messages = host.poll();
        assert_eq!(messages.len(), 1, "Failed at size {}", size);

        let (_, recv_frame) = &messages[0];
        assert_eq!(recv_frame.desc.payload_len as usize, size);

        // Verify payload content
        let recv_payload = match &recv_frame.payload {
            Payload::Inline => &recv_frame.desc.inline_payload[..size],
            Payload::Owned(data) => data.as_slice(),
            Payload::Bytes(data) => data.as_ref(),
        };
        assert_eq!(
            recv_payload,
            payload.as_slice(),
            "Payload mismatch at size {}",
            size
        );
    }
}

/// shm[verify shm.slot.exhaustion]
#[test]
fn stress_slot_exhaustion_recovery() {
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots
        ring_size: 256,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    const TOTAL_MESSAGES: u32 = 100;
    let large_payload = vec![0u8; 100]; // Larger than inline, requires slot

    let mut sent = 0u32;
    let mut received = 0u32;

    while received < TOTAL_MESSAGES {
        // Try to send (may fail due to slot exhaustion)
        if sent < TOTAL_MESSAGES {
            match guest.send(make_data_frame(sent, large_payload.clone())) {
                Ok(()) => sent += 1,
                Err(roam_shm::guest::SendError::SlotExhausted) => {
                    // Expected when slots are full - need to drain first
                }
                Err(roam_shm::guest::SendError::RingFull) => {
                    // Also possible
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        // Receive to free up slots
        let messages = host.poll();
        received += messages.len() as u32;
    }

    assert_eq!(received, TOTAL_MESSAGES);
}

/// shm[verify shm.guest.detach]
/// shm[verify shm.crash.recovery]
#[test]
fn stress_guest_attach_detach_cycle() {
    let config = SegmentConfig {
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    const CYCLES: usize = 10;

    for cycle in 0..CYCLES {
        // Attach guests
        let mut guests: Vec<_> = (0..4).map(|_| ShmGuest::attach(region).unwrap()).collect();

        // Each sends a message
        for (i, guest) in guests.iter_mut().enumerate() {
            let frame = make_data_frame((cycle * 100 + i) as u32, vec![i as u8; 4]);
            guest.send(frame).unwrap();
        }

        // Host receives all
        let messages = host.poll();
        assert_eq!(messages.len(), 4, "Cycle {} failed", cycle);

        // Guests detach (drop)
        drop(guests);

        // Poll to process goodbye states
        let _ = host.poll();
    }
}

#[test]
fn stress_concurrent_send_recv() {
    // Skip under Miri - this test uses threads which Miri doesn't handle well
    if cfg!(miri) {
        return;
    }

    let config = SegmentConfig {
        ring_size: 256,
        ..SegmentConfig::default()
    };

    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    const ITERATIONS: u32 = 500;

    // Since we can't easily share mutable host/guest across threads,
    // we'll do a coordinated dance in a single thread with simulated concurrency

    let mut guest_sent = 0u32;
    let mut host_received = 0u32;
    let mut host_sent = 0u32;
    let mut guest_received = 0u32;

    while guest_received < ITERATIONS {
        // Guest sends if possible
        if guest_sent < ITERATIONS
            && guest
                .send(make_data_frame(guest_sent, vec![0u8; 8]))
                .is_ok()
        {
            guest_sent += 1;
        }

        // Host polls and responds
        let messages = host.poll();
        for (pid, frame) in messages {
            assert_eq!(pid, peer_id);
            host_received += 1;

            // Send response
            if host
                .send(peer_id, make_data_frame(frame.desc.id, vec![1u8; 8]))
                .is_ok()
            {
                host_sent += 1;
            }
        }

        // Guest receives
        while let Some(frame) = guest.recv() {
            assert_eq!(frame.desc.id, guest_received);
            guest_received += 1;
        }
    }

    assert_eq!(guest_sent, ITERATIONS);
    assert_eq!(host_received, ITERATIONS);
    assert_eq!(host_sent, ITERATIONS);
    assert_eq!(guest_received, ITERATIONS);
}

#[test]
fn stress_max_guests() {
    let config = SegmentConfig {
        max_guests: 255, // Maximum allowed
        ring_size: 16,
        slots_per_guest: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Attach maximum guests
    let mut guests: Vec<_> = (0..255)
        .map(|_| ShmGuest::attach(region).unwrap())
        .collect();

    // Verify all got unique IDs
    let ids: std::collections::HashSet<_> = guests.iter().map(|g| g.peer_id().get()).collect();
    assert_eq!(ids.len(), 255);

    // Each sends one message
    for (i, guest) in guests.iter_mut().enumerate() {
        guest
            .send(make_data_frame(i as u32, vec![i as u8; 4]))
            .unwrap();
    }

    // Host receives all
    let messages = host.poll();
    assert_eq!(messages.len(), 255);

    // Trying to attach one more should fail
    let result = ShmGuest::attach(region);
    assert!(result.is_err());
}
