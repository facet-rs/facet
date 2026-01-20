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
use roam_stream::{ConnectionHandle, HandshakeConfig, NoDispatcher, accept};
use std::process::{Child, Command, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command as TokioCommand;

/// A connection to the dibs service.
pub struct ServiceConnection {
    /// The roam connection handle for making calls
    handle: ConnectionHandle,
    /// The driver task handle (keeps connection alive)
    _driver: tokio::task::JoinHandle<()>,
    /// The spawned child process (if held)
    _child: Option<Child>,
    /// The binary mtime (for staleness checks)
    pub binary_mtime: Option<std::time::SystemTime>,
    /// The migrations directory path
    pub migrations_dir: Option<std::path::PathBuf>,
}

impl ServiceConnection {
    /// Get a typed client for calling service methods.
    pub fn client(&self) -> DibsServiceClient<ConnectionHandle> {
        DibsServiceClient::new(self.handle.clone())
    }

    /// Check if any migration files are newer than the binary.
    ///
    /// Returns `Some(path)` with the path of a stale file, or `None` if all files are fresh.
    pub fn check_migrations_stale(&self) -> Option<std::path::PathBuf> {
        let binary_mtime = self.binary_mtime?;
        let migrations_dir = self.migrations_dir.as_ref()?;

        if !migrations_dir.exists() {
            return None;
        }

        // Check all .rs files in the migrations directory
        let entries = std::fs::read_dir(migrations_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > binary_mtime {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
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
    let (handle, _incoming, driver) =
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
        _child: Some(child),
        binary_mtime: None,
        migrations_dir: None,
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

/// A line of output from the build process.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    /// The text content
    pub text: String,
    /// Whether this came from stderr (vs stdout)
    pub is_stderr: bool,
}

/// A build process that captures output and eventually yields a connection.
pub struct BuildProcess {
    /// Receiver for output lines from background tasks
    output_rx: tokio::sync::mpsc::UnboundedReceiver<BuildOutput>,
    /// The TCP listener waiting for the service to connect back
    listener: TcpListener,
    /// Handle to the child process (to check exit status)
    child_handle: tokio::task::JoinHandle<Option<std::process::ExitStatus>>,
    /// Cached exit status
    exit_status: Option<std::process::ExitStatus>,
    /// The binary path (for mtime checks)
    binary_path: Option<std::path::PathBuf>,
    /// The migrations directory path
    migrations_dir: Option<std::path::PathBuf>,
}

impl BuildProcess {
    /// Poll for the next line of output (non-blocking).
    pub fn try_read_line(&mut self) -> Option<BuildOutput> {
        self.output_rx.try_recv().ok()
    }

    /// Check if the child process has exited (async version).
    pub async fn check_exit(&mut self) -> Option<std::process::ExitStatus> {
        if self.exit_status.is_some() {
            return self.exit_status;
        }
        // Check if the background task completed
        if self.child_handle.is_finished() {
            // Poll it to get the result
            use tokio::time::{Duration, timeout};
            if let Ok(Ok(status)) = timeout(Duration::from_millis(1), &mut self.child_handle).await
            {
                self.exit_status = status;
                return status;
            }
        }
        None
    }

    /// Try to accept a connection from the service.
    ///
    /// Returns `None` if no connection is ready yet.
    pub async fn try_accept(&mut self) -> Result<Option<ServiceConnection>, ServiceError> {
        use tokio::time::{Duration, timeout};

        // Try to accept with a very short timeout (non-blocking feel)
        match timeout(Duration::from_millis(10), self.listener.accept()).await {
            Ok(Ok((stream, _peer_addr))) => {
                // Establish roam session
                let (handle, _incoming, driver) =
                    accept(stream, HandshakeConfig::default(), NoDispatcher)
                        .await
                        .map_err(|e| {
                            ServiceError::Connection(format!("Roam handshake failed: {}", e))
                        })?;

                // Spawn the driver
                let driver_handle = tokio::spawn(async move {
                    if let Err(e) = driver.run().await {
                        eprintln!("Roam driver error: {}", e);
                    }
                });

                // Get binary mtime
                let binary_mtime = self
                    .binary_path
                    .as_ref()
                    .and_then(|p| p.metadata().ok().and_then(|m| m.modified().ok()));

                Ok(Some(ServiceConnection {
                    handle,
                    _driver: driver_handle,
                    _child: None,
                    binary_mtime,
                    migrations_dir: self.migrations_dir.clone(),
                }))
            }
            Ok(Err(e)) => Err(ServiceError::Connection(format!("Accept failed: {}", e))),
            Err(_) => Ok(None), // Timeout - no connection yet
        }
    }
}

/// Start building/running the service, returning a BuildProcess that can be polled.
pub async fn start_service(config: &Config) -> Result<BuildProcess, ServiceError> {
    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ServiceError::Spawn(format!("Failed to bind to port: {}", e)))?;
    let addr = listener
        .local_addr()
        .map_err(|e| ServiceError::Spawn(format!("Failed to get local address: {}", e)))?;

    // Build the command
    let mut cmd = if let Some(binary) = &config.db.binary {
        TokioCommand::new(binary)
    } else if let Some(crate_name) = &config.db.crate_name {
        let mut cmd = TokioCommand::new("cargo");
        cmd.args(["run", "-p", crate_name, "--"]);
        cmd
    } else {
        return Err(ServiceError::Config(
            "No db.crate_name or db.binary specified in dibs.toml".to_string(),
        ));
    };

    // Tell the service where to connect
    cmd.env("DIBS_CLI_ADDR", addr.to_string());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Spawn the service
    let mut child = cmd
        .spawn()
        .map_err(|e| ServiceError::Spawn(format!("Failed to spawn db service: {}", e)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ServiceError::Spawn("Failed to capture stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ServiceError::Spawn("Failed to capture stderr".to_string()))?;

    // Create channel for output
    let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn background task to read stdout
    let tx_stdout = output_tx.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let _ = tx_stdout.send(BuildOutput {
                        text: line.trim_end().to_string(),
                        is_stderr: false,
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Spawn background task to read stderr
    let tx_stderr = output_tx;
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let _ = tx_stderr.send(BuildOutput {
                        text: line.trim_end().to_string(),
                        is_stderr: true,
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Spawn background task to wait for child exit
    let child_handle = tokio::spawn(async move { child.wait().await.ok() });

    // Determine binary path for mtime checks
    let binary_path = if let Some(crate_name) = &config.db.crate_name {
        // For cargo run, the binary is in target/debug/<crate_name>
        let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
        Some(std::path::PathBuf::from(format!(
            "{}/debug/{}",
            target_dir, crate_name
        )))
    } else {
        config.db.binary.as_ref().map(std::path::PathBuf::from)
    };

    // Migrations directory is relative to current working directory
    let migrations_dir = Some(std::path::PathBuf::from("src/migrations"));

    Ok(BuildProcess {
        output_rx,
        listener,
        child_handle,
        exit_status: None,
        binary_path,
        migrations_dir,
    })
}
