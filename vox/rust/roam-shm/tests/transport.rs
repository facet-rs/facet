//! Transport-level integration tests.
//!
//! These tests verify that the SHM transport correctly handles roam wire Messages,
//! including metadata encoding and round-trip preservation.
//!
//! shm[verify shm.metadata.in-payload]
//! shm[verify shm.payload.encoding]
//! shm[verify shm.scope]

use roam_shm::guest::ShmGuest;
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use roam_shm::transport::{ShmGuestTransport, frame_to_message, message_to_frame};
use roam_wire::{Message, MetadataValue};

fn create_host_and_guest() -> (ShmHost, ShmGuest) {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();
    let guest = ShmGuest::attach(region).unwrap();
    (host, guest)
}

#[test]
fn guest_transport_send_request() {
    let (mut host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();
    let mut transport = ShmGuestTransport::new(guest);

    // Send a Request message through the transport
    let msg = Message::Request {
        request_id: 42,
        method_id: 123,
        metadata: vec![(
            "auth".to_string(),
            MetadataValue::String("token123".to_string()),
        )],
        channels: vec![],
        payload: b"request body".to_vec(),
    };

    transport.send(&msg).unwrap();

    // Host should receive it
    let mut messages = host.poll();
    assert_eq!(messages.len(), 1);

    let (recv_peer_id, frame) = messages.pop().unwrap();
    assert_eq!(recv_peer_id, peer_id);

    // Convert back to Message and verify
    let decoded = frame_to_message(frame).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn guest_transport_recv_response() {
    let (mut host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();
    let mut transport = ShmGuestTransport::new(guest);

    // Host sends a Response message
    let msg = Message::Response {
        request_id: 42,
        metadata: vec![],
        payload: b"response body".to_vec(),
    };

    let frame = message_to_frame(&msg).unwrap();
    host.send(peer_id, frame).unwrap();

    // Guest transport should receive it
    let received = transport.try_recv().unwrap().unwrap();
    assert_eq!(received, msg);
}

#[test]
fn host_guest_transport_roundtrip() {
    let (mut host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Guest sends request
    let request = Message::Request {
        request_id: 1,
        method_id: 100,
        metadata: vec![
            (
                "key1".to_string(),
                MetadataValue::String("value1".to_string()),
            ),
            ("key2".to_string(), MetadataValue::Bytes(vec![1, 2, 3, 4])),
        ],
        channels: vec![],
        payload: b"hello server".to_vec(),
    };
    guest_transport.send(&request).unwrap();

    // Host receives and processes
    let mut messages = host.poll();
    assert_eq!(messages.len(), 1);
    let (_, frame) = messages.pop().unwrap();
    let decoded_request = frame_to_message(frame).unwrap();
    assert_eq!(decoded_request, request);

    // Host sends response
    let response = Message::Response {
        request_id: 1,
        metadata: vec![],
        payload: b"hello client".to_vec(),
    };
    let response_frame = message_to_frame(&response).unwrap();
    host.send(peer_id, response_frame).unwrap();

    // Guest receives response
    let decoded_response = guest_transport.try_recv().unwrap().unwrap();
    assert_eq!(decoded_response, response);
}

#[test]
fn streaming_data_messages() {
    let (mut host, guest) = create_host_and_guest();
    let _peer_id = guest.peer_id();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Send multiple Data messages (simulating a stream)
    for i in 0..5 {
        let data = Message::Data {
            channel_id: 7,
            payload: format!("chunk {}", i).into_bytes(),
        };
        guest_transport.send(&data).unwrap();
    }

    // Send Close
    let close = Message::Close { channel_id: 7 };
    guest_transport.send(&close).unwrap();

    // Host receives all
    let mut messages = host.poll();
    assert_eq!(messages.len(), 6);

    // Reverse to pop in order
    messages.reverse();
    for i in 0..5 {
        let (_, frame) = messages.pop().unwrap();
        let msg = frame_to_message(frame).unwrap();
        assert!(matches!(msg, Message::Data { channel_id: 7, .. }));
        if let Message::Data { payload, .. } = msg {
            assert_eq!(payload, format!("chunk {}", i).into_bytes());
        }
    }

    let (_, last_frame) = messages.pop().unwrap();
    let last_msg = frame_to_message(last_frame).unwrap();
    assert!(matches!(last_msg, Message::Close { channel_id: 7 }));
}

#[test]
fn cancel_message() {
    let (mut host, guest) = create_host_and_guest();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Send a request, then cancel it
    let request = Message::Request {
        request_id: 99,
        method_id: 1,
        metadata: vec![],
        channels: vec![],
        payload: vec![],
    };
    guest_transport.send(&request).unwrap();

    let cancel = Message::Cancel { request_id: 99 };
    guest_transport.send(&cancel).unwrap();

    // Host receives both
    let mut messages = host.poll();
    assert_eq!(messages.len(), 2);

    messages.reverse();
    let (_, frame1) = messages.pop().unwrap();
    let (_, frame2) = messages.pop().unwrap();
    let msg1 = frame_to_message(frame1).unwrap();
    let msg2 = frame_to_message(frame2).unwrap();

    assert!(matches!(msg1, Message::Request { request_id: 99, .. }));
    assert!(matches!(msg2, Message::Cancel { request_id: 99 }));
}

#[test]
fn reset_message() {
    let (mut host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Host sends Reset to guest
    let reset = Message::Reset { channel_id: 42 };
    let frame = message_to_frame(&reset).unwrap();
    host.send(peer_id, frame).unwrap();

    // Guest receives it
    let received = guest_transport.try_recv().unwrap().unwrap();
    assert_eq!(received, reset);
}

#[test]
fn goodbye_message() {
    let (mut host, guest) = create_host_and_guest();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Guest sends Goodbye
    let goodbye = Message::Goodbye {
        reason: "shutting down".to_string(),
    };
    guest_transport.send(&goodbye).unwrap();

    // Host receives it
    let mut messages = host.poll();
    assert_eq!(messages.len(), 1);

    let (_, frame) = messages.pop().unwrap();
    let msg = frame_to_message(frame).unwrap();
    assert_eq!(msg, goodbye);
}

#[test]
fn large_metadata() {
    let (mut host, guest) = create_host_and_guest();
    let mut guest_transport = ShmGuestTransport::new(guest);

    // Create a request with lots of metadata
    let mut metadata = Vec::new();
    for i in 0..50 {
        metadata.push((
            format!("key{}", i),
            MetadataValue::String(format!("value{}", i)),
        ));
    }

    let request = Message::Request {
        request_id: 1,
        method_id: 1,
        metadata,
        channels: vec![],
        payload: b"small payload".to_vec(),
    };

    guest_transport.send(&request).unwrap();

    let mut messages = host.poll();
    assert_eq!(messages.len(), 1);

    let (_, frame) = messages.pop().unwrap();
    let decoded = frame_to_message(frame).unwrap();
    assert_eq!(decoded, request);
}

#[test]
fn empty_metadata_and_payload() {
    let (mut host, guest) = create_host_and_guest();
    let mut guest_transport = ShmGuestTransport::new(guest);

    let request = Message::Request {
        request_id: 1,
        method_id: 1,
        metadata: vec![],
        channels: vec![],
        payload: vec![],
    };

    guest_transport.send(&request).unwrap();

    let mut messages = host.poll();
    let (_, frame) = messages.pop().unwrap();
    let decoded = frame_to_message(frame).unwrap();
    assert_eq!(decoded, request);
}

#[test]
fn recv_timeout_no_message() {
    let (_host, guest) = create_host_and_guest();
    let mut transport = ShmGuestTransport::new(guest);

    // No message sent, should timeout
    let result = transport.recv_timeout(std::time::Duration::from_millis(10));
    assert!(result.unwrap().is_none());
}
