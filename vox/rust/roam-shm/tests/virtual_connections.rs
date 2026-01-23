//! Integration tests for virtual connections over SHM transport.
//!
//! These tests verify that virtual connections work correctly over SHM,
//! allowing multiple logical connections to be multiplexed over a single
//! physical SHM link between guest and host.

use facet_testhelpers::test;

use roam::session::ConnectionHandle;
use roam_session::{Rx, Tx};
use roam_shm::driver::{IncomingConnections, establish_guest, establish_multi_peer_host};
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;

use roam_shm::transport::ShmGuestTransport;
use roam_wire::Metadata;

/// Simple service for virtual connection tests.
#[roam::service]
trait TestService {
    async fn echo(&self, input: String) -> String;
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn stream_sum(&self, numbers: Rx<i32>) -> i64;
    async fn generate(&self, count: u32, output: Tx<i32>);
}

/// Service implementation.
#[derive(Clone)]
struct TestServiceImpl {
    name: String,
}

impl TestServiceImpl {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl TestService for TestServiceImpl {
    async fn echo(&self, _cx: &roam::Context, input: String) -> String {
        format!("[{}] {}", self.name, input)
    }

    async fn add(&self, _cx: &roam::Context, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn stream_sum(&self, _cx: &roam::Context, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += n as i64;
        }
        total
    }

    async fn generate(&self, _cx: &roam::Context, count: u32, output: Tx<i32>) {
        for i in 0..count {
            if output.send(&(i as i32)).await.is_err() {
                break;
            }
        }
    }
}

/// Test fixture that sets up a guest-host pair with virtual connection support.
struct VirtualConnTestFixture {
    guest_handle: ConnectionHandle,
    guest_incoming: IncomingConnections,
    host_handle: ConnectionHandle,
    host_incoming: IncomingConnections,
    _dir: tempfile::TempDir,
}

impl VirtualConnTestFixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("segment");
        let config = SegmentConfig::default();
        let mut host = ShmHost::create(&path, config).unwrap();

        let ticket = host
            .add_peer(roam_shm::spawn::AddPeerOptions {
                peer_name: Some("test-guest".to_string()),
                on_death: None,
                ..Default::default()
            })
            .unwrap();
        let peer_id = ticket.peer_id;
        let spawn_args = ticket.into_spawn_args();

        let guest_dispatcher = TestServiceDispatcher::new(TestServiceImpl::new("guest"));
        let host_dispatcher = TestServiceDispatcher::new(TestServiceImpl::new("host"));

        let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
        let (guest_handle, guest_incoming, guest_driver) =
            establish_guest(guest_transport, guest_dispatcher);

        let (host_driver, mut handles, mut host_incoming_map, _driver_handle) =
            establish_multi_peer_host(host, vec![(peer_id, host_dispatcher)]);
        let host_handle = handles.remove(&peer_id).unwrap();
        let host_incoming = host_incoming_map.remove(&peer_id).unwrap();

        tokio::spawn(guest_driver.run());
        tokio::spawn(host_driver.run());

        // Give drivers time to start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        Self {
            guest_handle,
            guest_incoming,
            host_handle,
            host_incoming,
            _dir: dir,
        }
    }
}

/// Test that a guest can initiate a virtual connection to the host.
#[test(tokio::test)]
async fn guest_initiates_virtual_connection() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;

    // Guest initiates a virtual connection
    let connect_fut = fixture.guest_handle.connect(Metadata::default(), None);

    // Host accepts the incoming connection
    let accept_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };

    let (guest_virtual, _host_virtual) = tokio::join!(connect_fut, accept_fut);
    let guest_virtual = guest_virtual.unwrap();

    // Use the virtual connection to make an RPC call
    let client = TestServiceClient::new(guest_virtual);
    let result = client
        .echo("hello from virtual!".to_string())
        .await
        .unwrap();
    assert_eq!(result, "[host] hello from virtual!");
}

/// Test that a host can initiate a virtual connection to the guest.
#[test(tokio::test)]
async fn host_initiates_virtual_connection() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut guest_incoming = fixture.guest_incoming;

    // Host initiates a virtual connection
    let connect_fut = fixture.host_handle.connect(Metadata::default(), None);

    // Guest accepts the incoming connection
    let accept_fut = async {
        let incoming = guest_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };

    let (host_virtual, _guest_virtual) = tokio::join!(connect_fut, accept_fut);
    let host_virtual = host_virtual.unwrap();

    // Use the virtual connection to make an RPC call
    let client = TestServiceClient::new(host_virtual);
    let result = client
        .echo("hello from host side!".to_string())
        .await
        .unwrap();
    assert_eq!(result, "[guest] hello from host side!");
}

/// Test that multiple virtual connections can coexist.
#[test(tokio::test)]
async fn multiple_virtual_connections() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;

    // Create first virtual connection
    let connect1_fut = fixture.guest_handle.connect(Metadata::default(), None);
    let accept1_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (virtual1, _) = tokio::join!(connect1_fut, accept1_fut);
    let virtual1 = virtual1.unwrap();

    // Create second virtual connection
    let connect2_fut = fixture.guest_handle.connect(Metadata::default(), None);
    let accept2_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (virtual2, _) = tokio::join!(connect2_fut, accept2_fut);
    let virtual2 = virtual2.unwrap();

    // Make concurrent calls on different virtual connections
    let client1 = TestServiceClient::new(virtual1);
    let client2 = TestServiceClient::new(virtual2);

    let (result1, result2) = tokio::join!(
        client1.echo("conn1".to_string()),
        client2.echo("conn2".to_string()),
    );

    assert_eq!(result1.unwrap(), "[host] conn1");
    assert_eq!(result2.unwrap(), "[host] conn2");
}

/// Test that root connection and virtual connections can be used simultaneously.
#[test(tokio::test)]
async fn root_and_virtual_connections_coexist() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;

    // Use root connection
    let root_client = TestServiceClient::new(fixture.guest_handle.clone());
    let root_result = root_client.echo("root".to_string()).await.unwrap();
    assert_eq!(root_result, "[host] root");

    // Create virtual connection
    let connect_fut = fixture.guest_handle.connect(Metadata::default(), None);
    let accept_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (virtual_handle, _) = tokio::join!(connect_fut, accept_fut);
    let virtual_handle = virtual_handle.unwrap();

    // Use virtual connection
    let virtual_client = TestServiceClient::new(virtual_handle);
    let virtual_result = virtual_client.echo("virtual".to_string()).await.unwrap();
    assert_eq!(virtual_result, "[host] virtual");

    // Root connection still works
    let root_result2 = root_client.echo("root again".to_string()).await.unwrap();
    assert_eq!(root_result2, "[host] root again");
}

/// Test streaming over a virtual connection.
#[test(tokio::test)]
async fn streaming_over_virtual_connection() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;

    // Create virtual connection
    let connect_fut = fixture.guest_handle.connect(Metadata::default(), None);
    let accept_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (virtual_handle, _) = tokio::join!(connect_fut, accept_fut);
    let virtual_handle = virtual_handle.unwrap();

    // Use virtual connection for streaming
    let client = TestServiceClient::new(virtual_handle);

    // Client-to-server streaming: create channel and pass rx to the method
    let (tx, rx) = roam::channel::<i32>();
    let sum_task = tokio::spawn({
        let client = client.clone();
        async move { client.stream_sum(rx).await }
    });

    // Send values
    for i in 1..=10 {
        tx.send(&i).await.unwrap();
    }
    drop(tx); // Close the stream
    let sum = sum_task.await.unwrap().unwrap();
    assert_eq!(sum, 55); // 1+2+...+10

    // Server-to-client streaming: create channel and pass tx to the method
    let (tx, mut rx) = roam::channel::<i32>();
    client.generate(5, tx).await.unwrap();

    let mut values = Vec::new();
    while let Ok(Some(v)) = rx.recv().await {
        values.push(v);
    }
    assert_eq!(values, vec![0, 1, 2, 3, 4]);
}

/// Test that a virtual connection can be rejected.
#[test(tokio::test)]
async fn virtual_connection_rejection() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;

    // Guest initiates a virtual connection
    let connect_fut = fixture.guest_handle.connect(Metadata::default(), None);

    // Host rejects the incoming connection
    let reject_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.reject("not allowed".to_string(), Metadata::default());
    };

    let (connect_result, _) = tokio::join!(connect_fut, reject_fut);

    // The connect should fail with a rejection error
    assert!(connect_result.is_err());
}

/// Test bidirectional virtual connections (guest and host each initiate one).
#[test(tokio::test)]
async fn bidirectional_virtual_connections() {
    let fixture = VirtualConnTestFixture::new().await;
    let mut host_incoming = fixture.host_incoming;
    let mut guest_incoming = fixture.guest_incoming;

    // Guest initiates connection to host
    let guest_connect_fut = fixture.guest_handle.connect(Metadata::default(), None);
    let host_accept_fut = async {
        let incoming = host_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (guest_to_host, _) = tokio::join!(guest_connect_fut, host_accept_fut);
    let guest_to_host = guest_to_host.unwrap();

    // Host initiates connection to guest
    let host_connect_fut = fixture.host_handle.connect(Metadata::default(), None);
    let guest_accept_fut = async {
        let incoming = guest_incoming.recv().await.unwrap();
        incoming.accept(Metadata::default(), None).await.unwrap()
    };
    let (host_to_guest, _) = tokio::join!(host_connect_fut, guest_accept_fut);
    let host_to_guest = host_to_guest.unwrap();

    // Use guest-initiated connection to call host service
    let client1 = TestServiceClient::new(guest_to_host);
    let result1 = client1.echo("to host".to_string()).await.unwrap();
    assert_eq!(result1, "[host] to host");

    // Use host-initiated connection to call guest service
    let client2 = TestServiceClient::new(host_to_guest);
    let result2 = client2.echo("to guest".to_string()).await.unwrap();
    assert_eq!(result2, "[guest] to guest");
}
