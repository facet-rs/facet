//! Basic example demonstrating rapace RPC.
//!
//! This example shows:
//! - Defining a service with `#[rapace::service]`
//! - Implementing the service
//! - Using `server.serve()` for the server loop
//! - Making unary and streaming RPC calls
//!
//! Run with: `cargo run --example basic -p rapace`

use std::sync::Arc;

use rapace::prelude::*;
use rapace_core::RpcSession;

// Define a calculator service with the #[rapace::service] attribute.
// This generates:
// - `CalculatorClient<T>` - client stub with async methods
// - `CalculatorServer<S>` - server dispatcher with serve() method
// - `CALCULATOR_METHOD_ID_*` constants and a `calculator_register` helper
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Calculator {
    /// Add two numbers (unary RPC).
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Multiply two numbers (unary RPC).
    async fn multiply(&self, a: i32, b: i32) -> i32;

    /// Generate numbers from 0 to n-1 (server-streaming RPC).
    async fn range(&self, n: u32) -> Streaming<u32>;
}

// Implement the service
struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }

    async fn range(&self, n: u32) -> Streaming<u32> {
        // Create a channel for streaming results
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        // Spawn a task to produce values
        tokio::spawn(async move {
            for i in 0..n {
                if tx.send(Ok(i)).await.is_err() {
                    break; // Client disconnected
                }
            }
        });

        // Return the stream
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an in-memory transport pair (client <-> server)
    let (client_transport, server_transport) = rapace::Transport::mem_pair();
    let client_transport = client_transport;
    let server_transport = server_transport;

    // Create the server and spawn it
    // The serve() method handles the frame loop automatically
    let server = CalculatorServer::new(CalculatorImpl);
    let server_handle = tokio::spawn(server.serve(server_transport));

    // Wrap in an RPC session
    let session = Arc::new(RpcSession::new(client_transport.clone()));

    // Spawn the session demux loop to route responses back to clients
    let session_clone = session.clone();
    tokio::spawn(async move {
        if let Err(e) = session_clone.run().await {
            eprintln!("Session error: {}", e);
        }
    });

    // Create the client
    let client = CalculatorClient::new(session);

    // Make some RPC calls
    println!("Calling add(2, 3)...");
    let sum = client.add(2, 3).await?;
    println!("  Result: {}", sum);

    println!("\nCalling multiply(4, 5)...");
    let product = client.multiply(4, 5).await?;
    println!("  Result: {}", product);

    println!("\nCalling range(5)...");
    let mut stream = client.range(5).await?;

    use tokio_stream::StreamExt;
    print!("  Stream items: ");
    while let Some(item) = stream.next().await {
        match item {
            Ok(n) => print!("{} ", n),
            Err(e) => eprintln!("Stream error: {}", e),
        }
    }
    println!();

    // Graceful shutdown
    client_transport.close();
    server_handle.abort();

    println!("\nDone!");
    Ok(())
}
