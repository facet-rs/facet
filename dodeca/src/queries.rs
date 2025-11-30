use crate::db::{Db, ParsedData, SourceFile};
use crate::types::{HtmlBody, Title};
use facet::Facet;
use pulldown_cmark::{Options, Parser, html};

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
