//! Basic example demonstrating rapace RPC.
//!
//! This example shows:
//! - Defining a service with `#[rapace::service]`
//! - Implementing the service
//! - Creating a client and server using the in-memory transport
//! - Making unary and streaming RPC calls
//!
//! Run with: `cargo run --example basic -p rapace`

use std::sync::Arc;

use rapace::prelude::*;

// Define a calculator service with the #[rapace::service] attribute.
// This generates:
// - `CalculatorClient<T>` - client stub with async methods
// - `CalculatorServer<S>` - server dispatcher
// - `calculator_methods` module with METHOD_ID_* constants
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an in-memory transport pair (client <-> server)
    let (client_transport, server_transport) = rapace::InProcTransport::pair();
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    // Create the server
    let server = CalculatorServer::new(CalculatorImpl);

    // Spawn the server loop to handle requests
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            loop {
                // Receive request frame
                let request = match server_transport.recv_frame().await {
                    Ok(frame) => frame,
                    Err(e) => {
                        eprintln!("Server recv error: {}", e);
                        break;
                    }
                };

                // Dispatch to the appropriate method
                // dispatch_streaming handles both unary and streaming methods
                if let Err(e) = server
                    .dispatch_streaming(
                        request.desc.method_id,
                        request.payload,
                        server_transport.as_ref(),
                    )
                    .await
                {
                    eprintln!("Server dispatch error: {}", e);
                }
            }
        }
    });

    // Create the client
    let client = CalculatorClient::new(client_transport.clone());

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
    client_transport.close().await?;
    server_handle.abort();

    println!("\nDone!");
    Ok(())
}
