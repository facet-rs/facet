//! Schema compatibility tests: two versions of a service talk to each other.
//!
//! Each test pair defines a "v1" and "v2" module with the same trait name
//! and method names but different field types. Because method IDs are
//! name-only (not type-dependent), the two versions route to the same
//! handler. Schema exchange sends type metadata before payloads, and
//! compatibility decode plans handle the schema differences.

use vox_core::{BareConduit, MemoryLink, acceptor_conduit, initiator_conduit, memory_link_pair};
use vox_types::{
    ConnectionSettings, HandshakeResult, MessageFamily, Parity, SessionRole, VoxError,
};

type MessageConduit = BareConduit<MessageFamily, MemoryLink>;

fn conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

fn test_acceptor_handshake(service: &'static str) -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Acceptor,
        our_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        our_schema: vec![],
        peer_schema: vec![],
        peer_metadata: vox::metadata().str("vox-service", service).build(),
    }
}

fn test_initiator_handshake(service: &'static str) -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        our_schema: vec![],
        peer_schema: vec![],
        peer_metadata: vox::metadata().str("vox-service", service).build(),
    }
}

// ============================================================================
// Compatible: added field with default
// ============================================================================

/// V1: the original schema — two fields.
mod point_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[vox::service]
    pub trait Geometry {
        async fn transform(&self, p: Point) -> Point;
    }
}

/// V2: added a field with a default — compatible in both directions.
mod point_v2 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
        #[facet(default)]
        pub z: f64,
    }

    #[vox::service]
    pub trait Geometry {
        async fn transform(&self, p: Point) -> Point;
    }
}

// V1 server implementation
#[derive(Clone)]
struct V1GeometryService;

impl point_v1::Geometry for V1GeometryService {
    async fn transform(&self, p: point_v1::Point) -> point_v1::Point {
        point_v1::Point {
            x: p.x * 2.0,
            y: p.y * 2.0,
        }
    }
}

// V2 server implementation
#[derive(Clone)]
struct V2GeometryService;

impl point_v2::Geometry for V2GeometryService {
    async fn transform(&self, p: point_v2::Point) -> point_v2::Point {
        point_v2::Point {
            x: p.x + 100.0,
            y: p.y + 100.0,
            z: p.z + 1.0,
        }
    }
}

#[tokio::test]
async fn v1_client_v2_server_fills_default() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake("Geometry"))
            .on_connection(point_v2::GeometryDispatcher::new(V2GeometryService))
            .establish::<point_v2::GeometryClient>()
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("Geometry"))
        .establish::<point_v1::GeometryClient>()
        .await
        .expect("client handshake failed");

    // Client sends V1 Point {x, y}. Server receives as V2 Point {x, y, z=0.0}.
    let result = client
        .transform(point_v1::Point { x: 1.0, y: 2.0 })
        .await
        .expect("call should succeed");

    // Server adds 100 to x and y, adds 1.0 to z (which was 0.0).
    // Response is V2 Point. Client reads it as V1 Point (drops z).
    assert_eq!(result.x, 101.0);
    assert_eq!(result.y, 102.0);

    server_task.abort();
}

#[tokio::test]
async fn v2_client_v1_server_skips_unknown_field() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake("Geometry"))
            .on_connection(point_v1::GeometryDispatcher::new(V1GeometryService))
            .establish::<point_v1::GeometryClient>()
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("Geometry"))
        .establish::<point_v2::GeometryClient>()
        .await
        .expect("client handshake failed");

    // Client sends V2 Point {x, y, z}. Server receives as V1 Point {x, y} — z is skipped.
    let result = client
        .transform(point_v2::Point {
            x: 3.0,
            y: 4.0,
            z: 99.0,
        })
        .await
        .expect("call should succeed");

    // Server doubles x and y. Response is V1 Point.
    // Client reads it as V2 Point (z fills default 0.0).
    assert_eq!(result.x, 6.0);
    assert_eq!(result.y, 8.0);
    assert_eq!(result.z, 0.0);

    server_task.abort();
}

// ============================================================================
// Compatible: channel item schemas
// ============================================================================

mod channel_rx_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Event {
        pub id: u32,
    }

    #[vox::service]
    pub trait EventSink {
        async fn consume(&self, events: vox::Rx<Event>) -> u32;
    }
}

mod channel_rx_v2 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Event {
        pub id: u32,
        #[facet(default)]
        pub priority: u32,
    }

    unsafe impl vox_types::Reborrow for Event {
        type Ref<'a> = Event;
    }

    #[vox::service]
    pub trait EventSink {
        async fn consume(&self, events: vox::Rx<Event>) -> u32;
    }
}

#[derive(Clone)]
struct ChannelRxV2Service;

impl channel_rx_v2::EventSink for ChannelRxV2Service {
    async fn consume(&self, mut events: vox::Rx<channel_rx_v2::Event>) -> u32 {
        let event = events
            .recv()
            .await
            .expect("channel receive should succeed")
            .expect("expected one event");
        event.get().id + event.get().priority
    }
}

// r[verify schema.interaction.channels]
// r[verify schema.exchange.channels]
// r[verify schema.exchange.channels.rx-args]
// r[verify rpc.channel.pair.binding-propagation]
// r[verify rpc.channel.pair.tx-read]
#[tokio::test]
async fn rx_channel_items_use_caller_writer_schema() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake("EventSink"))
            .on_connection(channel_rx_v2::EventSinkDispatcher::new(ChannelRxV2Service))
            .establish::<channel_rx_v2::EventSinkClient>()
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("EventSink"))
        .establish::<channel_rx_v1::EventSinkClient>()
        .await
        .expect("client handshake failed");

    let (tx, rx) = vox::channel::<channel_rx_v1::Event>();
    let call_task = tokio::task::spawn(async move { client.consume(rx).await });

    tx.send(channel_rx_v1::Event { id: 7 })
        .await
        .expect("channel send should succeed");

    let total = call_task
        .await
        .expect("call task should finish")
        .expect("call should succeed");
    assert_eq!(total, 7);

    server_task.abort();
}

mod channel_tx_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Event {
        pub id: u32,
    }

    unsafe impl vox_types::Reborrow for Event {
        type Ref<'a> = Event;
    }

    #[vox::service]
    pub trait EventSource {
        async fn produce(&self, events: vox::Tx<Event>) -> u32;
    }
}

mod channel_tx_v2 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Event {
        pub priority: u32,
        pub id: u32,
    }

    #[vox::service]
    pub trait EventSource {
        async fn produce(&self, events: vox::Tx<Event>) -> u32;
    }
}

#[derive(Clone)]
struct ChannelTxV2Service;

impl channel_tx_v2::EventSource for ChannelTxV2Service {
    async fn produce(&self, events: vox::Tx<channel_tx_v2::Event>) -> u32 {
        events
            .send(channel_tx_v2::Event {
                priority: 99,
                id: 7,
            })
            .await
            .expect("channel send should succeed");
        1
    }
}

// r[verify schema.interaction.channels]
// r[verify schema.exchange.channels]
// r[verify schema.exchange.channels.tx-args]
// r[verify rpc.channel.pair.binding-propagation]
// r[verify rpc.channel.pair.rx-take]
#[tokio::test]
async fn tx_channel_items_use_callee_writer_schema() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller =
            acceptor_conduit(server_conduit, test_acceptor_handshake("EventSource"))
                .on_connection(channel_tx_v2::EventSourceDispatcher::new(
                    ChannelTxV2Service,
                ))
                .establish::<channel_tx_v2::EventSourceClient>()
                .await
                .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("EventSource"))
        .establish::<channel_tx_v1::EventSourceClient>()
        .await
        .expect("client handshake failed");

    let (tx, mut rx) = vox::channel::<channel_tx_v1::Event>();
    let ack = client.produce(tx).await.expect("call should succeed");
    assert_eq!(ack, 1);

    let event = rx
        .recv()
        .await
        .expect("channel receive should succeed")
        .expect("expected one event");
    assert_eq!(event.get().id, 7);

    server_task.abort();
}

// ============================================================================
// Compatible: field reorder
// ============================================================================

mod reordered_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Pair {
        pub first: String,
        pub second: u32,
    }

    #[vox::service]
    pub trait PairService {
        async fn echo(&self, p: Pair) -> Pair;
    }
}

mod reordered_v2 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Pair {
        pub second: u32,
        pub first: String,
    }

    #[vox::service]
    pub trait PairService {
        async fn echo(&self, p: Pair) -> Pair;
    }
}

#[derive(Clone)]
struct PairEchoV1;

impl reordered_v1::PairService for PairEchoV1 {
    async fn echo(&self, p: reordered_v1::Pair) -> reordered_v1::Pair {
        p
    }
}

#[tokio::test]
async fn reordered_fields_are_matched_by_name() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller =
            acceptor_conduit(server_conduit, test_acceptor_handshake("PairService"))
                .on_connection(reordered_v1::PairServiceDispatcher::new(PairEchoV1))
                .establish::<reordered_v1::PairServiceClient>()
                .await
                .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("PairService"))
        .establish::<reordered_v2::PairServiceClient>()
        .await
        .expect("client handshake failed");

    // Client sends V2 Pair {second, first}. Server receives as V1 Pair {first, second}.
    let result = client
        .echo(reordered_v2::Pair {
            second: 42,
            first: "hello".into(),
        })
        .await
        .expect("call should succeed");

    assert_eq!(result.second, 42);
    assert_eq!(result.first, "hello");

    server_task.abort();
}

// ============================================================================
// Compatible: combined add + remove + reorder
// ============================================================================

mod evolved_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Config {
        pub name: String,
        pub timeout_ms: u64,
        pub retries: u32,
    }

    #[vox::service]
    pub trait ConfigService {
        async fn get(&self) -> Config;
    }
}

mod evolved_v2 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Config {
        pub retries: u32,
        pub name: String,
        // timeout_ms removed, priority added with default
        #[facet(default)]
        pub priority: u32,
    }

    #[vox::service]
    pub trait ConfigService {
        async fn get(&self) -> Config;
    }
}

#[derive(Clone)]
struct ConfigServiceV1;

impl evolved_v1::ConfigService for ConfigServiceV1 {
    async fn get(&self) -> evolved_v1::Config {
        evolved_v1::Config {
            name: "prod".into(),
            timeout_ms: 5000,
            retries: 3,
        }
    }
}

#[tokio::test]
async fn evolved_schema_combined_changes() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller =
            acceptor_conduit(server_conduit, test_acceptor_handshake("ConfigService"))
                .on_connection(evolved_v1::ConfigServiceDispatcher::new(ConfigServiceV1))
                .establish::<evolved_v1::ConfigServiceClient>()
                .await
                .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("ConfigService"))
        .establish::<evolved_v2::ConfigServiceClient>()
        .await
        .expect("client handshake failed");

    // V1 server returns {name, timeout_ms, retries}.
    // V2 client reads as {retries, name, priority} — reordered, timeout_ms skipped, priority defaults.
    let result = client.get().await.expect("call should succeed");

    assert_eq!(result.name, "prod");
    assert_eq!(result.retries, 3);
    assert_eq!(result.priority, 0); // default

    server_task.abort();
}

// ============================================================================
// Incompatible: missing required field without default
// ============================================================================

/// Old daemon: only has basic fields.
mod status_old {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct DaemonStatus {
        pub uptime_ms: u64,
        pub listen: String,
    }

    #[vox::service]
    pub trait Daemon {
        async fn status(&self) -> DaemonStatus;
        async fn ping(&self) -> u32;
    }
}

/// New client: expects additional required fields that the old daemon doesn't have.
mod status_new {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct DaemonStatus {
        pub uptime_ms: u64,
        pub listen: String,
        pub pid: u32,                // required, no default — incompatible
        pub executable_path: String, // required, no default — incompatible
    }

    #[vox::service]
    pub trait Daemon {
        async fn status(&self) -> DaemonStatus;
        async fn ping(&self) -> u32;
    }
}

#[derive(Clone)]
struct OldDaemonService;

impl status_old::Daemon for OldDaemonService {
    async fn status(&self) -> status_old::DaemonStatus {
        status_old::DaemonStatus {
            uptime_ms: 12345,
            listen: "local:///tmp/daemon.vox".into(),
        }
    }

    async fn ping(&self) -> u32 {
        42
    }
}

// Incompatible request args: missing required field without default
mod command_old {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Config {
        pub limit: u32,
    }

    #[vox::service]
    pub trait Control {
        async fn configure(&self, config: Config) -> u32;
        async fn ping(&self) -> u32;
    }
}

mod command_new {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Config {
        pub limit: u32,
        pub mode: String,
    }

    #[vox::service]
    pub trait Control {
        async fn configure(&self, config: Config) -> u32;
        async fn ping(&self) -> u32;
    }
}

#[derive(Clone)]
struct NewControlService;

impl command_new::Control for NewControlService {
    async fn configure(&self, config: command_new::Config) -> u32 {
        config.limit + config.mode.len() as u32
    }

    async fn ping(&self) -> u32 {
        7
    }
}

// r[verify schema.errors.call-level]
// r[verify schema.errors.call-level.callee]
#[tokio::test]
async fn callee_args_schema_error_is_call_level() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake("Control"))
            .on_connection(command_new::ControlDispatcher::new(NewControlService))
            .establish::<command_new::ControlClient>()
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("Control"))
        .establish::<command_old::ControlClient>()
        .await
        .expect("client handshake failed");

    let err = client
        .configure(command_old::Config { limit: 5 })
        .await
        .expect_err("callee should reject incompatible request args");
    assert!(
        matches!(&err, VoxError::InvalidPayload(msg) if msg.contains("Incompatible")),
        "expected InvalidPayload with a schema-incompatibility failure, got: {err:?}"
    );

    let pong = client
        .ping()
        .await
        .expect("connection should remain open after callee decode failure");
    assert_eq!(pong, 7);

    server_task.abort();
}

// r[verify schema.errors.same-peer-terminal]
// r[verify schema.errors.call-level]
// r[verify schema.errors.call-level.caller]
// r[verify rpc.fallible.vox-error.outcome]
#[tokio::test]
async fn missing_required_field_is_same_peer_terminal() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let _server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake("Daemon"))
            .on_connection(status_old::DaemonDispatcher::new(OldDaemonService))
            .establish::<status_old::DaemonClient>()
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let client = initiator_conduit(client_conduit, test_initiator_handshake("Daemon"))
        .establish::<status_new::DaemonClient>()
        .await
        .expect("client handshake failed");

    // New client calls old daemon. The response has DaemonStatus with only
    // {uptime_ms, listen}, but the client expects {uptime_ms, listen, pid,
    // executable_path}. The phon compat decode rejects the missing required
    // (non-default) reader fields up front.
    let err = client.status().await.expect_err("call should fail");

    // The error must be InvalidPayload (schema compatibility failure).
    assert!(
        matches!(&err, VoxError::InvalidPayload(msg) if msg.contains("Incompatible")),
        "expected InvalidPayload with a schema-incompatibility failure, got: {err:?}"
    );

    // And it must not be classified as a session interruption.
    assert!(
        !err.is_session_interruption(),
        "schema incompatibility must be terminal for the current peer schema"
    );

    let pong = client
        .ping()
        .await
        .expect("connection should remain open after caller decode failure");
    assert_eq!(pong, 42);

    server_task.abort();
}
