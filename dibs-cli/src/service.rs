//! Service connection handling for dibs CLI.
//!
//! This module handles spawning the user's db crate as a roam service
//! and connecting to it.
//!
//! # Connection Model
//!
//! The CLI acts as the TCP *server* (acceptor), and the spawned db crate
//! connects to it as a client (initiator). This avoids race conditions:
//! the CLI is already listening before spawning the child.
//!
//! Roam supports bidirectional RPC, so both sides can call each other.

use crate::config::Config;
use dibs_proto::DibsServiceClient;
use roam_stream::{ConnectionHandle, Driver, HandshakeConfig, NoDispatcher, accept};
use std::process::{Child, Command, Stdio};
use tokio::net::TcpListener;

/// A connection to the dibs service.
pub struct ServiceConnection {
    /// The roam connection handle for making calls
    handle: ConnectionHandle,
    /// The driver task handle (keeps connection alive)
    _driver: tokio::task::JoinHandle<()>,
    /// The spawned child process
    _child: Child,
}

impl ServiceConnection {
    /// Get a typed client for calling service methods.
    pub fn client(&self) -> DibsServiceClient<ConnectionHandle> {
        DibsServiceClient::new(self.handle.clone())
    }
}

/// Connect to the dibs service specified in the config.
///
/// 1. Binds to a random local port
/// 2. Spawns the db crate with `DIBS_CLI_ADDR` pointing to our listener
/// 3. Accepts the incoming connection from the child
/// 4. Returns a handle for making RPC calls
pub async fn connect_to_service(config: &Config) -> Result<ServiceConnection, ServiceError> {
    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ServiceError::Spawn(format!("Failed to bind to port: {}", e)))?;
    let addr = listener
        .local_addr()
        .map_err(|e| ServiceError::Spawn(format!("Failed to get local address: {}", e)))?;

    // Build the command to spawn the service
    let mut cmd = if let Some(binary) = &config.db.binary {
        Command::new(binary)
    } else if let Some(crate_name) = &config.db.crate_name {
        let mut cmd = Command::new("cargo");
        cmd.args(["run", "-p", crate_name, "--"]);
        cmd
    } else {
        return Err(ServiceError::Config(
            "No db.crate_name or db.binary specified in dibs.toml".to_string(),
        ));
    };

    // Tell the service where to connect
    cmd.env("DIBS_CLI_ADDR", addr.to_string());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    // Spawn the service
    let child = cmd
        .spawn()
        .map_err(|e| ServiceError::Spawn(format!("Failed to spawn db service: {}", e)))?;

    // Accept the incoming connection from the child
    // TODO: Add a timeout here
    let (stream, _peer_addr) = listener
        .accept()
        .await
        .map_err(|e| ServiceError::Connection(format!("Failed to accept connection: {}", e)))?;

    // Establish roam session (we're the acceptor)
    let (handle, driver): (ConnectionHandle, Driver<_, NoDispatcher>) =
        accept(stream, HandshakeConfig::default(), NoDispatcher)
            .await
            .map_err(|e| ServiceError::Connection(format!("Roam handshake failed: {}", e)))?;

    // Spawn the driver to handle the connection
    let driver_handle = tokio::spawn(async move {
        if let Err(e) = driver.run().await {
            eprintln!("Roam driver error: {}", e);
        }
    });

    Ok(ServiceConnection {
        handle,
        _driver: driver_handle,
        _child: child,
    })
}

/// Errors that can occur when connecting to the service.
#[derive(Debug)]
pub enum ServiceError {
    /// Configuration error
    Config(String),
    /// Failed to spawn the service
    Spawn(String),
    /// Connection error
    Connection(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Config(e) => write!(f, "Configuration error: {}", e),
            ServiceError::Spawn(e) => write!(f, "Failed to spawn service: {}", e),
            ServiceError::Connection(e) => write!(f, "Connection error: {}", e),
        }
    }
}

impl std::error::Error for ServiceError {}
