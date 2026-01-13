//! Integration tests for the SHM driver.
//!
//! These tests verify that roam RPC services can run over SHM transport,
//! including proper request/response handling and streaming.
//!
//! shm[verify shm.handshake]
//! shm[verify shm.flow.no-credit-message]

#![cfg(feature = "tokio")]

use std::pin::Pin;

use roam_session::{ChannelRegistry, Rx, ServiceDispatcher, Tx, channel, dispatch_call};
use roam_shm::driver::{establish_guest, establish_host_peer, establish_multi_peer_host};
use roam_shm::guest::ShmGuest;
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use roam_shm::transport::ShmGuestTransport;

/// A service with both unary and streaming methods for testing.
#[derive(Clone)]
struct TestService;

impl ServiceDispatcher for TestService {
    fn dispatch(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        match method_id {
            // Echo method: returns the input unchanged
            1 => dispatch_call::<String, String, (), _, _>(
                payload,
                request_id,
                registry,
                |input: String| async move { Ok(input) },
            ),
            // Add method: adds two numbers
            2 => dispatch_call::<(i32, i32), i32, (), _, _>(
                payload,
                request_id,
                registry,
                |(a, b): (i32, i32)| async move { Ok(a + b) },
            ),
            // Sum method: client streams numbers, server returns sum
            // Client creates channel(), passes tx in args (keeping rx to send data)
            // Server receives Rx<i32> after bind_streams hydrates it
            3 => dispatch_call::<Rx<i32>, i64, (), _, _>(
                payload,
                request_id,
                registry,
                |mut input: Rx<i32>| async move {
                    // Server receives data from client via input stream
                    let mut sum: i64 = 0;
                    while let Ok(Some(value)) = input.recv().await {
                        sum += value as i64;
                    }
                    Ok(sum)
                },
            ),
            // Generate method: server streams numbers back to client
            // Client creates channel(), passes rx in args (keeping tx to receive data)
            // Server receives Tx<i32> after bind_streams hydrates it with task_tx
            4 => dispatch_call::<(u32, Tx<i32>), (), (), _, _>(
                payload,
                request_id,
                registry,
                |(count, output): (u32, Tx<i32>)| async move {
                    // Server sends data to client via output stream
                    for i in 0..count {
                        output.send(&(i as i32)).await.ok();
                    }
                    // Tx is dropped here, which sends Close
                    Ok(())
                },
            ),
            _ => roam_session::dispatch_unknown_method(request_id, registry),
        }
    }
}

fn create_host_and_guest() -> (ShmHost, ShmGuest) {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();
    let guest = ShmGuest::attach(region).unwrap();
    (host, guest)
}

#[tokio::test]
async fn guest_calls_host_echo() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    // Set up guest-side driver (client)
    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);

    // Set up host-side driver (server)
    let (host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    // Spawn both drivers
    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    // Make an echo call from guest to host
    let input = "Hello, SHM!".to_string();
    let payload = facet_postcard::to_vec(&input).unwrap();

    let response = guest_handle.call_raw(1, payload).await.unwrap();

    // Response format: [0] = success marker, [1..] = serialized result
    assert_eq!(response[0], 0, "Expected success marker");
    let result: String = facet_postcard::from_slice(&response[1..]).unwrap();
    assert_eq!(result, input);

    // Clean shutdown - drop handles to close channels
    drop(guest_handle);
    drop(host_handle);

    // Wait for drivers to finish (they should exit when channels close)
    // Use a timeout to avoid hanging
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        let _ = guest_driver_handle.await;
        let _ = host_driver_handle.await;
    })
    .await;
}

#[tokio::test]
async fn guest_calls_host_add() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (_host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Make an add call from guest to host
    let args = (17i32, 25i32);
    let payload = facet_postcard::to_vec(&args).unwrap();

    let response = guest_handle.call_raw(2, payload).await.unwrap();

    assert_eq!(response[0], 0, "Expected success marker");
    let result: i32 = facet_postcard::from_slice(&response[1..]).unwrap();
    assert_eq!(result, 42);
}

#[tokio::test]
async fn host_calls_guest() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Make an echo call from host to guest
    let input = "Hello from host!".to_string();
    let payload = facet_postcard::to_vec(&input).unwrap();

    let response = host_handle.call_raw(1, payload).await.unwrap();

    assert_eq!(response[0], 0, "Expected success marker");
    let result: String = facet_postcard::from_slice(&response[1..]).unwrap();
    assert_eq!(result, input);
}

#[tokio::test]
async fn unknown_method_returns_error() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (_host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Call unknown method
    let payload = facet_postcard::to_vec(&"test").unwrap();
    let response = guest_handle.call_raw(999, payload).await.unwrap();

    // Response format: [1] = error marker
    assert_eq!(response[0], 1, "Expected error marker");
}

#[tokio::test]
async fn multiple_sequential_calls() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (_host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Make multiple calls sequentially
    for i in 0i32..10 {
        let args = (i, i * 2);
        let payload = facet_postcard::to_vec(&args).unwrap();
        let response = guest_handle.call_raw(2, payload).await.unwrap();
        assert_eq!(response[0], 0);
        let result: i32 = facet_postcard::from_slice(&response[1..]).unwrap();
        assert_eq!(result, i + i * 2);
    }
}

// ============================================================================
// Streaming tests - verify Tx/Rx work over SHM transport
// ============================================================================

/// Test client streaming: client sends multiple values, server returns aggregate.
///
/// shm[verify shm.handshake]
#[tokio::test]
async fn client_streaming_sum() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (_host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create a channel for streaming numbers to the server
    // For Rx<i32> in args: caller keeps tx to send, passes rx
    let (tx, rx) = channel::<i32>();

    // Spawn a task to send data. We need to do this before call() because
    // call() blocks until the response is received.
    let sender_task = tokio::spawn(async move {
        // Give call() a moment to set up stream binding
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        for i in 1..=10i32 {
            if tx.send(&i).await.is_err() {
                break;
            }
            // Yield between sends to let the runtime process messages
            tokio::task::yield_now().await;
        }
        // Close the stream by dropping tx (this happens automatically)
    });

    // Use call() which handles stream binding (assigns channel IDs, sets up routing)
    let mut args = rx;
    let response = guest_handle.call(3, &mut args).await.unwrap();

    // Wait for sender to complete
    sender_task.await.unwrap();

    assert_eq!(response[0], 0, "Expected success marker");
    let result: i64 = facet_postcard::from_slice(&response[1..]).unwrap();
    assert_eq!(result, 55); // 1+2+3+...+10 = 55
}

/// Test server streaming: server sends multiple values back to client.
///
/// shm[verify shm.flow.no-credit-message]
#[tokio::test]
async fn server_streaming_generate() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, TestService);
    let (_host_handle, host_driver) = establish_host_peer(host, peer_id, TestService);

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create a channel for receiving numbers from the server
    // For Tx<i32> in args: caller keeps rx to receive, passes tx
    let (tx, mut rx) = channel::<i32>();

    // Use call() which handles stream binding
    let mut args = (5u32, tx);

    // Spawn a task to collect streamed values
    let collector = tokio::spawn(async move {
        let mut values = Vec::new();
        while let Ok(Some(value)) = rx.recv().await {
            values.push(value);
        }
        values
    });

    // Make the call - use call() for proper stream binding
    let response = guest_handle.call(4, &mut args).await.unwrap();

    assert_eq!(response[0], 0, "Expected success marker");

    // Wait for all streamed values
    let values = collector.await.unwrap();
    assert_eq!(values, vec![0, 1, 2, 3, 4]);
}

// ============================================================================
// Multi-peer host driver tests
// ============================================================================

fn create_host_and_two_guests() -> (ShmHost, ShmGuest, ShmGuest) {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let guest1 = ShmGuest::attach(region.clone()).unwrap();
    let guest2 = ShmGuest::attach(region).unwrap();

    (host, guest1, guest2)
}

/// Test that multi-peer host driver can handle multiple guests.
#[tokio::test]
async fn multi_peer_host_two_guests() {
    let (host, guest1, guest2) = create_host_and_two_guests();
    let peer_id1 = guest1.peer_id();
    let peer_id2 = guest2.peer_id();

    // Set up guest drivers
    let guest1_transport = ShmGuestTransport::new(guest1);
    let (guest1_handle, guest1_driver) = establish_guest(guest1_transport, TestService);

    let guest2_transport = ShmGuestTransport::new(guest2);
    let (guest2_handle, guest2_driver) = establish_guest(guest2_transport, TestService);

    // Set up multi-peer host driver
    let (host_driver, host_handles) =
        establish_multi_peer_host(host, vec![(peer_id1, TestService), (peer_id2, TestService)]);

    // Spawn all drivers
    tokio::spawn(guest1_driver.run());
    tokio::spawn(guest2_driver.run());
    tokio::spawn(host_driver.run());

    // Both guests can make calls
    let input1 = "Hello from guest 1".to_string();
    let payload1 = facet_postcard::to_vec(&input1).unwrap();
    let response1 = guest1_handle.call_raw(1, payload1).await.unwrap();
    assert_eq!(response1[0], 0);
    let result1: String = facet_postcard::from_slice(&response1[1..]).unwrap();
    assert_eq!(result1, input1);

    let input2 = "Hello from guest 2".to_string();
    let payload2 = facet_postcard::to_vec(&input2).unwrap();
    let response2 = guest2_handle.call_raw(1, payload2).await.unwrap();
    assert_eq!(response2[0], 0);
    let result2: String = facet_postcard::from_slice(&response2[1..]).unwrap();
    assert_eq!(result2, input2);

    // Host can call specific guests
    let host_handle1 = host_handles.get(&peer_id1).unwrap();
    let input3 = "Hello to guest 1 from host".to_string();
    let payload3 = facet_postcard::to_vec(&input3).unwrap();
    let response3 = host_handle1.call_raw(1, payload3).await.unwrap();
    assert_eq!(response3[0], 0);
    let result3: String = facet_postcard::from_slice(&response3[1..]).unwrap();
    assert_eq!(result3, input3);

    let host_handle2 = host_handles.get(&peer_id2).unwrap();
    let input4 = "Hello to guest 2 from host".to_string();
    let payload4 = facet_postcard::to_vec(&input4).unwrap();
    let response4 = host_handle2.call_raw(1, payload4).await.unwrap();
    assert_eq!(response4[0], 0);
    let result4: String = facet_postcard::from_slice(&response4[1..]).unwrap();
    assert_eq!(result4, input4);
}

/// Test concurrent calls from multiple guests.
#[tokio::test]
async fn multi_peer_concurrent_calls() {
    let (host, guest1, guest2) = create_host_and_two_guests();
    let peer_id1 = guest1.peer_id();
    let peer_id2 = guest2.peer_id();

    let guest1_transport = ShmGuestTransport::new(guest1);
    let (guest1_handle, guest1_driver) = establish_guest(guest1_transport, TestService);

    let guest2_transport = ShmGuestTransport::new(guest2);
    let (guest2_handle, guest2_driver) = establish_guest(guest2_transport, TestService);

    let (host_driver, _host_handles) =
        establish_multi_peer_host(host, vec![(peer_id1, TestService), (peer_id2, TestService)]);

    tokio::spawn(guest1_driver.run());
    tokio::spawn(guest2_driver.run());
    tokio::spawn(host_driver.run());

    // Make concurrent calls from both guests
    let task1 = {
        let handle = guest1_handle.clone();
        tokio::spawn(async move {
            for i in 0i32..5 {
                let args = (i, 100);
                let payload = facet_postcard::to_vec(&args).unwrap();
                let response = handle.call_raw(2, payload).await.unwrap();
                assert_eq!(response[0], 0);
                let result: i32 = facet_postcard::from_slice(&response[1..]).unwrap();
                assert_eq!(result, i + 100);
            }
        })
    };

    let task2 = {
        let handle = guest2_handle.clone();
        tokio::spawn(async move {
            for i in 0i32..5 {
                let args = (i, 200);
                let payload = facet_postcard::to_vec(&args).unwrap();
                let response = handle.call_raw(2, payload).await.unwrap();
                assert_eq!(response[0], 0);
                let result: i32 = facet_postcard::from_slice(&response[1..]).unwrap();
                assert_eq!(result, i + 200);
            }
        })
    };

    task1.await.unwrap();
    task2.await.unwrap();
}
