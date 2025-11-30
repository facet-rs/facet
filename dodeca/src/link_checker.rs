//! Link checking for generated HTML
//!
//! Query-based: works directly with HTML content from SiteOutput,
//! no disk I/O needed. External links are cached by (url, date).

use crate::db::ExternalLinkStatus;
use crate::types::Route;
use chrono::NaiveDate;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use url::Url;

/// A broken link found during checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    /// The route of the page containing the broken link
    pub source_route: Route,
    /// The href value that's broken
    pub href: String,
    /// Why it's broken
    pub reason: String,
    /// Is this an external link?
    pub is_external: bool,
}

/// Results from link checking
#[derive(Debug, Default, Clone)]
pub struct LinkCheckResult {
    pub total_links: usize,
    pub internal_links: usize,
    pub external_links: usize,
    pub external_checked: usize,
    pub broken_links: Vec<BrokenLink>,
}

impl LinkCheckResult {
    pub fn is_ok(&self) -> bool {
        self.broken_links.is_empty()
    }

    pub fn internal_broken(&self) -> usize {
        self.broken_links.iter().filter(|l| !l.is_external).count()
    }

    pub fn external_broken(&self) -> usize {
        self.broken_links.iter().filter(|l| l.is_external).count()
    }
}

/// Regex to extract href attributes from anchor tags
static HREF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<a\s[^>]*href=["']([^"']+)["']"#).unwrap());

/// A page with its route and HTML content
pub struct Page<'a> {
    pub route: &'a Route,
    pub html: &'a str,
}

/// An extracted link with its source
#[derive(Debug, Clone)]
pub struct ExtractedLink {
    pub source_route: Route,
    pub href: String,
}

/// Extract all links from pages, categorized
pub fn extract_links<'a>(pages: impl Iterator<Item = Page<'a>>) -> ExtractedLinks {
    let mut result = ExtractedLinks::default();

    let pages: Vec<_> = pages.collect();
    result.known_routes = pages.iter().map(|p| p.route.as_str().to_string()).collect();

    for page in &pages {
        for cap in HREF_REGEX.captures_iter(page.html) {
            let href = &cap[1];
            result.total += 1;

            if href.starts_with("http://") || href.starts_with("https://") {
                result.external.push(ExtractedLink {
                    source_route: page.route.clone(),
                    href: href.to_string(),
                });
            } else if href.starts_with('#')
                || href.starts_with("mailto:")
                || href.starts_with("tel:")
                || href.starts_with("javascript:")
            {
                // Skip anchors and special links
            } else {
                result.internal.push(ExtractedLink {
                    source_route: page.route.clone(),
                    href: href.to_string(),
                });
            }
        }
    }

    result
}

/// Extracted links from all pages
#[derive(Debug, Default)]
pub struct ExtractedLinks {
    pub total: usize,
    pub internal: Vec<ExtractedLink>,
    pub external: Vec<ExtractedLink>,
    pub known_routes: HashSet<String>,
}

/// Check internal links only (fast, no network)
pub fn check_internal_links(extracted: &ExtractedLinks) -> LinkCheckResult {
    let mut result = LinkCheckResult {
        total_links: extracted.total,
        internal_links: extracted.internal.len(),
        external_links: extracted.external.len(),
        ..Default::default()
    };

    for link in &extracted.internal {
        if let Some(reason) =
            check_internal_link(&link.source_route, &link.href, &extracted.known_routes)
        {
            result.broken_links.push(BrokenLink {
                source_route: link.source_route.clone(),
                href: link.href.clone(),
                reason,
                is_external: false,
            });
        }
    }

    result
}

/// Options for external link checking
#[derive(Debug, Clone, Default)]
pub struct ExternalLinkOptions {
    /// Domains to skip checking (anti-bot policies, known flaky, etc.)
    pub skip_domains: HashSet<String>,
}

impl ExternalLinkOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn skip_domains(mut self, domains: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for domain in domains {
            self.skip_domains.insert(domain.into());
        }
        self
    }
}

/// Extract domain from URL
fn get_domain(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
}

/// Check external links with HTTP requests
/// Uses date for cache key - same URL + same date = cached
pub async fn check_external_links(
    extracted: &ExtractedLinks,
    cache: &mut HashMap<(String, NaiveDate), ExternalLinkStatus>,
    date: NaiveDate,
    options: &ExternalLinkOptions,
) -> (Vec<BrokenLink>, usize) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("dodeca-link-checker/1.0")
        .build()
        .expect("failed to build HTTP client");

    let mut broken = Vec::new();
    let mut skipped = 0usize;

    // Deduplicate URLs - no need to check the same URL twice
    let mut unique_urls: HashMap<&str, Vec<&ExtractedLink>> = HashMap::new();
    for link in &extracted.external {
        unique_urls.entry(&link.href).or_default().push(link);
    }

    let total_unique = unique_urls.len();

    for (url, links) in unique_urls {
        // Check if domain should be skipped
        if let Some(domain) = get_domain(url) {
            if options.skip_domains.contains(&domain) {
                skipped += 1;
                continue;
            }
        }

        let cache_key = (url.to_string(), date);

        // Check cache first
        let status = if let Some(status) = cache.get(&cache_key) {
            status.clone()
        } else {
            // Make HTTP HEAD request
            let status = check_url(&client, url).await;
            cache.insert(cache_key, status.clone());
            status
        };

        // Report broken links
        if let ExternalLinkStatus::Ok = status {
            // Link is fine
        } else {
            let reason = match &status {
                ExternalLinkStatus::Ok => unreachable!(),
                ExternalLinkStatus::Error(code) => format!("HTTP {code}"),
                ExternalLinkStatus::Failed(msg) => msg.clone(),
            };

            // Report for each page that uses this URL
            for link in links {
                broken.push(BrokenLink {
                    source_route: link.source_route.clone(),
                    href: link.href.clone(),
                    reason: reason.clone(),
                    is_external: true,
                });
            }
        }
    }

    (broken, total_unique - skipped)
}

/// Check a single URL
async fn check_url(client: &reqwest::Client, url: &str) -> ExternalLinkStatus {
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            if (200..400).contains(&status) {
                ExternalLinkStatus::Ok
            } else {
                ExternalLinkStatus::Error(status)
            }
        }
        Err(e) => ExternalLinkStatus::Failed(e.to_string()),
    }
}

/// Check all links (internal only, for backwards compatibility)
pub fn check_links<'a>(pages: impl Iterator<Item = Page<'a>>) -> LinkCheckResult {
    let extracted = extract_links(pages);
    check_internal_links(&extracted)
}

/// Check if an internal link is valid
/// Returns None if valid, Some(reason) if broken
fn check_internal_link(
    source_route: &Route,
    href: &str,
    known_routes: &HashSet<String>,
) -> Option<String> {
    // Split href into path and fragment
    let (path, _fragment) = match href.find('#') {
        Some(idx) => (&href[..idx], Some(&href[idx + 1..])),
        None => (href, None),
    };

    // Empty path means same-page anchor - always valid
    if path.is_empty() {
        return None;
    }

    // Resolve the target route
    let target_route = if path.starts_with('/') {
        // Absolute path
        normalize_route(path)
    } else {
        // Relative path - resolve from source route
        let base = source_route.as_str();
        normalize_route(&format!("{base}{path}"))
    };

    // Check if route exists
    if known_routes.contains(&target_route) {
        return None;
    }

    // Also try without trailing slash (some links omit it)
    let without_slash = target_route.trim_end_matches('/');
    if !without_slash.is_empty() {
        let with_slash = format!("{without_slash}/");
        if known_routes.contains(&with_slash) {
            return None;
        }
    }

    // Check for static files (e.g., /main.css, /favicon.ico)
    // These won't be in known_routes but are valid
    if is_likely_static_file(path) {
        return None;
    }

    Some(format!("target '{target_route}' not found"))
}

/// Check if a path looks like a static file
fn is_likely_static_file(path: &str) -> bool {
    let extensions = [
        ".css", ".js", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".woff", ".woff2", ".ttf",
        ".eot", ".pdf", ".zip", ".tar", ".gz",
    ];
    extensions.iter().any(|ext| path.ends_with(ext))
}

/// Normalize a route path (handle .. and ., ensure leading/trailing slashes)
fn normalize_route(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }

    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_route() {
        assert_eq!(normalize_route("/learn/page/"), "/learn/page/");
        assert_eq!(normalize_route("/learn/page"), "/learn/page/");
        assert_eq!(normalize_route("/learn/page/../"), "/learn/");
        assert_eq!(normalize_route("/learn/./page/"), "/learn/page/");
        assert_eq!(normalize_route("/learn/../extend/"), "/extend/");
        assert_eq!(normalize_route("/"), "/");
    }

    #[test]
    fn test_check_links_finds_broken() {
        let root = Route::from_static("/");
        let exists = Route::from_static("/exists/");
        let pages = vec![
            Page {
                route: &root,
                html: r#"<a href="/exists/">ok</a> <a href="/missing/">broken</a>"#,
            },
            Page {
                route: &exists,
                html: r#"<a href="/">back</a>"#,
            },
        ];

        let result = check_links(pages.into_iter());
        assert_eq!(result.total_links, 3);
        assert_eq!(result.broken_links.len(), 1);
        assert_eq!(result.broken_links[0].href, "/missing/");
    }

    #[test]
    fn test_relative_links() {
        let learn = Route::from_static("/learn/");
        let learn_page = Route::from_static("/learn/page/");
        let extend = Route::from_static("/extend/");
        let pages = vec![
            Page {
                route: &learn,
                html: r#"<a href="page/">relative</a> <a href="../extend/">up</a>"#,
            },
            Page {
                route: &learn_page,
                html: "",
            },
            Page {
                route: &extend,
                html: "",
            },
        ];

        let result = check_links(pages.into_iter());
        assert!(result.is_ok(), "broken: {:?}", result.broken_links);
    }

    #[test]
    fn test_skips_external_and_special() {
        let root = Route::from_static("/");
        let pages = vec![Page {
            route: &root,
            html: "<a href=\"https://example.com\">external</a>\
                   <a href=\"#anchor\">anchor</a>\
                   <a href=\"mailto:x@y.z\">email</a>\
                   <a href=\"/style.css\">static</a>",
        }];

        let result = check_links(pages.into_iter());
        assert!(result.is_ok());
        assert_eq!(result.external_links, 1);
    }

    #[test]
    fn test_extract_links() {
        let root = Route::from_static("/");
        let pages = vec![Page {
            route: &root,
            html: "<a href=\"https://example.com\">ext</a>\
                   <a href=\"/page/\">int</a>\
                   <a href=\"#anchor\">anchor</a>",
        }];

        let extracted = extract_links(pages.into_iter());
        assert_eq!(extracted.total, 3);
        assert_eq!(extracted.external.len(), 1);
        assert_eq!(extracted.internal.len(), 1);
    }
}
