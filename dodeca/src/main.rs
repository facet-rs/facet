mod cache_bust;
mod cas;
mod config;
mod db;
mod font_subsetter;
mod link_checker;
mod logging;
mod og;
mod queries;
mod render;
mod search;
mod serve;
mod template;
mod tui;
mod types;
mod url_rewrite;

use crate::config::ResolvedConfig;
use crate::db::{
    Database, OutputFile, QueryStats, SassFile, SassRegistry, SourceFile, SourceRegistry,
    StaticFile, StaticRegistry, TemplateFile, TemplateRegistry,
};
use crate::queries::build_site;
use crate::tui::LogEvent;
use crate::types::{
    Route, SassContent, SassPath, SassPathRef, SourceContent, SourcePath, SourcePathRef,
    StaticPath, TemplateContent, TemplatePath, TemplatePathRef,
};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use color_eyre::{Result, eyre::eyre};
use ignore::WalkBuilder;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

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

/// Resolved configuration for a build
struct ResolvedBuildConfig {
    content_dir: Utf8PathBuf,
    output_dir: Utf8PathBuf,
    skip_domains: Vec<String>,
}

/// Resolve content and output directories from CLI args or config file
fn resolve_dirs(
    content: Option<Utf8PathBuf>,
    output: Option<Utf8PathBuf>,
) -> Result<ResolvedBuildConfig> {
    // If both are specified, use them directly (no config file needed)
    if let (Some(c), Some(o)) = (&content, &output) {
        return Ok(ResolvedBuildConfig {
            content_dir: c.clone(),
            output_dir: o.clone(),
            skip_domains: vec![],
        });
    }

    // Try to find config file
    let config = ResolvedConfig::discover()?;

    match config {
        Some(cfg) => {
            let content_dir = content.unwrap_or(cfg.content_dir);
            let output_dir = output.unwrap_or(cfg.output_dir);
            Ok(ResolvedBuildConfig {
                content_dir,
                output_dir,
                skip_domains: cfg.skip_domains,
            })
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
            tui: _use_tui,
        } => {
            let cfg = resolve_dirs(content, output)?;

            // Always use the mini build TUI for now
            build_with_mini_tui(&cfg.content_dir, &cfg.output_dir, &cfg.skip_domains)?;
        }
        Command::Serve {
            content,
            output,
            address,
            port,
            open,
            no_tui,
        } => {
            let cfg = resolve_dirs(content, output)?;
            let (content_dir, output_dir) = (cfg.content_dir, cfg.output_dir);

            // Check if we should use TUI
            use std::io::IsTerminal;
            let use_tui = !no_tui && std::io::stdout().is_terminal();

            if use_tui {
                serve_with_tui(&content_dir, &output_dir, &address, port, open).await?;
            } else {
                // Plain mode - no TUI, serve from Salsa
                logging::init_standard_tracing();
                serve_plain(&content_dir, &address, port, open).await?;
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
                    let url = format!("http://{ip}:{port}");
                    println!("  {} {}", "→".cyan(), terminal_link(&url, &url));
                }
            }
        }
    } else {
        let url = format!("http://{address}:{port}");
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
    /// Static files keyed by static path
    pub static_files: BTreeMap<StaticPath, StaticFile>,
    /// Query statistics (if tracking enabled)
    pub stats: Option<Arc<QueryStats>>,
}

impl BuildContext {
    pub fn new(content_dir: &Utf8Path, output_dir: &Utf8Path) -> Self {
        Self::with_stats(content_dir, output_dir, None)
    }

    pub fn with_stats(
        content_dir: &Utf8Path,
        output_dir: &Utf8Path,
        stats: Option<Arc<QueryStats>>,
    ) -> Self {
        let db = match &stats {
            Some(s) => Database::new_with_stats(Arc::clone(s)),
            None => Database::new(),
        };
        Self {
            db,
            content_dir: content_dir.to_owned(),
            output_dir: output_dir.to_owned(),
            sources: BTreeMap::new(),
            templates: BTreeMap::new(),
            sass_files: BTreeMap::new(),
            static_files: BTreeMap::new(),
            stats,
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

    /// Get the static directory (sibling to content dir)
    pub fn static_dir(&self) -> Utf8PathBuf {
        self.content_dir
            .parent()
            .unwrap_or(&self.content_dir)
            .join("static")
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

    /// Load all static files into the database
    pub fn load_static(&mut self) -> Result<()> {
        let static_dir = self.static_dir();
        if !static_dir.exists() {
            return Ok(()); // No static directory is fine
        }

        let static_files: Vec<Utf8PathBuf> = WalkBuilder::new(&static_dir)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
            .collect();

        for path in static_files {
            let content = fs::read(&path)?;
            let relative = path
                .strip_prefix(&static_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let static_path = StaticPath::new(relative);
            let static_file = StaticFile::new(&self.db, static_path.clone(), content);
            self.static_files.insert(static_path, static_file);
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

// inject_livereload is now in render.rs
use render::inject_livereload;

/// Get the output path for an HTML route
fn route_to_path(output_dir: &Utf8Path, route: &Route) -> Utf8PathBuf {
    let route_str = route.as_str().trim_matches('/');
    if route_str.is_empty() {
        output_dir.join("index.html")
    } else {
        output_dir.join(route_str).join("index.html")
    }
}

/// Build statistics
#[derive(Debug, Default)]
pub struct BuildStats {
    pub html_written: usize,
    pub html_skipped: usize,
    pub css_written: bool,
    pub css_skipped: bool,
    pub static_written: usize,
    pub static_skipped: usize,
}

pub fn build(
    content_dir: &Utf8PathBuf,
    output_dir: &Utf8PathBuf,
    mode: BuildMode,
    progress: Option<tui::ProgressReporter>,
    render_options: render::RenderOptions,
) -> Result<BuildContext> {
    use std::time::Instant;

    let start = Instant::now();
    let verbose = progress.is_none(); // Print to stdout when no TUI progress

    // Open content-addressed storage at base dir
    let base_dir = content_dir.parent().unwrap_or(content_dir);
    let cas_path = base_dir.join(".dodeca.db");
    let store = cas::ContentStore::open(&cas_path)?;

    // Create query stats for tracking
    let query_stats = QueryStats::new();
    let mut ctx = BuildContext::with_stats(content_dir, output_dir, Some(Arc::clone(&query_stats)));

    // Phase 1: Load everything into Salsa
    ctx.load_sources()?;
    ctx.load_templates()?;
    ctx.load_sass()?;
    ctx.load_static()?;

    if verbose {
        println!(
            "{} {} sources, {} templates, {} sass, {} static",
            "Loaded".cyan(),
            ctx.sources.len(),
            ctx.templates.len(),
            ctx.sass_files.len(),
            ctx.static_files.len()
        );
    }

    // Create registries for the query
    let source_vec: Vec<_> = ctx.sources.values().copied().collect();
    let template_vec: Vec<_> = ctx.templates.values().copied().collect();
    let sass_vec: Vec<_> = ctx.sass_files.values().copied().collect();
    let static_vec: Vec<_> = ctx.static_files.values().copied().collect();

    let source_registry = SourceRegistry::new(&ctx.db, source_vec);
    let template_registry = TemplateRegistry::new(&ctx.db, template_vec);
    let sass_registry = SassRegistry::new(&ctx.db, sass_vec);
    let static_registry = StaticRegistry::new(&ctx.db, static_vec);

    // Update progress: parsing phase
    if let Some(ref p) = progress {
        p.update(|prog| prog.parse.start(ctx.sources.len()));
    }

    // THE query - produces all outputs (fonts are automatically subsetted)
    let site_output = build_site(
        &ctx.db,
        source_registry,
        template_registry,
        sass_registry,
        static_registry,
    );

    if let Some(ref p) = progress {
        p.update(|prog| {
            prog.parse.finish();
            prog.render.start(site_output.files.len());
        });
    }

    // Write outputs to disk, only if changed
    let mut stats = BuildStats::default();

    for output in &site_output.files {
        match output {
            OutputFile::Html { route, content } => {
                // Check for render errors in production mode
                if !render_options.dev_mode && content.contains(render::RENDER_ERROR_MARKER) {
                    let error_start = content.find("<pre>").map(|i| i + 5).unwrap_or(0);
                    let error_end = content.find("</pre>").unwrap_or(content.len());
                    let error_msg = &content[error_start..error_end];
                    return Err(eyre!(
                        "Template error rendering {}: {}",
                        route.as_str(),
                        error_msg
                    ));
                }

                // Apply livereload injection if needed
                let final_html = inject_livereload(content, render_options);
                let path = route_to_path(output_dir, route);

                if store.write_if_changed(&path, final_html.as_bytes())? {
                    stats.html_written += 1;
                } else {
                    stats.html_skipped += 1;
                }
            }
            OutputFile::Css { path, content } => {
                let dest = output_dir.join(path.as_str());
                if store.write_if_changed(&dest, content.as_bytes())? {
                    stats.css_written = true;
                }
            }
            OutputFile::Static { path, content } => {
                let dest = output_dir.join(path.as_str());
                if store.write_if_changed(&dest, content)? {
                    stats.static_written += 1;
                } else {
                    stats.static_skipped += 1;
                }
            }
        }
    }

    if let Some(ref p) = progress {
        p.update(|prog| {
            prog.render.finish();
            prog.sass.finish();
        });
    }

    if verbose {
        let up_to_date =
            stats.html_skipped + stats.static_skipped + if stats.css_written { 0 } else { 1 };

        let mut parts = Vec::new();
        if stats.html_written > 0 {
            parts.push(format!("{} HTML", stats.html_written));
        }
        if stats.static_written > 0 {
            parts.push(format!("{} static", stats.static_written));
        }
        if stats.css_written {
            parts.push("CSS".to_string());
        }

        if parts.is_empty() {
            println!("{} ({} up-to-date)", "Wrote".cyan(), up_to_date);
        } else if up_to_date > 0 {
            println!(
                "{} {} ({} up-to-date)",
                "Wrote".cyan(),
                parts.join(", "),
                up_to_date
            );
        } else {
            println!("{} {}", "Wrote".cyan(), parts.join(", "));
        }
    }

    if mode == BuildMode::Full {
        // Check internal links
        let pages = site_output.files.iter().filter_map(|f| match f {
            OutputFile::Html { route, content } => Some(link_checker::Page {
                route,
                html: content,
            }),
            _ => None,
        });
        let link_result = link_checker::check_links(pages);

        if let Some(ref p) = progress {
            p.update(|prog| prog.links.finish());
        }

        if !link_result.is_ok() {
            for broken in &link_result.broken_links {
                eprintln!(
                    "{}: {} -> {}",
                    broken.source_route.as_str().yellow(),
                    broken.href.red(),
                    broken.reason
                );
            }
            return Err(eyre!(
                "Found {} broken link(s)",
                link_result.broken_links.len()
            ));
        }

        if verbose {
            println!(
                "{} {} links ({} internal, {} external)",
                "Checked".cyan(),
                link_result.total_links,
                link_result.internal_links,
                link_result.external_links
            );
        }

        // TODO: build_search_index(output_dir).await?;
        if let Some(ref p) = progress {
            p.update(|prog| prog.search.finish());
        }
    }

    if verbose {
        // Show query stats
        println!(
            "{} {} executed, {} reused",
            "Queries".cyan(),
            query_stats.executed(),
            query_stats.reused()
        );

        let elapsed = start.elapsed();
        println!(
            "\n{} in {:.2}s → {}",
            "Done".green().bold(),
            elapsed.as_secs_f64(),
            output_dir.cyan()
        );
    }

    Ok(ctx)
}

/// Build with mini inline TUI - shows progress without taking over screen
fn build_with_mini_tui(
    content_dir: &Utf8PathBuf,
    output_dir: &Utf8PathBuf,
    skip_domains: &[String],
) -> Result<()> {
    use crossterm::cursor;
    use ratatui::prelude::*;
    use ratatui::widgets::Paragraph;
    use std::io::{self, IsTerminal};
    use std::time::Instant;

    let start = Instant::now();
    let is_terminal = io::stdout().is_terminal();

    // Open CAS
    let base_dir = content_dir.parent().unwrap_or(content_dir);
    let cas_path = base_dir.join(".dodeca.db");
    let store = cas::ContentStore::open(&cas_path)?;

    // Create query stats
    let query_stats = QueryStats::new();
    let stats_for_display = Arc::clone(&query_stats);

    // Create build context with stats
    let mut ctx = BuildContext::with_stats(content_dir, output_dir, Some(query_stats));

    // Load files
    ctx.load_sources()?;
    ctx.load_templates()?;
    ctx.load_sass()?;
    ctx.load_static()?;

    let input_count =
        ctx.sources.len() + ctx.templates.len() + ctx.sass_files.len() + ctx.static_files.len();

    // Set up inline terminal (3 lines) - only if we have a TTY
    let mut terminal = if is_terminal {
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, cursor::Hide)?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let options = ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(3),
        };
        Some(ratatui::Terminal::with_options(backend, options)?)
    } else {
        None
    };

    // Draw progress (only when we have a terminal)
    let draw_progress = |terminal: &mut Option<ratatui::Terminal<_>>,
                         phase: &str,
                         stats: &QueryStats,
                         written: usize,
                         skipped: usize| {
        if let Some(term) = terminal {
            term.draw(|frame| {
                let area = frame.area();

                let text = vec![
                    Line::from(vec![
                        Span::styled("Phase: ", Style::default().fg(Color::Cyan)),
                        Span::raw(phase),
                    ]),
                    Line::from(vec![
                        Span::styled("Queries: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!(
                            "{} executed, {} reused",
                            stats.executed(),
                            stats.reused()
                        )),
                    ]),
                    Line::from(vec![
                        Span::styled("Output: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{written} written, {skipped} up-to-date")),
                    ]),
                ];

                let paragraph = Paragraph::new(text);
                frame.render_widget(paragraph, area);
            })
            .ok();
        }
    };

    // Initial draw
    draw_progress(&mut terminal, "Loading...", &stats_for_display, 0, 0);

    // Create registries
    let source_vec: Vec<_> = ctx.sources.values().copied().collect();
    let template_vec: Vec<_> = ctx.templates.values().copied().collect();
    let sass_vec: Vec<_> = ctx.sass_files.values().copied().collect();
    let static_vec: Vec<_> = ctx.static_files.values().copied().collect();

    let source_registry = SourceRegistry::new(&ctx.db, source_vec);
    let template_registry = TemplateRegistry::new(&ctx.db, template_vec);
    let sass_registry = SassRegistry::new(&ctx.db, sass_vec);
    let static_registry = StaticRegistry::new(&ctx.db, static_vec);

    draw_progress(&mut terminal, "Building...", &stats_for_display, 0, 0);

    // Run the build query
    let site_output = build_site(
        &ctx.db,
        source_registry,
        template_registry,
        sass_registry,
        static_registry,
    );

    draw_progress(&mut terminal, "Writing...", &stats_for_display, 0, 0);

    // Write outputs
    let mut written = 0usize;
    let mut skipped = 0usize;

    let render_options = render::RenderOptions {
        livereload: false,
        dev_mode: false,
    };

    for output in &site_output.files {
        match output {
            OutputFile::Html { route, content } => {
                if !render_options.dev_mode && content.contains(render::RENDER_ERROR_MARKER) {
                    // Clean up terminal before error
                    if terminal.is_some() {
                        drop(terminal);
                        crossterm::execute!(io::stdout(), cursor::Show)?;
                    }

                    let error_start = content.find("<pre>").map(|i| i + 5).unwrap_or(0);
                    let error_end = content.find("</pre>").unwrap_or(content.len());
                    let error_msg = &content[error_start..error_end];
                    return Err(eyre!(
                        "Template error rendering {}: {}",
                        route.as_str(),
                        error_msg
                    ));
                }

                let final_html = inject_livereload(content, render_options);
                let path = route_to_path(output_dir, route);

                if store.write_if_changed(&path, final_html.as_bytes())? {
                    written += 1;
                } else {
                    skipped += 1;
                }
            }
            OutputFile::Css { path, content } => {
                let dest = output_dir.join(path.as_str());
                if store.write_if_changed(&dest, content.as_bytes())? {
                    written += 1;
                } else {
                    skipped += 1;
                }
            }
            OutputFile::Static { path, content } => {
                let dest = output_dir.join(path.as_str());
                if store.write_if_changed(&dest, content)? {
                    written += 1;
                } else {
                    skipped += 1;
                }
            }
        }

        // Update display periodically
        draw_progress(
            &mut terminal,
            "Writing...",
            &stats_for_display,
            written,
            skipped,
        );
    }

    // Check links
    draw_progress(
        &mut terminal,
        "Checking links...",
        &stats_for_display,
        written,
        skipped,
    );

    let pages = site_output.files.iter().filter_map(|f| match f {
        OutputFile::Html { route, content } => Some(link_checker::Page {
            route,
            html: content,
        }),
        _ => None,
    });
    let extracted = link_checker::extract_links(pages);
    let mut link_result = link_checker::check_internal_links(&extracted);

    // Check external links with date-based caching
    let today = chrono::Local::now().date_naive();
    let external_options =
        link_checker::ExternalLinkOptions::new().skip_domains(skip_domains.iter().cloned());

    // Run external link checking in a separate thread with its own runtime
    let extracted_external = extracted.external.clone();
    let known_routes = extracted.known_routes.clone();
    let (external_broken, external_checked) = {
        let ext = link_checker::ExtractedLinks {
            total: extracted.total,
            internal: vec![],
            external: extracted_external,
            known_routes,
        };
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let mut cache = std::collections::HashMap::new();
                link_checker::check_external_links(&ext, &mut cache, today, &external_options).await
            })
        })
        .join()
        .map_err(|_| eyre!("external link check thread panicked"))?
    };

    link_result.external_checked = external_checked;
    link_result.broken_links.extend(external_broken);

    if !link_result.is_ok() {
        // Clean up terminal before error
        if terminal.is_some() {
            drop(terminal);
            crossterm::execute!(io::stdout(), cursor::Show)?;
        }

        for broken in &link_result.broken_links {
            let prefix = if broken.is_external { "[ext]" } else { "[int]" };
            eprintln!(
                "{} {}: {} -> {}",
                prefix.dimmed(),
                broken.source_route.as_str().yellow(),
                broken.href.as_str().red(),
                broken.reason
            );
        }
        return Err(eyre!(
            "Found {} broken link(s) ({} internal, {} external)",
            link_result.broken_links.len(),
            link_result.internal_broken(),
            link_result.external_broken()
        ));
    }

    // Build search index
    draw_progress(
        &mut terminal,
        "Indexing...",
        &stats_for_display,
        written,
        skipped,
    );

    // Use a new runtime for pagefind since we're called from sync context within tokio
    let search_files = {
        let output = site_output.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(search::build_search_index(&output))
        })
        .join()
        .map_err(|_| eyre!("search thread panicked"))??
    };

    // Write search index files
    for (path, content) in &search_files {
        let dest = output_dir.join(path.trim_start_matches('/'));
        store.write_if_changed(&dest, content)?;
    }

    // Final state
    draw_progress(&mut terminal, "Done!", &stats_for_display, written, skipped);

    // Clean up terminal
    if terminal.is_some() {
        drop(terminal);
        crossterm::execute!(io::stdout(), cursor::Show)?;
    }

    // Print final summary
    let elapsed = start.elapsed();
    println!(
        "{} {} inputs → {} queries ({} executed) → {} written, {} up-to-date + {} search",
        "Build".green().bold(),
        input_count,
        stats_for_display.total(),
        stats_for_display.executed(),
        written,
        skipped,
        search_files.len()
    );
    println!(
        "{} {} internal, {} external links",
        "Links".green().bold(),
        link_result.internal_links,
        link_result.external_checked
    );
    println!(
        "{} in {:.2}s → {}",
        "Done".green().bold(),
        elapsed.as_secs_f64(),
        output_dir.cyan()
    );

    Ok(())
}

/// Plain serve mode (no TUI) - serves directly from Salsa
async fn serve_plain(
    content_dir: &Utf8PathBuf,
    address: &str,
    port: u16,
    open: bool,
) -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::watch;

    let render_options = render::RenderOptions {
        livereload: false, // No live reload in plain mode
        dev_mode: true,
    };

    // Create the site server
    let server = Arc::new(serve::SiteServer::new(render_options));

    // Load source files
    println!("{}", "Loading source files...".dimmed());
    let md_files: Vec<Utf8PathBuf> = WalkBuilder::new(content_dir)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
        .collect();

    {
        let db = server.db.lock().unwrap();
        let mut sources = server.sources.write().unwrap();

        for path in &md_files {
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(content_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let source_path = SourcePath::new(relative);
            let source_content = SourceContent::new(content);
            let source = SourceFile::new(&*db, source_path, source_content);
            sources.push(source);
        }
    }
    println!("  Loaded {} source files", md_files.len());

    // Load templates
    let parent_dir = content_dir.parent().unwrap_or(content_dir);
    let templates_dir = parent_dir.join("templates");

    if templates_dir.exists() {
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

        let db = server.db.lock().unwrap();
        let mut templates = server.templates.write().unwrap();

        for path in &template_files {
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(&templates_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let template_path = TemplatePath::new(relative);
            let template_content = TemplateContent::new(content);
            let template = TemplateFile::new(&*db, template_path, template_content);
            templates.push(template);
        }
        println!("  Loaded {} templates", templates.len());
    }

    // Load static files into Salsa
    let static_dir = parent_dir.join("static");
    if static_dir.exists() {
        let walker = WalkBuilder::new(&static_dir).build();
        let db = server.db.lock().unwrap();
        let mut static_files = server.static_files.write().unwrap();
        let mut count = 0;

        for entry in walker {
            let entry = entry?;
            let path = Utf8Path::from_path(entry.path())
                .ok_or_else(|| eyre!("Non-UTF8 path in static directory"))?;

            if path.is_file() {
                let relative = path.strip_prefix(&static_dir)?;
                let content = fs::read(path)?;

                let static_path = StaticPath::new(relative.to_string());
                let static_file = StaticFile::new(&*db, static_path, content);
                static_files.push(static_file);
                count += 1;
            }
        }
        println!("  Loaded {count} static files");
    }

    // Build search index in background
    println!("{}", "Building search index...".dimmed());
    let server_for_search = server.clone();
    std::thread::spawn(move || {
        match rebuild_search_for_serve(&server_for_search) {
            Ok(search_files) => {
                let count = search_files.len();
                // Debug: print all paths
                let mut paths: Vec<_> = search_files.keys().collect();
                paths.sort();
                for path in &paths {
                    println!("  Search file: {path}");
                }
                let mut sf = server_for_search.search_files.write().unwrap();
                *sf = search_files;
                println!("  Search index ready ({count} files)");
            }
            Err(e) => {
                eprintln!("{} Search index error: {}", "error:".red(), e);
            }
        }
    });

    print_server_urls(address, port);

    if open {
        let url = format!("http://127.0.0.1:{port}");
        if let Err(e) = open::that(&url) {
            eprintln!("{} Failed to open browser: {}", "warning:".yellow(), e);
        }
    }

    // Start server
    let ips = if address == "0.0.0.0" {
        let mut ips = vec![std::net::Ipv4Addr::LOCALHOST];
        ips.extend(tui::get_lan_ips());
        ips
    } else {
        vec![address.parse().unwrap_or(std::net::Ipv4Addr::LOCALHOST)]
    };

    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    serve::run_on_ips(server, &ips, port, shutdown_rx).await?;

    Ok(())
}

/// Rebuild search index for serve mode
///
/// Takes a snapshot of the server's current state, builds all HTML via Salsa,
/// and updates the search index. Salsa memoization makes this fast for unchanged pages.
fn rebuild_search_for_serve(server: &serve::SiteServer) -> Result<search::SearchFiles> {
    use crate::db::{SassRegistry, SourceRegistry, StaticRegistry, TemplateRegistry};

    // Lock the database and get current registries
    let db = server.db.lock().map_err(|_| eyre!("db lock poisoned"))?;
    let sources = server
        .sources
        .read()
        .map_err(|_| eyre!("sources lock poisoned"))?;
    let templates = server
        .templates
        .read()
        .map_err(|_| eyre!("templates lock poisoned"))?;
    let sass_files = server
        .sass_files
        .read()
        .map_err(|_| eyre!("sass lock poisoned"))?;

    // Create registries
    let source_registry = SourceRegistry::new(&*db, sources.clone());
    let template_registry = TemplateRegistry::new(&*db, templates.clone());
    let sass_registry = SassRegistry::new(&*db, sass_files.clone());
    let static_registry = StaticRegistry::new(&*db, vec![]); // Empty - search doesn't need static files

    // Build the site (Salsa will cache/reuse unchanged computations)
    let site_output = queries::build_site(
        &*db,
        source_registry,
        template_registry,
        sass_registry,
        static_registry,
    );

    // Drop locks before async work
    drop(db);
    drop(sources);
    drop(templates);
    drop(sass_files);

    // Build search index in a separate thread with its own runtime
    let output = site_output.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(search::build_search_index(&output))
    })
    .join()
    .map_err(|_| eyre!("search thread panicked"))?
}

/// Serve with TUI progress display and file watching
///
/// This serves content directly from Salsa - no files written to disk.
/// HTTP requests query the Salsa database, which caches/memoizes results.
async fn serve_with_tui(
    content_dir: &Utf8PathBuf,
    _output_dir: &Utf8PathBuf,
    address: &str,
    port: u16,
    open: bool,
) -> Result<()> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::Arc;
    use std::sync::mpsc;
    use tokio::sync::watch;

    // Create channels
    let (progress_tx, progress_rx) = tui::progress_channel();
    let (server_tx, server_rx) = tui::server_status_channel();
    let (event_tx, event_rx) = tui::event_channel();

    // Initialize tracing with TUI layer - routes log events to Activity panel
    let filter_handle = logging::init_tui_tracing(event_tx.clone());

    // Render options with live reload enabled (development mode)
    let render_options = render::RenderOptions {
        livereload: true,
        dev_mode: true,
    };

    // Create the site server - serves directly from Salsa, no disk I/O
    let server = Arc::new(serve::SiteServer::new(render_options));

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
        ips.iter().map(|ip| format!("http://{ip}:{port}")).collect()
    }

    // Set initial server status
    let initial_ips = get_bind_ips(initial_mode);
    let _ = server_tx.send(tui::ServerStatus {
        urls: build_urls(&initial_ips, port),
        is_running: false,
        bind_mode: initial_mode,
    });

    // Load source files into the server
    let _ = event_tx.send(LogEvent::info("Loading source files..."));
    progress_tx.send_modify(|prog| prog.parse.start(0));

    let md_files: Vec<Utf8PathBuf> = WalkBuilder::new(content_dir)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter_map(|e| Utf8PathBuf::from_path_buf(e.into_path()).ok())
        .collect();

    {
        let db = server.db.lock().unwrap();
        let mut sources = server.sources.write().unwrap();

        for path in &md_files {
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(content_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let source_path = SourcePath::new(relative);
            let source_content = SourceContent::new(content);
            let source = SourceFile::new(&*db, source_path, source_content);
            sources.push(source);
        }
    }

    progress_tx.send_modify(|prog| prog.parse.finish());
    let _ = event_tx.send(LogEvent::info(format!(
        "Loaded {} source files",
        md_files.len()
    )));

    // Load template files into the server
    let parent_dir = content_dir.parent().unwrap_or(content_dir);
    let templates_dir = parent_dir.join("templates");

    if templates_dir.exists() {
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

        let db = server.db.lock().unwrap();
        let mut templates = server.templates.write().unwrap();

        for path in &template_files {
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(&templates_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let template_path = TemplatePath::new(relative);
            let template_content = TemplateContent::new(content);
            let template = TemplateFile::new(&*db, template_path, template_content);
            templates.push(template);
        }

        let _ = event_tx.send(LogEvent::info(format!(
            "Loaded {} templates",
            templates.len()
        )));
    }

    // Load static files into Salsa
    let static_dir = parent_dir.join("static");
    if static_dir.exists() {
        let walker = WalkBuilder::new(&static_dir).build();
        let db = server.db.lock().unwrap();
        let mut static_files = server.static_files.write().unwrap();
        let mut count = 0;

        for entry in walker {
            let entry = entry?;
            let path = Utf8Path::from_path(entry.path())
                .ok_or_else(|| eyre!("Non-UTF8 path in static directory"))?;

            if path.is_file() {
                let relative = path.strip_prefix(&static_dir)?;
                let content = fs::read(path)?;

                let static_path = StaticPath::new(relative.to_string());
                let static_file = StaticFile::new(&*db, static_path, content);
                static_files.push(static_file);
                count += 1;
            }
        }

        let _ = event_tx.send(LogEvent::info(format!("Loaded {count} static files")));
    }

    // Load SASS files into Salsa (CSS compiled on-demand via query)
    let sass_dir = parent_dir.join("sass");
    if sass_dir.exists() {
        let sass_files_list: Vec<Utf8PathBuf> = WalkBuilder::new(&sass_dir)
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

        let db = server.db.lock().unwrap();
        let mut sass_files = server.sass_files.write().unwrap();

        for path in &sass_files_list {
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(&sass_dir)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| path.to_string());

            let sass_path = SassPath::new(relative);
            let sass_content = SassContent::new(content);
            let sass_file = SassFile::new(&*db, sass_path, sass_content);
            sass_files.push(sass_file);
        }

        let _ = event_tx.send(LogEvent::info(format!(
            "Loaded {} SASS files",
            sass_files.len()
        )));
    }

    // Build initial search index in background
    let _ = event_tx.send(LogEvent::info("Building search index..."));
    let server_for_search = server.clone();
    let event_tx_for_search = event_tx.clone();
    std::thread::spawn(move || match rebuild_search_for_serve(&server_for_search) {
        Ok(search_files) => {
            let count = search_files.len();
            let mut sf = server_for_search.search_files.write().unwrap();
            *sf = search_files;
            let _ = event_tx_for_search.send(LogEvent::info(format!(
                "Search index ready ({count} files)"
            )));
        }
        Err(e) => {
            let _ = event_tx_for_search.send(LogEvent::error(format!("Search index error: {e}")));
        }
    });

    // Mark all tasks as ready - in serve mode, everything is computed on-demand via Salsa
    progress_tx.send_modify(|prog| {
        prog.render.finish();
        prog.sass.finish();
    });

    let _ = event_tx.send(LogEvent::info("Server ready - content served from memory"));

    // Set up file watcher for content and templates
    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(watch_tx, notify::Config::default())?;
    watcher.watch(content_dir.as_std_path(), RecursiveMode::Recursive)?;

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
    let server_for_http = server.clone();
    let server_tx_clone = server_tx.clone();
    let event_tx_clone = event_tx.clone();

    let start_server = |server: Arc<serve::SiteServer>,
                        mode: tui::BindMode,
                        port: u16,
                        shutdown_rx: watch::Receiver<bool>,
                        server_tx: tui::ServerStatusTx,
                        event_tx: tui::EventTx| {
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
                let _ = event_tx.send(LogEvent::info(format!("  → {ip}")));
            }

            if let Err(e) = serve::run_on_ips(server, &ips, port, shutdown_rx).await {
                let _ = event_tx.send(LogEvent::error(format!("Server error: {e}")));
            }
        })
    };

    let mut server_handle = start_server(
        server_for_http.clone(),
        initial_mode,
        port,
        shutdown_rx.clone(),
        server_tx_clone.clone(),
        event_tx_clone.clone(),
    );

    // Open browser if requested
    if open {
        let url = format!("http://127.0.0.1:{port}");
        if let Err(e) = open::that(&url) {
            let _ = event_tx.send(LogEvent::warn(format!("Failed to open browser: {e}")));
        }
    }

    // Spawn file watcher handler
    let content_for_watcher = content_dir.clone();
    let templates_dir_for_watcher = templates_dir.clone();
    let sass_dir_for_watcher = sass_dir.clone();
    let event_tx_for_watcher = event_tx.clone();
    let server_for_watcher = server.clone();

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

                    // Only react to modify/create events
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

                                // Update the file in the server's Salsa database
                                if path.extension() == Some("md") {
                                    if let Ok(relative) = path.strip_prefix(&content_for_watcher) {
                                        if let Ok(content) = fs::read_to_string(path) {
                                            let mut db = server_for_watcher.db.lock().unwrap();
                                            let sources =
                                                server_for_watcher.sources.read().unwrap();

                                            // Find existing source and update it
                                            let relative_str = relative.to_string();
                                            for source in sources.iter() {
                                                if source.path(&*db).as_str() == relative_str {
                                                    use salsa::Setter;
                                                    source
                                                        .set_content(&mut *db)
                                                        .to(SourceContent::new(content.clone()));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                } else if path.extension() == Some("html") {
                                    if let Ok(relative) =
                                        path.strip_prefix(&templates_dir_for_watcher)
                                    {
                                        if let Ok(content) = fs::read_to_string(path) {
                                            let mut db = server_for_watcher.db.lock().unwrap();
                                            let templates =
                                                server_for_watcher.templates.read().unwrap();

                                            // Find existing template and update it
                                            let relative_str = relative.to_string();
                                            for template in templates.iter() {
                                                if template.path(&*db).as_str() == relative_str {
                                                    use salsa::Setter;
                                                    template
                                                        .set_content(&mut *db)
                                                        .to(TemplateContent::new(content.clone()));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                } else if path.extension() == Some("scss")
                                    || path.extension() == Some("sass")
                                {
                                    if let Ok(relative) = path.strip_prefix(&sass_dir_for_watcher) {
                                        if let Ok(content) = fs::read_to_string(path) {
                                            let mut db = server_for_watcher.db.lock().unwrap();
                                            let sass_files =
                                                server_for_watcher.sass_files.read().unwrap();

                                            // Find existing sass file and update it
                                            let relative_str = relative.to_string();
                                            for sass_file in sass_files.iter() {
                                                if sass_file.path(&*db).as_str() == relative_str {
                                                    use salsa::Setter;
                                                    sass_file
                                                        .set_content(&mut *db)
                                                        .to(SassContent::new(content.clone()));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Trigger live reload - browsers will re-fetch and get fresh content from Salsa
                            server_for_watcher.trigger_reload();
                            let _ = event_tx_for_watcher.send(LogEvent::info("Reload triggered"));

                            // Rebuild search index in background (debounced with live reload)
                            let server_for_search = server_for_watcher.clone();
                            let event_tx_for_search = event_tx_for_watcher.clone();
                            std::thread::spawn(move || {
                                match rebuild_search_for_serve(&server_for_search) {
                                    Ok(search_files) => {
                                        let count = search_files.len();
                                        let mut sf =
                                            server_for_search.search_files.write().unwrap();
                                        *sf = search_files;
                                        let _ = event_tx_for_search.send(LogEvent::info(format!(
                                            "Search index updated ({count} files)"
                                        )));
                                    }
                                    Err(e) => {
                                        let _ = event_tx_for_search.send(LogEvent::error(format!(
                                            "Search rebuild error: {e}"
                                        )));
                                    }
                                }
                            });

                            last_rebuild = Instant::now();
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let _ = event_tx_for_watcher.send(LogEvent::error(format!("Watch error: {e}")));
                }
            }
        }
    });

    // Spawn command handler for rebinding
    let server_for_cmd = server.clone();
    let server_tx_for_cmd = server_tx.clone();
    let event_tx_for_cmd = event_tx.clone();
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
                server_for_cmd.clone(),
                new_mode,
                port,
                new_shutdown_rx,
                server_tx_for_cmd.clone(),
                event_tx_for_cmd.clone(),
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
