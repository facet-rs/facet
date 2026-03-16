//! Schema compatibility tests: two versions of a service talk to each other.
//!
//! Each test pair defines a "v1" and "v2" module with the same trait name
//! and method names but different field types. Because method IDs are
//! name-only (not type-dependent), the two versions route to the same
//! handler. Schema exchange sends type metadata before payloads, and
//! translation plans handle the schema differences.

use roam_core::{BareConduit, MemoryLink, acceptor, initiator_conduit, memory_link_pair};
use roam_types::MessageFamily;

type MessageConduit = BareConduit<MessageFamily, MemoryLink>;

fn conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
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

    #[roam::service]
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

    #[roam::service]
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

// r[verify schema.translation.fill-defaults]
// r[verify schema.interaction.channels]
#[tokio::test]
async fn v1_client_v2_server_fills_default() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let (_server_caller, _sh) = acceptor(server_conduit)
            .establish::<point_v2::GeometryClient>(point_v2::GeometryDispatcher::new(
                V2GeometryService,
            ))
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit)
        .establish::<point_v1::GeometryClient>(())
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

// r[verify schema.translation.skip-unknown]
#[tokio::test]
async fn v2_client_v1_server_skips_unknown_field() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let (_server_caller, _sh) = acceptor(server_conduit)
            .establish::<point_v1::GeometryClient>(point_v1::GeometryDispatcher::new(
                V1GeometryService,
            ))
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit)
        .establish::<point_v2::GeometryClient>(())
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
// Compatible: field reorder
// ============================================================================

mod reordered_v1 {
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct Pair {
        pub first: String,
        pub second: u32,
    }

    #[roam::service]
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

    #[roam::service]
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

// r[verify schema.translation.reorder]
#[tokio::test]
async fn reordered_fields_are_matched_by_name() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let (_server_caller, _sh) = acceptor(server_conduit)
            .establish::<reordered_v1::PairServiceClient>(reordered_v1::PairServiceDispatcher::new(
                PairEchoV1,
            ))
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit)
        .establish::<reordered_v2::PairServiceClient>(())
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

    #[roam::service]
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

    #[roam::service]
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

// r[verify schema.translation.fill-defaults]
// r[verify schema.translation.skip-unknown]
// r[verify schema.translation.reorder]
#[tokio::test]
async fn evolved_schema_combined_changes() {
    let (client_conduit, server_conduit) = conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let (_server_caller, _sh) = acceptor(server_conduit)
            .establish::<evolved_v1::ConfigServiceClient>(evolved_v1::ConfigServiceDispatcher::new(
                ConfigServiceV1,
            ))
            .await
            .expect("server handshake failed");
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit)
        .establish::<evolved_v2::ConfigServiceClient>(())
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
