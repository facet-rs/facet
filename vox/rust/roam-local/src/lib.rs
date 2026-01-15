//! Cross-platform local IPC for roam.
//!
//! This crate provides a unified API for local inter-process communication:
//! - **Unix**: Uses Unix domain sockets
//! - **Windows**: Uses named pipes
//!
//! # Usage
//!
//! ## Server
//!
//! ```ignore
//! use roam_local::LocalListener;
//!
//! // On Unix: path like "/tmp/my-app.sock"
//! // On Windows: pipe name like r"\\.\pipe\my-app"
//! let mut listener = LocalListener::bind(endpoint)?;
//!
//! loop {
//!     let stream = listener.accept().await?;
//!     // stream implements AsyncRead + AsyncWrite
//!     tokio::spawn(handle_connection(stream));
//! }
//! ```
//!
//! ## Client
//!
//! ```ignore
//! use roam_local::connect;
//!
//! let stream = connect(endpoint).await?;
//! // stream implements AsyncRead + AsyncWrite
//! ```
//!
//! # Platform Differences
//!
//! The main API is the same across platforms, but there are some differences:
//!
//! - **Endpoint format**: Unix uses filesystem paths, Windows uses pipe names
//!   (e.g., `\\.\pipe\my-app`)
//! - **Stream types**: On Unix, both client and server use `UnixStream`. On Windows,
//!   the client uses `NamedPipeClient` and the server uses `NamedPipeServer`.
//!   Both implement `AsyncRead + AsyncWrite`.
//! - **Cleanup**: Unix sockets leave files that may need manual cleanup. Windows
//!   named pipes are automatically cleaned up when all handles close.

#![deny(unsafe_code)]

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

// Re-export platform-specific implementations
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
pub use windows::*;

/// Generate a pipe name for Windows from a Unix-style path.
///
/// This is useful when you want to use the same "conceptual" endpoint
/// across platforms. It hashes the path to create a valid pipe name.
///
/// # Example
///
/// ```
/// use roam_local::path_to_pipe_name;
///
/// let pipe = path_to_pipe_name("/home/user/project/.tracey/daemon.sock");
/// // Returns something like r"\\.\pipe\roam-a1b2c3d4e5f6..."
/// ```
#[cfg(windows)]
pub fn path_to_pipe_name(path: impl AsRef<std::path::Path>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    path.as_ref().hash(&mut hasher);
    let hash = hasher.finish();

    format!(r"\\.\pipe\roam-{:016x}", hash)
}

/// On Unix, this just returns the path as a string.
/// Provided for API compatibility when writing cross-platform code.
#[cfg(unix)]
pub fn path_to_pipe_name(path: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    path.as_ref().to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_roundtrip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = std::env::temp_dir();
        let sock_path = dir.join(format!("roam-local-test-{}.sock", std::process::id()));

        // Clean up any stale socket
        let _ = std::fs::remove_file(&sock_path);

        // Spawn server task
        let sock_path_clone = sock_path.clone();
        let server = tokio::spawn(async move {
            let listener = LocalListener::bind(&sock_path_clone).unwrap();
            let mut stream = listener.accept().await.unwrap();

            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");

            stream.write_all(b"world").await.unwrap();
        });

        // Give server time to bind
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect as client
        let mut client = connect(&sock_path).await.unwrap();
        client.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");

        server.await.unwrap();

        // Cleanup
        let _ = std::fs::remove_file(&sock_path);
    }
}
