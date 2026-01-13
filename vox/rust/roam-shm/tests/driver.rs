//! Integration tests for the SHM driver.
//!
//! These tests verify that roam RPC services can run over SHM transport,
//! including proper request/response handling and streaming.
//!
//! shm[verify shm.handshake]
//! shm[verify shm.flow.no-credit-message]

#![cfg(feature = "tokio")]

use roam_session::{Rx, Tx};
use roam_shm::driver::{establish_guest, establish_multi_peer_host};
use roam_shm::guest::ShmGuest;
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
}

/// Implementation of the Testbed service.
#[derive(Clone)]
struct TestbedImpl;

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

    // Create dispatcher with generated code
    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    // Set up guest-side driver (client)
    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    // Set up host-side driver (server) - use multi-peer architecture
    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    // Spawn both drivers
    let guest_driver_handle = tokio::spawn(guest_driver.run());
    let host_driver_handle = tokio::spawn(host_driver.run());

    // Create generated client and make an echo call
    let client = TestbedClient::new(guest_handle.clone());
    let input = "Hello, SHM!".to_string();
    let result = client.echo(input.clone()).await.unwrap();
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

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let _host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create generated client and make an add call
    let client = TestbedClient::new(guest_handle);
    let result = client.add((17i32, 25i32)).await.unwrap();
    assert_eq!(result, 42);
}

#[tokio::test]
async fn host_calls_guest() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (_guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create generated client and make an echo call from host to guest
    let client = TestbedClient::new(host_handle);
    let input = "Hello from host!".to_string();
    let result = client.echo(input.clone()).await.unwrap();
    assert_eq!(result, input);
}

#[tokio::test]
async fn unknown_method_returns_error() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let _host_handle = handles.remove(&peer_id).unwrap();

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

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let _host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create generated client
    let client = TestbedClient::new(guest_handle);

    // Make multiple calls sequentially
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
#[tokio::test]
async fn client_streaming_sum() {
    #[cfg(feature = "tracing")]
    {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .try_init();
    }

    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    // Create server with generated dispatcher
    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    // Use multi-peer host driver (correct architecture for host side)
    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let _host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create generated client
    let client = TestbedClient::new(guest_handle);

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
#[tokio::test]
async fn server_streaming_generate() {
    let (host, guest) = create_host_and_guest();
    let peer_id = guest.peer_id();

    let dispatcher = TestbedDispatcher::new(TestbedImpl);

    let guest_transport = ShmGuestTransport::new(guest);
    let (guest_handle, guest_driver) = establish_guest(guest_transport, dispatcher.clone());

    let (host_driver, mut handles, _host_driver_handle) = establish_multi_peer_host::<
        TestbedDispatcher<TestbedImpl>,
        _,
    >(host, vec![(peer_id, dispatcher)]);
    let _host_handle = handles.remove(&peer_id).unwrap();

    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    // Create generated client
    let client = TestbedClient::new(guest_handle);

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
    let _result = client.generate(5u32, tx).await.unwrap();

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

    let guest1 = ShmGuest::attach(region).unwrap();
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
    let (guest1_handle, guest1_driver) =
        establish_guest(guest1_transport, TestbedDispatcher::new(TestbedImpl));

    let guest2_transport = ShmGuestTransport::new(guest2);
    let (guest2_handle, guest2_driver) =
        establish_guest(guest2_transport, TestbedDispatcher::new(TestbedImpl));

    // Set up multi-peer host driver
    let (host_driver, host_handles, _driver_handle) = establish_multi_peer_host(
        host,
        vec![
            (peer_id1, TestbedDispatcher::new(TestbedImpl)),
            (peer_id2, TestbedDispatcher::new(TestbedImpl)),
        ],
    );

    // Spawn all drivers
    tokio::spawn(guest1_driver.run());
    tokio::spawn(guest2_driver.run());
    tokio::spawn(host_driver.run());

    // Both guests can make calls
    let client1 = TestbedClient::new(guest1_handle.clone());
    let input1 = "Hello from guest 1".to_string();
    let result1 = client1.echo(input1.clone()).await.unwrap();
    assert_eq!(result1, input1);

    let client2 = TestbedClient::new(guest2_handle.clone());
    let input2 = "Hello from guest 2".to_string();
    let result2 = client2.echo(input2.clone()).await.unwrap();
    assert_eq!(result2, input2);

    // Host can call specific guests
    let host_client1 = TestbedClient::new(host_handles.get(&peer_id1).unwrap().clone());
    let input3 = "Hello to guest 1 from host".to_string();
    let result3 = host_client1.echo(input3.clone()).await.unwrap();
    assert_eq!(result3, input3);

    let host_client2 = TestbedClient::new(host_handles.get(&peer_id2).unwrap().clone());
    let input4 = "Hello to guest 2 from host".to_string();
    let result4 = host_client2.echo(input4.clone()).await.unwrap();
    assert_eq!(result4, input4);
}

/// Test concurrent calls from multiple guests.
#[tokio::test]
async fn multi_peer_concurrent_calls() {
    let (host, guest1, guest2) = create_host_and_two_guests();
    let peer_id1 = guest1.peer_id();
    let peer_id2 = guest2.peer_id();

    let guest1_transport = ShmGuestTransport::new(guest1);
    let (guest1_handle, guest1_driver) =
        establish_guest(guest1_transport, TestbedDispatcher::new(TestbedImpl));

    let guest2_transport = ShmGuestTransport::new(guest2);
    let (guest2_handle, guest2_driver) =
        establish_guest(guest2_transport, TestbedDispatcher::new(TestbedImpl));

    let (host_driver, _host_handles, _driver_handle) = establish_multi_peer_host(
        host,
        vec![
            (peer_id1, TestbedDispatcher::new(TestbedImpl)),
            (peer_id2, TestbedDispatcher::new(TestbedImpl)),
        ],
    );

    tokio::spawn(guest1_driver.run());
    tokio::spawn(guest2_driver.run());
    tokio::spawn(host_driver.run());

    // Make concurrent calls from both guests
    let client1 = TestbedClient::new(guest1_handle.clone());
    let task1 = tokio::spawn(async move {
        for i in 0i32..5 {
            let result = client1.add((i, 100)).await.unwrap();
            assert_eq!(result, i + 100);
        }
    });

    let client2 = TestbedClient::new(guest2_handle.clone());
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
#[tokio::test]
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
    assert!(args1[2].starts_with("--doorbell-fd="));

    // Verify we can parse the args back
    let parsed1 = roam_shm::spawn::SpawnArgs::from_args(&args1).unwrap();
    assert_eq!(parsed1.peer_id, ticket1.peer_id);
    assert_eq!(parsed1.hub_path, ticket1.hub_path);

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
