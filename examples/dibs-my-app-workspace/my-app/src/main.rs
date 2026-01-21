//! my-app: WebSocket server exposing SquelService for admin UI.
//!
//! This binary serves as the main application server, providing:
//! - WebSocket endpoint for roam RPC (SquelService)
//! - Schema introspection and CRUD operations for all registered tables

use dibs::SquelServiceImpl;
use dibs_proto::SquelServiceDispatcher;
use roam_stream::HandshakeConfig;
use roam_websocket::{WsTransport, ws_accept};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

// Import my-app-db to register its tables via inventory
use my_app_db as _;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
        .parse()?;

    let listener = TcpListener::bind(addr).await?;
    println!("SquelService listening on ws://{}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        println!("New connection from {}", peer_addr);

        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    let transport = WsTransport::new(ws_stream);
                    let dispatcher = SquelServiceDispatcher::new(SquelServiceImpl::new());

                    match ws_accept(transport, HandshakeConfig::default(), dispatcher).await {
                        Ok((handle, _incoming, driver)) => {
                            println!("Roam handshake complete with {}", peer_addr);

                            // Run the driver to completion
                            if let Err(e) = driver.run().await {
                                eprintln!("Connection error with {}: {}", peer_addr, e);
                            }

                            drop(handle);
                            println!("Connection closed: {}", peer_addr);
                        }
                        Err(e) => {
                            eprintln!("Handshake failed with {}: {}", peer_addr, e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("WebSocket upgrade failed for {}: {}", peer_addr, e);
                }
            }
        });
    }
}
