mod db;
mod queries;
mod render;
mod serve;
mod template;
mod tui;
mod types;

use crate::db::{Database, ParsedData, SourceFile};
use crate::queries::parse_file;
use crate::types::{HtmlBody, Route, SourceContent, SourcePath, SourcePathRef, Title};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use color_eyre::Result;
use ignore::WalkBuilder;
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
        /// Content directory
        #[arg(short, long, default_value = "content")]
        content: Utf8PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "public")]
        output: Utf8PathBuf,
    },

    /// Build and serve with live reload
    Serve {
        /// Content directory
        #[arg(short, long, default_value = "content")]
        content: Utf8PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "public")]
        output: Utf8PathBuf,

        /// Address to bind on
        #[arg(short, long, default_value = "127.0.0.1")]
        address: String,

        /// Port to serve on
        #[arg(short, long, default_value = "4000")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Command::Build { content, output } => {
            build(&content, &output, BuildMode::Full)?;
        }
        Command::Serve {
            content,
            output,
            address,
            port,
        } => {
            serve::run(&content, &output, &address, port).await?;
        }
    }

    Ok(())
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
) -> Result<BuildContext> {
    let mut ctx = BuildContext::new(content_dir, output_dir);

    // Phase 1: Load all source files
    ctx.load_sources()?;

    // Phase 2: Parse all files (memoized by Salsa)
    let parsed = ctx.parse_all();

    // Phase 3: Build the site tree
    let (sections, pages) = build_tree(&parsed);

    // Phase 4: Render all pages
    render::render_all(&sections, &pages, output_dir)?;

    // Phase 5: Compile Sass
    render::compile_sass(content_dir, output_dir)?;

    if mode == BuildMode::Full {
        // TODO: check_links().await?;
        // TODO: build_search_index(output_dir).await?;
    }

    Ok(ctx)
}
