//! Windows named pipe implementation for local IPC.

use std::io;
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};

/// A local IPC stream (named pipe on Windows).
pub type LocalStream = NamedPipeClient;

/// A local IPC server stream (the connected server end of a named pipe).
pub type LocalServerStream = NamedPipeServer;

/// A local IPC listener (named pipe server on Windows).
///
/// Windows named pipes work differently from Unix sockets:
/// - Each connection requires a new server instance
/// - At least one server must always exist to avoid client `NotFound` errors
/// - The listener manages this by always having a "next" server ready
pub struct LocalListener {
    /// The pipe name (e.g., `\\.\pipe\my-pipe`)
    pipe_name: String,
    /// The next server instance waiting for connections
    next_server: NamedPipeServer,
}

impl LocalListener {
    /// Bind to the given pipe name.
    ///
    /// The name should be a Windows named pipe path like `\\.\pipe\my-pipe`.
    /// Unlike Unix sockets, named pipes don't create files - they exist in a
    /// virtual namespace managed by Windows.
    ///
    /// Note: We don't use `first_pipe_instance(true)` because we want to allow
    /// taking over from a stale/crashed daemon. If another server exists and is
    /// actively using the pipe, clients will connect to whichever server is
    /// available - this is fine for our use case.
    pub fn bind(pipe_name: impl Into<String>) -> io::Result<Self> {
        let pipe_name = pipe_name.into();

        let next_server = ServerOptions::new().create(&pipe_name)?;

        Ok(Self {
            pipe_name,
            next_server,
        })
    }

    /// Accept a new connection.
    ///
    /// Returns the server stream for the new connection. Note that on Windows,
    /// the server end of the pipe (`NamedPipeServer`) is what you use for I/O
    /// after accepting.
    pub async fn accept(&mut self) -> io::Result<LocalServerStream> {
        // Wait for a client to connect to the current server
        self.next_server.connect().await?;

        // Take the connected server
        let connected = std::mem::replace(
            &mut self.next_server,
            // Create the next server instance immediately to ensure
            // there's always a server available for new clients
            ServerOptions::new().create(&self.pipe_name)?,
        );

        Ok(connected)
    }
}

/// Connect to a local IPC endpoint.
///
/// On Windows, this connects to a named pipe at the given path.
pub async fn connect(pipe_name: impl AsRef<str>) -> io::Result<LocalStream> {
    let pipe_name = pipe_name.as_ref();

    // Try to connect, with a brief retry for ERROR_PIPE_BUSY
    loop {
        match ClientOptions::new().open(pipe_name) {
            Ok(client) => return Ok(client),
            Err(e) if e.raw_os_error() == Some(231) => {
                // ERROR_PIPE_BUSY (231) - all pipe instances are busy
                // Wait briefly and retry
                moire::sleep!(std::time::Duration::from_millis(50), "pipe.busy.retry").await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Check if a local IPC endpoint exists.
///
/// On Windows, we attempt to open the pipe to check if it exists.
/// This is not perfect but there's no direct "exists" check for named pipes.
pub fn endpoint_exists(pipe_name: impl AsRef<str>) -> bool {
    // Try to open the pipe - if it succeeds or returns BUSY, it exists
    match ClientOptions::new().open(pipe_name.as_ref()) {
        Ok(_) => true,
        Err(e) => {
            // ERROR_PIPE_BUSY means it exists but all instances are in use
            e.raw_os_error() == Some(231)
        }
    }
}

/// Remove a local IPC endpoint.
///
/// On Windows, named pipes are automatically cleaned up when all handles
/// are closed. This function is a no-op for API compatibility with Unix.
pub fn remove_endpoint(_pipe_name: impl AsRef<str>) -> io::Result<()> {
    // Named pipes don't need explicit cleanup - they disappear when
    // all server and client handles are closed
    Ok(())
}
