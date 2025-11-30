//! Precise URL rewriting using proper parsers
//!
//! - CSS: Uses lightningcss visitor API to find and rewrite `url()` values
//! - HTML: Uses lol_html to rewrite attributes and inline style/script content
//! - JS: Uses OXC parser to find string literals and rewrite asset paths

use std::collections::HashMap;

// OXC imports for JS string literal rewriting
use oxc::ast_visit::Visit;

/// Rewrite URLs in CSS using lightningcss parser
///
/// Only rewrites actual `url()` values in CSS, not text that happens to look like URLs.
pub fn rewrite_urls_in_css(css: &str, path_map: &HashMap<String, String>) -> String {
    use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
    use lightningcss::visitor::Visit;

    // Parse the CSS
    let mut stylesheet = match StyleSheet::parse(css, ParserOptions::default()) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to parse CSS for URL rewriting: {:?}", e);
            return css.to_string();
        }
    };

    // Visit and rewrite URLs
    let mut visitor = UrlRewriter { path_map };
    if let Err(e) = stylesheet.visit(&mut visitor) {
        tracing::warn!("Failed to visit CSS: {:?}", e);
        return css.to_string();
    }

    // Serialize back to string
    match stylesheet.to_css(PrinterOptions::default()) {
        Ok(result) => result.code,
        Err(e) => {
            tracing::warn!("Failed to serialize CSS: {:?}", e);
            css.to_string()
        }
    }
}

/// Visitor that rewrites URLs in CSS
struct UrlRewriter<'a> {
    path_map: &'a HashMap<String, String>,
}

impl<'i, 'a> lightningcss::visitor::Visitor<'i> for UrlRewriter<'a> {
    type Error = std::convert::Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        lightningcss::visit_types!(URLS)
    }

    fn visit_url(
        &mut self,
        url: &mut lightningcss::values::url::Url<'i>,
    ) -> Result<(), Self::Error> {
        let url_str = url.url.as_ref();
        if let Some(new_url) = self.path_map.get(url_str) {
            url.url = new_url.clone().into();
        }
        Ok(())
    }
}

/// Rewrite URLs in HTML using lol_html parser
///
/// Rewrites:
/// - `href` and `src` attributes
/// - `srcset` attribute values
/// - Inline `<style>` tag content (via lightningcss)
/// - String literals in `<script>` tags that match known asset paths
pub fn rewrite_urls_in_html(html: &str, path_map: &HashMap<String, String>) -> String {
    use lol_html::{RewriteStrSettings, element, rewrite_str, text};

    // Clone path_map for the closures
    let href_map = path_map.clone();
    let src_map = path_map.clone();
    let srcset_map = path_map.clone();
    let style_map = path_map.clone();

    let result = rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                // Rewrite href attributes (links, stylesheets)
                element!("[href]", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        if let Some(new_href) = href_map.get(&href) {
                            el.set_attribute("href", new_href).ok();
                        }
                    }
                    Ok(())
                }),
                // Rewrite src attributes (images, scripts)
                element!("[src]", |el| {
                    if let Some(src) = el.get_attribute("src") {
                        if let Some(new_src) = src_map.get(&src) {
                            el.set_attribute("src", new_src).ok();
                        }
                    }
                    Ok(())
                }),
                // Rewrite srcset attributes (responsive images)
                element!("[srcset]", |el| {
                    if let Some(srcset) = el.get_attribute("srcset") {
                        let new_srcset = rewrite_srcset(&srcset, &srcset_map);
                        el.set_attribute("srcset", &new_srcset).ok();
                    }
                    Ok(())
                }),
                // Rewrite inline <style> content
                text!("style", |text| {
                    let css = text.as_str();
                    if !css.trim().is_empty() {
                        let rewritten = rewrite_urls_in_css(css, &style_map);
                        text.replace(&rewritten, lol_html::html_content::ContentType::Text);
                    }
                    Ok(())
                }),
            ],
            ..Default::default()
        },
    );

    // First pass: HTML attributes and style tags via lol_html
    let html_rewritten = match result {
        Ok(rewritten) => rewritten,
        Err(e) => {
            tracing::warn!("Failed to rewrite HTML URLs: {:?}", e);
            html.to_string()
        }
    };

    // Second pass: rewrite script content via regex (lol_html doesn't allow script text replacement)
    rewrite_script_tags(&html_rewritten, path_map)
}

/// Rewrite script tag content in HTML
///
/// Since lol_html doesn't allow direct modification of `<script>` text content,
/// we use regex to find and rewrite script tags.
fn rewrite_script_tags(html: &str, path_map: &HashMap<String, String>) -> String {
    use regex::Regex;

    // Match <script>...</script> tags, capturing the content
    // Using (?s) for DOTALL mode so . matches newlines
    let script_re = Regex::new(r"(?s)(<script[^>]*>)(.*?)(</script>)").unwrap();

    let result = script_re.replace_all(html, |caps: &regex::Captures| {
        let open_tag = &caps[1];
        let content = &caps[2];
        let close_tag = &caps[3];

        let rewritten_content = rewrite_string_literals_in_js(content, path_map);
        format!("{open_tag}{rewritten_content}{close_tag}")
    });

    result.into_owned()
}

/// Rewrite string literals in JavaScript that contain asset paths
///
/// Uses OXC parser to properly parse JavaScript and find string literals,
/// then replaces paths with cache-busted versions.
fn rewrite_string_literals_in_js(js: &str, path_map: &HashMap<String, String>) -> String {
    use oxc::allocator::Allocator;
    use oxc::ast_visit::Visit;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    // Parse the JavaScript
    let allocator = Allocator::default();
    let source_type = SourceType::mjs(); // Treat as ES module
    let parser_result = Parser::new(&allocator, js, source_type).parse();

    if parser_result.panicked || !parser_result.errors.is_empty() {
        // If parsing fails, return unchanged (could be a snippet or invalid JS)
        tracing::debug!("JS parsing failed, returning unchanged");
        return js.to_string();
    }

    // Collect string literal positions and their replacement values
    let mut replacements: Vec<(u32, u32, String)> = Vec::new(); // (start, end, new_value)
    let mut collector = StringCollector {
        source: js,
        path_map,
        replacements: &mut replacements,
    };
    collector.visit_program(&parser_result.program);

    // Apply replacements in reverse order (so offsets stay valid)
    if replacements.is_empty() {
        return js.to_string();
    }

    replacements.sort_by(|a, b| b.0.cmp(&a.0)); // Sort by start position, descending

    let mut result = js.to_string();
    for (start, end, new_value) in replacements {
        result.replace_range(start as usize..end as usize, &new_value);
    }

    result
}

/// Visitor that collects string literals for replacement
struct StringCollector<'a> {
    source: &'a str,
    path_map: &'a HashMap<String, String>,
    replacements: &'a mut Vec<(u32, u32, String)>,
}

impl<'a> Visit<'_> for StringCollector<'a> {
    fn visit_string_literal(&mut self, lit: &oxc::ast::ast::StringLiteral<'_>) {
        let value = lit.value.as_str();
        let mut new_value = value.to_string();
        let mut changed = false;

        for (old_path, new_path) in self.path_map.iter() {
            if new_value.contains(old_path.as_str()) {
                new_value = new_value.replace(old_path, new_path);
                changed = true;
            }
        }

        if changed {
            // Get the original source including quotes
            let start = lit.span.start;
            let end = lit.span.end;
            let original = &self.source[start as usize..end as usize];
            let quote = original.chars().next().unwrap_or('"');
            self.replacements
                .push((start, end, format!("{quote}{new_value}{quote}")));
        }
    }

    fn visit_template_literal(&mut self, lit: &oxc::ast::ast::TemplateLiteral<'_>) {
        // Handle template literal quasi strings
        for quasi in &lit.quasis {
            let value = quasi.value.raw.as_str();
            let mut new_value = value.to_string();
            let mut changed = false;

            for (old_path, new_path) in self.path_map.iter() {
                if new_value.contains(old_path.as_str()) {
                    new_value = new_value.replace(old_path, new_path);
                    changed = true;
                }
            }

            if changed {
                // For template literals, we only replace the quasi part
                let start = quasi.span.start;
                let end = quasi.span.end;
                self.replacements.push((start, end, new_value));
            }
        }

        // Continue visiting expressions inside template literal
        for expr in &lit.expressions {
            self.visit_expression(expr);
        }
    }
}

/// Rewrite URLs in a srcset attribute value
///
/// srcset format: "url1 1x, url2 2x" or "url1 100w, url2 200w"
fn rewrite_srcset(srcset: &str, path_map: &HashMap<String, String>) -> String {
    srcset
        .split(',')
        .map(|entry| {
            let entry = entry.trim();
            // Split into URL and descriptor (e.g., "1x", "100w")
            let parts: Vec<&str> = entry.split_whitespace().collect();
            if parts.is_empty() {
                return entry.to_string();
            }

            let url = parts[0];
            let descriptor = parts.get(1).copied();

            let new_url = path_map.get(url).map(|s| s.as_str()).unwrap_or(url);

            match descriptor {
                Some(d) => format!("{new_url} {d}"),
                None => new_url.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_css_urls() {
        let css = r#"
            @font-face {
                font-family: "Inter";
                src: url("/fonts/Inter.woff2") format("woff2");
            }
            body {
                background: url("/images/bg.png");
            }
        "#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/fonts/Inter.woff2".to_string(),
            "/fonts/Inter.abc123.woff2".to_string(),
        );
        path_map.insert(
            "/images/bg.png".to_string(),
            "/images/bg.def456.png".to_string(),
        );

        let result = rewrite_urls_in_css(css, &path_map);

        assert!(result.contains("/fonts/Inter.abc123.woff2"));
        assert!(result.contains("/images/bg.def456.png"));
        assert!(!result.contains("\"/fonts/Inter.woff2\""));
        assert!(!result.contains("\"/images/bg.png\""));
    }

    #[test]
    fn test_rewrite_html_urls() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
    <link rel="stylesheet" href="/main.css">
</head>
<body>
    <img src="/images/logo.png" alt="Logo">
    <a href="/about/">About</a>
    <p>Check out /main.css in your browser</p>
</body>
</html>"#;

        let mut path_map = HashMap::new();
        path_map.insert("/main.css".to_string(), "/main.abc123.css".to_string());
        path_map.insert(
            "/images/logo.png".to_string(),
            "/images/logo.def456.png".to_string(),
        );

        let result = rewrite_urls_in_html(html, &path_map);

        // Attributes should be rewritten
        assert!(result.contains("href=\"/main.abc123.css\""));
        assert!(result.contains("src=\"/images/logo.def456.png\""));

        // Text content should NOT be rewritten
        assert!(result.contains("Check out /main.css in your browser"));

        // Non-matching href should be unchanged
        assert!(result.contains("href=\"/about/\""));
    }

    #[test]
    fn test_rewrite_inline_style() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body {
            background: url("/images/bg.png");
        }
    </style>
</head>
<body></body>
</html>"#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/images/bg.png".to_string(),
            "/images/bg.abc123.png".to_string(),
        );

        let result = rewrite_urls_in_html(html, &path_map);

        assert!(result.contains("/images/bg.abc123.png"));
        assert!(!result.contains("\"/images/bg.png\""));
    }

    #[test]
    fn test_rewrite_inline_script() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
    <script>
        const logo = "/images/logo.png";
        const other = '/images/icon.svg';
        const template = `/images/hero.jpg`;
        const notAnAsset = "/about/";
    </script>
</head>
<body></body>
</html>"#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/images/logo.png".to_string(),
            "/images/logo.abc123.png".to_string(),
        );
        path_map.insert(
            "/images/icon.svg".to_string(),
            "/images/icon.def456.svg".to_string(),
        );
        path_map.insert(
            "/images/hero.jpg".to_string(),
            "/images/hero.789xyz.jpg".to_string(),
        );

        let result = rewrite_urls_in_html(html, &path_map);

        // Known assets should be rewritten
        assert!(result.contains("\"/images/logo.abc123.png\""));
        assert!(result.contains("'/images/icon.def456.svg'"));
        assert!(result.contains("`/images/hero.789xyz.jpg`"));

        // Unknown paths should NOT be rewritten
        assert!(result.contains("\"/about/\""));
    }

    #[test]
    fn test_script_text_not_rewritten() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
    <script>
        // Comment: /images/logo.png
        console.log("Loading /images/logo.png");
    </script>
</head>
<body></body>
</html>"#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/images/logo.png".to_string(),
            "/images/logo.abc123.png".to_string(),
        );

        let result = rewrite_urls_in_html(html, &path_map);

        // The string literal should be rewritten
        assert!(result.contains("\"Loading /images/logo.abc123.png\""));

        // The comment should NOT be rewritten (it's not a string literal)
        assert!(result.contains("// Comment: /images/logo.png"));
    }

    #[test]
    fn test_css_url_in_text_not_rewritten() {
        let css = r#"
            /* This comment mentions /fonts/Inter.woff2 */
            body { font-family: sans-serif; }
        "#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/fonts/Inter.woff2".to_string(),
            "/fonts/Inter.abc123.woff2".to_string(),
        );

        let result = rewrite_urls_in_css(css, &path_map);

        // Comment should not be rewritten (lightningcss strips comments by default)
        // The important thing is we don't crash or produce invalid CSS
        assert!(!result.contains("Inter.abc123"));
    }

    #[test]
    fn test_rewrite_srcset() {
        let mut path_map = HashMap::new();
        path_map.insert(
            "/img/hero.png".to_string(),
            "/img/hero.abc123.png".to_string(),
        );
        path_map.insert(
            "/img/hero-2x.png".to_string(),
            "/img/hero-2x.def456.png".to_string(),
        );

        let srcset = "/img/hero.png 1x, /img/hero-2x.png 2x";
        let result = rewrite_srcset(srcset, &path_map);

        assert_eq!(
            result,
            "/img/hero.abc123.png 1x, /img/hero-2x.def456.png 2x"
        );
    }

    #[test]
    fn test_js_string_literal_rewriting() {
        let js = r#"
            const a = "/images/logo.png";
            const b = '/images/icon.svg';
            const c = `/images/hero.jpg`;
            const d = "/not/in/map.png";
        "#;

        let mut path_map = HashMap::new();
        path_map.insert(
            "/images/logo.png".to_string(),
            "/images/logo.abc.png".to_string(),
        );
        path_map.insert(
            "/images/icon.svg".to_string(),
            "/images/icon.def.svg".to_string(),
        );
        path_map.insert(
            "/images/hero.jpg".to_string(),
            "/images/hero.ghi.jpg".to_string(),
        );

        let result = rewrite_string_literals_in_js(js, &path_map);

        assert!(result.contains("\"/images/logo.abc.png\""));
        assert!(result.contains("'/images/icon.def.svg'"));
        assert!(result.contains("`/images/hero.ghi.jpg`"));
        assert!(result.contains("\"/not/in/map.png\"")); // unchanged
    }
}
