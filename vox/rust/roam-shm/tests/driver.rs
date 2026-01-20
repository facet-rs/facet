//! Integration tests for the SHM driver.
//!
//! These tests verify that roam RPC services can run over SHM transport,
//! including proper request/response handling and streaming.
//!
//! shm[verify shm.handshake]
//! shm[verify shm.flow.no-credit-message]

use facet_testhelpers::test;

use roam_session::{Rx, Tx};
use roam_shm::driver::{establish_guest, establish_multi_peer_host};
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use roam_shm::peer::PeerId;
use roam_shm::transport::ShmGuestTransport;

/// Testbed service for integration tests - uses #[roam::service] generated code.
#[roam::service]
trait Testbed {
    async fn echo(&self, input: String) -> String;
    async fn add(&self, args: (i32, i32)) -> i32;
    async fn sum(&self, numbers: Rx<i32>) -> i64;
    async fn generate(&self, count: u32, output: Tx<i32>);
    /// Generate large strings (>32 bytes to require slots)
    async fn generate_large(&self, count: u32, output: Tx<String>);
    /// Receive large strings from caller (for testing host→guest streaming)
    async fn consume_large(&self, input: Rx<String>) -> u32;
    /// Streaming call that fails after receiving some data.
    /// Returns (success: bool, count: u32) - if success is false, error occurred after count messages
    async fn consume_then_fail(&self, input: Rx<String>, fail_after: u32) -> (bool, u32);
    /// Recursive call - calls back to caller with depth-1, until depth=0
    async fn recursive_call(&self, depth: u32) -> u32;
}

/// Implementation of the Testbed service (basic, no callbacks).
#[derive(Clone)]
struct TestbedImpl;

/// Implementation that can call back to the other side (for recursive tests).
#[derive(Clone)]
struct RecursiveTestbedImpl {
    callback_handle: Option<roam_session::ConnectionHandle>,
}

impl RecursiveTestbedImpl {
    fn new() -> Self {
        Self { callback_handle: None }
    }

    fn with_callback(handle: roam_session::ConnectionHandle) -> Self {
        Self { callback_handle: Some(handle) }
    }
}

impl Testbed for RecursiveTestbedImpl {
    async fn echo(&self, input: String) -> String {
        input
    }

    async fn add(&self, (a, b): (i32, i32)) -> i32 {
        a + b
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += n as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        for i in 0..count {
            if output.send(&(i as i32)).await.is_err() {
                break;
            }
        }
    }

    async fn generate_large(&self, count: u32, output: Tx<String>) {
        for i in 0..count {
            let large_string = format!("message_{:04}_padding_to_exceed_32_bytes_inline_limit", i);
            if output.send(&large_string).await.is_err() {
                break;
            }
        }
    }

    async fn consume_large(&self, mut input: Rx<String>) -> u32 {
        let mut count = 0u32;
        while let Ok(Some(_)) = input.recv().await {
            count += 1;
        }
        count
    }

    async fn consume_then_fail(&self, mut input: Rx<String>, fail_after: u32) -> (bool, u32) {
        let mut count = 0u32;
        while let Ok(Some(_)) = input.recv().await {
            count += 1;
            if count >= fail_after {
                return (false, count);
            }
        }
        (true, count)
    }

    async fn recursive_call(&self, depth: u32) -> u32 {
        eprintln!("recursive_call called with depth={}", depth);
        if depth == 0 {
            return 0;
        }

        if let Some(ref handle) = self.callback_handle {
            let client = TestbedClient::new(handle.clone());
            match client.recursive_call(depth - 1).await {
                Ok(result) => {
                    eprintln!("recursive_call depth={} got result={}", depth, result);
                    result + 1
                }
                Err(e) => {
                    eprintln!("recursive_call depth={} failed: {:?}", depth, e);
                    // Return depth to indicate where we failed
                    depth
                }
            }
        } else {
            eprintln!("recursive_call depth={} has no callback handle!", depth);
            depth
        }
    }
}

impl Testbed for TestbedImpl {
    async fn echo(&self, input: String) -> String {
        input
    }

    async fn add(&self, (a, b): (i32, i32)) -> i32 {
        a + b
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        eprintln!("server: sum called");
        let mut total = 0i64;
        loop {
            eprintln!("server: calling recv");
            match numbers.recv().await {
                Ok(Some(n)) => {
                    eprintln!("server: received {n}");
                    total += n as i64;
                }
                Ok(None) => {
                    eprintln!("server: received None, closing");
                    break;
                }
                Err(e) => {
                    eprintln!("server: recv error: {e:?}");
                    break;
                }
            }
        }
        eprintln!("server: returning {total}");
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        for i in 0..count {
            if output.send(&(i as i32)).await.is_err() {
                break;
            }
        }
    }

    async fn generate_large(&self, count: u32, output: Tx<String>) {
        // Generate strings >32 bytes to force slot allocation (not inline)
        for i in 0..count {
            let large_string = format!("message_{:04}_padding_to_exceed_32_bytes_inline_limit", i);
            if output.send(&large_string).await.is_err() {
                break;
            }
        }
    }

    async fn consume_large(&self, mut input: Rx<String>) -> u32 {
        let mut count = 0u32;
        while let Ok(Some(_value)) = input.recv().await {
            count += 1;
            // Small delay - backpressure happens due to limited slots, not slow consumption
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        count
    }

    async fn consume_then_fail(&self, mut input: Rx<String>, fail_after: u32) -> (bool, u32) {
        let mut count = 0u32;
        while let Ok(Some(_value)) = input.recv().await {
            count += 1;
            if count >= fail_after {
                // Return failure after receiving fail_after messages
                // This leaves the stream in a partially consumed state
                return (false, count);
            }
        }
        (true, count)
    }

    async fn recursive_call(&self, depth: u32) -> u32 {
        // Basic impl without callback - just returns depth
        depth
    }
}

struct TestFixture {
    guest_handle: roam_session::ConnectionHandle,
    host_handle: roam_session::ConnectionHandle,
    _dir: tempfile::TempDir, // Keep temp dir alive
}

fn setup_test() -> TestFixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    // Add peer to get spawn ticket with doorbell handle that host monitors
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("test-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    let peer_id = ticket.peer_id;
    // Consume the ticket to get SpawnArgs with owned doorbell handle
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    // Create guest transport from spawn args (consumes doorbell handle)
    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    TestFixture {
        guest_handle,
        host_handle,
        _dir: dir,
    }
}

#[test(tokio::test)]
async fn guest_calls_host_echo() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.guest_handle.clone());
    let input = "Hello, SHM!".to_string();
    let result = client.echo(input.clone()).await.unwrap();
    assert_eq!(result, input);

    // Give drivers time to process before dropping everything
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}

#[test(tokio::test)]
async fn guest_calls_host_add() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.guest_handle);
    let result = client.add((17i32, 25i32)).await.unwrap();
    assert_eq!(result, 42);
}

#[test(tokio::test)]
async fn host_calls_guest() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.host_handle);
    let input = "Hello from host!".to_string();
    let result = client.echo(input.clone()).await.unwrap();
    assert_eq!(result, input);
}

#[test(tokio::test)]
async fn unknown_method_returns_error() {
    let fixture = setup_test();

    let payload = facet_postcard::to_vec(&"test").unwrap();
    let response = fixture.guest_handle.call_raw(999, payload).await.unwrap();

    assert_eq!(response[0], 1, "Expected error marker");
}

#[test(tokio::test)]
async fn multiple_sequential_calls() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.guest_handle);
    for i in 0i32..10 {
        let result = client.add((i, i * 2)).await.unwrap();
        assert_eq!(result, i + i * 2);
    }
}

// ============================================================================
// Streaming tests - verify Tx/Rx work over SHM transport
// ============================================================================

/// Test client streaming: client sends multiple values, server returns aggregate.
///
/// shm[verify shm.handshake]
#[test(tokio::test)]
async fn client_streaming_sum() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.guest_handle);

    // Test streaming - correct order: channel, spawn sender, call+await
    let (tx, rx) = roam::channel::<i32>();

    // Spawn task to send numbers (before calling)
    let sender = tokio::spawn(async move {
        for i in 1..=10 {
            eprintln!("CLIENT: sending {i}");
            match tx.send(&i).await {
                Ok(()) => eprintln!("CLIENT: sent {i} successfully"),
                Err(e) => {
                    eprintln!("CLIENT: send failed: {e}");
                    break;
                }
            }
        }
        eprintln!("CLIENT: sender done, dropping tx");
    });

    // Call and await the result
    eprintln!("calling sum");
    let result = client.sum(rx).await.unwrap();
    eprintln!("got result: {result}");
    assert_eq!(result, 55, "Expected sum 1+2+...+10=55, got {}", result);

    // Wait for sender to complete
    sender.await.unwrap();
}

/// Test server streaming: server sends multiple values back to client.
///
/// shm[verify shm.flow.no-credit-message]
#[test(tokio::test)]
async fn server_streaming_generate() {
    let fixture = setup_test();

    let client = TestbedClient::new(fixture.guest_handle);

    // Create channel for server-to-client streaming
    let (tx, mut rx) = roam::channel::<i32>();

    // Spawn task to collect streamed values (before calling)
    let collector = tokio::spawn(async move {
        let mut values = Vec::new();
        while let Ok(Some(value)) = rx.recv().await {
            values.push(value);
        }
        values
    });

    // Call and await the result
    client.generate(5u32, tx).await.unwrap();

    // Wait for all streamed values
    let values = collector.await.unwrap();
    assert_eq!(values, vec![0, 1, 2, 3, 4]);
}

// ============================================================================
// Multi-peer host driver tests
// ============================================================================

struct MultiPeerFixture {
    guest1_handle: roam_session::ConnectionHandle,
    guest2_handle: roam_session::ConnectionHandle,
    host_handles: std::collections::HashMap<PeerId, roam_session::ConnectionHandle>,
    _dir: tempfile::TempDir,
}

fn setup_multi_peer_test() -> MultiPeerFixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    // Add first peer via ticket (proper doorbell setup)
    let ticket1 = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("guest-1".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id1 = ticket1.peer_id;
    let spawn_args1 = ticket1.into_spawn_args();
    let guest1_transport = ShmGuestTransport::from_spawn_args(spawn_args1).unwrap();

    // Add second peer via ticket
    let ticket2 = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("guest-2".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id2 = ticket2.peer_id;
    let spawn_args2 = ticket2.into_spawn_args();
    let guest2_transport = ShmGuestTransport::from_spawn_args(spawn_args2).unwrap();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    // Set up guest drivers
    let (guest1_handle, guest1_driver) = establish_guest(guest1_transport, dispatcher.clone());
    let (guest2_handle, guest2_driver) = establish_guest(guest2_transport, dispatcher.clone());

    // Set up multi-peer host driver
    let (host_driver, host_handles, _driver_handle) = establish_multi_peer_host(
        host,
        vec![
            (peer_id1, TestbedDispatcher::new(TestbedImpl)),
            (peer_id2, TestbedDispatcher::new(TestbedImpl)),
        ],
    );

    tokio::spawn(guest1_driver.run());
    tokio::spawn(guest2_driver.run());
    tokio::spawn(host_driver.run());

    MultiPeerFixture {
        guest1_handle,
        guest2_handle,
        host_handles,
        _dir: dir,
    }
}

/// Test that multi-peer host driver can handle multiple guests.
#[test(tokio::test)]
async fn multi_peer_host_two_guests() {
    let fixture = setup_multi_peer_test();

    // Both guests can make calls
    let client1 = TestbedClient::new(fixture.guest1_handle.clone());
    let input1 = "Hello from guest 1".to_string();
    let result1 = client1.echo(input1.clone()).await.unwrap();
    assert_eq!(result1, input1);

    let client2 = TestbedClient::new(fixture.guest2_handle.clone());
    let input2 = "Hello from guest 2".to_string();
    let result2 = client2.echo(input2.clone()).await.unwrap();
    assert_eq!(result2, input2);

    // Host can call specific guests
    for (peer_id, handle) in &fixture.host_handles {
        let host_client = TestbedClient::new(handle.clone());
        let input = format!("Hello to peer {} from host", peer_id.get());
        let result = host_client.echo(input.clone()).await.unwrap();
        assert_eq!(result, input);
    }
}

/// Test concurrent calls from multiple guests.
#[test(tokio::test)]
async fn multi_peer_concurrent_calls() {
    let fixture = setup_multi_peer_test();

    // Make concurrent calls from both guests
    let client1 = TestbedClient::new(fixture.guest1_handle.clone());
    let task1 = tokio::spawn(async move {
        for i in 0i32..5 {
            let result = client1.add((i, 100)).await.unwrap();
            assert_eq!(result, i + 100);
        }
    });

    let client2 = TestbedClient::new(fixture.guest2_handle.clone());
    let task2 = tokio::spawn(async move {
        for i in 0i32..5 {
            let result = client2.add((i, 200)).await.unwrap();
            assert_eq!(result, i + 200);
        }
    });

    task1.await.unwrap();
    task2.await.unwrap();
}

/// Test true lazy spawning: dynamically create peers after driver is built.
///
/// This demonstrates the fix for issue #40: we can call create_peer() on the
/// driver handle to dynamically create new peers without pre-registering them
/// before building the driver. This enables true lazy spawning where peers are
/// created on-demand rather than all at initialization.
#[test(tokio::test)]
async fn test_dynamic_peer_creation_no_preregistration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic_peers.shm");

    let config = SegmentConfig::default();
    let host = ShmHost::create(&path, config).unwrap();

    // Build driver with ZERO pre-registered peers
    // This is the key: no need to call host.add_peer() before building
    let (host_driver, initial_handles, driver_handle) =
        establish_multi_peer_host(host, Vec::<(PeerId, TestbedDispatcher<TestbedImpl>)>::new());

    // Verify we started with zero peers
    assert_eq!(initial_handles.len(), 0);

    // Spawn driver
    tokio::spawn(host_driver.run());

    // TRUE LAZY SPAWN #1: Create peer on-demand AFTER driver is built
    // This calls host.add_peer() dynamically via channel - the KEY FIX!
    let ticket1 = driver_handle
        .create_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("dynamic-peer-1".to_string()),
            on_death: None,
        })
        .await
        .unwrap();

    // Verify ticket is valid and usable
    assert_eq!(ticket1.peer_id.get(), 1);
    assert_eq!(ticket1.hub_path, path);
    let args1 = ticket1.to_args();
    assert_eq!(args1.len(), 3);
    assert!(args1[0].contains(&path.to_string_lossy().to_string()));
    assert_eq!(args1[1], "--peer-id=1");
    assert!(args1[2].starts_with(&format!("{}=", shm_primitives::DoorbellHandle::ARG_NAME)));

    // Verify we can parse the args back (but forget parsed to avoid double-close)
    let parsed1 = roam_shm::spawn::SpawnArgs::from_args(&args1).unwrap();
    assert_eq!(parsed1.peer_id, ticket1.peer_id);
    assert_eq!(parsed1.hub_path, ticket1.hub_path);
    std::mem::forget(parsed1); // Don't close the FD twice

    // TRUE LAZY SPAWN #2: Create ANOTHER peer dynamically!
    // Before this fix, you HAD to create all tickets before building the driver
    let ticket2 = driver_handle
        .create_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("dynamic-peer-2".to_string()),
            on_death: None,
        })
        .await
        .unwrap();

    // Verify second ticket
    assert_eq!(ticket2.peer_id.get(), 2);
    assert_eq!(ticket2.hub_path, path);
    let args2 = ticket2.to_args();
    assert_eq!(args2[1], "--peer-id=2");

    // Create a THIRD peer to really prove the point
    let ticket3 = driver_handle
        .create_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("dynamic-peer-3".to_string()),
            on_death: None,
        })
        .await
        .unwrap();

    assert_eq!(ticket3.peer_id.get(), 3);

    // This test proves the fix: we can dynamically create as many peers as we want
    // AFTER the driver is built, with NO pre-registration. The driver owns the host
    // and we send commands via channel to call host.add_peer() dynamically.
}

/// Integration test: spawn actual child processes dynamically.
///
/// This is the REAL test for issue #40. It:
/// - Starts a multi-peer host driver with ZERO pre-registered peers
/// - Dynamically creates peers on-demand via `driver_handle.create_peer()`
/// - Spawns actual child processes using `SpawnTicket::spawn()`
/// - Makes RPC calls from host to guest processes
/// - Verifies full multi-process communication works
#[tokio::test(flavor = "current_thread")]
#[ignore] // Requires more debugging
async fn test_lazy_spawn_real_processes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lazy_spawn.shm");

    let config = SegmentConfig::default();
    let host = ShmHost::create(&path, config).unwrap();

    // Build driver with ZERO pre-registered peers (true lazy spawning)
    let (host_driver, initial_handles, driver_handle) =
        establish_multi_peer_host(host, Vec::<(PeerId, TestbedDispatcher<TestbedImpl>)>::new());

    assert_eq!(initial_handles.len(), 0, "should start with zero peers");

    // Spawn the driver
    tokio::spawn(host_driver.run());

    // Create first peer dynamically AFTER driver is running
    let ticket1 = driver_handle
        .create_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("guest1".to_string()),
            on_death: None,
        })
        .await
        .expect("failed to create peer 1");

    // Spawn the actual child process
    let guest_binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("guest_process");

    let cmd = std::process::Command::new(&guest_binary);
    let mut child1 = ticket1.spawn(cmd).expect("failed to spawn guest 1");

    // Give the child time to attach
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Register a dispatcher for this peer so we can get the handle
    let peer_id1 = ticket1.peer_id;
    let peer1_handle = driver_handle
        .add_peer(peer_id1, TestbedDispatcher::new(TestbedImpl))
        .await
        .expect("failed to add peer 1 dispatcher");

    // Make an RPC call from host to guest 1
    let input1 = "Hello from host to guest 1".to_string();
    let payload1 = facet_postcard::to_vec(&input1).unwrap();
    let response1 = peer1_handle.call_raw(1, payload1).await.unwrap();
    assert_eq!(response1[0], 0);
    let result1: String = facet_postcard::from_slice(&response1[1..]).unwrap();
    assert_eq!(result1, input1);

    // Create SECOND peer dynamically while first is still running
    let ticket2 = driver_handle
        .create_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("guest2".to_string()),
            on_death: None,
        })
        .await
        .expect("failed to create peer 2");

    let cmd2 = std::process::Command::new(&guest_binary);
    let mut child2 = ticket2.spawn(cmd2).expect("failed to spawn guest 2");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Register dispatcher for peer 2
    let peer_id2 = ticket2.peer_id;
    let peer2_handle = driver_handle
        .add_peer(peer_id2, TestbedDispatcher::new(TestbedImpl))
        .await
        .expect("failed to add peer 2 dispatcher");

    // Make RPC call to guest 2
    let input2 = "Hello from host to guest 2".to_string();
    let payload2 = facet_postcard::to_vec(&input2).unwrap();
    let response2 = peer2_handle.call_raw(1, payload2).await.unwrap();
    assert_eq!(response2[0], 0);
    let result2: String = facet_postcard::from_slice(&response2[1..]).unwrap();
    assert_eq!(result2, input2);

    // Verify both guests are still alive and responding
    let add_payload1 = facet_postcard::to_vec(&(10i32, 20i32)).unwrap();
    let add_response1 = peer1_handle.call_raw(2, add_payload1).await.unwrap();
    assert_eq!(add_response1[0], 0);
    let sum1: i32 = facet_postcard::from_slice(&add_response1[1..]).unwrap();
    assert_eq!(sum1, 30);

    let add_payload2 = facet_postcard::to_vec(&(100i32, 200i32)).unwrap();
    let add_response2 = peer2_handle.call_raw(2, add_payload2).await.unwrap();
    assert_eq!(add_response2[0], 0);
    let sum2: i32 = facet_postcard::from_slice(&add_response2[1..]).unwrap();
    assert_eq!(sum2, 300);

    // Clean shutdown
    drop(peer1_handle);
    drop(peer2_handle);
    drop(driver_handle);

    // Wait for children to exit
    let _ = child1.wait();
    let _ = child2.wait();
}

// ============================================================================
// Backpressure tests - verify host→guest flow control
// ============================================================================

/// Test host→guest backpressure with streaming: host should wait for slots when sending many messages.
///
/// This test verifies that when the host sends many messages to a guest via streaming,
/// it properly waits for slots to become available instead of failing with
/// SlotExhausted errors.
///
/// The scenario:
/// 1. Configure very few host slots (4)
/// 2. Host streams many large messages (requiring slots) via Tx channel
/// 3. Guest receives slowly
/// 4. Without backpressure: immediate SlotExhausted errors
/// 5. With backpressure: host waits for guest to free slots, all messages delivered
#[test(tokio::test)]
async fn host_to_guest_backpressure_streaming() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("backpressure.shm");

    // Configure with very few host slots to trigger exhaustion quickly
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots - will exhaust quickly
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Add peer via ticket (proper doorbell setup)
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("slow-guest".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    // Create guest transport
    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    // Spawn drivers
    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Give drivers time to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Host streams many LARGE strings to guest via server streaming
    // Each string is >32 bytes to force slot allocation (not inline)
    const NUM_VALUES: u32 = 20; // More than 4 slots worth of messages

    let client = TestbedClient::new(host_handle);

    // Create channel for server-to-client streaming
    let (tx, mut rx) = roam::channel::<String>();

    // Spawn task to collect streamed values with artificial delay to cause backpressure
    let collector = tokio::spawn(async move {
        let mut values = Vec::new();
        while let Ok(Some(value)) = rx.recv().await {
            values.push(value);
            // Artificial delay to slow down consumption and trigger backpressure
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        values
    });

    // Call generate_large - host sends NUM_VALUES large messages to guest
    // Each message requires a slot (>32 bytes). With only 4 slots and 20 messages,
    // this MUST use backpressure to succeed.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.generate_large(NUM_VALUES, tx),
    )
    .await;

    match result {
        Ok(Ok(())) => {
            // Wait for all streamed values
            let values = collector.await.unwrap();
            assert_eq!(
                values.len(),
                NUM_VALUES as usize,
                "Expected {} values, got {}",
                NUM_VALUES,
                values.len()
            );
            // Verify values match expected pattern
            for (i, value) in values.iter().enumerate() {
                let expected = format!("message_{:04}_padding_to_exceed_32_bytes_inline_limit", i);
                assert_eq!(value, &expected, "Message {} mismatch", i);
            }
        }
        Ok(Err(e)) => panic!("generate_large() failed: {:?}", e),
        Err(_) => {
            panic!("generate_large() timed out - likely deadlock due to missing backpressure")
        }
    }
}

/// Test host→guest backpressure: host streams large messages TO guest.
///
/// This is the critical test for the slot exhaustion bug. The host sends
/// many large messages to a slow guest consumer. Without backpressure in
/// the host driver, this fails with SlotExhausted errors.
///
/// The scenario:
/// 1. Configure very few host slots (4)
/// 2. Host calls consume_large on guest, streaming 20 large messages
/// 3. Guest consumes slowly (10ms delay per message)
/// 4. Without backpressure: host fails with SlotExhausted
/// 5. With backpressure: host waits for slots, all messages delivered
#[test(tokio::test)]
async fn host_to_guest_backpressure_host_streaming() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("h2g_backpressure.shm");

    // Configure with very few host slots to trigger exhaustion quickly
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots - will exhaust quickly
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Add peer via ticket (proper doorbell setup)
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("slow-consumer".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    // Create guest transport
    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    // Spawn drivers
    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Give drivers time to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // HOST streams large messages TO GUEST
    // This is the direction that was broken - host sending to guest
    const NUM_VALUES: u32 = 20;

    let client = TestbedClient::new(host_handle);

    // Create channel - host will send, guest will receive
    let (tx, rx) = roam::channel::<String>();

    // Spawn task to send large messages from host side
    let sender = tokio::spawn(async move {
        for i in 0..NUM_VALUES {
            let large_string = format!("host_msg_{:04}_padding_to_exceed_32_bytes_inline_limit", i);
            if tx.send(&large_string).await.is_err() {
                eprintln!("HOST: send {} failed", i);
                return i;
            }
            eprintln!("HOST: sent message {}", i);
        }
        eprintln!("HOST: all {} messages sent", NUM_VALUES);
        NUM_VALUES
    });

    // Call consume_large - guest will receive and count messages
    // With only 4 host slots and 20 messages, host MUST wait for backpressure
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(10), client.consume_large(rx)).await;

    // Wait for sender to complete
    let sent_count = sender.await.unwrap();

    match result {
        Ok(Ok(received_count)) => {
            assert_eq!(sent_count, NUM_VALUES, "Not all messages were sent");
            assert_eq!(
                received_count, NUM_VALUES,
                "Expected {} messages received, got {}",
                NUM_VALUES, received_count
            );
        }
        Ok(Err(e)) => panic!("consume_large() failed: {:?}", e),
        Err(_) => panic!(
            "consume_large() timed out - likely deadlock due to missing host→guest backpressure"
        ),
    }
}

/// Test that slot exhaustion during concurrent RPC calls does NOT cause protocol violations.
///
/// This reproduces the bug from dodeca where many concurrent browser requests
/// caused slot exhaustion, which corrupted channel state and led to `streaming.unknown`
/// protocol violations.
///
/// The scenario:
/// 1. Configure very few slots (4)
/// 2. Make many concurrent RPC calls (30+) from host to guest
/// 3. Each call sends a large payload requiring slots
/// 4. Slot exhaustion WILL occur
/// 5. Expected: calls either succeed or fail cleanly with backpressure
/// 6. Bug behavior: `streaming.unknown` protocol violation crashes the connection
#[test(tokio::test)]
async fn slot_exhaustion_should_not_corrupt_channel_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slot_exhaustion.shm");

    // Configure with very few slots to trigger exhaustion quickly
    let config = SegmentConfig {
        slots_per_guest: 4, // Very few slots
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("concurrent-test".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    // Spawn drivers
    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    // Give drivers time to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Make many concurrent RPC calls with large payloads
    // This should trigger slot exhaustion
    // In dodeca, the payload was ~4500 bytes. With 4 slots, we need payloads
    // big enough that 4+ concurrent calls exhaust all slots.
    const NUM_CONCURRENT_CALLS: usize = 30;
    const PAYLOAD_SIZE: usize = 5000; // ~5KB per payload, similar to dodeca

    let mut handles = Vec::new();
    for i in 0..NUM_CONCURRENT_CALLS {
        let client = TestbedClient::new(host_handle.clone());
        let handle = tokio::spawn(async move {
            // Large payload to force slot allocation - ~5KB like dodeca's HTML payloads
            let large_input = format!(
                "concurrent_call_{:04}_{}",
                i,
                "X".repeat(PAYLOAD_SIZE)
            );
            match client.echo(large_input.clone()).await {
                Ok(result) => {
                    assert_eq!(result, large_input);
                    Ok(i)
                }
                Err(e) => {
                    // Backpressure errors are acceptable - protocol violations are NOT
                    let err_str = format!("{:?}", e);
                    if err_str.contains("streaming.unknown") {
                        panic!("BUG: slot exhaustion caused protocol violation: {}", err_str);
                    }
                    Err(e)
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all calls to complete (with timeout)
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        futures::future::join_all(handles),
    )
    .await
    .expect("Concurrent calls timed out - likely deadlock");

    // Count successes and failures
    let mut successes = 0;
    let mut failures = 0;
    for result in results {
        match result {
            Ok(Ok(_)) => successes += 1,
            Ok(Err(_)) => failures += 1,
            Err(e) => panic!("Task panicked: {:?}", e),
        }
    }

    eprintln!(
        "Concurrent calls: {} succeeded, {} failed (backpressure)",
        successes, failures
    );

    // At least some calls should succeed
    assert!(
        successes > 0,
        "Expected at least some calls to succeed, but all {} failed",
        NUM_CONCURRENT_CALLS
    );

    // Verify drivers are still healthy (not crashed from protocol violations)
    assert!(
        !guest_driver_handle.is_finished(),
        "Guest driver crashed unexpectedly"
    );
    assert!(
        !host_driver_handle.is_finished(),
        "Host driver crashed unexpectedly"
    );
}

/// Test that streaming calls that fail partway through don't corrupt channel state.
///
/// This reproduces a potential bug where:
/// 1. Host starts streaming data to guest
/// 2. Guest returns error partway through (leaving stream partially consumed)
/// 3. Concurrent calls on same connection cause channel confusion
/// 4. Protocol violation occurs due to orphaned/misrouted channels
#[test(tokio::test)]
async fn streaming_errors_should_not_corrupt_channel_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("streaming_errors.shm");

    // Configure with very few slots to increase contention
    let config = SegmentConfig {
        slots_per_guest: 4,
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("streaming-errors-test".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Make many concurrent streaming calls where guest fails after receiving some data
    // This stresses the cleanup logic when streams are partially consumed
    const NUM_CONCURRENT_CALLS: usize = 20;
    const MESSAGES_PER_CALL: u32 = 10;
    const FAIL_AFTER: u32 = 3; // Guest will fail after receiving 3 messages

    let mut task_handles = Vec::new();
    for i in 0..NUM_CONCURRENT_CALLS {
        let client = TestbedClient::new(host_handle.clone());
        let handle = tokio::spawn(async move {
            // Create channel - host will send, guest will receive
            let (tx, rx) = roam::channel::<String>();

            // Spawn sender task that keeps sending even after guest fails
            let sender = tokio::spawn(async move {
                for j in 0..MESSAGES_PER_CALL {
                    let msg = format!("call_{}_msg_{:04}_padding_for_slot_allocation", i, j);
                    if tx.send(&msg).await.is_err() {
                        eprintln!("call {}: sender stopped at msg {}", i, j);
                        return j;
                    }
                }
                MESSAGES_PER_CALL
            });

            // Call consume_then_fail - guest will fail after FAIL_AFTER messages
            let result = client.consume_then_fail(rx, FAIL_AFTER).await;

            // Wait for sender
            let sent = sender.await.unwrap();

            match result {
                Ok((false, _count)) => {
                    // Expected - guest failed after FAIL_AFTER messages
                    Ok((i, sent, "failed_as_expected"))
                }
                Ok((true, _count)) => {
                    // Unexpected - shouldn't succeed with FAIL_AFTER < MESSAGES_PER_CALL
                    Ok((i, sent, "success"))
                }
                Err(e) => {
                    let err_str = format!("{:?}", e);
                    if err_str.contains("streaming.unknown") {
                        panic!("BUG: streaming error caused protocol violation: {}", err_str);
                    }
                    Err(e)
                }
            }
        });
        task_handles.push(handle);
    }

    // Wait for all calls with timeout
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        futures::future::join_all(task_handles),
    )
    .await
    .expect("Streaming error test timed out - likely deadlock");

    let mut successes = 0;
    let mut expected_failures = 0;
    let mut transport_errors = 0;

    for result in results {
        match result {
            Ok(Ok((_, _, status))) => {
                if status == "failed_as_expected" {
                    expected_failures += 1;
                } else {
                    successes += 1;
                }
            }
            Ok(Err(_)) => transport_errors += 1,
            Err(e) => panic!("Task panicked: {:?}", e),
        }
    }

    eprintln!(
        "Streaming error test: {} expected failures, {} successes, {} transport errors",
        expected_failures, successes, transport_errors
    );

    // Most calls should complete (either with expected failure or success)
    let completed = expected_failures + successes;
    assert!(
        completed > NUM_CONCURRENT_CALLS / 2,
        "Expected most calls to complete, but only {} of {} did",
        completed,
        NUM_CONCURRENT_CALLS
    );

    // Verify drivers are still healthy
    assert!(
        !guest_driver_handle.is_finished(),
        "Guest driver crashed - likely protocol violation"
    );
    assert!(
        !host_driver_handle.is_finished(),
        "Host driver crashed - likely protocol violation"
    );
}

/// Aggressive stress test mixing streaming and non-streaming calls with slot exhaustion.
///
/// This test runs a chaotic mix of:
/// - Simple echo calls (no streaming)
/// - Streaming calls that succeed
/// - Streaming calls that fail partway
/// - All happening concurrently with only 4 slots
#[test(tokio::test)]
async fn mixed_calls_with_slot_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed_calls.shm");

    let config = SegmentConfig {
        slots_per_guest: 4,
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("mixed-calls-test".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);
    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Run multiple rounds of mixed calls
    for round in 0..5 {
        let mut task_handles = Vec::new();

        // Mix of different call types
        for i in 0..10 {
            let client = TestbedClient::new(host_handle.clone());
            let call_type = (round * 10 + i) % 3;

            let handle = tokio::spawn(async move {
                match call_type {
                    0 => {
                        // Simple echo - no streaming
                        let msg = format!("round_{}_echo_{}_padding_for_slots", round, i);
                        match client.echo(msg.clone()).await {
                            Ok(result) => {
                                assert_eq!(result, msg);
                                Ok("echo_ok")
                            }
                            Err(e) => {
                                let err_str = format!("{:?}", e);
                                if err_str.contains("streaming.unknown") {
                                    panic!("BUG in echo: {}", err_str);
                                }
                                Err(e)
                            }
                        }
                    }
                    1 => {
                        // Streaming call that completes successfully
                        let (tx, rx) = roam::channel::<String>();
                        let sender = tokio::spawn(async move {
                            for j in 0..5 {
                                let msg = format!("r{}_c{}_msg{}_ok", round, i, j);
                                if tx.send(&msg).await.is_err() {
                                    return j;
                                }
                            }
                            5u32
                        });

                        let result = client.consume_large(rx).await;
                        let _ = sender.await;

                        match result {
                            Ok(_count) => Ok("stream_ok"),
                            Err(e) => {
                                let err_str = format!("{:?}", e);
                                if err_str.contains("streaming.unknown") {
                                    panic!("BUG in stream_ok: {}", err_str);
                                }
                                Err(e)
                            }
                        }
                    }
                    _ => {
                        // Streaming call that fails partway
                        let (tx, rx) = roam::channel::<String>();
                        let sender = tokio::spawn(async move {
                            for j in 0..10 {
                                let msg = format!("r{}_c{}_msg{}_fail", round, i, j);
                                if tx.send(&msg).await.is_err() {
                                    return j;
                                }
                            }
                            10u32
                        });

                        let result = client.consume_then_fail(rx, 3).await;
                        let _ = sender.await;

                        match result {
                            Ok(_) => Ok("stream_fail_ok"),
                            Err(e) => {
                                let err_str = format!("{:?}", e);
                                if err_str.contains("streaming.unknown") {
                                    panic!("BUG in stream_fail: {}", err_str);
                                }
                                Err(e)
                            }
                        }
                    }
                }
            });
            task_handles.push(handle);
        }

        // Wait for all calls in this round
        let results = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            futures::future::join_all(task_handles),
        )
        .await
        .expect("Round timed out");

        let mut ok_count = 0;
        let mut err_count = 0;
        for result in results {
            match result {
                Ok(Ok(_)) => ok_count += 1,
                Ok(Err(_)) => err_count += 1,
                Err(e) => panic!("Task panicked: {:?}", e),
            }
        }
        eprintln!("Round {}: {} ok, {} err", round, ok_count, err_count);
    }

    // Verify drivers survived
    assert!(!guest_driver_handle.is_finished(), "Guest driver crashed");
    assert!(!host_driver_handle.is_finished(), "Host driver crashed");
}

// ============================================================================
// Separate services for recursive call testing (guest and host have different APIs)
// ============================================================================

/// Service provided by the GUEST (cell) - host calls this
#[roam::service]
trait CellService {
    /// Process something, may call back to host
    async fn process(&self, depth: u32) -> u32;
    /// Process with streaming - receives data while recursing
    async fn process_streaming(&self, depth: u32, data: Rx<String>) -> u32;
}

/// Service provided by the HOST - guest calls this
#[roam::service]
trait HostService {
    /// Host-side processing, may call back to guest
    async fn host_process(&self, depth: u32) -> u32;
    /// Host processing with streaming
    async fn host_process_streaming(&self, depth: u32, data: Rx<String>) -> u32;
}

/// Test recursive calls: cell -> host -> cell -> host -> ...
///
/// This is the "endless charade" scenario where:
/// 1. Host calls guest.process(N)
/// 2. Guest calls host.host_process(N-1)
/// 3. Host calls guest.process(N-2)
/// ... and so on until depth=0
///
/// Each in-flight call holds resources. With limited slots, we want to know:
/// - Do we detect the cycle? (No, there's no cycle detection)
/// - Do we error out gracefully? (Should hit backpressure/timeout)
/// - Does it corrupt channel state? (This is what we're testing)
#[test(tokio::test)]
async fn recursive_calls_with_slot_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recursive.shm");

    // Very few slots - recursive calls will exhaust them quickly
    let config = SegmentConfig {
        slots_per_guest: 4,
        ring_size: 64,
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("recursive-test".to_string()),
            on_death: None,
        })
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    // Lazy handles for bidirectional calls
    let guest_to_host: std::sync::Arc<std::sync::OnceLock<roam_session::ConnectionHandle>> =
        std::sync::Arc::new(std::sync::OnceLock::new());
    let host_to_guest: std::sync::Arc<std::sync::OnceLock<roam_session::ConnectionHandle>> =
        std::sync::Arc::new(std::sync::OnceLock::new());

    // Guest-side implementation of CellService
    #[derive(Clone)]
    struct CellServiceImpl {
        to_host: std::sync::Arc<std::sync::OnceLock<roam_session::ConnectionHandle>>,
    }

    impl CellService for CellServiceImpl {
        async fn process(&self, depth: u32) -> u32 {
            eprintln!("GUEST: process(depth={})", depth);
            if depth == 0 {
                return 0;
            }

            if let Some(handle) = self.to_host.get() {
                let client = HostServiceClient::new(handle.clone());
                eprintln!("GUEST: calling host.host_process({})", depth - 1);

                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    client.host_process(depth - 1)
                ).await {
                    Ok(Ok(r)) => {
                        eprintln!("GUEST: got result {} for depth {}", r, depth);
                        r + 1
                    }
                    Ok(Err(e)) => {
                        eprintln!("GUEST: call failed at depth {}: {:?}", depth, e);
                        1000 + depth
                    }
                    Err(_) => {
                        eprintln!("GUEST: timeout at depth {}", depth);
                        2000 + depth
                    }
                }
            } else {
                eprintln!("GUEST: no host handle!");
                depth
            }
        }

        async fn process_streaming(&self, depth: u32, _data: Rx<String>) -> u32 {
            // TODO: implement streaming version
            depth
        }
    }

    // Host-side implementation of HostService
    #[derive(Clone)]
    struct HostServiceImpl {
        to_guest: std::sync::Arc<std::sync::OnceLock<roam_session::ConnectionHandle>>,
    }

    impl HostService for HostServiceImpl {
        async fn host_process(&self, depth: u32) -> u32 {
            eprintln!("HOST: host_process(depth={})", depth);
            if depth == 0 {
                return 0;
            }

            if let Some(handle) = self.to_guest.get() {
                let client = CellServiceClient::new(handle.clone());
                eprintln!("HOST: calling guest.process({})", depth - 1);

                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    client.process(depth - 1)
                ).await {
                    Ok(Ok(r)) => {
                        eprintln!("HOST: got result {} for depth {}", r, depth);
                        r + 1
                    }
                    Ok(Err(e)) => {
                        eprintln!("HOST: call failed at depth {}: {:?}", depth, e);
                        1000 + depth
                    }
                    Err(_) => {
                        eprintln!("HOST: timeout at depth {}", depth);
                        2000 + depth
                    }
                }
            } else {
                eprintln!("HOST: no guest handle!");
                depth
            }
        }

        async fn host_process_streaming(&self, depth: u32, _data: Rx<String>) -> u32 {
            // TODO: implement streaming version
            depth
        }
    }

    let cell_impl = CellServiceImpl { to_host: guest_to_host.clone() };
    let host_impl = HostServiceImpl { to_guest: host_to_guest.clone() };

    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    // Guest provides CellService, uses guest_outbound to call host
    let (guest_outbound, guest_driver) = establish_guest(
        guest_transport,
        CellServiceDispatcher::new(cell_impl)
    );

    // Host provides HostService, uses host_outbound to call guest
    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, HostServiceDispatcher::new(host_impl))]);
    let host_outbound = handles.remove(&peer_id).unwrap();

    // Wire up the callbacks
    let _ = guest_to_host.set(guest_outbound.clone()); // guest calls host via guest's outbound
    let _ = host_to_guest.set(host_outbound.clone());  // host calls guest via host's outbound

    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Test 1: Simple recursive call (should work)
    eprintln!("\n=== Test 1: depth=2 (should succeed) ===");
    let client = CellServiceClient::new(host_outbound.clone());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.process(2)
    ).await;

    match &result {
        Ok(Ok(n)) => eprintln!("depth=2 returned {}", n),
        Ok(Err(e)) => eprintln!("depth=2 failed: {:?}", e),
        Err(_) => eprintln!("depth=2 timed out"),
    }

    // Test 2: Deeper recursion - might exhaust slots
    eprintln!("\n=== Test 2: depth=10 (may exhaust slots) ===");
    let result2 = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.process(10)
    ).await;

    match &result2 {
        Ok(Ok(n)) => eprintln!("depth=10 returned {}", n),
        Ok(Err(e)) => {
            let err_str = format!("{:?}", e);
            eprintln!("depth=10 failed: {}", err_str);
            if err_str.contains("streaming.unknown") {
                panic!("BUG: recursive calls caused protocol violation: {}", err_str);
            }
        }
        Err(_) => eprintln!("depth=10 timed out (expected with slot exhaustion)"),
    }

    // Test 3: Multiple concurrent recursive calls
    eprintln!("\n=== Test 3: 5 concurrent depth=5 calls ===");
    let mut tasks = Vec::new();
    for i in 0..5 {
        let client = CellServiceClient::new(host_outbound.clone());
        tasks.push(tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                client.process(5)
            ).await;
            (i, result)
        }));
    }

    let results = futures::future::join_all(tasks).await;
    for result in results {
        match result {
            Ok((i, Ok(Ok(n)))) => eprintln!("task {} returned {}", i, n),
            Ok((i, Ok(Err(e)))) => {
                let err_str = format!("{:?}", e);
                eprintln!("task {} failed: {}", i, err_str);
                if err_str.contains("streaming.unknown") {
                    panic!("BUG: concurrent recursive calls caused protocol violation");
                }
            }
            Ok((i, Err(_))) => eprintln!("task {} timed out", i),
            Err(e) => panic!("task panicked: {:?}", e),
        }
    }

    // Check if drivers are still alive
    let guest_alive = !guest_driver_handle.is_finished();
    let host_alive = !host_driver_handle.is_finished();

    eprintln!("\n=== Driver status ===");
    eprintln!("Guest driver alive: {}", guest_alive);
    eprintln!("Host driver alive: {}", host_alive);

    assert!(guest_alive, "Guest driver crashed - likely protocol violation");
    assert!(host_alive, "Host driver crashed - likely protocol violation");
}
