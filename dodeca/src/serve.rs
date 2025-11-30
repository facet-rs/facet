use crate::{BuildMode, build};
use axum::Router;
use camino::Utf8Path;
use color_eyre::Result;
use tower_http::services::ServeDir;

pub async fn run(
    content_dir: &Utf8Path,
    output_dir: &Utf8Path,
    address: &str,
    port: u16,
) -> Result<()> {
    // Initial build
    println!("Building site...");
    let _ctx = build(
        &content_dir.to_owned(),
        &output_dir.to_owned(),
        BuildMode::Quick,
    )?;
    println!("Build complete!");

    // TODO: Add file watcher that uses _ctx.db for incremental rebuilds
    // When a file changes:
    // 1. Update the SourceFile input in the database
    // 2. Re-run parse_file() - Salsa will only recompute if content changed
    // 3. Re-run build_sections/build_pages - only affected items recompute
    // 4. Re-render only changed pages

    // Start server
    let app = Router::new().fallback_service(ServeDir::new(output_dir.as_std_path()));

    let addr = format!("{}:{}", address, port);
    println!("Serving at http://{}:{}", address, port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
