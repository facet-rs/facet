//! WebSocket peer server for testing TypeScript clients.
//!
//! This is a full vox implementation that TypeScript browser tests can
//! connect to. It uses the vox runtime (dispatcher, channels, etc.) to
//! provide a real vox peer for the TypeScript client to talk to.

use spec_proto::TestbedDispatcher;
use std::env;
use subject_rust::TestbedService;
use tokio::net::TcpListener;
use vox_core::acceptor_transport;
use vox_websocket::WsLink;

const PEER_SERVER_RUNTIME_STACK_BYTES: usize = 32 * 1024 * 1024;

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(PEER_SERVER_RUNTIME_STACK_BYTES)
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;
    rt.block_on(run())
}

async fn run() -> Result<(), String> {
    let port = env::var("WS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    eprintln!("WebSocket server listening on ws://{}", addr);

    // Print port on stdout for Playwright to parse
    println!("{}", port);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Accept error: {}", e);
                continue;
            }
        };

        eprintln!("New connection from {}", peer);

        tokio::spawn(async move {
            let ws_link = match WsLink::server(stream).await {
                Ok(link) => link,
                Err(e) => {
                    eprintln!("WebSocket handshake failed: {}", e);
                    return;
                }
            };

            let connection = match acceptor_transport(ws_link)
                .on_connection(TestbedDispatcher::new(TestbedService))
                .establish_connection()
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Session handshake failed: {:?}", e);
                    return;
                }
            };

            eprintln!("Connection established with {}", peer);
            connection.closed().await;
        });
    }
}
