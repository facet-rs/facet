//! Integration tests for SHM host-guest message roundtrip.
//!
//! shm[verify shm.topology.hub]
//! shm[verify shm.topology.hub.calls]

use roam_frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
use roam_shm::guest::ShmGuest;
use roam_shm::host::{PollResult, ShmHost};
use roam_shm::layout::SegmentConfig;
use roam_shm::msg_type;

/// Create a simple request frame with inline payload.
fn make_request(id: u32, payload: &[u8]) -> Frame {
    let mut desc = MsgDesc::new(msg_type::REQUEST, id, 0);
    if payload.len() <= INLINE_PAYLOAD_LEN {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
    }
    Frame {
        desc,
        payload: Payload::Inline,
    }
}

/// Create a response frame.
fn make_response(id: u32, payload: &[u8]) -> Frame {
    let mut desc = MsgDesc::new(msg_type::RESPONSE, id, 0);
    if payload.len() <= INLINE_PAYLOAD_LEN {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
    }
    Frame {
        desc,
        payload: Payload::Inline,
    }
}

/// shm[verify shm.guest.attach]
/// shm[verify shm.ordering.ring-publish]
/// shm[verify shm.ordering.ring-consume]
#[test]
fn guest_to_host_inline_message() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Guest sends a request to host
    let request = make_request(1, b"hello host");
    guest.send(request).unwrap();

    // Host polls and receives the message
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);

    let (recv_peer_id, frame) = &messages[0];
    assert_eq!(*recv_peer_id, peer_id);
    assert_eq!(frame.desc.msg_type, msg_type::REQUEST);
    assert_eq!(frame.desc.id, 1);
    assert_eq!(&frame.desc.inline_payload[..10], b"hello host");
}

/// shm[verify shm.ordering.ring-publish]
/// shm[verify shm.ordering.ring-consume]
#[test]
fn host_to_guest_inline_message() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Host sends a response to guest
    let response = make_response(42, b"hello guest");
    host.send(peer_id, response).unwrap();

    // Guest receives the message
    let frame = guest.recv().unwrap();
    assert_eq!(frame.desc.msg_type, msg_type::RESPONSE);
    assert_eq!(frame.desc.id, 42);
    assert_eq!(&frame.desc.inline_payload[..11], b"hello guest");
}

#[test]
fn bidirectional_roundtrip() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Guest sends request
    let request = make_request(100, b"ping");
    guest.send(request).unwrap();

    // Host receives and responds
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].1.desc.id, 100);

    let response = make_response(100, b"pong");
    host.send(peer_id, response).unwrap();

    // Guest receives response
    let frame = guest.recv().unwrap();
    assert_eq!(frame.desc.msg_type, msg_type::RESPONSE);
    assert_eq!(frame.desc.id, 100);
    assert_eq!(&frame.desc.inline_payload[..4], b"pong");
}

/// shm[verify shm.topology.peer-id]
/// shm[verify shm.segment.peer-table]
#[test]
fn multiple_guests_isolated() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest1 = ShmGuest::attach(region).unwrap();
    let mut guest2 = ShmGuest::attach(region).unwrap();
    let peer1 = guest1.peer_id();
    let peer2 = guest2.peer_id();

    assert_ne!(peer1, peer2);

    // Each guest sends a message
    guest1.send(make_request(1, b"from guest1")).unwrap();
    guest2.send(make_request(2, b"from guest2")).unwrap();

    // Host receives both
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 2);

    // Verify messages came from correct guests
    let msg1 = messages.iter().find(|(p, _)| *p == peer1).unwrap();
    let msg2 = messages.iter().find(|(p, _)| *p == peer2).unwrap();
    assert_eq!(msg1.1.desc.id, 1);
    assert_eq!(msg2.1.desc.id, 2);

    // Host sends different responses to each guest
    host.send(peer1, make_response(1, b"reply to g1")).unwrap();
    host.send(peer2, make_response(2, b"reply to g2")).unwrap();

    // Each guest receives only their response
    let frame1 = guest1.recv().unwrap();
    let frame2 = guest2.recv().unwrap();

    assert_eq!(frame1.desc.id, 1);
    assert_eq!(frame2.desc.id, 2);

    // No cross-talk
    assert!(guest1.recv().is_none());
    assert!(guest2.recv().is_none());
}

/// shm[verify shm.payload.slot]
/// shm[verify shm.slot.allocate]
#[test]
fn large_payload_via_slot() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Create a payload larger than inline capacity (32 bytes)
    let large_payload: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();

    let desc = MsgDesc::new(msg_type::DATA, 999, 0);

    let frame = Frame {
        desc,
        payload: Payload::Owned(large_payload.clone()),
    };

    // Guest sends large message
    guest.send(frame).unwrap();

    // Host receives it
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);

    let (recv_peer_id, frame) = &messages[0];
    assert_eq!(*recv_peer_id, peer_id);
    assert_eq!(frame.desc.msg_type, msg_type::DATA);
    assert_eq!(frame.desc.payload_len, 1000);

    // Verify payload content
    match &frame.payload {
        Payload::Owned(data) => {
            assert_eq!(data.len(), 1000);
            assert_eq!(*data, large_payload);
        }
        _ => panic!("Expected Owned payload"),
    }
}

/// shm[verify shm.goodbye.host]
#[test]
fn host_goodbye_prevents_guest_send() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Host says goodbye
    host.goodbye("shutting down");

    // Guest should see host goodbye
    assert!(guest.is_host_goodbye());

    // Guest send should fail
    let result = guest.send(make_request(1, b"test"));
    assert!(result.is_err());
}

#[test]
fn many_messages_in_sequence() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    const NUM_MESSAGES: u32 = 100;

    // Send many messages
    for i in 0..NUM_MESSAGES {
        let payload = format!("message {}", i);
        guest.send(make_request(i, payload.as_bytes())).unwrap();
    }

    // Receive all
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), NUM_MESSAGES as usize);

    for (i, (recv_peer_id, frame)) in messages.iter().enumerate() {
        assert_eq!(*recv_peer_id, peer_id);
        assert_eq!(frame.desc.id, i as u32);
    }
}

/// shm[verify shm.ring.full]
#[test]
fn ring_backpressure() {
    // Use a small ring to test backpressure
    let config = SegmentConfig {
        ring_size: 4, // Very small ring
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Fill the ring (ring_size - 1 = 3 messages before full)
    for i in 0..3 {
        guest.send(make_request(i, b"x")).unwrap();
    }

    // Next send should fail with RingFull
    let result = guest.send(make_request(99, b"x"));
    assert!(matches!(result, Err(roam_shm::guest::SendError::RingFull)));

    // After host polls, ring has space again
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 3);

    // Now guest can send again
    guest.send(make_request(100, b"x")).unwrap();
}

/// shm[verify shm.slot.free]
#[test]
fn slot_reclamation_guest_to_host() {
    // Test that slots are properly reclaimed after host consumes messages
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots to force exhaustion
        ring_size: 256,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    let large_payload = vec![0u8; 100]; // Larger than inline, requires slot

    // Send more messages than we have slots
    // This should work because host frees slots after consuming
    for i in 0..20u32 {
        // Guest sends
        let frame = Frame {
            desc: MsgDesc::new(msg_type::DATA, i, 0),
            payload: Payload::Owned(large_payload.clone()),
        };
        guest.send(frame).unwrap();

        // Host consumes (and frees slot)
        let PollResult { messages, .. } = host.poll();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1.desc.id, i);
    }
}

/// shm[verify shm.slot.free]
#[test]
fn slot_reclamation_host_to_guest() {
    // Test that slots are properly reclaimed after guest consumes messages
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots
        ring_size: 256,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    let large_payload = vec![0u8; 100];

    // Send more messages than we have slots
    for i in 0..20u32 {
        // Host sends
        let frame = Frame {
            desc: MsgDesc::new(msg_type::DATA, i, 0),
            payload: Payload::Owned(large_payload.clone()),
        };
        host.send(peer_id, frame).unwrap();

        // Guest consumes (and frees slot)
        let frame = guest.recv().unwrap();
        assert_eq!(frame.desc.id, i);
    }
}
