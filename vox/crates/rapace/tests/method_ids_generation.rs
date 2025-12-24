//! Test that validates the METHOD_IDS constant generation and cell_service! macro integration.
//!
//! This test validates the fix for issue #82, where the cell_service! macro required
//! manual method ID specification, but the #[rapace::service] macro didn't provide
//! a way to get all method IDs as a collection.

use rapace::RpcSession;

#[allow(async_fn_in_trait)]
#[rapace::service]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn subtract(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, a: i32, b: i32) -> i32;
}

struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn subtract(&self, a: i32, b: i32) -> i32 {
        a - b
    }

    async fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }
}

#[test]
fn test_method_ids_constant_exists() {
    // Verify that the METHOD_IDS constant is generated and accessible
    let method_ids = CalculatorServer::<CalculatorImpl>::METHOD_IDS;

    // Should have exactly 3 method IDs (add, subtract, multiply)
    assert_eq!(method_ids.len(), 3);

    // Verify that the individual constants match what's in the array
    assert!(method_ids.contains(&CALCULATOR_METHOD_ID_ADD));
    assert!(method_ids.contains(&CALCULATOR_METHOD_ID_SUBTRACT));
    assert!(method_ids.contains(&CALCULATOR_METHOD_ID_MULTIPLY));
}

#[test]
fn test_method_ids_are_unique() {
    // Verify that all method IDs are unique (no hash collisions within service)
    let method_ids = CalculatorServer::<CalculatorImpl>::METHOD_IDS;
    let mut sorted = method_ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    assert_eq!(
        sorted.len(),
        method_ids.len(),
        "METHOD_IDS contains duplicates!"
    );
}

#[test]
fn test_method_ids_are_deterministic() {
    // Verify that method IDs are deterministic (same every time)
    // This is important for distributed systems
    let ids1 = CalculatorServer::<CalculatorImpl>::METHOD_IDS;
    let ids2 = CalculatorServer::<CalculatorImpl>::METHOD_IDS;

    assert_eq!(ids1, ids2);
}

#[test]
fn test_individual_method_id_constants() {
    // Verify that individual method ID constants are generated correctly
    // These constants should match what's in METHOD_IDS

    // The constants should be non-zero (FNV-1a hash shouldn't be 0 for these strings)
    assert_ne!(CALCULATOR_METHOD_ID_ADD, 0);
    assert_ne!(CALCULATOR_METHOD_ID_SUBTRACT, 0);
    assert_ne!(CALCULATOR_METHOD_ID_MULTIPLY, 0);

    // All should be different
    assert_ne!(CALCULATOR_METHOD_ID_ADD, CALCULATOR_METHOD_ID_SUBTRACT);
    assert_ne!(CALCULATOR_METHOD_ID_ADD, CALCULATOR_METHOD_ID_MULTIPLY);
    assert_ne!(CALCULATOR_METHOD_ID_SUBTRACT, CALCULATOR_METHOD_ID_MULTIPLY);
}

#[tokio_test_lite::test]
async fn test_end_to_end_with_method_ids() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;

    // Create an in-memory transport pair
    let (client_transport, server_transport) = rapace::Transport::mem_pair();

    // Create server
    let server = CalculatorServer::new(CalculatorImpl);
    let server_handle = tokio::spawn(server.serve(server_transport));

    // Create client session and spawn demux loop
    let session = Arc::new(RpcSession::new(client_transport.clone()));
    let session_clone = session.clone();
    tokio::spawn(async move {
        let _ = session_clone.run().await;
    });
    let client = CalculatorClient::new(session);

    // Test that RPCs work correctly (verifying that method IDs are correct)
    let result = client.add(5, 3).await?;
    assert_eq!(result, 8);

    let result = client.subtract(10, 4).await?;
    assert_eq!(result, 6);

    let result = client.multiply(7, 6).await?;
    assert_eq!(result, 42);

    // Cleanup
    client_transport.close();
    server_handle.abort();

    Ok(())
}
