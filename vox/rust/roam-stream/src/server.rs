//! TCP server for accepting roam connections.

use roam_wire::Hello;
use tokio::net::{TcpListener, TcpStream};

use crate::connection::{Connection, ConnectionError, ServiceDispatcher, hello_exchange_acceptor};
use crate::framing::CobsFramed;

/// Type alias for TCP-based connections.
pub type TcpConnection = Connection<TcpStream>;

/// Configuration for the server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Our Hello message to send to peers.
    pub hello: Hello,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hello: Hello::V1 {
                max_payload_size: 1024 * 1024,
                initial_stream_credit: 64 * 1024,
            },
        }
    }
}

/// A TCP server that accepts roam connections.
pub struct Server {
    config: ServerConfig,
}

impl Server {
    /// Create a new server with default configuration.
    pub fn new() -> Self {
        Self {
            config: ServerConfig::default(),
        }
    }

    /// Create a server from environment variables.
    ///
    /// Looks for `PEER_ADDR` to connect to (for subject mode).
    pub fn from_env() -> Self {
        Self::new()
    }

    /// Set the Hello message configuration.
    pub fn with_hello(mut self, hello: Hello) -> Self {
        self.config.hello = hello;
        self
    }

    /// Accept a single connection from a listener.
    pub async fn accept(&self, listener: &TcpListener) -> Result<TcpConnection, ConnectionError> {
        let (stream, _addr) = listener.accept().await?;
        self.handshake(stream).await
    }

    /// Connect to a peer address and perform handshake as initiator.
    pub async fn connect(&self, addr: &str) -> Result<TcpConnection, ConnectionError> {
        let stream = TcpStream::connect(addr).await?;
        let io = CobsFramed::new(stream);
        crate::connection::hello_exchange_initiator(io, self.config.hello.clone()).await
    }

    /// Perform handshake on an accepted connection.
    async fn handshake(&self, stream: TcpStream) -> Result<TcpConnection, ConnectionError> {
        let io = CobsFramed::new(stream);
        hello_exchange_acceptor(io, self.config.hello.clone()).await
    }

    /// Run a single connection with a dispatcher.
    ///
    /// This connects to the peer (from PEER_ADDR env var), performs handshake,
    /// and runs the message loop until the connection closes.
    pub async fn run_subject<D>(&self, dispatcher: &D) -> Result<(), ConnectionError>
    where
        D: ServiceDispatcher,
    {
        let addr = std::env::var("PEER_ADDR")
            .map_err(|_| ConnectionError::Dispatch("PEER_ADDR env var not set".into()))?;

        let mut conn = self.connect(&addr).await?;
        conn.run(dispatcher).await
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
