//! Unix socket implementation for local IPC.

use std::io;
use std::path::Path;
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
        let (stream, _addr) = self.inner.accept().await?;
        Ok(stream)
    }
}

/// Connect to a local IPC endpoint.
///
/// On Unix, this connects to a Unix socket at the given path.
pub async fn connect(path: impl AsRef<Path>) -> io::Result<LocalStream> {
    UnixStream::connect(path).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn unique_socket_path(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::path::PathBuf::from(format!("/tmp/rl-{tag}-{}-{nanos}.sock", std::process::id()))
    }

    #[tokio::test]
    async fn endpoint_lifecycle_bind_connect_accept_remove() {
        let path = unique_socket_path("lifecycle");
        assert!(!endpoint_exists(&path));

        let listener = LocalListener::bind(&path).expect("bind should succeed");
        assert!(endpoint_exists(&path));

        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept should succeed");
            let mut buf = [0_u8; 4];
            stream
                .read_exact(&mut buf)
                .await
                .expect("server read should succeed");
            assert_eq!(&buf, b"ping");
            stream
                .write_all(b"pong")
                .await
                .expect("server write should succeed");
        });

        let mut client = connect(&path).await.expect("connect should succeed");
        client
            .write_all(b"ping")
            .await
            .expect("client write should succeed");
        let mut buf = [0_u8; 4];
        client
            .read_exact(&mut buf)
            .await
            .expect("client read should succeed");
        assert_eq!(&buf, b"pong");

        server.await.expect("server task should complete");
        remove_endpoint(&path).expect("remove endpoint should succeed");
        assert!(!endpoint_exists(&path));
    }

    #[tokio::test]
    async fn connect_to_missing_endpoint_returns_not_found() {
        let path = unique_socket_path("missing-connect");
        let err = connect(&path)
            .await
            .expect_err("connect should fail for missing endpoint");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn remove_missing_endpoint_returns_not_found() {
        let path = unique_socket_path("missing-remove");
        let err = remove_endpoint(&path).expect_err("remove should fail for missing endpoint");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
