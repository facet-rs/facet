//! Simple TCP server for testing Swift client.
//!
//! Run with: cargo run -p tcp-server

use rapace::prelude::*;
use tokio::net::TcpListener;

/// Simple echo service for testing
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Echo {
    /// Echo back the input string
    async fn echo(&self, message: String) -> String;

    /// Add two numbers
    async fn add(&self, a: i32, b: i32) -> i32;
}

struct EchoImpl;

impl Echo for EchoImpl {
    async fn echo(&self, message: String) -> String {
        println!("  echo({:?}) called", message);
        format!("Echo: {}", message)
    }

    async fn add(&self, a: i32, b: i32) -> i32 {
        println!("  add({}, {}) called", a, b);
        a + b
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Print method IDs for Swift client
    println!("=== Method IDs ===");
    println!("Echo.echo: 0x{:08X} ({})", ECHO_METHOD_ID_ECHO, ECHO_METHOD_ID_ECHO);
    println!("Echo.add:  0x{:08X} ({})", ECHO_METHOD_ID_ADD, ECHO_METHOD_ID_ADD);
    println!();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9876";
    let listener = TcpListener::bind(addr).await?;
    println!("Echo server listening on {}", addr);
    println!("Waiting for Swift client...\n");

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        println!("New connection from {}", peer_addr);

        tokio::spawn(async move {
            let transport = rapace::AnyTransport::stream(socket);
            let server = EchoServer::new(EchoImpl);

            if let Err(e) = server.serve(transport).await {
                eprintln!("Connection error from {}: {}", peer_addr, e);
            }
            println!("Connection from {} closed", peer_addr);
        });
    }
}
