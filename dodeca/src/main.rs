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
                build(&content_dir, &output_dir, BuildMode::Full, None)?;
            }
        }
        Command::Serve {
            content,
            output,
            address,
            port,
            open,
        } => {
            let (content_dir, output_dir) = resolve_dirs(content, output)?;

            // Build first, then show URLs
            println!("{}", "Building...".dimmed());
            build(&content_dir, &output_dir, BuildMode::Quick, None)?;

            // Print where the server is available
            print_server_urls(&address, port);

            // Open browser if requested
            if open {
                let url = format!("http://127.0.0.1:{}", port);
                if let Err(e) = open::that(&url) {
                    eprintln!("{} Failed to open browser: {}", "warning:".yellow(), e);
                }
            }

            // Start serving (blocks forever)
            serve::run(&output_dir, &address, port).await?;
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
    progress: Option<tui::SharedProgress>,
) -> Result<BuildContext> {
    let mut ctx = BuildContext::new(content_dir, output_dir);

    // Phase 1: Load all source files
    ctx.load_sources()?;

    // Phase 2: Parse all files (memoized by Salsa)
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.parse.start(ctx.sources.len());
    }
    let parsed = ctx.parse_all();
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.parse.finish();
    }

    // Phase 3: Build the site tree
    let (sections, pages) = build_tree(&parsed);

    // Phase 4: Render all pages
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.render.start(sections.len() + pages.len());
    }
    render::render_all(&sections, &pages, output_dir)?;
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.render.finish();
    }

    // Phase 5: Compile Sass
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.sass.start(1);
    }
    render::compile_sass(content_dir, output_dir)?;
    if let Some(ref p) = progress {
        let mut prog = p.lock().unwrap();
        prog.sass.finish();
    }

    if mode == BuildMode::Full {
        // TODO: check_links().await?;
        // TODO: build_search_index(output_dir).await?;
        if let Some(ref p) = progress {
            let mut prog = p.lock().unwrap();
            prog.links.finish();
            prog.search.finish();
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
        return build(content_dir, output_dir, BuildMode::Full, None).map(|_| ());
    }

    let progress = tui::new_shared_progress();

    // Spawn build in a separate thread
    let content = content_dir.clone();
    let output = output_dir.clone();
    let build_progress = progress.clone();

    let build_handle =
        std::thread::spawn(move || build(&content, &output, BuildMode::Full, Some(build_progress)));

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
