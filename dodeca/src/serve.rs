use axum::Router;
use camino::Utf8Path;
use color_eyre::Result;
use std::net::Ipv4Addr;
use tokio::sync::watch;
use tower_http::services::ServeDir;

/// Start the HTTP server on a single address (build should be done before calling this)
pub async fn run(output_dir: &Utf8Path, address: &str, port: u16) -> Result<()> {
    let app = Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()));

    let addr = format!("{}:{}", address, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Start HTTP servers on multiple specific IP addresses
/// Each IP gets its own listener - we do NOT bind to 0.0.0.0
pub async fn run_on_ips(
    output_dir: &Utf8Path,
    ips: &[Ipv4Addr],
    port: u16,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    use tokio::task::JoinSet;

    let app = Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()));

    let mut join_set = JoinSet::new();

    // Create a listener for each specific IP
    for ip in ips {
        let addr = format!("{}:{}", ip, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let app_clone = app.clone();
        let mut shutdown_rx_clone = shutdown_rx.clone();

        join_set.spawn(async move {
            let shutdown_future = async move {
                while !*shutdown_rx_clone.borrow() {
                    if shutdown_rx_clone.changed().await.is_err() {
                        break;
                    }
                }
            };

            axum::serve(listener, app_clone)
                .with_graceful_shutdown(shutdown_future)
                .await
        });
    }

    // Wait for shutdown signal, then wait for all servers to stop
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            result = join_set.join_next() => {
                if let Some(res) = result {
                    // A server task finished (probably error)
                    if let Err(e) = res {
                        eprintln!("Server task error: {}", e);
                    }
                } else {
                    // All tasks done
                    break;
                }
            }
        }
    }

    // Wait for remaining tasks to finish gracefully
    while let Some(_) = join_set.join_next().await {}

    Ok(())
}
