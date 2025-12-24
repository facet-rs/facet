//! TCP server example demonstrating rapace RPC over TCP.
//!
//! This example shows:
//! - Running a service over TCP
//! - Using `server.serve()` with a TCP stream
//!
//! Run the server with: `cargo run --example tcp_server -p rapace --features stream`
//! Then connect with a client (see tcp_client example).

use rapace::prelude::*;
use tokio::net::TcpListener;

// Define the same calculator service as in basic.rs
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, a: i32, b: i32) -> i32;
    async fn range(&self, n: u32) -> Streaming<u32>;
}

struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        println!("  add({}, {}) called", a, b);
        a + b
    }

    async fn multiply(&self, a: i32, b: i32) -> i32 {
        println!("  multiply({}, {}) called", a, b);
        a * b
    }

    async fn range(&self, n: u32) -> Streaming<u32> {
        println!("  range({}) called", n);
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0..n {
                if tx.send(Ok(i)).await.is_err() {
                    break;
                }
            }
        });
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
    let addr = "127.0.0.1:9000";
    let listener = TcpListener::bind(addr).await?;
    println!("Calculator server listening on {}", addr);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        println!("New connection from {}", peer_addr);

        tokio::spawn(async move {
            // Create transport from the TCP stream
            let transport = rapace::Transport::stream(socket);

            // Create server and serve requests
            let server = CalculatorServer::new(CalculatorImpl);
            if let Err(e) = server.serve(transport).await {
                eprintln!("Connection error from {}: {}", peer_addr, e);
            }
            println!("Connection from {} closed", peer_addr);
        });
    }
}
