//! Unix socket implementation for local IPC.

use std::io;
use std::path::Path;

use peeps::PeepableFutureExt;
use tokio::net::{UnixListener, UnixStream};

/// A local IPC stream (Unix socket on Unix platforms).
pub type LocalStream = UnixStream;

/// A local IPC listener (Unix socket listener on Unix platforms).
pub struct LocalListener {
    inner: UnixListener,
}

impl LocalListener {
    /// Bind to the given socket path.
    ///
    /// The path should be a filesystem path where the socket file will be created.
    /// Parent directories must exist.
    pub fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        let inner = UnixListener::bind(path)?;
        Ok(Self { inner })
    }

    /// Accept a new connection.
    ///
    /// Returns the stream for the new connection.
    pub async fn accept(&self) -> io::Result<LocalStream> {
        let (stream, _addr) = self
            .inner
            .accept()
            .peepable("local_listener.accept")
            .await?;
        Ok(stream)
    }
}

/// Connect to a local IPC endpoint.
///
/// On Unix, this connects to a Unix socket at the given path.
pub async fn connect(path: impl AsRef<Path>) -> io::Result<LocalStream> {
    UnixStream::connect(path)
        .peepable("unix_stream.connect")
        .await
}

/// Check if a local IPC endpoint exists.
///
/// On Unix, this checks if the socket file exists.
pub fn endpoint_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

/// Remove a local IPC endpoint.
///
/// On Unix, this removes the socket file.
pub fn remove_endpoint(path: impl AsRef<Path>) -> io::Result<()> {
    std::fs::remove_file(path)
}
