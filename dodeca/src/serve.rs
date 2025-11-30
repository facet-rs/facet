//! HTTP server that serves content directly from the Salsa database
//!
//! No files are read from disk - everything is queried from Salsa on demand.
//! This enables instant incremental rebuilds with zero disk I/O.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use color_eyre::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::{broadcast, watch};

use crate::db::{
    Database, OutputFile, SassFile, SassRegistry, SourceFile, SourceRegistry, StaticFile,
    StaticRegistry, TemplateFile, TemplateRegistry,
};
use crate::queries::build_site;
use crate::render::{RenderOptions, inject_livereload};

/// Shared state for the dev server
pub struct SiteServer {
    /// The Salsa database - all queries go through here
    /// Uses Mutex instead of RwLock because Database contains RefCell (not Sync)
    pub db: Mutex<Database>,
    /// Source files (for creating registries)
    pub sources: RwLock<Vec<SourceFile>>,
    /// Template files (for creating registries)
    pub templates: RwLock<Vec<TemplateFile>>,
    /// SASS files (for creating registries)
    pub sass_files: RwLock<Vec<SassFile>>,
    /// Static files (in Salsa)
    pub static_files: RwLock<Vec<StaticFile>>,
    /// Search index files (pagefind): path -> content
    pub search_files: RwLock<HashMap<String, Vec<u8>>>,
    /// Live reload broadcast
    pub livereload_tx: broadcast::Sender<()>,
    /// Render options (dev mode, etc.)
    pub render_options: RenderOptions,
}

impl SiteServer {
    pub fn new(render_options: RenderOptions) -> Self {
        let (livereload_tx, _) = broadcast::channel(16);
        Self {
            db: Mutex::new(Database::new()),
            sources: RwLock::new(Vec::new()),
            templates: RwLock::new(Vec::new()),
            sass_files: RwLock::new(Vec::new()),
            static_files: RwLock::new(Vec::new()),
            search_files: RwLock::new(HashMap::new()),
            livereload_tx,
            render_options,
        }
    }

    /// Notify all connected browsers to reload
    pub fn trigger_reload(&self) {
        let _ = self.livereload_tx.send(());
    }

    /// Build the full site via Salsa query - returns all outputs with cache-busted paths
    fn build_site_output(&self) -> Option<crate::db::SiteOutput> {
        let db = self.db.lock().ok()?;
        let sources = self.sources.read().ok()?;
        let templates = self.templates.read().ok()?;
        let sass_files = self.sass_files.read().ok()?;
        let static_files = self.static_files.read().ok()?;

        let source_registry = SourceRegistry::new(&*db, sources.clone());
        let template_registry = TemplateRegistry::new(&*db, templates.clone());
        let sass_registry = SassRegistry::new(&*db, sass_files.clone());
        let static_registry = StaticRegistry::new(&*db, static_files.clone());

        Some(build_site(
            &*db,
            source_registry,
            template_registry,
            sass_registry,
            static_registry,
        ))
    }

    /// Find content for a given path from the site output
    fn find_content(&self, path: &str) -> Option<ServeContent> {
        let site_output = self.build_site_output()?;

        // Normalize path for route lookup
        let route_path = if path == "/" {
            "/".to_string()
        } else {
            let trimmed = path.trim_end_matches('/');
            format!("{trimmed}/")
        };

        for output in &site_output.files {
            match output {
                OutputFile::Html { route, content } => {
                    if route.as_str() == route_path {
                        let html = inject_livereload(content, self.render_options);
                        return Some(ServeContent::Html(html));
                    }
                }
                OutputFile::Css {
                    path: css_path,
                    content,
                } => {
                    // CSS has cache-busted path like "/main.a1b2c3d4.css"
                    let css_url = format!("/{}", css_path.as_str());
                    if path == css_url {
                        return Some(ServeContent::Css(content.clone()));
                    }
                }
                OutputFile::Static {
                    path: static_path,
                    content,
                } => {
                    // Static files have cache-busted paths
                    let static_url = format!("/{}", static_path.as_str());
                    if path == static_url {
                        let mime = mime_from_extension(path);
                        return Some(ServeContent::Static(content.clone(), mime));
                    }
                }
            }
        }

        None
    }
}

/// Content types that can be served
enum ServeContent {
    Html(String),
    Css(String),
    Static(Vec<u8>, &'static str),
}

/// Cache-Control header for cache-busted assets (1 year, immutable)
const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Cache-Control header for HTML (must revalidate to get new asset URLs)
const CACHE_NO_CACHE: &str = "no-cache";

/// Handler for all content requests
async fn content_handler(State(server): State<Arc<SiteServer>>, request: Request) -> Response {
    let path = request.uri().path();

    // Check search files first (pagefind - not part of build_site, no cache control)
    {
        let search_files = server.search_files.read().unwrap();
        if let Some(content) = search_files.get(path) {
            let mime = mime_from_extension(path);
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(content.clone()))
                .unwrap();
        }
    }

    // Try to serve from build_site output (HTML, CSS, static files - all cache-busted)
    if let Some(content) = server.find_content(path) {
        return match content {
            ServeContent::Html(html) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CACHE_CONTROL, CACHE_NO_CACHE)
                .body(Body::from(html))
                .unwrap(),
            ServeContent::Css(css) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
                .header(header::CACHE_CONTROL, CACHE_IMMUTABLE)
                .body(Body::from(css))
                .unwrap(),
            ServeContent::Static(bytes, mime) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, CACHE_IMMUTABLE)
                .body(Body::from(bytes))
                .unwrap(),
        };
    }

    // 404
    StatusCode::NOT_FOUND.into_response()
}

/// WebSocket handler for live reload
async fn livereload_handler(
    ws: WebSocketUpgrade,
    State(server): State<Arc<SiteServer>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_livereload_socket(socket, server))
}

async fn handle_livereload_socket(socket: WebSocket, server: Arc<SiteServer>) {
    let (mut sender, mut receiver) = socket.split();
    let mut reload_rx = server.livereload_tx.subscribe();

    // Send initial connection confirmation
    let _ = sender.send(Message::Text("connected".into())).await;

    loop {
        tokio::select! {
            result = reload_rx.recv() => {
                match result {
                    Ok(()) => {
                        if sender.send(Message::Text("reload".into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Middleware to log HTTP requests with status code and latency
async fn log_requests(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status().as_u16();
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    if status >= 500 {
        tracing::error!("{} {} {:.1}ms", status, path, latency_ms);
    } else if status >= 400 {
        tracing::warn!("{} {} {:.1}ms", status, path, latency_ms);
    } else {
        tracing::info!("{} {} {:.1}ms", status, path, latency_ms);
    }

    response
}

/// Build the axum router
pub fn build_router(server: Arc<SiteServer>) -> Router {
    Router::new()
        .route("/__livereload", get(livereload_handler))
        .fallback(content_handler)
        .with_state(server)
        .layer(middleware::from_fn(log_requests))
}

/// Start HTTP servers on multiple specific IP addresses
pub async fn run_on_ips(
    server: Arc<SiteServer>,
    ips: &[Ipv4Addr],
    port: u16,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    use tokio::task::JoinSet;

    let app = build_router(server);
    let mut join_set = JoinSet::new();

    for ip in ips {
        let addr = format!("{ip}:{port}");
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

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            result = join_set.join_next() => {
                if let Some(res) = result {
                    if let Err(e) = res {
                        eprintln!("Server task error: {e}");
                    }
                } else {
                    break;
                }
            }
        }
    }

    while (join_set.join_next().await).is_some() {}

    Ok(())
}

/// Guess MIME type from file extension
pub fn mime_from_extension(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("xml") => "application/xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("md") => "text/markdown; charset=utf-8",
        Some("jxl") => "image/jxl",
        Some("wasm") => "application/wasm",
        // Pagefind-specific extensions
        Some("pf_index") | Some("pf_meta") | Some("pagefind") => "application/octet-stream",
        _ => "application/octet-stream",
    }
}
