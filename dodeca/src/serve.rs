use axum::Router;
use camino::Utf8Path;
use color_eyre::Result;
use tower_http::services::ServeDir;

/// Start the HTTP server (build should be done before calling this)
pub async fn run(output_dir: &Utf8Path, address: &str, port: u16) -> Result<()> {
    // TODO: Add file watcher for incremental rebuilds
    // When a file changes:
    // 1. Update the SourceFile input in the database
    // 2. Re-run parse_file() - Salsa will only recompute if content changed
    // 3. Re-run build_sections/build_pages - only affected items recompute
    // 4. Re-render only changed pages

    let app = Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()));

    let addr = format!("{}:{}", address, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
