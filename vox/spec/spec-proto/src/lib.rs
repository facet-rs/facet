#![deny(unsafe_code)]

use roam::service;
use roam::session::{Pull, Push};

/// Simple echo service for conformance testing.
#[service]
pub trait Echo {
    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;
}

/// Streaming service for cross-language conformance testing.
///
/// Tests Push/Pull semantics, stream lifecycle, and bidirectional streaming.
#[service]
pub trait Streaming {
    /// Client pushes numbers, server returns their sum.
    ///
    /// Tests: client-to-server streaming (`Push<T>` → scalar return).
    /// r[impl streaming.client-to-server] - Client sends stream, server returns scalar.
    async fn sum(&self, numbers: Pull<i32>) -> i64;

    /// Client sends a count, server returns that many numbers.
    ///
    /// Tests: server-to-client streaming (scalar → `Pull<T>`).
    /// r[impl streaming.server-to-client] - Client sends scalar, server returns stream.
    async fn range(&self, count: u32) -> Push<u32>;

    /// Client pushes strings, server echoes each back.
    ///
    /// Tests: bidirectional streaming (`Push<T>` ↔ `Pull<T>`).
    /// r[impl streaming.bidirectional] - Both sides stream simultaneously.
    async fn pipe(&self, input: Pull<String>) -> Push<String>;

    /// Client pushes numbers, server returns (sum, count, average).
    ///
    /// Tests: aggregating a stream into a compound result.
    async fn stats(&self, numbers: Pull<i32>) -> (i64, u64, f64);
}

pub fn all_services() -> Vec<roam::schema::ServiceDetail> {
    vec![echo_service_detail(), streaming_service_detail()]
}
