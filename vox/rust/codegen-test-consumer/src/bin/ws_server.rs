//! WebSocket server for browser testing.
//!
//! Serves the Calculator service over WebSocket on port 9000.

use codegen_test_consumer::calculator::{CalculatorDispatcher, CalculatorHandler};
use roam::session::{Pull, Push};
use roam_stream::Hello;
use roam_websocket::{WsTransport, ws_accept};
use std::env;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

struct Calculator;

#[allow(clippy::manual_async_fn)]
impl CalculatorHandler for Calculator {
    fn add(
        &self,
        a: i32,
        b: i32,
    ) -> impl std::future::Future<Output = Result<i32, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(a + b) }
    }

    fn multiply(
        &self,
        a: i32,
        b: i32,
    ) -> impl std::future::Future<Output = Result<i32, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(a * b) }
    }

    fn sum_stream(
        &self,
        _numbers: Pull<i32>,
    ) -> impl std::future::Future<Output = Result<i64, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(0) } // Stub for now
    }

    fn range(
        &self,
        _count: u32,
        _output: Push<u32>,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(()) } // Stub for now
    }
}

#[tokio::main]
async fn main() {
    let port = env::var("WS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr).await.unwrap();
    eprintln!("WebSocket server listening on ws://{}", addr);

    // Print port on stdout for Playwright to parse
    println!("{}", port);

    let hello = Hello::V1 {
        max_payload_size: 1024 * 1024,
        initial_stream_credit: 64 * 1024,
    };

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Accept error: {}", e);
                continue;
            }
        };

        eprintln!("New connection from {}", peer);

        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("WebSocket handshake failed: {}", e);
                continue;
            }
        };

        let transport = WsTransport::new(ws_stream);
        let hello = hello.clone();

        tokio::spawn(async move {
            let dispatcher = CalculatorDispatcher::new(Calculator);
            match ws_accept(transport, hello).await {
                Ok(mut conn) => {
                    eprintln!("Connection established with {}", peer);
                    if let Err(e) = conn.run(&dispatcher).await {
                        eprintln!("Connection error: {:?}", e);
                    }
                    eprintln!("Connection closed: {}", peer);
                }
                Err(e) => {
                    eprintln!("Hello exchange failed: {:?}", e);
                }
            }
        });
    }
}
