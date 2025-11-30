use crate::db::{
    CharSet, Db, Heading, OgImageOutput, OgTemplateFile, OutputFile, Page, ParsedData,
    RenderedHtml, SassFile, SassRegistry, Section, SiteOutput, SiteTree, SourceFile,
    SourceRegistry, StaticFile, StaticRegistry, TemplateFile, TemplateRegistry,
};
use crate::types::{HtmlBody, Route, SassContent, StaticPath, TemplateContent, Title};
use crate::url_rewrite::{rewrite_urls_in_css, rewrite_urls_in_html};
use facet::Facet;
use facet_value::Value;
use pulldown_cmark::{Options, Parser, html};
use std::collections::{BTreeMap, HashMap};

/// Load a template file's content - tracked by Salsa for dependency tracking
#[salsa::tracked]
pub fn load_template(db: &dyn Db, template: TemplateFile) -> TemplateContent {
    template.content(db).clone()
}

/// Load all templates and return a map of path -> content
/// This tracked query records dependencies on all template files
#[salsa::tracked]
pub fn load_all_templates<'db>(
    db: &'db dyn Db,
    registry: TemplateRegistry<'db>,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for template in registry.templates(db) {
        let path = template.path(db).as_str().to_string();
        let content = load_template(db, *template);
        result.insert(path, content.as_str().to_string());
    }
    result
}

/// Load a sass file's content - tracked by Salsa for dependency tracking
#[salsa::tracked]
pub fn load_sass(db: &dyn Db, sass: SassFile) -> SassContent {
    sass.content(db).clone()
}

/// Load all sass files and return a map of path -> content
/// This tracked query records dependencies on all sass files
#[salsa::tracked]
pub fn load_all_sass<'db>(db: &'db dyn Db, registry: SassRegistry<'db>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for sass in registry.files(db) {
        let path = sass.path(db).as_str().to_string();
        let content = load_sass(db, *sass);
        result.insert(path, content.as_str().to_string());
    }
    result
}

/// Compiled CSS output
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompiledCss(pub String);

/// Compile SASS to CSS - tracked by Salsa for dependency tracking
/// Returns None if compilation fails
#[salsa::tracked]
pub fn compile_sass<'db>(db: &'db dyn Db, registry: SassRegistry<'db>) -> Option<CompiledCss> {
    // Load all sass files - creates dependency on each
    let sass_map = load_all_sass(db, registry);

    // Find main.scss
    let main_content = sass_map.get("main.scss")?;

    // Create an in-memory filesystem for grass
    let fs = InMemorySassFs::new(&sass_map);

    // Compile with grass using in-memory fs
    let options = grass::Options::default().fs(&fs);

    match grass::from_string(main_content.clone(), &options) {
        Ok(css) => Some(CompiledCss(css)),
        Err(e) => {
            tracing::error!("SASS compilation failed: {}", e);
            None
        }
    }
}

/// In-memory filesystem for grass SASS compiler
#[derive(Debug)]
struct InMemorySassFs {
    files: HashMap<std::path::PathBuf, Vec<u8>>,
}

impl InMemorySassFs {
    fn new(sass_map: &HashMap<String, String>) -> Self {
        let files = sass_map
            .iter()
            .map(|(path, content)| (std::path::PathBuf::from(path), content.as_bytes().to_vec()))
            .collect();
        Self { files }
    }
}

impl grass::Fs for InMemorySassFs {
    fn is_dir(&self, path: &std::path::Path) -> bool {
        // Check if any file is under this directory
        self.files.keys().any(|f| f.starts_with(path))
    }

    fn is_file(&self, path: &std::path::Path) -> bool {
        self.files.contains_key(path)
    }

    fn read(&self, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
        self.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {path:?}"),
            )
        })
    }
}

/// Frontmatter parsed from TOML
///
/// Known fields are extracted; unknown fields are ignored.
#[derive(Debug, Clone, Default, Facet)]
#[allow(dead_code)] // Fields reserved for future template use
pub struct Frontmatter {
    #[facet(default)]
    pub title: String,
    #[facet(default)]
    pub weight: i32,
    pub description: Option<String>,
    pub template: Option<String>,
}

/// Parse a source file into ParsedData
/// This is the main tracked function - Salsa memoizes the result
#[salsa::tracked]
pub fn parse_file(db: &dyn Db, source: SourceFile) -> ParsedData {
    let content = source.content(db);
    let path = source.path(db);

    // Split frontmatter and body
    let (frontmatter_str, markdown) = split_frontmatter(content.as_str());

    // Parse frontmatter as Value first, then convert to Frontmatter
    // This allows unknown fields to be silently ignored
    let frontmatter: Frontmatter = if frontmatter_str.is_empty() {
        Frontmatter::default()
    } else {
        match facet_toml::from_str::<Value>(&frontmatter_str) {
            Ok(value) => facet_value::from_value(value).unwrap_or_default(),
            Err(e) => {
                eprintln!("Failed to parse frontmatter for {path:?}: {e:?}");
                eprintln!("Frontmatter was:\n{frontmatter_str}");
                Frontmatter::default()
            }
        }
    };

    // Convert markdown to HTML and extract headings
    let (html, headings) = render_markdown(&markdown);
    let body_html = HtmlBody::new(html);

    // Determine if this is a section (_index.md)
    let is_section = path.is_section_index();

    // Compute URL route
    let route = path.to_route();

    ParsedData {
        source_path: path.clone(),
        route,
        title: Title::new(frontmatter.title),
        weight: frontmatter.weight,
        body_html,
        is_section,
        headings,
    }
}

/// Build the site tree from all source files
/// This tracked query depends on all parse_file results
#[salsa::tracked]
pub fn build_tree<'db>(db: &'db dyn Db, sources: SourceRegistry<'db>) -> SiteTree {
    let mut sections: BTreeMap<Route, Section> = BTreeMap::new();
    let mut pages: BTreeMap<Route, Page> = BTreeMap::new();

    // Parse all files - this creates dependencies on each parse_file
    let parsed: Vec<ParsedData> = sources
        .sources(db)
        .iter()
        .map(|source| parse_file(db, *source))
        .collect();

    // First pass: create all sections
    for data in parsed.iter().filter(|d| d.is_section) {
        sections.insert(
            data.route.clone(),
            Section {
                route: data.route.clone(),
                title: data.title.clone(),
                weight: data.weight,
                body_html: data.body_html.clone(),
                headings: data.headings.clone(),
            },
        );
    }

    // Ensure root section exists
    sections.entry(Route::root()).or_insert_with(|| Section {
        route: Route::root(),
        title: Title::from_static("Home"),
        weight: 0,
        body_html: HtmlBody::from_static(""),
        headings: Vec::new(),
    });

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
                headings: data.headings.clone(),
            },
        );
    }

    SiteTree { sections, pages }
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

/// Render a single page to HTML
/// This tracked query depends on the page content, templates, and site tree
#[salsa::tracked]
pub fn render_page<'db>(
    db: &'db dyn Db,
    route: Route,
    sources: SourceRegistry<'db>,
    templates: TemplateRegistry<'db>,
) -> RenderedHtml {
    use crate::render::render_page_to_html;

    // Build tree (cached by Salsa)
    let site_tree = build_tree(db, sources);

    // Load templates (cached by Salsa)
    let template_map = load_all_templates(db, templates);

    // Find the page
    let page = site_tree
        .pages
        .get(&route)
        .expect("Page not found for route");

    // Render to HTML
    let html = render_page_to_html(page, &site_tree, &template_map);
    RenderedHtml(html)
}

/// Render a single section to HTML
/// This tracked query depends on the section content, templates, and site tree
#[salsa::tracked]
pub fn render_section<'db>(
    db: &'db dyn Db,
    route: Route,
    sources: SourceRegistry<'db>,
    templates: TemplateRegistry<'db>,
) -> RenderedHtml {
    use crate::render::render_section_to_html;

    // Build tree (cached by Salsa)
    let site_tree = build_tree(db, sources);

    // Load templates (cached by Salsa)
    let template_map = load_all_templates(db, templates);

    // Find the section
    let section = site_tree
        .sections
        .get(&route)
        .expect("Section not found for route");

    // Render to HTML
    let html = render_section_to_html(section, &site_tree, &template_map);
    RenderedHtml(html)
}

/// Load a single static file's content - tracked by Salsa
#[salsa::tracked]
pub fn load_static(db: &dyn Db, file: StaticFile) -> Vec<u8> {
    file.content(db).clone()
}

/// Load all static files - returns map of path -> content
#[salsa::tracked]
pub fn load_all_static<'db>(
    db: &'db dyn Db,
    registry: StaticRegistry<'db>,
) -> HashMap<String, Vec<u8>> {
    let mut result = HashMap::new();
    for file in registry.files(db) {
        let path = file.path(db).as_str().to_string();
        let content = load_static(db, *file);
        result.insert(path, content);
    }
    result
}

/// Subset a font file to only include specified characters
/// Returns WOFF2 compressed bytes, or None if subsetting fails
#[salsa::tracked]
pub fn subset_font<'db>(
    db: &'db dyn Db,
    font_file: StaticFile,
    chars: CharSet<'db>,
) -> Option<Vec<u8>> {
    use crate::font_subsetter::subset_font_to_woff2;
    use std::collections::HashSet;

    let font_data = font_file.content(db);
    let char_set: HashSet<char> = chars.chars(db).iter().copied().collect();

    match subset_font_to_woff2(font_data, &char_set) {
        Ok(subsetted) => Some(subsetted),
        Err(e) => {
            tracing::warn!(
                "Failed to subset font {}: {}",
                font_file.path(db).as_str(),
                e
            );
            None
        }
    }
}

/// Build the complete site - THE top-level query
/// This produces all output files that need to be written to disk.
/// Fonts are automatically subsetted, all assets are cache-busted.
#[salsa::tracked]
pub fn build_site<'db>(
    db: &'db dyn Db,
    sources: SourceRegistry<'db>,
    templates: TemplateRegistry<'db>,
    sass: SassRegistry<'db>,
    static_files: StaticRegistry<'db>,
) -> SiteOutput {
    use crate::cache_bust::{cache_busted_path, content_hash};
    use crate::font_subsetter;

    // Build the site tree (tracked via Salsa)
    let site_tree = build_tree(db, sources);

    // --- Phase 1: Render HTML (need content for font analysis) ---
    let mut html_outputs: Vec<(Route, String)> = Vec::new();

    for route in site_tree.sections.keys() {
        let rendered = render_section(db, route.clone(), sources, templates);
        html_outputs.push((route.clone(), rendered.0));
    }

    for route in site_tree.pages.keys() {
        let rendered = render_page(db, route.clone(), sources, templates);
        html_outputs.push((route.clone(), rendered.0));
    }

    // --- Phase 2: Compile CSS ---
    let css_content = compile_sass(db, sass);
    let css_str = css_content.as_ref().map(|c| c.0.as_str()).unwrap_or("");

    // --- Phase 3: Analyze fonts for subsetting ---
    let html_refs: Vec<&str> = html_outputs.iter().map(|(_, h)| h.as_str()).collect();
    let combined_html = html_refs.join("\n");
    let inline_css = font_subsetter::extract_css_from_html(&combined_html);
    let all_css = format!("{css_str}\n{inline_css}");
    let font_analysis = font_subsetter::analyze_fonts(&combined_html, &all_css);

    // --- Phase 4: Process static files (with font subsetting) and build path mapping ---
    // Maps original path (e.g., "fonts/Inter.woff2") to (new_path, content)
    let mut static_outputs: HashMap<String, (String, Vec<u8>)> = HashMap::new();

    for file in static_files.files(db) {
        let path = file.path(db).as_str();

        // Get content (possibly subsetted for fonts)
        let content = if is_font_file(path) {
            if let Some(chars) = find_chars_for_font_file(path, &font_analysis) {
                if !chars.is_empty() {
                    let mut sorted_chars: Vec<char> = chars.into_iter().collect();
                    sorted_chars.sort();
                    let char_set = CharSet::new(db, sorted_chars);

                    if let Some(subsetted) = subset_font(db, *file, char_set) {
                        subsetted
                    } else {
                        load_static(db, *file)
                    }
                } else {
                    load_static(db, *file)
                }
            } else {
                load_static(db, *file)
            }
        } else {
            load_static(db, *file)
        };

        // Hash content and generate cache-busted path
        let hash = content_hash(&content);
        let new_path = cache_busted_path(path, &hash);

        static_outputs.insert(path.to_string(), (new_path, content));
    }

    // Build path rewrite map: "/fonts/Inter.woff2" -> "/fonts/Inter.a1b2c3d4.woff2"
    let static_path_map: HashMap<String, String> = static_outputs
        .iter()
        .map(|(old, (new, _))| (format!("/{old}"), format!("/{new}")))
        .collect();

    // --- Phase 5: Rewrite CSS and hash it ---
    let (css_path, css_final) = if let Some(ref css) = css_content {
        let rewritten_css = rewrite_urls_in_css(&css.0, &static_path_map);
        let css_hash = content_hash(rewritten_css.as_bytes());
        let css_path = cache_busted_path("main.css", &css_hash);
        (Some(css_path), Some(rewritten_css))
    } else {
        (None, None)
    };

    // Add CSS to path map for HTML rewriting
    let mut all_path_map = static_path_map;
    if let Some(ref path) = css_path {
        all_path_map.insert("/main.css".to_string(), format!("/{path}"));
    }

    // --- Phase 6: Rewrite HTML ---
    let mut files = Vec::new();

    for (route, html) in html_outputs {
        let rewritten_html = rewrite_urls_in_html(&html, &all_path_map);
        files.push(OutputFile::Html {
            route,
            content: rewritten_html,
        });
    }

    // --- Phase 6b: Generate OG images for all pages and sections ---
    // OG images are placed at /og/<route-slug>.svg and /og/<route-slug>.png
    for section in site_tree.sections.values() {
        if let Some(og) = render_og_image(db, section.title.clone(), None, None) {
            let route_slug = route_to_og_slug(&section.route);
            files.push(OutputFile::Static {
                path: StaticPath::new(format!("og/{route_slug}.svg")),
                content: og.svg.into_bytes(),
            });
            files.push(OutputFile::Static {
                path: StaticPath::new(format!("og/{route_slug}.png")),
                content: og.png,
            });
        }
    }

    for page in site_tree.pages.values() {
        if let Some(og) = render_og_image(db, page.title.clone(), None, None) {
            let route_slug = route_to_og_slug(&page.route);
            files.push(OutputFile::Static {
                path: StaticPath::new(format!("og/{route_slug}.svg")),
                content: og.svg.into_bytes(),
            });
            files.push(OutputFile::Static {
                path: StaticPath::new(format!("og/{route_slug}.png")),
                content: og.png,
            });
        }
    }

    // --- Phase 7: Add CSS and static files to output ---
    if let (Some(path), Some(content)) = (css_path, css_final) {
        files.push(OutputFile::Css {
            path: StaticPath::new(path),
            content,
        });
    }

    for (new_path, content) in static_outputs.into_values() {
        files.push(OutputFile::Static {
            path: StaticPath::new(new_path),
            content,
        });
    }

    SiteOutput { files }
}

/// Convert a route to a slug for OG image filenames
///
/// Examples:
/// - "/" -> "index"
/// - "/learn/" -> "learn"
/// - "/learn/page/" -> "learn-page"
fn route_to_og_slug(route: &Route) -> String {
    let trimmed = route.as_str().trim_matches('/');
    if trimmed.is_empty() {
        "index".to_string()
    } else {
        trimmed.replace('/', "-")
    }
}

/// Check if a path is a font file
fn is_font_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".ttf")
        || lower.ends_with(".otf")
        || lower.ends_with(".woff")
        || lower.ends_with(".woff2")
}

/// Find the character set needed for a font file based on @font-face analysis
fn find_chars_for_font_file(
    path: &str,
    analysis: &crate::font_subsetter::FontAnalysis,
) -> Option<std::collections::HashSet<char>> {
    // Normalize path for comparison (remove leading slash if present)
    let normalized = path.trim_start_matches('/');

    // Find @font-face rules that reference this font file
    for face in &analysis.font_faces {
        let face_src = face.src.trim_start_matches('/');
        if face_src == normalized {
            // Found a match - return chars for this font-family
            return analysis.chars_per_font.get(&face.family).cloned();
        }
    }

    None
}

/// Split content into frontmatter and body
fn split_frontmatter(content: &str) -> (String, String) {
    let content = content.trim_start();

    // Check for +++ delimiters (TOML frontmatter)
    if let Some(rest) = content.strip_prefix("+++") {
        if let Some(end) = rest.find("+++") {
            let frontmatter = rest[..end].trim().to_string();
            let body = rest[end + 3..].trim_start().to_string();
            return (frontmatter, body);
        }
    }

    // No frontmatter found
    (String::new(), content.to_string())
}

/// Render markdown to HTML, resolving internal links and extracting headings
fn render_markdown(markdown: &str) -> (String, Vec<Heading>) {
    use pulldown_cmark::{Event, HeadingLevel, Tag};

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES;

    let parser = Parser::new_ext(markdown, options);

    // Collect headings while processing
    let mut headings = Vec::new();
    let mut current_heading: Option<(u8, String, String)> = None; // (level, id, text)

    // Transform events to resolve @/ links and extract headings
    let events: Vec<Event> = parser
        .map(|event| match event {
            Event::Start(Tag::Heading { level, ref id, .. }) => {
                let level_num = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_heading = Some((
                    level_num,
                    id.as_ref().map(|s| s.to_string()).unwrap_or_default(),
                    String::new(),
                ));
                event
            }
            Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                if let Some((level, id, text)) = current_heading.take() {
                    // Generate ID from text if not provided
                    let id = if id.is_empty() { slugify(&text) } else { id };
                    headings.push(Heading {
                        title: text,
                        id,
                        level,
                    });
                }
                event
            }
            Event::Text(ref text) | Event::Code(ref text) => {
                if let Some((_, _, ref mut heading_text)) = current_heading {
                    heading_text.push_str(text);
                }
                event
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let resolved = resolve_internal_link(&dest_url);
                Event::Start(Tag::Link {
                    link_type,
                    dest_url: resolved.into(),
                    title,
                    id,
                })
            }
            other => other,
        })
        .collect();

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());

    // Also extract headings from any inline HTML in the output
    let html_headings = extract_html_headings(&html_output);

    // Merge: add HTML headings that aren't duplicates (by id)
    for h in html_headings {
        if !headings.iter().any(|existing| existing.id == h.id) {
            headings.push(h);
        }
    }

    (html_output, headings)
}

/// Convert text to a URL-safe slug for heading IDs
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Extract headings from HTML content (for inline HTML headings)
fn extract_html_headings(html: &str) -> Vec<Heading> {
    use regex::Regex;

    let mut headings = Vec::new();

    // Match <h1> through <h6> tags with optional id attribute
    // Pattern: <h[1-6](?:\s+id="([^"]*)")?>([^<]*)</h[1-6]>
    let re = Regex::new(r#"<h([1-6])(?:\s[^>]*?id="([^"]*)"[^>]*)?>([^<]*)</h[1-6]>"#).unwrap();

    for cap in re.captures_iter(html) {
        let level: u8 = cap[1].parse().unwrap_or(1);
        let id = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let title = cap[3].trim().to_string();

        if !title.is_empty() {
            let id = if id.is_empty() { slugify(&title) } else { id };
            headings.push(Heading { title, id, level });
        }
    }

    headings
}

/// Render an OG image for a page/section
///
/// This query is tracked by Salsa for caching. If the title and template
/// haven't changed, the cached image is returned.
#[salsa::tracked]
pub fn render_og_image(
    db: &dyn Db,
    title: Title,
    description: Option<String>,
    template: Option<OgTemplateFile>,
) -> Option<OgImageOutput> {
    use crate::og;
    use std::collections::HashMap;

    // Get template content (use provided or default)
    let template_content = template
        .map(|t| t.content(db).clone())
        .unwrap_or_else(|| og::DEFAULT_TEMPLATE.to_string());

    // Build variables for the template
    let mut vars = HashMap::new();
    vars.insert("title".to_string(), title.as_str().to_string());
    if let Some(desc) = description {
        vars.insert("description".to_string(), desc);
    }

    // Render the OG image
    match og::render_og_image(&template_content, &vars) {
        Ok(image) => Some(OgImageOutput {
            svg: image.svg,
            png: image.png,
        }),
        Err(e) => {
            tracing::warn!("Failed to render OG image for '{}': {}", title.as_str(), e);
            None
        }
    }
}

/// Resolve Zola-style @/ internal links to URL paths
fn resolve_internal_link(link: &str) -> String {
    if let Some(path) = link.strip_prefix("@/") {
        // Convert @/learn/_index.md -> /learn/
        // Convert @/learn/page.md -> /learn/page/
        let mut path = path.to_string();

        // Remove .md extension
        if path.ends_with(".md") {
            path = path[..path.len() - 3].to_string();
        }

        // Handle _index -> parent directory
        if path.ends_with("/_index") {
            path = path[..path.len() - 7].to_string();
        } else if path == "_index" {
            path = String::new();
        }

        // Ensure leading and trailing slashes
        if path.is_empty() {
            "/".to_string()
        } else {
            format!("/{path}/")
        }
    } else {
        link.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let content = r#"+++
title = "Hello"
weight = 10
+++

# Content here
"#;
        let (fm, body) = split_frontmatter(content);
        assert!(fm.contains("title"));
        assert!(body.contains("# Content"));
    }

    #[test]
    fn test_resolve_internal_link() {
        // Section index files
        assert_eq!(resolve_internal_link("@/learn/_index.md"), "/learn/");
        assert_eq!(
            resolve_internal_link("@/learn/showcases/_index.md"),
            "/learn/showcases/"
        );
        assert_eq!(resolve_internal_link("@/_index.md"), "/");

        // Regular pages
        assert_eq!(resolve_internal_link("@/learn/page.md"), "/learn/page/");
        assert_eq!(
            resolve_internal_link("@/learn/migration/serde.md"),
            "/learn/migration/serde/"
        );

        // External links unchanged
        assert_eq!(
            resolve_internal_link("https://example.com"),
            "https://example.com"
        );
        assert_eq!(resolve_internal_link("/some/path/"), "/some/path/");
    }
}
