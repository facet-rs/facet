//! Minimal replacement for `axum::serve` that doesn't pull in `tokio-macros`.
//!
//! This crate provides a simple `serve` function that accepts a `TcpListener`
//! and an axum `Router` (or any compatible service), serving HTTP requests
//! without requiring the `tokio` feature from axum.

#![deny(unsafe_code)]

use std::time::Duration;

use http_body::Body;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tower_service::Service;

/// Serve HTTP requests using the provided listener and service.
///
/// This is a minimal replacement for `axum::serve` that works without
/// axum's `tokio` feature (which pulls in `tokio-macros` and `syn`).
///
/// Like `axum::serve`, this function loops forever accepting connections.
/// Socket errors cause a 1-second sleep before retrying.
///
/// # Example
///
/// ```ignore
/// use axum::{Router, routing::get};
/// use tokio::net::TcpListener;
///
/// let app = Router::new().route("/", get(|| async { "Hello!" }));
/// let listener = TcpListener::bind("0.0.0.0:3000").await?;
/// rapace_axum_serve::serve(listener, app).await?;
/// ```
pub async fn serve<S, B>(listener: TcpListener, service: S) -> std::io::Result<()>
where
    S: Service<http::Request<Incoming>, Response = http::Response<B>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    loop {
        let (stream, _remote_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(_e) => {
                // Socket errors trigger a retry after sleeping (like axum does)
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let svc = service.clone();

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            // Wrap the tower service to make it hyper-compatible
            let hyper_svc = TowerToHyperService::new(svc);
            let builder = Builder::new(TokioExecutor::new());
            let conn = builder.serve_connection(io, hyper_svc);

            // Connection errors are common (client disconnect, etc.) - ignore them
            let _ = conn.await;
        });
    }
}
