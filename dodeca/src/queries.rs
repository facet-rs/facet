use crate::db::{
    Db, Heading, Page, ParsedData, RenderedHtml, SassFile, SassRegistry, Section, SiteTree,
    SourceFile, SourceRegistry, TemplateFile, TemplateRegistry,
};
use crate::types::{HtmlBody, Route, SassContent, TemplateContent, Title};
use facet::Facet;
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

/// Frontmatter parsed via facet-toml
#[derive(Debug, Clone, Facet, Default)]
pub struct Frontmatter {
    #[facet(default)]
    pub title: String,

    #[facet(default)]
    pub weight: i32,

    #[facet(default)]
    pub description: Option<String>,

    #[facet(default)]
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

    // Parse frontmatter using facet-toml
    let frontmatter: Frontmatter = if frontmatter_str.is_empty() {
        Frontmatter::default()
    } else {
        facet_toml::from_str(&frontmatter_str).unwrap_or_default()
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
    if !sections.contains_key(&Route::root()) {
        sections.insert(
            Route::root(),
            Section {
                route: Route::root(),
                title: Title::from_static("Home"),
                weight: 0,
                body_html: HtmlBody::from_static(""),
                headings: Vec::new(),
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

/// Split content into frontmatter and body
fn split_frontmatter(content: &str) -> (String, String) {
    let content = content.trim_start();

    // Check for +++ delimiters (TOML frontmatter)
    if content.starts_with("+++") {
        let rest = &content[3..];
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
            format!("/{}/", path)
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
