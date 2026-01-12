//! Unix socket echo example demonstrating peer-to-peer connection.
//!
//! Run with: cargo run --example unix_echo
//!
//! This spawns two tasks: one accepts a connection, one connects.
//! They perform handshake and then the initiator sends a message.

use std::path::PathBuf;

use roam_stream::{ConnectionError, Connector, HandshakeConfig, NoDispatcher, accept, connect};
use tokio::net::{UnixListener, UnixStream};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Use a temp file for the socket
    let socket_path =
        std::env::temp_dir().join(format!("roam-example-{}.sock", std::process::id()));

    // Clean up any leftover socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    println!("Listening on {}", socket_path.display());

    // Spawn the acceptor
    let acceptor = tokio::spawn(async move { run_acceptor(listener).await });

    // Give listener time to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Run the initiator
    run_initiator(&socket_path).await?;

    // Wait for acceptor to finish
    acceptor.await?.map_err(|e| format!("{e:?}"))?;

    // Clean up socket
    let _ = std::fs::remove_file(&socket_path);

    println!("Done!");
    Ok(())
}

async fn run_acceptor(listener: UnixListener) -> Result<(), ConnectionError> {
    let (stream, _) = listener.accept().await?;
    println!("Accepted connection");

    // Use accept() - no reconnection for accepted connections
    let (handle, driver) = accept(stream, HandshakeConfig::default(), NoDispatcher).await?;
    println!("Acceptor: handshake complete");

    // Spawn the driver
    let driver_handle = tokio::spawn(driver.run());

    // Wait for the driver to finish
    let _ = driver_handle.await;
    let _ = handle;

    Ok(())
}

/// Connector for Unix sockets.
struct UnixConnector {
    path: PathBuf,
}

impl Connector for UnixConnector {
    type Transport = UnixStream;

    async fn connect(&self) -> std::io::Result<UnixStream> {
        UnixStream::connect(&self.path).await
    }
}

async fn run_initiator(
    socket_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Connecting to {}", socket_path.display());

    // Use connect() - automatic reconnection built-in
    let connector = UnixConnector {
        path: socket_path.to_path_buf(),
    };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Get the handle
    let handle = client.handle().await.map_err(|e| format!("{e}"))?;
    println!("Initiator: connected and handshake complete");
    let _ = handle;

    Ok(())
}
