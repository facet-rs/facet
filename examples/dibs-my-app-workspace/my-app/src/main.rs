//! my-app: WebSocket server exposing SquelService for admin UI.
//!
//! This binary serves as the main application server, providing:
//! - WebSocket endpoint for vox RPC (SquelService)
//! - Schema introspection and CRUD operations for all registered tables

use dibs::SquelServiceImpl;
use dibs_proto::SquelServiceDispatcher;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_postgres::NoTls;
use tokio_tungstenite::accept_async;
use vox_websocket::WsLink;

// Import my-app-db to register its tables via inventory
use my_app_db as _;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    // Connect to the database
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/my_app".to_string());
    let (client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;
    let client = Arc::new(client);

    // Spawn the connection task
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
        .parse()?;

    let listener = TcpListener::bind(addr).await?;
    println!("SquelService listening on ws://{}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        println!("New connection from {}", peer_addr);

        let client = client.clone();
        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    let link = WsLink::new(ws_stream);
                    let dispatcher = SquelServiceDispatcher::new(SquelServiceImpl::new(client));

                    match vox::acceptor_on(link)
                        .on_connection(dispatcher)
                        .establish::<vox::NoopClient>()
                        .await
                    {
                        Ok(client) => {
                            println!("Vox handshake complete with {}", peer_addr);

                            let _ = client.caller.closed().await;
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
