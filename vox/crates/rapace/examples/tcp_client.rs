//! TCP client example demonstrating rapace RPC over TCP.
//!
//! This example shows:
//! - Connecting to a TCP server
//! - Making RPC calls over the network
//!
//! First start the server: `cargo run --example tcp_server -p rapace --features stream`
//! Then run the client: `cargo run --example tcp_client -p rapace --features stream`

use std::sync::Arc;

use rapace::prelude::*;
use rapace_core::RpcSession;
use tokio::net::TcpStream;

// Define the same calculator service (needed for the generated client)
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, a: i32, b: i32) -> i32;
    async fn range(&self, n: u32) -> Streaming<u32>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9000";
    println!("Connecting to {}...", addr);

    // Connect to the server
    let stream = TcpStream::connect(addr).await?;
    println!("Connected!");

    // Wrap in transport
    let transport = Arc::new(rapace::StreamTransport::new(stream));

    // Wrap in an RPC session
    let session = RpcSession::new(transport.clone());

    // Create client
    let client = CalculatorClient::new(Arc::new(session));

    // Make RPC calls
    println!("\nCalling add(10, 20)...");
    let sum = client.add(10, 20).await?;
    println!("  Result: {}", sum);

    println!("\nCalling multiply(6, 7)...");
    let product = client.multiply(6, 7).await?;
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

    // Close connection
    transport.close().await?;
    println!("\nDone!");

    Ok(())
}
