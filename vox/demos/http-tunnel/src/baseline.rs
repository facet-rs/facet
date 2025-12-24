//! Baseline HTTP server for benchmarking.
//!
//! This is a simple axum server that provides the same endpoints as the
//! tunneled version, but without any rapace overhead. Used to measure
//! the pure overhead of the rapace tunnel.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p rapace-http-tunnel --bin http_baseline
//! ```
//!
//! Then benchmark with:
//! ```bash
//! oha http://127.0.0.1:4000/small -z 10s -c 8
//! oha http://127.0.0.1:4000/large -z 10s -c 64
//! ```

use axum::{Router, routing::get};
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Small response body - just "ok"
const SMALL_RESPONSE: &str = "ok";

/// Large response body - ~256KB of repeated text
fn large_response() -> String {
    // Create a ~256KB response by repeating a pattern
    let pattern = "The quick brown fox jumps over the lazy dog. ";
    let repeat_count = (256 * 1024) / pattern.len();
    pattern.repeat(repeat_count)
}

async fn handle_small() -> &'static str {
    SMALL_RESPONSE
}

async fn handle_large() -> String {
    large_response()
}

async fn handle_health() -> &'static str {
    "healthy"
}

fn main() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main())
}

async fn async_main() -> eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let app = Router::new()
        .route("/small", get(handle_small))
        .route("/large", get(handle_large))
        .route("/health", get(handle_health));

    let addr: SocketAddr = "127.0.0.1:4000".parse()?;
    tracing::info!("Baseline HTTP server listening on {}", addr);
    tracing::info!("Endpoints:");
    tracing::info!("  GET /small  - returns 'ok' (2 bytes)");
    tracing::info!("  GET /large  - returns ~256KB body");
    tracing::info!("  GET /health - health check");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
