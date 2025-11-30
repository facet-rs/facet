use crate::db::{Database, Page, SassFile, SassRegistry, Section, TemplateFile, TemplateRegistry};
use crate::queries::{load_all_sass, load_all_templates};
use crate::template::{Context, Engine, InMemoryLoader, Value};
use crate::types::{HtmlBody, Route, RouteRef, SassPath, TemplatePath, Title};
use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::Result;
use maud::{DOCTYPE, Markup, PreEscaped, html};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Load all templates through Salsa tracked query, returning an InMemoryLoader.
/// This ensures Salsa tracks dependencies on all template files.
pub fn load_templates_tracked(
    db: &Database,
    templates: &BTreeMap<TemplatePath, TemplateFile>,
) -> InMemoryLoader {
    // Create interned registry from template files
    let template_vec: Vec<_> = templates.values().copied().collect();
    let registry = TemplateRegistry::new(db, template_vec);

    // Call tracked query - Salsa records dependencies on all templates
    let template_map = load_all_templates(db, registry);

    // Build InMemoryLoader from the result
    let mut loader = InMemoryLoader::new();
    for (path, content) in template_map {
        loader.add(path, content);
    }
    loader
}

/// Options for rendering
#[derive(Default, Clone, Copy)]
pub struct RenderOptions {
    /// Whether to inject live reload script
    pub livereload: bool,
}

/// Stats from rendering
#[derive(Default)]
pub struct RenderStats {
    /// Number of files written (changed)
    pub written: usize,
    /// Number of files skipped (unchanged)
    pub skipped: usize,
}

/// Render all sections and pages to the output directory
pub fn render_all(
    sections: &BTreeMap<Route, Section>,
    pages: &BTreeMap<Route, Page>,
    output_dir: &Utf8Path,
    options: RenderOptions,
) -> Result<RenderStats> {
    // Ensure output directory exists
    fs::create_dir_all(output_dir)?;

    // Collect render data
    let section_data: Vec<_> = sections
        .values()
        .map(|s| RenderData {
            route: s.route.clone(),
            title: s.title.clone(),
            body_html: s.body_html.clone(),
        })
        .collect();

    let page_data: Vec<_> = pages
        .values()
        .map(|p| RenderData {
            route: p.route.clone(),
            title: p.title.clone(),
            body_html: p.body_html.clone(),
        })
        .collect();

    // Build sidebar info
    let sidebar_sections: Vec<SidebarSection> = sections
        .values()
        .map(|s| SidebarSection {
            route: s.route.clone(),
            title: s.title.clone(),
            weight: s.weight,
        })
        .collect();

    let sidebar_pages: Vec<SidebarPage> = pages
        .values()
        .map(|p| SidebarPage {
            route: p.route.clone(),
            title: p.title.clone(),
            weight: p.weight,
            section_route: p.section_route.clone(),
        })
        .collect();

    let sidebar_info = SidebarInfo {
        sections: sidebar_sections,
        pages: sidebar_pages,
    };

    // Render sections in parallel, collecting stats
    use std::sync::atomic::{AtomicUsize, Ordering};
    let written = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);

    section_data.par_iter().try_for_each(|data| {
        let was_written = render_item(data, &sidebar_info, output_dir, options)?;
        if was_written {
            written.fetch_add(1, Ordering::Relaxed);
        } else {
            skipped.fetch_add(1, Ordering::Relaxed);
        }
        Ok::<_, color_eyre::Report>(())
    })?;

    // Render pages in parallel
    page_data.par_iter().try_for_each(|data| {
        let was_written = render_item(data, &sidebar_info, output_dir, options)?;
        if was_written {
            written.fetch_add(1, Ordering::Relaxed);
        } else {
            skipped.fetch_add(1, Ordering::Relaxed);
        }
        Ok::<_, color_eyre::Report>(())
    })?;

    Ok(RenderStats {
        written: written.load(Ordering::Relaxed),
        skipped: skipped.load(Ordering::Relaxed),
    })
}

/// Data needed to render a page
struct RenderData {
    route: Route,
    title: Title,
    body_html: HtmlBody,
}

/// Sidebar section info (for rendering navigation)
#[derive(Clone)]
struct SidebarSection {
    route: Route,
    title: Title,
    weight: i32,
}

/// Sidebar page info
#[derive(Clone)]
struct SidebarPage {
    route: Route,
    title: Title,
    weight: i32,
    section_route: Route,
}

/// All sidebar information
struct SidebarInfo {
    sections: Vec<SidebarSection>,
    pages: Vec<SidebarPage>,
}

impl SidebarInfo {
    fn get_section(&self, route: &RouteRef) -> Option<&SidebarSection> {
        self.sections
            .iter()
            .find(|s| s.route.as_str() == route.as_str())
    }

    fn top_section_for(&self, route: &RouteRef) -> Option<&SidebarSection> {
        if route.is_in_section("learn") {
            self.get_section(RouteRef::from_static("/learn/"))
        } else if route.is_in_section("extend") {
            self.get_section(RouteRef::from_static("/extend/"))
        } else if route.is_in_section("contribute") {
            self.get_section(RouteRef::from_static("/contribute/"))
        } else {
            None
        }
    }

    fn pages_in_section(&self, section_route: &RouteRef) -> Vec<&SidebarPage> {
        let mut pages: Vec<_> = self
            .pages
            .iter()
            .filter(|p| p.section_route.as_str() == section_route.as_str())
            .collect();
        pages.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.title.as_str().cmp(b.title.as_str()))
        });
        pages
    }

    fn subsections(&self, section_route: &RouteRef) -> Vec<&SidebarSection> {
        let mut subs: Vec<_> = self
            .sections
            .iter()
            .filter(|s| {
                s.route.as_str() != section_route.as_str()
                    && s.route.as_str().starts_with(section_route.as_str())
                    && s.route.as_str()[section_route.as_str().len()..]
                        .trim_matches('/')
                        .chars()
                        .filter(|c| *c == '/')
                        .count()
                        == 0
            })
            .collect();
        subs.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.title.as_str().cmp(b.title.as_str()))
        });
        subs
    }
}

/// Render a single item (section or page)
/// Returns true if the file was actually written (content changed)
fn render_item(
    data: &RenderData,
    sidebar: &SidebarInfo,
    output_dir: &Utf8Path,
    options: RenderOptions,
) -> Result<bool> {
    let html = render_full_page(sidebar, &data.route, &data.title, &data.body_html, options);
    let new_content = html.into_string();

    let out_path = output_path(output_dir, &data.route);
    fs::create_dir_all(out_path.parent().unwrap_or(output_dir))?;

    // Only write if content actually changed
    let needs_write = match fs::read_to_string(&out_path) {
        Ok(existing) => existing != new_content,
        Err(_) => true, // File doesn't exist, need to write
    };

    if needs_write {
        fs::write(&out_path, new_content)?;
    }

    Ok(needs_write)
}

/// Get the output file path for a route
fn output_path(output_dir: &Utf8Path, route: &Route) -> Utf8PathBuf {
    let relative = route.as_str().trim_start_matches('/');
    if relative.is_empty() {
        output_dir.join("index.html")
    } else {
        output_dir.join(relative).join("index.html")
    }
}

/// Render a full HTML page with layout
fn render_full_page(
    sidebar: &SidebarInfo,
    route: &Route,
    title: &Title,
    body_html: &HtmlBody,
    options: RenderOptions,
) -> Markup {
    let has_sidebar = route.is_in_section("learn")
        || route.is_in_section("extend")
        || route.is_in_section("contribute");

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title.as_str()) " - facet" }
                link rel="stylesheet" href="/main.css";
                link rel="stylesheet" href="/pagefind/pagefind-ui.css";
                link rel="icon" type="image/png" href="/favicon.png";
            }
            body {
                (render_nav(route))

                @if has_sidebar {
                    div.docs-layout {
                        @if let Some(top_section) = sidebar.top_section_for(route.as_ref()) {
                            (render_sidebar(sidebar, top_section, route))
                        }
                        main.docs-content {
                            article {
                                h1.page-title { (title.as_str()) }
                                (PreEscaped(body_html.as_str()))
                            }
                        }
                    }
                } @else {
                    div.container {
                        main.content {
                            (PreEscaped(body_html.as_str()))
                        }
                    }
                }

                script src="/pagefind/pagefind-ui.js" {}
                (render_scripts())
                @if options.livereload {
                    (render_livereload_script())
                }
            }
        }
    }
}

/// Render the top navigation
fn render_nav(current_route: &Route) -> Markup {
    let in_learn = current_route.is_in_section("learn");
    let in_extend = current_route.is_in_section("extend");
    let in_contribute = current_route.is_in_section("contribute");

    html! {
        nav.site-nav {
            a.site-nav-brand href="/" {
                img.site-nav-logo src="/favicon.png" alt="";
                span { "facet" }
            }
            div.site-nav-links {
                a href="/learn/" class=[in_learn.then_some("active")] { "Learn" }
                a href="/extend/" class=[in_extend.then_some("active")] { "Extend" }
                a href="/contribute/" class=[in_contribute.then_some("active")] { "Contribute" }
            }
            div.site-nav-search id="search" {}
            a.site-nav-github href="https://github.com/facet-rs/facet" title="GitHub" {
                (github_icon())
            }
        }
    }
}

/// Render the sidebar navigation
fn render_sidebar(
    sidebar: &SidebarInfo,
    section: &SidebarSection,
    current_route: &Route,
) -> Markup {
    html! {
        aside.sidebar {
            nav {
                div.sidebar-header {
                    a href=(section.route.as_str()) { (section.title.as_str()) }
                }
                (render_section_tree(sidebar, &section.route, current_route))
            }
        }
    }
}

/// Recursively render a section's navigation tree
fn render_section_tree(
    sidebar: &SidebarInfo,
    section_route: &Route,
    current_route: &Route,
) -> Markup {
    let pages = sidebar.pages_in_section(section_route.as_ref());
    let subsections = sidebar.subsections(section_route.as_ref());

    if pages.is_empty() && subsections.is_empty() {
        return html! {};
    }

    html! {
        ul {
            @for page in pages {
                li {
                    a href=(page.route.as_str())
                      class=[is_active(&page.route, current_route).then_some("active")] {
                        (page.title.as_str())
                    }
                }
            }
            @for subsection in subsections {
                li.has-children {
                    a href=(subsection.route.as_str())
                      class=[is_active_or_ancestor(&subsection.route, current_route).then_some("active")] {
                        (subsection.title.as_str())
                    }
                    (render_section_tree(sidebar, &subsection.route, current_route))
                }
            }
        }
    }
}

fn is_active(route: &Route, current: &Route) -> bool {
    route == current
}

fn is_active_or_ancestor(section_route: &Route, current: &Route) -> bool {
    current.as_str().starts_with(section_route.as_str())
}

fn github_icon() -> Markup {
    html! {
        svg viewBox="0 0 16 16" width="24" height="24" fill="currentColor" {
            path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" {}
        }
    }
}

fn render_scripts() -> Markup {
    let script_content = r##"
document.addEventListener('DOMContentLoaded', function() {
    new PagefindUI({
        element: "#search",
        showSubResults: true,
        showImages: false,
        translations: { placeholder: "Search" }
    });

    document.addEventListener('keydown', function(e) {
        const searchInput = document.querySelector('#search input');
        if (!searchInput) return;
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
            e.preventDefault();
            searchInput.focus();
            searchInput.select();
        }
        if (e.key === '/' && e.target.tagName !== 'INPUT') {
            e.preventDefault();
            searchInput.focus();
            searchInput.select();
        }
    });
});
"##;
    html! {
        script {
            (PreEscaped(script_content))
        }
    }
}

/// A Salsa-backed filesystem for grass that tracks dependencies
#[derive(Debug)]
pub struct SalsaFs {
    /// Map of absolute path -> content (loaded through Salsa tracked queries)
    files: HashMap<PathBuf, Vec<u8>>,
    /// The sass directory root (for resolving relative paths)
    sass_dir: PathBuf,
}

impl SalsaFs {
    /// Create a new SalsaFs from the database and sass files
    pub fn new(
        db: &Database,
        sass_files: &BTreeMap<SassPath, SassFile>,
        sass_dir: &Utf8Path,
    ) -> Self {
        // Load all sass files through Salsa tracked query
        let sass_vec: Vec<_> = sass_files.values().copied().collect();
        let registry = SassRegistry::new(db, sass_vec);
        let content_map = load_all_sass(db, registry);

        // Build path -> content map with absolute paths
        let mut files = HashMap::new();
        for (rel_path, content) in content_map {
            let abs_path = sass_dir.join(&rel_path);
            files.insert(abs_path.into_std_path_buf(), content.into_bytes());
        }

        Self {
            files,
            sass_dir: sass_dir.as_std_path().to_path_buf(),
        }
    }
}

impl grass::Fs for SalsaFs {
    fn is_dir(&self, path: &Path) -> bool {
        // Check if path is the sass directory or a parent of any file
        if path == self.sass_dir {
            return true;
        }
        // Check if any file is under this directory
        self.files.keys().any(|f| f.starts_with(path))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn read(&self, path: &Path) -> std::result::Result<Vec<u8>, std::io::Error> {
        self.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {:?}", path),
            )
        })
    }
}

/// Compile Sass to CSS using Salsa-tracked file loading
pub fn compile_sass_tracked(
    db: &Database,
    sass_files: &BTreeMap<SassPath, SassFile>,
    sass_dir: &Utf8Path,
    output_dir: &Utf8Path,
) -> Result<()> {
    let main_scss = sass_dir.join("main.scss");

    if main_scss.exists() {
        // Create Salsa-backed filesystem
        let salsa_fs = SalsaFs::new(db, sass_files, sass_dir);

        // Compile with our custom Fs
        let options = grass::Options::default().fs(&salsa_fs);
        let css = grass::from_path(&main_scss, &options)
            .map_err(|e| color_eyre::eyre::eyre!("Sass compilation failed: {}", e))?;

        fs::write(output_dir.join("main.css"), css)?;
    }

    Ok(())
}

/// Compile Sass to CSS (legacy - no Salsa tracking)
pub fn compile_sass(content_dir: &Utf8Path, output_dir: &Utf8Path) -> Result<()> {
    let sass_dir = content_dir.parent().unwrap_or(content_dir).join("sass");
    let main_scss = sass_dir.join("main.scss");

    if main_scss.exists() {
        let css = grass::from_path(&main_scss, &grass::Options::default())
            .map_err(|e| color_eyre::eyre::eyre!("Sass compilation failed: {}", e))?;

        fs::write(output_dir.join("main.css"), css)?;
    }

    Ok(())
}

/// Render the live reload script (injected in serve mode)
fn render_livereload_script() -> Markup {
    let script_content = r##"
(function() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = protocol + '//' + window.location.host + '/__livereload';
    let ws;
    let reconnectTimer;

    function connect() {
        ws = new WebSocket(wsUrl);

        ws.onopen = function() {
            console.log('[livereload] connected');
        };

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

        ws.onerror = function() {
            ws.close();
        };
    }

    connect();
})();
"##;
    html! {
        script {
            (PreEscaped(script_content))
        }
    }
}

// ============================================================================
// Template-based rendering
// ============================================================================

/// Site configuration for templates
#[derive(Clone)]
pub struct SiteConfig {
    pub title: String,
    pub description: String,
    pub base_url: String,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            title: "facet".to_string(),
            description: "A Rust reflection library".to_string(),
            base_url: "/".to_string(),
        }
    }
}

/// Template-based site renderer
pub struct TemplateRenderer {
    engine: Engine,
    config: SiteConfig,
    sections: Arc<BTreeMap<Route, Section>>,
    pages: Arc<BTreeMap<Route, Page>>,
}

impl TemplateRenderer {
    /// Create a new template renderer with Salsa-backed template loading
    pub fn new(
        db: &Database,
        templates: &BTreeMap<TemplatePath, TemplateFile>,
        config: SiteConfig,
        sections: BTreeMap<Route, Section>,
        pages: BTreeMap<Route, Page>,
    ) -> Self {
        // Load templates through Salsa tracked queries for dependency tracking
        let loader = load_templates_tracked(db, templates);
        let engine = Engine::new(loader);

        Self {
            engine,
            config,
            sections: Arc::new(sections),
            pages: Arc::new(pages),
        }
    }

    /// Render all pages and sections
    pub fn render_all(
        &mut self,
        output_dir: &Utf8Path,
        options: RenderOptions,
    ) -> Result<RenderStats> {
        fs::create_dir_all(output_dir)?;

        let mut written = 0;
        let mut skipped = 0;

        // Render sections
        let section_routes: Vec<_> = self.sections.keys().cloned().collect();
        for route in section_routes {
            if let Some(section) = self.sections.get(&route).cloned() {
                let was_written = self.render_section(&section, output_dir, options)?;
                if was_written {
                    written += 1;
                } else {
                    skipped += 1;
                }
            }
        }

        // Render pages
        let page_routes: Vec<_> = self.pages.keys().cloned().collect();
        for route in page_routes {
            if let Some(page) = self.pages.get(&route).cloned() {
                let was_written = self.render_page(&page, output_dir, options)?;
                if was_written {
                    written += 1;
                } else {
                    skipped += 1;
                }
            }
        }

        Ok(RenderStats { written, skipped })
    }

    /// Render a single section
    fn render_section(
        &mut self,
        section: &Section,
        output_dir: &Utf8Path,
        options: RenderOptions,
    ) -> Result<bool> {
        let mut ctx = self.build_base_context();

        // Add section data
        let section_data = self.section_to_value(section);
        ctx.set("section", section_data);
        ctx.set(
            "current_path",
            Value::String(section.route.as_str().to_string()),
        );

        // Determine template: use index.html for root, section.html otherwise
        let template_name = if section.route.as_str() == "/" {
            "index.html"
        } else {
            "section.html"
        };

        let html = self
            .engine
            .render(template_name, &ctx)
            .map_err(|e| color_eyre::eyre::eyre!("{:?}", e))?;
        let html = self.inject_livereload(&html, options);

        self.write_output(output_dir, &section.route, &html)
    }

    /// Render a single page
    fn render_page(
        &mut self,
        page: &Page,
        output_dir: &Utf8Path,
        options: RenderOptions,
    ) -> Result<bool> {
        let mut ctx = self.build_base_context();

        // Add page data
        let page_data = self.page_to_value(page);
        ctx.set("page", page_data);
        ctx.set(
            "current_path",
            Value::String(page.route.as_str().to_string()),
        );

        let html = self
            .engine
            .render("page.html", &ctx)
            .map_err(|e| color_eyre::eyre::eyre!("{:?}", e))?;
        let html = self.inject_livereload(&html, options);

        self.write_output(output_dir, &page.route, &html)
    }

    /// Build the base context with config and global functions
    fn build_base_context(&self) -> Context {
        let mut ctx = Context::new();

        // Add config
        let mut config_map = HashMap::new();
        config_map.insert(
            "title".to_string(),
            Value::String(self.config.title.clone()),
        );
        config_map.insert(
            "description".to_string(),
            Value::String(self.config.description.clone()),
        );
        config_map.insert(
            "base_url".to_string(),
            Value::String(self.config.base_url.clone()),
        );
        ctx.set("config", Value::Dict(config_map));

        // Register get_url function
        let base_url = self.config.base_url.clone();
        ctx.register_fn(
            "get_url",
            Box::new(move |_args, kwargs| {
                let path = kwargs
                    .iter()
                    .find(|(k, _)| k == "path")
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_default();

                // Prepend base_url
                let url = if path.starts_with('/') {
                    path
                } else {
                    format!(
                        "{}{}",
                        base_url.trim_end_matches('/'),
                        if path.is_empty() {
                            "".to_string()
                        } else {
                            format!("/{}", path)
                        }
                    )
                };
                Ok(Value::String(url))
            }),
        );

        // Register get_section function
        let sections = self.sections.clone();
        let pages = self.pages.clone();
        ctx.register_fn(
            "get_section",
            Box::new(move |_args, kwargs| {
                let path = kwargs
                    .iter()
                    .find(|(k, _)| k == "path")
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_default();

                // Convert path like "learn/_index.md" to route "/learn/"
                let route = path_to_route(&path);

                // Find section by route
                if let Some(section) = sections.get(&route) {
                    let mut section_map = HashMap::new();
                    section_map.insert(
                        "title".to_string(),
                        Value::String(section.title.as_str().to_string()),
                    );
                    section_map.insert(
                        "permalink".to_string(),
                        Value::String(section.route.as_str().to_string()),
                    );
                    section_map.insert("path".to_string(), Value::String(path.clone()));
                    section_map.insert(
                        "content".to_string(),
                        Value::String(section.body_html.as_str().to_string()),
                    );

                    // Collect pages in this section
                    let section_pages: Vec<Value> = pages
                        .values()
                        .filter(|p| p.section_route == section.route)
                        .map(|p| {
                            let mut page_map = HashMap::new();
                            page_map.insert(
                                "title".to_string(),
                                Value::String(p.title.as_str().to_string()),
                            );
                            page_map.insert(
                                "permalink".to_string(),
                                Value::String(p.route.as_str().to_string()),
                            );
                            page_map.insert(
                                "path".to_string(),
                                Value::String(route_to_path(p.route.as_str())),
                            );
                            page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
                            Value::Dict(page_map)
                        })
                        .collect();
                    section_map.insert("pages".to_string(), Value::List(section_pages));

                    // Collect subsections
                    let subsections: Vec<Value> = sections
                        .values()
                        .filter(|s| {
                            s.route != section.route
                                && s.route.as_str().starts_with(section.route.as_str())
                                && s.route.as_str()[section.route.as_str().len()..]
                                    .trim_matches('/')
                                    .chars()
                                    .filter(|c| *c == '/')
                                    .count()
                                    == 0
                        })
                        .map(|s| Value::String(route_to_path(s.route.as_str())))
                        .collect();
                    section_map.insert("subsections".to_string(), Value::List(subsections));

                    Ok(Value::Dict(section_map))
                } else {
                    Ok(Value::None)
                }
            }),
        );

        ctx
    }

    /// Convert a Section to a Value for the template context
    fn section_to_value(&self, section: &Section) -> Value {
        let mut map = HashMap::new();
        map.insert(
            "title".to_string(),
            Value::String(section.title.as_str().to_string()),
        );
        map.insert(
            "content".to_string(),
            Value::String(section.body_html.as_str().to_string()),
        );
        map.insert(
            "permalink".to_string(),
            Value::String(section.route.as_str().to_string()),
        );
        map.insert(
            "path".to_string(),
            Value::String(route_to_path(section.route.as_str())),
        );
        map.insert("weight".to_string(), Value::Int(section.weight as i64));

        // Add pages in this section
        let section_pages: Vec<Value> = self
            .pages
            .values()
            .filter(|p| p.section_route == section.route)
            .map(|p| {
                let mut page_map = HashMap::new();
                page_map.insert(
                    "title".to_string(),
                    Value::String(p.title.as_str().to_string()),
                );
                page_map.insert(
                    "permalink".to_string(),
                    Value::String(p.route.as_str().to_string()),
                );
                page_map.insert(
                    "path".to_string(),
                    Value::String(route_to_path(p.route.as_str())),
                );
                page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
                Value::Dict(page_map)
            })
            .collect();
        map.insert("pages".to_string(), Value::List(section_pages));

        // Add subsections
        let subsections: Vec<Value> = self
            .sections
            .values()
            .filter(|s| {
                s.route != section.route
                    && s.route.as_str().starts_with(section.route.as_str())
                    && s.route.as_str()[section.route.as_str().len()..]
                        .trim_matches('/')
                        .chars()
                        .filter(|c| *c == '/')
                        .count()
                        == 0
            })
            .map(|s| Value::String(route_to_path(s.route.as_str())))
            .collect();
        map.insert("subsections".to_string(), Value::List(subsections));

        // Empty TOC for now
        map.insert("toc".to_string(), Value::List(vec![]));

        Value::Dict(map)
    }

    /// Convert a Page to a Value for the template context
    fn page_to_value(&self, page: &Page) -> Value {
        let mut map = HashMap::new();
        map.insert(
            "title".to_string(),
            Value::String(page.title.as_str().to_string()),
        );
        map.insert(
            "content".to_string(),
            Value::String(page.body_html.as_str().to_string()),
        );
        map.insert(
            "permalink".to_string(),
            Value::String(page.route.as_str().to_string()),
        );
        map.insert(
            "path".to_string(),
            Value::String(route_to_path(page.route.as_str())),
        );
        map.insert("weight".to_string(), Value::Int(page.weight as i64));

        // Empty TOC for now
        map.insert("toc".to_string(), Value::List(vec![]));

        Value::Dict(map)
    }

    /// Inject livereload script if enabled
    fn inject_livereload(&self, html: &str, options: RenderOptions) -> String {
        if options.livereload {
            let livereload_script = r##"<script>
(function() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = protocol + '//' + window.location.host + '/__livereload';
    let ws;
    let reconnectTimer;

    function connect() {
        ws = new WebSocket(wsUrl);

        ws.onopen = function() {
            console.log('[livereload] connected');
        };

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

        ws.onerror = function() {
            ws.close();
        };
    }

    connect();
})();
</script>"##;
            // Insert before </body>
            html.replace("</body>", &format!("{}</body>", livereload_script))
        } else {
            html.to_string()
        }
    }

    /// Write output file, returning true if content changed
    fn write_output(&self, output_dir: &Utf8Path, route: &Route, content: &str) -> Result<bool> {
        let out_path = output_path(output_dir, route);
        fs::create_dir_all(out_path.parent().unwrap_or(output_dir))?;

        let needs_write = match fs::read_to_string(&out_path) {
            Ok(existing) => existing != content,
            Err(_) => true,
        };

        if needs_write {
            fs::write(&out_path, content)?;
        }

        Ok(needs_write)
    }
}

/// Convert a source path like "learn/_index.md" to a route like "/learn/"
fn path_to_route(path: &str) -> Route {
    let mut p = path.to_string();

    // Remove .md extension
    if p.ends_with(".md") {
        p = p[..p.len() - 3].to_string();
    }

    // Handle _index
    if p.ends_with("/_index") {
        p = p[..p.len() - 7].to_string();
    } else if p == "_index" {
        p = String::new();
    }

    // Ensure leading and trailing slashes
    if p.is_empty() {
        Route::root()
    } else {
        Route::new(format!("/{}/", p))
    }
}

/// Convert a route like "/learn/" back to a path like "learn/_index.md"
fn route_to_path(route: &str) -> String {
    let r = route.trim_matches('/');
    if r.is_empty() {
        "_index.md".to_string()
    } else {
        format!("{}/_index.md", r)
    }
}

// ============================================================================
// Pure render functions for Salsa tracked queries
// ============================================================================

use crate::db::SiteTree;

/// Pure function to render a page to HTML (for Salsa tracking)
pub fn render_page_to_html(
    page: &Page,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> String {
    let mut loader = InMemoryLoader::new();
    for (path, content) in templates {
        loader.add(path.clone(), content.clone());
    }
    let mut engine = Engine::new(loader);

    let mut ctx = build_render_context(site_tree);
    ctx.set("page", page_to_value(page));
    ctx.set(
        "current_path",
        Value::String(page.route.as_str().to_string()),
    );

    engine
        .render("page.html", &ctx)
        .unwrap_or_else(|e| format!("<!-- Render error: {:?} -->", e))
}

/// Pure function to render a section to HTML (for Salsa tracking)
pub fn render_section_to_html(
    section: &Section,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> String {
    let mut loader = InMemoryLoader::new();
    for (path, content) in templates {
        loader.add(path.clone(), content.clone());
    }
    let mut engine = Engine::new(loader);

    let mut ctx = build_render_context(site_tree);
    ctx.set("section", section_to_value(section, site_tree));
    ctx.set(
        "current_path",
        Value::String(section.route.as_str().to_string()),
    );

    let template_name = if section.route.as_str() == "/" {
        "index.html"
    } else {
        "section.html"
    };

    engine
        .render(template_name, &ctx)
        .unwrap_or_else(|e| format!("<!-- Render error: {:?} -->", e))
}

/// Build the render context with config and global functions
fn build_render_context(site_tree: &SiteTree) -> Context {
    let mut ctx = Context::new();

    // Add config
    let mut config_map = HashMap::new();
    config_map.insert("title".to_string(), Value::String("facet".to_string()));
    config_map.insert(
        "description".to_string(),
        Value::String("A Rust reflection library".to_string()),
    );
    config_map.insert("base_url".to_string(), Value::String("/".to_string()));
    ctx.set("config", Value::Dict(config_map));

    // Register get_url function
    ctx.register_fn(
        "get_url",
        Box::new(move |_args, kwargs| {
            let path = kwargs
                .iter()
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();

            let url = if path.starts_with('/') {
                path
            } else if path.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", path)
            };
            Ok(Value::String(url))
        }),
    );

    // Register get_section function
    let sections = site_tree.sections.clone();
    let pages = site_tree.pages.clone();
    ctx.register_fn(
        "get_section",
        Box::new(move |_args, kwargs| {
            let path = kwargs
                .iter()
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();

            let route = path_to_route(&path);

            if let Some(section) = sections.get(&route) {
                let mut section_map = HashMap::new();
                section_map.insert(
                    "title".to_string(),
                    Value::String(section.title.as_str().to_string()),
                );
                section_map.insert(
                    "permalink".to_string(),
                    Value::String(section.route.as_str().to_string()),
                );
                section_map.insert("path".to_string(), Value::String(path.clone()));
                section_map.insert(
                    "content".to_string(),
                    Value::String(section.body_html.as_str().to_string()),
                );

                let section_pages: Vec<Value> = pages
                    .values()
                    .filter(|p| p.section_route == section.route)
                    .map(|p| {
                        let mut page_map = HashMap::new();
                        page_map.insert(
                            "title".to_string(),
                            Value::String(p.title.as_str().to_string()),
                        );
                        page_map.insert(
                            "permalink".to_string(),
                            Value::String(p.route.as_str().to_string()),
                        );
                        page_map.insert(
                            "path".to_string(),
                            Value::String(route_to_path(p.route.as_str())),
                        );
                        page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
                        Value::Dict(page_map)
                    })
                    .collect();
                section_map.insert("pages".to_string(), Value::List(section_pages));

                let subsections: Vec<Value> = sections
                    .values()
                    .filter(|s| {
                        s.route != section.route
                            && s.route.as_str().starts_with(section.route.as_str())
                            && s.route.as_str()[section.route.as_str().len()..]
                                .trim_matches('/')
                                .chars()
                                .filter(|c| *c == '/')
                                .count()
                                == 0
                    })
                    .map(|s| Value::String(route_to_path(s.route.as_str())))
                    .collect();
                section_map.insert("subsections".to_string(), Value::List(subsections));

                Ok(Value::Dict(section_map))
            } else {
                Ok(Value::None)
            }
        }),
    );

    ctx
}

/// Convert a Page to a Value for template context
fn page_to_value(page: &Page) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "title".to_string(),
        Value::String(page.title.as_str().to_string()),
    );
    map.insert(
        "content".to_string(),
        Value::String(page.body_html.as_str().to_string()),
    );
    map.insert(
        "permalink".to_string(),
        Value::String(page.route.as_str().to_string()),
    );
    map.insert(
        "path".to_string(),
        Value::String(route_to_path(page.route.as_str())),
    );
    map.insert("weight".to_string(), Value::Int(page.weight as i64));
    map.insert("toc".to_string(), Value::List(vec![]));
    Value::Dict(map)
}

/// Convert a Section to a Value for template context
fn section_to_value(section: &Section, site_tree: &SiteTree) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "title".to_string(),
        Value::String(section.title.as_str().to_string()),
    );
    map.insert(
        "content".to_string(),
        Value::String(section.body_html.as_str().to_string()),
    );
    map.insert(
        "permalink".to_string(),
        Value::String(section.route.as_str().to_string()),
    );
    map.insert(
        "path".to_string(),
        Value::String(route_to_path(section.route.as_str())),
    );
    map.insert("weight".to_string(), Value::Int(section.weight as i64));

    // Add pages in this section
    let section_pages: Vec<Value> = site_tree
        .pages
        .values()
        .filter(|p| p.section_route == section.route)
        .map(|p| {
            let mut page_map = HashMap::new();
            page_map.insert(
                "title".to_string(),
                Value::String(p.title.as_str().to_string()),
            );
            page_map.insert(
                "permalink".to_string(),
                Value::String(p.route.as_str().to_string()),
            );
            page_map.insert(
                "path".to_string(),
                Value::String(route_to_path(p.route.as_str())),
            );
            page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
            Value::Dict(page_map)
        })
        .collect();
    map.insert("pages".to_string(), Value::List(section_pages));

    // Add subsections
    let subsections: Vec<Value> = site_tree
        .sections
        .values()
        .filter(|s| {
            s.route != section.route
                && s.route.as_str().starts_with(section.route.as_str())
                && s.route.as_str()[section.route.as_str().len()..]
                    .trim_matches('/')
                    .chars()
                    .filter(|c| *c == '/')
                    .count()
                    == 0
        })
        .map(|s| Value::String(route_to_path(s.route.as_str())))
        .collect();
    map.insert("subsections".to_string(), Value::List(subsections));
    map.insert("toc".to_string(), Value::List(vec![]));

    Value::Dict(map)
}
