mod config;
mod db;
mod queries;
mod render;
mod serve;
mod template;
mod tui;
mod types;

use crate::config::ResolvedConfig;
use crate::db::{Database, ParsedData, SourceFile};
use crate::queries::parse_file;
use crate::types::{HtmlBody, Route, SourceContent, SourcePath, SourcePathRef, Title};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use color_eyre::{Result, eyre::eyre};
use ignore::WalkBuilder;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::fs;

#[derive(Parser)]
#[command(name = "dodeca", about = "Static site generator for facet docs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build the site (blocks on link checking and search index)
    Build {
        /// Content directory (uses .config/dodeca.kdl if not specified)
        #[arg(short, long)]
        content: Option<Utf8PathBuf>,

        /// Output directory (uses .config/dodeca.kdl if not specified)
        #[arg(short, long)]
        output: Option<Utf8PathBuf>,

        /// Show TUI progress display
        #[arg(long)]
        tui: bool,
    },

    /// Build and serve with live reload
    Serve {
        /// Content directory (uses .config/dodeca.kdl if not specified)
        #[arg(short, long)]
        content: Option<Utf8PathBuf>,

        /// Output directory (uses .config/dodeca.kdl if not specified)
        #[arg(short, long)]
        output: Option<Utf8PathBuf>,

        /// Address to bind on
        #[arg(short, long, default_value = "127.0.0.1")]
        address: String,

        /// Port to serve on
        #[arg(short, long, default_value = "4000")]
        port: u16,

        /// Open browser after starting server
        #[arg(long)]
        open: bool,

        /// Disable TUI (show plain output instead)
        #[arg(long)]
        no_tui: bool,
    },
}

/// Resolve content and output directories from CLI args or config file
fn resolve_dirs(
    content: Option<Utf8PathBuf>,
    output: Option<Utf8PathBuf>,
) -> Result<(Utf8PathBuf, Utf8PathBuf)> {
    // If both are specified, use them directly
    if let (Some(c), Some(o)) = (&content, &output) {
        return Ok((c.clone(), o.clone()));
    }

    // Try to find config file
    let config = ResolvedConfig::discover()?;

    match config {
        Some(cfg) => {
            let content_dir = content.unwrap_or(cfg.content_dir);
            let output_dir = output.unwrap_or(cfg.output_dir);
            Ok((content_dir, output_dir))
        }
        None => Err(eyre!(
            "{}\n\n\
                 Create a config file at {} with:\n\n\
                 \x20   {}\n\
                 \x20   {}\n\n\
                 Or specify both {} and {} on the command line.",
            "No configuration found.".red().bold(),
            ".config/dodeca.kdl".cyan(),
            "content \"path/to/content\"".green(),
            "output \"path/to/output\"".green(),
            "--content".yellow(),
            "--output".yellow()
        )),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            content,
            output,
            tui: use_tui,
        } => {
            let (content_dir, output_dir) = resolve_dirs(content, output)?;

            if use_tui {
                build_with_tui(&content_dir, &output_dir)?;
            } else {
                build(
                    &content_dir,
                    &output_dir,
                    BuildMode::Full,
                    None,
                    Default::default(),
                )?;
            }
        }
        Command::Serve {
            content,
            output,
            address,
            port,
            open,
            no_tui,
        } => {
            let (content_dir, output_dir) = resolve_dirs(content, output)?;

            // Check if we should use TUI
            use std::io::IsTerminal;
            let use_tui = !no_tui && std::io::stdout().is_terminal();

            if use_tui {
                serve_with_tui(&content_dir, &output_dir, &address, port, open).await?;
            } else {
                // Plain mode - no TUI (no live reload without TUI for now)
                println!("{}", "Building...".dimmed());
                build(
                    &content_dir,
                    &output_dir,
                    BuildMode::Quick,
                    None,
                    Default::default(),
                )?;

                print_server_urls(&address, port);

                if open {
                    let url = format!("http://127.0.0.1:{}", port);
                    if let Err(e) = open::that(&url) {
                        eprintln!("{} Failed to open browser: {}", "warning:".yellow(), e);
                    }
                }

                serve::run(&output_dir, &address, port).await?;
            }
        }
    }

    Ok(())
}

/// Print server URLs with terminal hyperlinks
fn print_server_urls(address: &str, port: u16) {
    println!("\n{}", "Server running at:".bold());

    if address == "0.0.0.0" {
        // List all interfaces
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for iface in interfaces {
                if let if_addrs::IfAddr::V4(addr) = iface.addr {
                    let ip = addr.ip;
                    let url = format!("http://{}:{}", ip, port);
                    println!("  {} {}", "→".cyan(), terminal_link(&url, &url));
                }
            }
        }
    } else {
        let url = format!("http://{}:{}", address, port);
        println!("  {} {}", "→".cyan(), terminal_link(&url, &url));
    }
    println!();
}

/// Create an OSC 8 terminal hyperlink
fn terminal_link(url: &str, text: &str) -> String {
    format!(
        "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
        url,
        text.blue().underline()
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    /// Full build - block on link checking and search index
    Full,
    /// Quick build - just HTML, async link checking
    Quick,
}

/// The build context with Salsa database
pub struct BuildContext {
    pub db: Database,
    pub content_dir: Utf8PathBuf,
    pub output_dir: Utf8PathBuf,
    /// Source files keyed by source path
    pub sources: BTreeMap<SourcePath, SourceFile>,
}

impl BuildContext {
    pub fn new(content_dir: &Utf8Path, output_dir: &Utf8Path) -> Self {
        Self {
            db: Database::new(),
            content_dir: content_dir.to_owned(),
            output_dir: output_dir.to_owned(),
            sources: BTreeMap::new(),
        }
    }

    /// Load all source files into the database
    pub fn load_sources(&mut self) -> Result<()> {
        let md_files: Vec<Utf8PathBuf> = WalkBuilder::new(&self.content_dir)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
            .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
            .collect();

        for path in md_files {
            let content = fs::read_to_string(&path)?;
            let relative = path
                .strip_prefix(&self.content_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let source_path = SourcePath::new(relative);
            let source_content = SourceContent::new(content);
            let source = SourceFile::new(&self.db, source_path.clone(), source_content);
            self.sources.insert(source_path, source);
        }

        Ok(())
    }

    /// Update a single source file (for incremental rebuilds)
    pub fn update_source(&mut self, relative_path: &SourcePathRef) -> Result<bool> {
        let full_path = self.content_dir.join(relative_path.as_str());
        if !full_path.exists() {
            // File was deleted
            self.sources.remove(relative_path);
            return Ok(true);
        }

        let content = fs::read_to_string(&full_path)?;
        let source_content = SourceContent::new(content);

        // Check if we already have this source
        if let Some(existing) = self.sources.get(relative_path) {
            // Update the content - Salsa will detect if it changed
            use salsa::Setter;
            existing.set_content(&mut self.db).to(source_content);
        } else {
            // New file
            let source_path = SourcePath::new(relative_path.to_string());
            let source = SourceFile::new(&self.db, source_path.clone(), source_content);
            self.sources.insert(source_path, source);
        }

        Ok(true)
    }

    /// Parse all source files (memoized by Salsa)
    pub fn parse_all(&self) -> Vec<ParsedData> {
        self.sources
            .values()
            .map(|source| parse_file(&self.db, *source))
            .collect()
    }
}

/// A section in the site tree
#[derive(Debug, Clone)]
pub struct Section {
    pub route: Route,
    pub title: Title,
    pub weight: i32,
    pub body_html: HtmlBody,
}

/// A page in the site tree
#[derive(Debug, Clone)]
pub struct Page {
    pub route: Route,
    pub title: Title,
    pub weight: i32,
    pub body_html: HtmlBody,
    pub section_route: Route,
}

/// Build the site tree from parsed data
fn build_tree(parsed: &[ParsedData]) -> (BTreeMap<Route, Section>, BTreeMap<Route, Page>) {
    let mut sections: BTreeMap<Route, Section> = BTreeMap::new();
    let mut pages: BTreeMap<Route, Page> = BTreeMap::new();

    // First pass: create all sections
    for data in parsed.iter().filter(|d| d.is_section) {
        sections.insert(
            data.route.clone(),
            Section {
                route: data.route.clone(),
                title: data.title.clone(),
                weight: data.weight,
                body_html: data.body_html.clone(),
            },
        );
    }

    // Ensure root section exists
    if !sections.contains_key(&Route::root()) {
        sections.insert(
            Route::root(),
            Section {
                route: Route::root(),
                title: Title::from_static("Home"),
                weight: 0,
                body_html: HtmlBody::from_static(""),
            },
        );
    }

    // Second pass: create pages and assign to sections
    for data in parsed.iter().filter(|d| !d.is_section) {
        let section_route = find_parent_section(&data.route, &sections);
        pages.insert(
            data.route.clone(),
            Page {
                route: data.route.clone(),
                title: data.title.clone(),
                weight: data.weight,
                body_html: data.body_html.clone(),
                section_route,
            },
        );
    }

    (sections, pages)
}

/// Find the nearest parent section for a route
fn find_parent_section(route: &Route, sections: &BTreeMap<Route, Section>) -> Route {
    let mut current = route.clone();

    loop {
        if sections.contains_key(&current) && current != *route {
            return current;
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return Route::root(),
        }
    }
}

pub fn build(
    content_dir: &Utf8PathBuf,
    output_dir: &Utf8PathBuf,
    mode: BuildMode,
    progress: Option<tui::ProgressReporter>,
    render_options: render::RenderOptions,
) -> Result<BuildContext> {
    let mut ctx = BuildContext::new(content_dir, output_dir);

    // Phase 1: Load all source files
    ctx.load_sources()?;

    // Phase 2: Parse all files (memoized by Salsa)
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.start(ctx.sources.len()));
    }
    let parsed = ctx.parse_all();
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.finish());
    }

    // Phase 3: Build the site tree
    let (sections, pages) = build_tree(&parsed);

    // Phase 4: Render all pages
    if let Some(ref p) = progress {
        p.update(|prog| prog.render.start(sections.len() + pages.len()));
    }
    render::render_all(&sections, &pages, output_dir, render_options)?;
    if let Some(ref p) = progress {
        p.update(|prog| prog.render.finish());
    }

    // Phase 5: Compile Sass
    if let Some(ref p) = progress {
        p.update(|prog| prog.sass.start(1));
    }
    render::compile_sass(content_dir, output_dir)?;
    if let Some(ref p) = progress {
        p.update(|prog| prog.sass.finish());
    }

    if mode == BuildMode::Full {
        // TODO: check_links().await?;
        // TODO: build_search_index(output_dir).await?;
        if let Some(ref p) = progress {
            p.update(|prog| {
                prog.links.finish();
                prog.search.finish();
            });
        }
    }

    Ok(ctx)
}

/// Build with TUI progress display
fn build_with_tui(content_dir: &Utf8PathBuf, output_dir: &Utf8PathBuf) -> Result<()> {
    use std::io::IsTerminal;

    // Check if we're running in a terminal
    if !std::io::stdout().is_terminal() {
        eprintln!(
            "{}",
            "TUI requires a terminal, falling back to normal build".yellow()
        );
        return build(
            content_dir,
            output_dir,
            BuildMode::Full,
            None,
            Default::default(),
        )
        .map(|_| ());
    }

    let progress = tui::new_shared_progress();

    // Spawn build in a separate thread
    let content = content_dir.clone();
    let output = output_dir.clone();
    let build_progress = progress.clone();

    let build_handle = std::thread::spawn(move || {
        build(
            &content,
            &output,
            BuildMode::Full,
            Some(tui::ProgressReporter::Shared(build_progress)),
            Default::default(),
        )
    });

    // Run TUI on main thread
    let mut terminal = tui::init_terminal()?;
    let mut app = tui::App::new(progress);
    let _ = app.run(&mut terminal);
    tui::restore_terminal()?;

    // Wait for build to complete
    build_handle
        .join()
        .map_err(|_| color_eyre::eyre::eyre!("Build thread panicked"))??;

    Ok(())
}

/// Serve with TUI progress display and file watching
async fn serve_with_tui(
    content_dir: &Utf8PathBuf,
    output_dir: &Utf8PathBuf,
    address: &str,
    port: u16,
    open: bool,
) -> Result<()> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::Arc;
    use std::sync::mpsc;
    use tokio::sync::watch;

    // Create channels (no more Arc<Mutex>!)
    let (progress_tx, progress_rx) = tui::progress_channel();
    let (server_tx, server_rx) = tui::server_status_channel();
    let (event_tx, event_rx) = tui::event_channel();

    // Live reload state (shared across server restarts)
    let livereload = Arc::new(serve::LiveReloadState::new());

    // Determine initial bind mode
    let initial_mode = if address == "0.0.0.0" {
        tui::BindMode::Lan
    } else {
        tui::BindMode::Local
    };

    // Get the IPs to bind to for a given mode
    fn get_bind_ips(mode: tui::BindMode) -> Vec<std::net::Ipv4Addr> {
        match mode {
            tui::BindMode::Local => vec![std::net::Ipv4Addr::LOCALHOST],
            tui::BindMode::Lan => {
                let mut ips = vec![std::net::Ipv4Addr::LOCALHOST];
                ips.extend(tui::get_lan_ips());
                ips
            }
        }
    }

    // Build URLs from IPs
    fn build_urls(ips: &[std::net::Ipv4Addr], port: u16) -> Vec<String> {
        ips.iter()
            .map(|ip| format!("http://{}:{}", ip, port))
            .collect()
    }

    // Set initial server status
    let initial_ips = get_bind_ips(initial_mode);
    let _ = server_tx.send(tui::ServerStatus {
        urls: build_urls(&initial_ips, port),
        is_running: false,
        bind_mode: initial_mode,
    });

    // Render options with live reload enabled
    let render_options = render::RenderOptions { livereload: true };

    // Initial build
    let _ = event_tx.send("Starting initial build...".to_string());

    let _ctx = build(
        content_dir,
        output_dir,
        BuildMode::Quick,
        Some(tui::ProgressReporter::Channel(progress_tx.clone())),
        render_options,
    )?;

    let _ = event_tx.send("Build complete".to_string());

    // Set up file watcher
    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(watch_tx, notify::Config::default())?;
    watcher.watch(content_dir.as_std_path(), RecursiveMode::Recursive)?;

    let _ = event_tx.send(format!("Watching {}", content_dir));

    // Command channel for TUI -> server communication
    let (cmd_tx, cmd_rx) = mpsc::channel::<tui::ServerCommand>();

    // Shutdown signal for the server
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Start server in background
    let output_for_server = output_dir.clone();
    let server_tx_clone = server_tx.clone();
    let event_tx_clone = event_tx.clone();
    let livereload_clone = livereload.clone();

    let start_server = |output: Utf8PathBuf,
                        mode: tui::BindMode,
                        port: u16,
                        shutdown_rx: watch::Receiver<bool>,
                        server_tx: tui::ServerStatusTx,
                        event_tx: tui::EventTx,
                        livereload: Arc<serve::LiveReloadState>| {
        tokio::spawn(async move {
            let ips = get_bind_ips(mode);

            // Update server status
            let _ = server_tx.send(tui::ServerStatus {
                urls: build_urls(&ips, port),
                is_running: true,
                bind_mode: mode,
            });

            // Log the binding
            let mode_str = match mode {
                tui::BindMode::Local => "localhost only",
                tui::BindMode::Lan => "LAN",
            };
            let _ = event_tx.send(format!("Binding to {} IPs ({})", ips.len(), mode_str));
            for ip in &ips {
                let _ = event_tx.send(format!("  → {}", ip));
            }

            if let Err(e) =
                serve::run_on_ips(&output, &ips, port, shutdown_rx, Some(livereload)).await
            {
                let _ = event_tx.send(format!("Server error: {}", e));
            }
        })
    };

    let mut server_handle = start_server(
        output_for_server.clone(),
        initial_mode,
        port,
        shutdown_rx.clone(),
        server_tx_clone.clone(),
        event_tx_clone.clone(),
        livereload_clone.clone(),
    );

    // Open browser if requested
    if open {
        let url = format!("http://127.0.0.1:{}", port);
        if let Err(e) = open::that(&url) {
            let _ = event_tx.send(format!("Failed to open browser: {}", e));
        }
    }

    // Spawn file watcher handler
    let content_for_watcher = content_dir.clone();
    let output_for_watcher = output_dir.clone();
    let progress_tx_for_watcher = progress_tx.clone();
    let event_tx_for_watcher = event_tx.clone();
    let livereload_for_watcher = livereload.clone();

    std::thread::spawn(move || {
        use std::time::Instant;
        let mut last_rebuild = Instant::now();
        let debounce = std::time::Duration::from_millis(100);

        while let Ok(res) = watch_rx.recv() {
            match res {
                Ok(event) => {
                    // Debounce rapid events
                    if last_rebuild.elapsed() < debounce {
                        continue;
                    }

                    // Only rebuild on modify/create events
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            let paths: Vec<_> = event
                                .paths
                                .iter()
                                .filter(|p| {
                                    p.extension()
                                        .map(|e| e == "md" || e == "scss")
                                        .unwrap_or(false)
                                })
                                .collect();

                            if paths.is_empty() {
                                continue;
                            }

                            for path in &paths {
                                if let Ok(rel) = path.strip_prefix(&content_for_watcher) {
                                    let _ = event_tx_for_watcher
                                        .send(format!("Changed: {}", rel.display()));
                                }
                            }

                            // Reset progress for rebuild
                            progress_tx_for_watcher.send_modify(|prog| {
                                prog.parse = tui::TaskProgress::new("Parsing");
                                prog.render = tui::TaskProgress::new("Rendering");
                                prog.sass = tui::TaskProgress::new("Sass");
                            });

                            // Rebuild with live reload enabled
                            match build(
                                &content_for_watcher,
                                &output_for_watcher,
                                BuildMode::Quick,
                                Some(tui::ProgressReporter::Channel(
                                    progress_tx_for_watcher.clone(),
                                )),
                                render::RenderOptions { livereload: true },
                            ) {
                                Ok(_) => {
                                    let _ =
                                        event_tx_for_watcher.send("Rebuild complete".to_string());
                                    // Trigger live reload in connected browsers
                                    livereload_for_watcher.trigger_reload();
                                }
                                Err(e) => {
                                    let _ =
                                        event_tx_for_watcher.send(format!("Build error: {}", e));
                                }
                            }

                            last_rebuild = Instant::now();
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let _ = event_tx_for_watcher.send(format!("Watch error: {}", e));
                }
            }
        }
    });

    // Spawn command handler for rebinding
    let output_for_cmd = output_dir.clone();
    let server_tx_for_cmd = server_tx.clone();
    let event_tx_for_cmd = event_tx.clone();
    let livereload_for_cmd = livereload.clone();
    // Use Arc<Mutex> for the shutdown sender so we can update it for each rebind
    let current_shutdown = Arc::new(std::sync::Mutex::new(shutdown_tx.clone()));
    let current_shutdown_for_handler = current_shutdown.clone();

    tokio::spawn(async move {
        while let Ok(cmd) = cmd_rx.recv() {
            let new_mode = match cmd {
                tui::ServerCommand::GoPublic => tui::BindMode::Lan,
                tui::ServerCommand::GoLocal => tui::BindMode::Local,
            };

            // Signal current server to shutdown
            {
                let shutdown = current_shutdown_for_handler.lock().unwrap();
                let _ = shutdown.send(true);
            }

            // Wait for server to stop
            let _ = server_handle.await;

            // Create new shutdown channel
            let (new_shutdown_tx, new_shutdown_rx) = watch::channel(false);

            // Update the current shutdown sender for next time
            {
                let mut shutdown = current_shutdown_for_handler.lock().unwrap();
                *shutdown = new_shutdown_tx;
            }

            let _ = event_tx_for_cmd.send("Restarting server...".to_string());

            // Start new server
            server_handle = start_server(
                output_for_cmd.clone(),
                new_mode,
                port,
                new_shutdown_rx,
                server_tx_for_cmd.clone(),
                event_tx_for_cmd.clone(),
                livereload_for_cmd.clone(),
            );
        }
    });

    // Run TUI on main thread
    let mut terminal = tui::init_terminal()?;
    let mut app = tui::ServeApp::new(progress_rx, server_rx, event_rx, cmd_tx);
    let _ = app.run(&mut terminal);
    tui::restore_terminal()?;

    // Signal server to shutdown (use current_shutdown in case it was swapped)
    {
        let shutdown = current_shutdown.lock().unwrap();
        let _ = shutdown.send(true);
    }

    Ok(())
}
