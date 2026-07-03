//! vixd — serve the vix daemon on a websocket.
//!
//! IDEs and tools connect here with generated vox clients (see the gen_ts
//! bin) and drive real demand-driven evaluations.
//!
//! Usage: `vixd [addr]` (default `127.0.0.1:4177`).

use vix_daemon::{DaemonDispatcher, DaemonService};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:4177".into());
    let listener = vox::WsListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("vixd: cannot bind {addr}: {e}"));
    tracing::info!(
        addr = %listener.local_addr().expect("local addr"),
        "vixd listening (ws)"
    );

    if let Err(e) = vox::serve_listener(listener, DaemonDispatcher::new(DaemonService::new())).await
    {
        tracing::error!("vixd: serve failed: {e}");
        std::process::exit(1);
    }
}
