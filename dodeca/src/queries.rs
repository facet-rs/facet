use crate::db::{
    Db, ParsedData, SassFile, SassRegistry, SourceFile, TemplateFile, TemplateRegistry,
};
use crate::types::{HtmlBody, SassContent, TemplateContent, Title};
use facet::Facet;
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;

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

    // Convert markdown to HTML
    let body_html = HtmlBody::new(render_markdown(&markdown));

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
    }
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

/// Render markdown to HTML, resolving internal links
fn render_markdown(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES;

    let parser = Parser::new_ext(markdown, options);

    // Transform events to resolve @/ links
    let parser = parser.map(|event| match event {
        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let resolved = resolve_internal_link(&dest_url);
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                link_type,
                dest_url: resolved.into(),
                title,
                id,
            })
        }
        other => other,
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
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
