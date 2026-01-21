//! TCP echo example demonstrating peer-to-peer connection.
//!
//! Run with: cargo run --example tcp_echo
//!
//! This spawns two tasks: one accepts a connection, one connects.
//! They perform handshake and then the initiator sends a message.

use roam_stream::{ConnectionError, Connector, HandshakeConfig, NoDispatcher, accept, connect};
use tokio::net::{TcpListener, TcpStream};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    println!("Listening on {addr}");

    // Spawn the acceptor
    let acceptor = tokio::spawn(async move { run_acceptor(listener).await });

    // Give listener time to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Run the initiator using connect()
    run_initiator(addr).await?;

    // Wait for acceptor to finish
    acceptor.await?.map_err(|e| format!("{e:?}"))?;

    println!("Done!");
    Ok(())
}

async fn run_acceptor(listener: TcpListener) -> Result<(), ConnectionError> {
    let (stream, peer_addr) = listener.accept().await?;
    println!("Accepted connection from {peer_addr}");

    // Use accept() - no reconnection for accepted connections
    let (handle, _incoming, driver) =
        accept(stream, HandshakeConfig::default(), NoDispatcher).await?;
    println!("Acceptor: handshake complete");

    // Spawn the driver
    let driver_handle = tokio::spawn(driver.run());

    // Wait for the driver to finish (peer will disconnect)
    let _ = driver_handle.await;

    // handle is available for making calls if needed
    let _ = handle;

    Ok(())
}

/// Connector for TCP streams.
struct TcpConnector {
    addr: std::net::SocketAddr,
}

impl Connector for TcpConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> std::io::Result<TcpStream> {
        TcpStream::connect(self.addr).await
    }
}

async fn run_initiator(
    addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Connecting to {addr}");

    // Use connect() - automatic reconnection built-in
    let connector = TcpConnector { addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Get the handle to make a raw call
    let handle = client.handle().await.map_err(|e| format!("{e}"))?;
    println!("Initiator: connected and handshake complete");

    // For this example, we just verify the connection works
    // In real usage, you'd do: let service = MyServiceClient::new(client);
    println!("Initiator: connection established");
    let _ = handle;

    // The client will be dropped here, closing the connection
    Ok(())
}
