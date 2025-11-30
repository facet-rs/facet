mod config;
mod db;
mod logging;
mod queries;
mod render;
mod serve;
mod template;
mod tui;
mod types;

use crate::config::ResolvedConfig;
use crate::db::{Database, SassFile, SourceFile, SourceRegistry, TemplateFile, TemplateRegistry};
use crate::queries::{build_tree, render_page, render_section};
use crate::tui::LogEvent;
use crate::types::{
    HtmlBody, Route, SassContent, SassPath, SassPathRef, SourceContent, SourcePath, SourcePathRef,
    TemplateContent, TemplatePath, TemplatePathRef, Title,
};
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
                    render::RenderOptions {
                        livereload: false,
                        dev_mode: false, // Production: fail on errors
                    },
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
                    render::RenderOptions {
                        livereload: false,
                        dev_mode: true, // Development: show error pages
                    },
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
    /// Template files keyed by template path
    pub templates: BTreeMap<TemplatePath, TemplateFile>,
    /// Sass/SCSS files keyed by sass path
    pub sass_files: BTreeMap<SassPath, SassFile>,
}

impl BuildContext {
    pub fn new(content_dir: &Utf8Path, output_dir: &Utf8Path) -> Self {
        Self {
            db: Database::new(),
            content_dir: content_dir.to_owned(),
            output_dir: output_dir.to_owned(),
            sources: BTreeMap::new(),
            templates: BTreeMap::new(),
            sass_files: BTreeMap::new(),
        }
    }

    /// Get the templates directory (sibling to content dir)
    pub fn templates_dir(&self) -> Utf8PathBuf {
        self.content_dir
            .parent()
            .unwrap_or(&self.content_dir)
            .join("templates")
    }

    /// Get the sass directory (sibling to content dir)
    pub fn sass_dir(&self) -> Utf8PathBuf {
        self.content_dir
            .parent()
            .unwrap_or(&self.content_dir)
            .join("sass")
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

    /// Load all template files into the database
    pub fn load_templates(&mut self) -> Result<()> {
        let templates_dir = self.templates_dir();
        if !templates_dir.exists() {
            return Ok(()); // No templates directory is fine
        }

        let template_files: Vec<Utf8PathBuf> = WalkBuilder::new(&templates_dir)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "html")
                    .unwrap_or(false)
            })
            .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
            .collect();

        for path in template_files {
            let content = fs::read_to_string(&path)?;
            let relative = path
                .strip_prefix(&templates_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let template_path = TemplatePath::new(relative);
            let template_content = TemplateContent::new(content);
            let template = TemplateFile::new(&self.db, template_path.clone(), template_content);
            self.templates.insert(template_path, template);
        }

        Ok(())
    }

    /// Load all Sass/SCSS files into the database
    pub fn load_sass(&mut self) -> Result<()> {
        let sass_dir = self.sass_dir();
        if !sass_dir.exists() {
            return Ok(()); // No sass directory is fine
        }

        let sass_files: Vec<Utf8PathBuf> = WalkBuilder::new(&sass_dir)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "scss" || ext == "sass")
                    .unwrap_or(false)
            })
            .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
            .collect();

        for path in sass_files {
            let content = fs::read_to_string(&path)?;
            let relative = path
                .strip_prefix(&sass_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let sass_path = SassPath::new(relative);
            let sass_content = SassContent::new(content);
            let sass_file = SassFile::new(&self.db, sass_path.clone(), sass_content);
            self.sass_files.insert(sass_path, sass_file);
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

    /// Update a single template file (for incremental rebuilds)
    pub fn update_template(&mut self, relative_path: &TemplatePathRef) -> Result<bool> {
        let templates_dir = self.templates_dir();
        let full_path = templates_dir.join(relative_path.as_str());
        if !full_path.exists() {
            // File was deleted
            self.templates.remove(relative_path);
            return Ok(true);
        }

        let content = fs::read_to_string(&full_path)?;
        let template_content = TemplateContent::new(content);

        // Check if we already have this template
        if let Some(existing) = self.templates.get(relative_path) {
            // Update the content - Salsa will detect if it changed
            use salsa::Setter;
            existing.set_content(&mut self.db).to(template_content);
        } else {
            // New file
            let template_path = TemplatePath::new(relative_path.to_string());
            let template = TemplateFile::new(&self.db, template_path.clone(), template_content);
            self.templates.insert(template_path, template);
        }

        Ok(true)
    }

    /// Update a single Sass file (for incremental rebuilds)
    pub fn update_sass(&mut self, relative_path: &SassPathRef) -> Result<bool> {
        let sass_dir = self.sass_dir();
        let full_path = sass_dir.join(relative_path.as_str());
        if !full_path.exists() {
            // File was deleted
            self.sass_files.remove(relative_path);
            return Ok(true);
        }

        let content = fs::read_to_string(&full_path)?;
        let sass_content = SassContent::new(content);

        // Check if we already have this sass file
        if let Some(existing) = self.sass_files.get(relative_path) {
            // Update the content - Salsa will detect if it changed
            use salsa::Setter;
            existing.set_content(&mut self.db).to(sass_content);
        } else {
            // New file
            let sass_path = SassPath::new(relative_path.to_string());
            let sass_file = SassFile::new(&self.db, sass_path.clone(), sass_content);
            self.sass_files.insert(sass_path, sass_file);
        }

        Ok(true)
    }
}

/// Inject livereload script if enabled
fn inject_livereload(html: &str, options: render::RenderOptions) -> String {
    if options.livereload {
        let livereload_script = r##"<script>
(function() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = protocol + '//' + window.location.host + '/__livereload';
    let ws;
    let reconnectTimer;

    function connect() {
        ws = new WebSocket(wsUrl);
        ws.onopen = function() { console.log('[livereload] connected'); };
        ws.onmessage = function(event) {
            if (event.data === 'reload') {
                console.log('[livereload] reloading...');
                window.location.reload();
            }
        };
        ws.onclose = function() {
            console.log('[livereload] disconnected, reconnecting...');
            clearTimeout(reconnectTimer);
            reconnectTimer = setTimeout(connect, 1000);
        };
    }
    connect();
})();
</script>"##;
        html.replace("</body>", &format!("{}</body>", livereload_script))
    } else {
        html.to_string()
    }
}

/// Write HTML output to the appropriate file path
fn write_html_output(
    output_dir: &Utf8Path,
    route: &Route,
    html: &str,
    dev_mode: bool,
) -> Result<()> {
    // In production mode, fail on render errors
    if !dev_mode && html.contains(render::RENDER_ERROR_MARKER) {
        // Extract error message from the HTML for the error output
        let error_start = html.find("<pre>").map(|i| i + 5).unwrap_or(0);
        let error_end = html.find("</pre>").unwrap_or(html.len());
        let error_msg = &html[error_start..error_end];
        return Err(eyre!(
            "Template error rendering {}: {}",
            route.as_str(),
            error_msg
        ));
    }

    let route_str = route.as_str().trim_matches('/');
    let out_path = if route_str.is_empty() {
        output_dir.join("index.html")
    } else {
        let dir = output_dir.join(route_str);
        fs::create_dir_all(&dir)?;
        dir.join("index.html")
    };
    fs::write(&out_path, html)?;
    Ok(())
}

pub fn build(
    content_dir: &Utf8PathBuf,
    output_dir: &Utf8PathBuf,
    mode: BuildMode,
    progress: Option<tui::ProgressReporter>,
    render_options: render::RenderOptions,
) -> Result<BuildContext> {
    let mut ctx = BuildContext::new(content_dir, output_dir);

    // Phase 1: Load all source files, templates, and sass
    ctx.load_sources()?;
    ctx.load_templates()?;
    ctx.load_sass()?;

    // Phase 2+3: Parse all files and build tree (tracked by Salsa)
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.start(ctx.sources.len()));
    }
    let source_vec: Vec<_> = ctx.sources.values().copied().collect();
    let source_registry = SourceRegistry::new(&ctx.db, source_vec);
    let site_tree = build_tree(&ctx.db, source_registry);
    let sections = site_tree.sections;
    let pages = site_tree.pages;
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.finish());
    }

    // Phase 4: Render all pages using templates (via Salsa tracked queries)
    if let Some(ref p) = progress {
        p.update(|prog| prog.render.start(sections.len() + pages.len()));
    }
    let template_vec: Vec<_> = ctx.templates.values().copied().collect();
    let template_registry = TemplateRegistry::new(&ctx.db, template_vec);

    // Render sections using tracked queries
    for route in sections.keys() {
        let rendered = render_section(&ctx.db, route.clone(), source_registry, template_registry);
        let html = inject_livereload(&rendered.0, render_options);
        write_html_output(output_dir, route, &html, render_options.dev_mode)?;
    }

    // Render pages using tracked queries
    for route in pages.keys() {
        let rendered = render_page(&ctx.db, route.clone(), source_registry, template_registry);
        let html = inject_livereload(&rendered.0, render_options);
        write_html_output(output_dir, route, &html, render_options.dev_mode)?;
    }

    if let Some(ref p) = progress {
        p.update(|prog| prog.render.finish());
    }

    // Phase 5: Compile Sass
    if let Some(ref p) = progress {
        p.update(|prog| prog.sass.start(1));
    }
    render::compile_sass_tracked(&ctx.db, &ctx.sass_files, &ctx.sass_dir(), output_dir)?;
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

/// Incremental rebuild using existing context (preserves Salsa memoization)
pub fn rebuild(
    ctx: &mut BuildContext,
    changed_paths: &[Utf8PathBuf],
    progress: Option<tui::ProgressReporter>,
    render_options: render::RenderOptions,
) -> Result<render::RenderStats> {
    let templates_dir = ctx.templates_dir();
    let sass_dir = ctx.sass_dir();

    // Update changed source files, templates, and sass in the Salsa database
    let mut template_changed = false;
    let mut sass_changed = false;
    for path in changed_paths {
        // Check if this is a template file
        if let Ok(relative) = path.strip_prefix(&templates_dir) {
            let relative_str = relative.to_string();
            let template_path = TemplatePathRef::from_str(&relative_str);
            let _ = ctx.update_template(template_path);
            template_changed = true;
        }
        // Check if this is a sass file
        else if let Ok(relative) = path.strip_prefix(&sass_dir) {
            let relative_str = relative.to_string();
            let sass_path = SassPathRef::from_str(&relative_str);
            let _ = ctx.update_sass(sass_path);
            sass_changed = true;
        }
        // Check if this is a source file
        else if let Ok(relative) = path.strip_prefix(&ctx.content_dir) {
            let relative_str = relative.to_string();
            let source_path = SourcePathRef::from_str(&relative_str);
            let _ = ctx.update_source(source_path);
        }
    }

    // Templates are always loaded fresh from disk (no caching in Engine),
    // so template changes are automatically picked up during render.
    // The template_changed flag could be used for logging/debugging.
    let _ = template_changed;

    // Parse all files and build tree (Salsa memoizes - unchanged files return cached results)
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.start(ctx.sources.len()));
    }
    let source_vec: Vec<_> = ctx.sources.values().copied().collect();
    let source_registry = SourceRegistry::new(&ctx.db, source_vec);
    let site_tree = build_tree(&ctx.db, source_registry);
    let sections = site_tree.sections;
    let pages = site_tree.pages;
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.finish());
    }

    // Render all pages using templates (via Salsa tracked queries)
    if let Some(ref p) = progress {
        p.update(|prog| prog.render.start(sections.len() + pages.len()));
    }
    let template_vec: Vec<_> = ctx.templates.values().copied().collect();
    let template_registry = TemplateRegistry::new(&ctx.db, template_vec);

    let mut written = 0;

    // Render sections using tracked queries
    for route in sections.keys() {
        let rendered = render_section(&ctx.db, route.clone(), source_registry, template_registry);
        let html = inject_livereload(&rendered.0, render_options);
        write_html_output(&ctx.output_dir, route, &html, render_options.dev_mode)?;
        written += 1;
    }

    // Render pages using tracked queries
    for route in pages.keys() {
        let rendered = render_page(&ctx.db, route.clone(), source_registry, template_registry);
        let html = inject_livereload(&rendered.0, render_options);
        write_html_output(&ctx.output_dir, route, &html, render_options.dev_mode)?;
        written += 1;
    }

    if let Some(ref p) = progress {
        p.update(|prog| prog.render.finish());
    }

    // Compile Sass if any .scss files changed (tracked via Salsa)
    if sass_changed {
        if let Some(ref p) = progress {
            p.update(|prog| prog.sass.start(1));
        }
        render::compile_sass_tracked(&ctx.db, &ctx.sass_files, &ctx.sass_dir(), &ctx.output_dir)?;
        if let Some(ref p) = progress {
            p.update(|prog| prog.sass.finish());
        }
    }

    Ok(render::RenderStats {
        written,
        skipped: 0,
    })
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

    // Initialize tracing with TUI layer - routes log events to Activity panel
    let filter_handle = logging::init_tui_tracing(event_tx.clone());

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

    // Render options with live reload enabled (development mode)
    let render_options = render::RenderOptions {
        livereload: true,
        dev_mode: true,
    };

    // Initial build - keep context for incremental rebuilds
    let _ = event_tx.send(LogEvent::info("Starting initial build..."));

    let ctx = build(
        content_dir,
        output_dir,
        BuildMode::Quick,
        Some(tui::ProgressReporter::Channel(progress_tx.clone())),
        render_options,
    )?;

    // Wrap context for sharing with watcher thread
    let ctx = std::sync::Arc::new(std::sync::Mutex::new(ctx));

    let _ = event_tx.send(LogEvent::info("Build complete"));

    // Set up file watcher for content and templates
    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(watch_tx, notify::Config::default())?;
    watcher.watch(content_dir.as_std_path(), RecursiveMode::Recursive)?;

    // Also watch templates and sass directories if they exist
    let parent_dir = content_dir.parent().unwrap_or(content_dir);
    let templates_dir = parent_dir.join("templates");
    let sass_dir = parent_dir.join("sass");

    let mut watched_dirs = vec![content_dir.to_string()];

    if templates_dir.exists() {
        watcher.watch(templates_dir.as_std_path(), RecursiveMode::Recursive)?;
        watched_dirs.push("templates".to_string());
    }
    if sass_dir.exists() {
        watcher.watch(sass_dir.as_std_path(), RecursiveMode::Recursive)?;
        watched_dirs.push("sass".to_string());
    }

    let _ = event_tx.send(LogEvent::info(format!(
        "Watching: {}",
        watched_dirs.join(", ")
    )));

    // Command channel for TUI -> server communication (async-compatible)
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<tui::ServerCommand>();

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
            let _ = event_tx.send(LogEvent::info(format!(
                "Binding to {} IPs ({})",
                ips.len(),
                mode_str
            )));
            for ip in &ips {
                let _ = event_tx.send(LogEvent::info(format!("  → {}", ip)));
            }

            if let Err(e) =
                serve::run_on_ips(&output, &ips, port, shutdown_rx, Some(livereload)).await
            {
                let _ = event_tx.send(LogEvent::error(format!("Server error: {}", e)));
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
            let _ = event_tx.send(LogEvent::warn(format!("Failed to open browser: {}", e)));
        }
    }

    // Spawn file watcher handler
    let content_for_watcher = content_dir.clone();
    let progress_tx_for_watcher = progress_tx.clone();
    let event_tx_for_watcher = event_tx.clone();
    let livereload_for_watcher = livereload.clone();
    let ctx_for_watcher = ctx.clone();

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
                            let paths: Vec<Utf8PathBuf> = event
                                .paths
                                .iter()
                                .filter(|p| {
                                    p.extension()
                                        .map(|e| e == "md" || e == "scss" || e == "html")
                                        .unwrap_or(false)
                                })
                                .filter_map(|p| Utf8PathBuf::from_path_buf(p.clone()).ok())
                                .collect();

                            if paths.is_empty() {
                                continue;
                            }

                            for path in &paths {
                                let _ = event_tx_for_watcher.send(LogEvent::info(format!(
                                    "Changed: {}",
                                    path.file_name().unwrap_or("?")
                                )));
                            }

                            // Reset progress for rebuild
                            progress_tx_for_watcher.send_modify(|prog| {
                                prog.parse = tui::TaskProgress::new("Parsing");
                                prog.render = tui::TaskProgress::new("Rendering");
                                prog.sass = tui::TaskProgress::new("Sass");
                            });

                            // Incremental rebuild using existing context (preserves Salsa memoization)
                            let mut ctx = ctx_for_watcher.lock().unwrap();
                            match rebuild(
                                &mut ctx,
                                &paths,
                                Some(tui::ProgressReporter::Channel(
                                    progress_tx_for_watcher.clone(),
                                )),
                                render::RenderOptions {
                                    livereload: true,
                                    dev_mode: true, // Development: show error pages
                                },
                            ) {
                                Ok(stats) => {
                                    let _ = event_tx_for_watcher.send(LogEvent::info(format!(
                                        "Rebuilt: {} written, {} unchanged",
                                        stats.written, stats.skipped
                                    )));
                                    // Trigger live reload in connected browsers
                                    livereload_for_watcher.trigger_reload();
                                }
                                Err(e) => {
                                    let _ = event_tx_for_watcher
                                        .send(LogEvent::error(format!("Build error: {}", e)));
                                }
                            }

                            last_rebuild = Instant::now();
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let _ =
                        event_tx_for_watcher.send(LogEvent::error(format!("Watch error: {}", e)));
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
        while let Some(cmd) = cmd_rx.recv().await {
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

            let _ = event_tx_for_cmd.send(LogEvent::info("Restarting server..."));

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
    let mut app = tui::ServeApp::new(progress_rx, server_rx, event_rx, cmd_tx, filter_handle);
    let _ = app.run(&mut terminal);
    tui::restore_terminal()?;

    // Signal server to shutdown (use current_shutdown in case it was swapped)
    {
        let shutdown = current_shutdown.lock().unwrap();
        let _ = shutdown.send(true);
    }

    Ok(())
}
