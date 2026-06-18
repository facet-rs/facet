//! Compatibility shim for local IPC transport.
//!
//! The local transport implementation now lives in `vox-stream`.
//! This crate re-exports that API so existing callers can migrate gradually.

#![deny(unsafe_code)]

pub use vox_stream::{
    LocalLink, LocalLinkAcceptor, LocalLinkSource, LocalListener, LocalServerStream, LocalStream,
    connect, endpoint_exists, local_link_source, path_to_pipe_name, remove_endpoint,
};

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unix_roundtrip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = std::env::temp_dir();
        let sock_path = dir.join(format!("vox-local-test-{}.sock", std::process::id()));

        let _ = std::fs::remove_file(&sock_path);

        let sock_path_clone = sock_path.clone();
        let server = tokio::spawn(async move {
            let listener = LocalListener::bind(&sock_path_clone).unwrap();
            let mut stream = listener.accept().await.unwrap();

            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");

            stream.write_all(b"world").await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = connect(&sock_path).await.unwrap();
        client.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");

        server.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);
    }
}
