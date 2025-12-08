//! rapace-testkit: Conformance test suite for rapace transports.
//!
//! Provides `TransportFactory` trait and shared test scenarios that all
//! transports must pass.
//!
//! # Usage
//!
//! Each transport crate implements `TransportFactory` and runs the shared tests:
//!
//! ```ignore
//! use rapace_testkit::{TransportFactory, TestError};
//!
//! struct MyTransportFactory;
//!
//! impl TransportFactory for MyTransportFactory {
//!     type Transport = MyTransport;
//!
//!     fn connect_pair() -> impl Future<Output = Result<(Self::Transport, Self::Transport), TestError>> + Send {
//!         async { /* create connected pair */ }
//!     }
//! }
//!
//! #[tokio::test]
//! async fn my_transport_unary_happy_path() {
//!     rapace_testkit::run_unary_happy_path::<MyTransportFactory>().await;
//! }
//! ```

use std::future::Future;
use std::sync::Arc;

use rapace_core::Transport;

/// Error type for test scenarios.
#[derive(Debug)]
pub enum TestError {
    /// Transport creation failed.
    Setup(String),
    /// RPC call failed.
    Rpc(rapace_core::RpcError),
    /// Transport error.
    Transport(rapace_core::TransportError),
    /// Assertion failed.
    Assertion(String),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Setup(msg) => write!(f, "setup error: {}", msg),
            TestError::Rpc(e) => write!(f, "RPC error: {}", e),
            TestError::Transport(e) => write!(f, "transport error: {}", e),
            TestError::Assertion(msg) => write!(f, "assertion failed: {}", msg),
        }
    }
}

impl std::error::Error for TestError {}

impl From<rapace_core::RpcError> for TestError {
    fn from(e: rapace_core::RpcError) -> Self {
        TestError::Rpc(e)
    }
}

impl From<rapace_core::TransportError> for TestError {
    fn from(e: rapace_core::TransportError) -> Self {
        TestError::Transport(e)
    }
}

/// Factory trait for creating transport pairs for testing.
///
/// Each transport implementation provides a factory that creates connected
/// pairs of transports for testing.
pub trait TransportFactory: Send + Sync + 'static {
    /// The transport type being tested.
    type Transport: Transport + Send + Sync + 'static;

    /// Create a connected pair of transports.
    ///
    /// Returns (client_side, server_side) where frames sent from client
    /// are received by server and vice versa.
    fn connect_pair() -> impl Future<Output = Result<(Self::Transport, Self::Transport), TestError>> + Send;
}

// ============================================================================
// Test service: Adder
// ============================================================================

/// Simple arithmetic service used for testing.
#[rapace_macros::service]
pub trait Adder {
    /// Add two numbers.
    async fn add(&self, a: i32, b: i32) -> i32;
}

/// Implementation of the Adder service for testing.
pub struct AdderImpl;

impl Adder for AdderImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

// ============================================================================
// Test scenarios
// ============================================================================

/// Run a single unary RPC call and verify the result.
///
/// This is the most basic test: client calls `add(2, 3)` and expects `5`.
pub async fn run_unary_happy_path<F: TransportFactory>() {
    let result = run_unary_happy_path_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_unary_happy_path failed: {}", e);
    }
}

async fn run_unary_happy_path_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = AdderServer::new(AdderImpl);

    // Spawn server task to handle one request
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;
            let response = server
                .dispatch(request.desc.method_id, request.payload)
                .await
                .map_err(TestError::Rpc)?;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client and make call
    let client = AdderClient::new(client_transport);
    let result = client.add(2, 3).await?;

    if result != 5 {
        return Err(TestError::Assertion(format!(
            "expected add(2, 3) = 5, got {}",
            result
        )));
    }

    // Wait for server to finish
    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Run multiple unary RPC calls sequentially.
///
/// Verifies that the transport correctly handles multiple request/response pairs.
pub async fn run_unary_multiple_calls<F: TransportFactory>() {
    let result = run_unary_multiple_calls_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_unary_multiple_calls failed: {}", e);
    }
}

async fn run_unary_multiple_calls_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = AdderServer::new(AdderImpl);

    // Spawn server task to handle multiple requests
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            for _ in 0..3 {
                let request = server_transport.recv_frame().await?;
                let response = server
                    .dispatch(request.desc.method_id, request.payload)
                    .await
                    .map_err(TestError::Rpc)?;
                server_transport.send_frame(&response).await?;
            }
            Ok::<_, TestError>(())
        }
    });

    let client = AdderClient::new(client_transport);

    // Multiple calls with different values
    let test_cases = [(1, 2, 3), (10, 20, 30), (-5, 5, 0)];

    for (a, b, expected) in test_cases {
        let result = client.add(a, b).await?;
        if result != expected {
            return Err(TestError::Assertion(format!(
                "expected add({}, {}) = {}, got {}",
                a, b, expected, result
            )));
        }
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}
