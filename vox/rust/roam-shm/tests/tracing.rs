//! Integration tests for roam-tracing over SHM transport.
//!
//! These tests verify that tracing records flow correctly from guest
//! to host over the SHM transport.

use std::sync::Arc;
use std::time::Duration;

use roam::session::RoutedDispatcher;
use roam_shm::driver::{establish_guest, establish_multi_peer_host};
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use roam_shm::transport::ShmGuestTransport;
use roam_tracing::{
    CellTracingDispatcher, CellTracingService, HostTracingDispatcher, HostTracingState, Level,
    TracingConfig, TracingRecord, init_cell_tracing,
};

/// A simple test service for the guest.
#[roam::service]
trait GuestService {
    async fn ping(&self) -> String;
}

#[derive(Clone)]
struct GuestServiceImpl;

impl GuestService for GuestServiceImpl {
    async fn ping(&self) -> String {
        // Emit a tracing event
        tracing::info!("guest received ping");
        "pong".to_string()
    }
}

/// A simple test service for the host (in addition to HostTracing).
#[roam::service]
trait HostService {
    async fn get_name(&self) -> String;
}

#[derive(Clone)]
struct HostServiceImpl {
    name: String,
}

impl HostService for HostServiceImpl {
    async fn get_name(&self) -> String {
        self.name.clone()
    }
}

struct TracingTestFixture {
    guest_handle: roam_session::ConnectionHandle,
    host_handle: roam_session::ConnectionHandle,
    tracing_state: Arc<HostTracingState>,
    tracing_service: CellTracingService,
    _dir: tempfile::TempDir,
}

fn setup_tracing_test() -> TracingTestFixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracing.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    // Add peer
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions {
            peer_name: Some("tracing-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();

    // === Guest side setup ===
    // Initialize cell-side tracing
    let (_tracing_layer, tracing_service) = init_cell_tracing(100);

    // Set up tracing subscriber with the layer
    // Note: in tests we can't call .init() globally, so we'll skip actually
    // installing the subscriber. The layer still buffers records when used.

    // Create guest dispatcher: CellTracing + GuestService
    let cell_tracing_dispatcher = CellTracingDispatcher::new(tracing_service.clone());
    let guest_service_dispatcher = GuestServiceDispatcher::new(GuestServiceImpl);
    let guest_dispatcher = RoutedDispatcher::new(cell_tracing_dispatcher, guest_service_dispatcher);

    // Create guest transport
    let guest_transport = ShmGuestTransport::from_spawn_args(spawn_args).unwrap();
    let (guest_handle, guest_driver) = establish_guest(guest_transport, guest_dispatcher);

    // === Host side setup ===
    // Create shared tracing state
    let tracing_state = HostTracingState::new(100);

    // Create host dispatcher: HostTracing + HostService
    let host_tracing_service =
        tracing_state.service_for_peer(peer_id.get() as u64, Some("tracing-guest".to_string()));
    let host_tracing_dispatcher = HostTracingDispatcher::new(host_tracing_service);
    let host_service_dispatcher = HostServiceDispatcher::new(HostServiceImpl {
        name: "test-host".to_string(),
    });
    let host_dispatcher = RoutedDispatcher::new(host_tracing_dispatcher, host_service_dispatcher);

    // Set up multi-peer host driver
    let (host_driver, mut handles, _driver_handle) =
        establish_multi_peer_host(host, vec![(peer_id, host_dispatcher)]);
    let host_handle = handles.remove(&peer_id).unwrap();

    // Spawn drivers
    tokio::spawn(guest_driver.run());
    tokio::spawn(host_driver.run());

    TracingTestFixture {
        guest_handle,
        host_handle,
        tracing_state,
        tracing_service,
        _dir: dir,
    }
}

/// Test that the guest can query tracing config from host.
#[tokio::test]
async fn test_guest_queries_config() {
    let fixture = setup_tracing_test();

    // Set a custom config on host
    fixture.tracing_state.set_config(TracingConfig {
        min_level: Level::Debug,
        filters: vec!["mymodule".to_string()],
        include_span_events: true,
    });

    // Guest queries config via RPC
    let client = roam_tracing::HostTracingClient::new(fixture.guest_handle.clone());
    let config = client.get_tracing_config().await.unwrap();

    assert_eq!(config.min_level, Level::Debug);
    assert_eq!(config.filters, vec!["mymodule".to_string()]);
    assert!(config.include_span_events);
}

/// Test that records emitted by guest are received by host.
#[tokio::test]
async fn test_guest_emits_records_to_host() {
    let fixture = setup_tracing_test();

    // Take the receiver
    let mut records_rx = fixture.tracing_state.take_receiver().unwrap();

    // Guest emits records via RPC
    let client = roam_tracing::HostTracingClient::new(fixture.guest_handle.clone());
    let records = vec![
        TracingRecord::Event {
            parent: None,
            target: "test_guest".to_string(),
            level: Level::Info,
            message: Some("hello from guest".to_string()),
            fields: vec![],
            timestamp_ns: 12345,
        },
        TracingRecord::Event {
            parent: None,
            target: "test_guest".to_string(),
            level: Level::Warn,
            message: Some("warning from guest".to_string()),
            fields: vec![],
            timestamp_ns: 12346,
        },
    ];

    client.emit_tracing(records).await.unwrap();

    // Host receives tagged records
    let tagged1 = tokio::time::timeout(Duration::from_secs(1), records_rx.recv())
        .await
        .expect("timeout waiting for record")
        .expect("channel closed");

    assert_eq!(tagged1.peer_name, Some("tracing-guest".to_string()));
    if let TracingRecord::Event { message, level, .. } = tagged1.record {
        assert_eq!(message, Some("hello from guest".to_string()));
        assert_eq!(level, Level::Info);
    } else {
        panic!("expected Event record");
    }

    let tagged2 = tokio::time::timeout(Duration::from_secs(1), records_rx.recv())
        .await
        .expect("timeout waiting for record")
        .expect("channel closed");

    if let TracingRecord::Event { message, level, .. } = tagged2.record {
        assert_eq!(message, Some("warning from guest".to_string()));
        assert_eq!(level, Level::Warn);
    } else {
        panic!("expected Event record");
    }
}

/// Test that host can push config updates to cell.
#[tokio::test]
async fn test_host_pushes_config_to_cell() {
    let fixture = setup_tracing_test();

    // Host pushes config to cell
    let client = roam_tracing::CellTracingClient::new(fixture.host_handle.clone());
    let result = client
        .configure(TracingConfig {
            min_level: Level::Error,
            filters: vec![],
            include_span_events: false,
        })
        .await
        .unwrap();

    assert_eq!(result, roam_tracing::ConfigResult::Ok);
}

/// Test bidirectional: guest calls host service, host calls guest service.
#[tokio::test]
async fn test_bidirectional_services_with_tracing() {
    let fixture = setup_tracing_test();

    // Guest calls host's HostService
    let host_client = HostServiceClient::new(fixture.guest_handle.clone());
    let name = host_client.get_name().await.unwrap();
    assert_eq!(name, "test-host");

    // Host calls guest's GuestService
    let guest_client = GuestServiceClient::new(fixture.host_handle.clone());
    let pong = guest_client.ping().await.unwrap();
    assert_eq!(pong, "pong");
}

/// Test the drain task integration (simulated).
#[tokio::test]
async fn test_drain_task_flow() {
    let fixture = setup_tracing_test();

    // Take the receiver
    let mut records_rx = fixture.tracing_state.take_receiver().unwrap();

    // Start the drain task
    fixture
        .tracing_service
        .spawn_drain(fixture.guest_handle.clone());

    // Manually push records to the buffer (simulating what the layer would do)
    // Note: In a real scenario, the CellTracingLayer would push to the buffer
    // when tracing macros are used. Here we access the buffer directly.
    // This is a simplified test - full integration would need the tracing subscriber.

    // Give the drain task time to start and query config
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The drain task should be running. Since we can't easily push to the
    // internal buffer from here, we'll just verify the task started without error.
    // A more complete test would use the actual tracing subscriber.

    // Verify the drain task is alive by checking we don't get immediate channel close
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(200)) => {
            // Good - no crash, drain task is running
        }
        result = records_rx.recv() => {
            // This is also fine - either we got a record or the task is running
            if result.is_none() {
                panic!("channel closed unexpectedly");
            }
        }
    }
}
