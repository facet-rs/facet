//! Integration tests for SHM host-guest message roundtrip.
//!
//! shm[verify shm.topology.hub]
//! shm[verify shm.topology.hub.calls]

use roam_shm::guest::ShmGuest;
use roam_shm::host::{PollResult, ShmHost};
use roam_shm::layout::SegmentConfig;
use roam_shm::msg::ShmMsg;
use roam_shm::msg_type;

/// Create a simple request message with payload.
fn make_request(id: u32, payload: &[u8]) -> ShmMsg {
    ShmMsg::new(msg_type::REQUEST, id, 0, payload.to_vec())
}

/// Create a response message.
fn make_response(id: u32, payload: &[u8]) -> ShmMsg {
    ShmMsg::new(msg_type::RESPONSE, id, 0, payload.to_vec())
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
    guest.send(&request).unwrap();

    // Host polls and receives the message
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);

    let (recv_peer_id, msg) = &messages[0];
    assert_eq!(*recv_peer_id, peer_id);
    assert_eq!(msg.msg_type, msg_type::REQUEST);
    assert_eq!(msg.id, 1);
    assert_eq!(msg.payload_bytes(), b"hello host");
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
    host.send(peer_id, &response).unwrap();

    // Guest receives the message
    let msg = guest.recv().unwrap();
    assert_eq!(msg.msg_type, msg_type::RESPONSE);
    assert_eq!(msg.id, 42);
    assert_eq!(msg.payload_bytes(), b"hello guest");
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
    guest.send(&request).unwrap();

    // Host receives and responds
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].1.id, 100);

    let response = make_response(100, b"pong");
    host.send(peer_id, &response).unwrap();

    // Guest receives response
    let msg = guest.recv().unwrap();
    assert_eq!(msg.msg_type, msg_type::RESPONSE);
    assert_eq!(msg.id, 100);
    assert_eq!(msg.payload_bytes(), b"pong");
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
    guest1.send(&make_request(1, b"from guest1")).unwrap();
    guest2.send(&make_request(2, b"from guest2")).unwrap();

    // Host receives both
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 2);

    // Verify messages came from correct guests
    let msg1 = messages.iter().find(|(p, _)| *p == peer1).unwrap();
    let msg2 = messages.iter().find(|(p, _)| *p == peer2).unwrap();
    assert_eq!(msg1.1.id, 1);
    assert_eq!(msg2.1.id, 2);

    // Host sends different responses to each guest
    host.send(peer1, &make_response(1, b"reply to g1")).unwrap();
    host.send(peer2, &make_response(2, b"reply to g2")).unwrap();

    // Each guest receives only their response
    let msg1 = guest1.recv().unwrap();
    let msg2 = guest2.recv().unwrap();

    assert_eq!(msg1.id, 1);
    assert_eq!(msg2.id, 2);

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

    let msg = ShmMsg::new(msg_type::DATA, 999, 0, large_payload.clone());

    // Guest sends large message
    guest.send(&msg).unwrap();

    // Host receives it
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), 1);

    let (recv_peer_id, recv_msg) = &messages[0];
    assert_eq!(*recv_peer_id, peer_id);
    assert_eq!(recv_msg.msg_type, msg_type::DATA);
    // Verify payload content
    assert_eq!(recv_msg.payload_bytes().len(), 1000);
    assert_eq!(recv_msg.payload_bytes(), &large_payload[..]);
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
    let result = guest.send(&make_request(1, b"test"));
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
        guest.send(&make_request(i, payload.as_bytes())).unwrap();
    }

    // Receive all
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), NUM_MESSAGES as usize);

    for (i, (recv_peer_id, msg)) in messages.iter().enumerate() {
        assert_eq!(*recv_peer_id, peer_id);
        assert_eq!(msg.id, i as u32);
    }
}

/// shm[verify shm.bipbuf.full]
#[test]
fn ring_backpressure() {
    // Use a small ring to test backpressure
    let config = SegmentConfig {
        bipbuf_capacity: 128, // Very small bipbuf so backpressure kicks in quickly
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Fill the bipbuf until backpressure kicks in
    let mut sent = 0u32;
    loop {
        match guest.send(&make_request(sent, b"x")) {
            Ok(()) => sent += 1,
            Err(roam_shm::guest::SendError::RingFull) => break,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }
    assert!(sent > 0, "should have sent at least one message");

    // After host polls, ring has space again
    let PollResult { messages, .. } = host.poll();
    assert_eq!(messages.len(), sent as usize);

    // Now guest can send again
    guest.send(&make_request(sent + 1, b"x")).unwrap();
}

/// shm[verify shm.slot.free]
#[test]
fn slot_reclamation_guest_to_host() {
    // Test that slots are properly reclaimed after host consumes messages
    let config = SegmentConfig {
        bipbuf_capacity: 4096,
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
        let msg = ShmMsg::new(msg_type::DATA, i, 0, large_payload.clone());
        guest.send(&msg).unwrap();

        // Host consumes (and frees slot)
        let PollResult { messages, .. } = host.poll();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1.id, i);
    }
}

/// shm[verify shm.slot.free]
#[test]
fn slot_reclamation_host_to_guest() {
    // Test that slots are properly reclaimed after guest consumes messages
    let config = SegmentConfig {
        bipbuf_capacity: 4096,
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
        let msg = ShmMsg::new(msg_type::DATA, i, 0, large_payload.clone());
        host.send(peer_id, &msg).unwrap();

        // Guest consumes (and frees slot)
        let recv_msg = guest.recv().unwrap();
        assert_eq!(recv_msg.id, i);
    }
}
