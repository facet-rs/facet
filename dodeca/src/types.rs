//! Strongly-typed string wrappers using aliri_braid
//!
//! # Why aliri_braid?
//!
//! Instead of passing `String` and `&str` everywhere (stringly-typed code),
//! we use distinct types that make the code self-documenting and prevent
//! mixing up different kinds of strings at compile time.
//!
//! # Types provided
//!
//! - [`SourcePath`] / [`SourcePathRef`] - relative path to source file (e.g., "learn/_index.md")
//! - [`Route`] / [`RouteRef`] - URL route (e.g., "/learn/")
//! - [`Title`] / [`TitleRef`] - page/section title
//! - [`HtmlBody`] / [`HtmlBodyRef`] - rendered HTML content
//! - [`SourceContent`] / [`SourceContentRef`] - raw markdown file content
//!
//! # How aliri_braid works
//!
//! The `#[braid]` macro generates two types from each definition:
//!
//! - **Owned type** (e.g., `Route`) - owns a `String`, like `String` itself
//! - **Borrowed type** (e.g., `RouteRef`) - a newtype over `str`, like `&str`
//!
//! ## Creating instances
//!
//! ```ignore
//! // From a String (owned)
//! let route = Route::new(some_string);
//!
//! // From a &'static str literal
//! let route = Route::from_static("/learn/");
//!
//! // Borrowed reference from a &str
//! let route_ref = RouteRef::from_str("/learn/");
//! ```
//!
//! ## Accessing the underlying string
//!
//! ```ignore
//! let route = Route::from_static("/learn/");
//!
//! // Get &str
//! let s: &str = route.as_str();
//!
//! // Get &RouteRef (borrowed version of the type)
//! let r: &RouteRef = route.as_ref();
//! ```
//!
//! ## In function signatures
//!
//! ```ignore
//! // Accept owned Route
//! fn takes_route(route: Route) { ... }
//!
//! // Accept borrowed &RouteRef (like &str but type-safe)
//! fn takes_route_ref(route: &RouteRef) { ... }
//!
//! // You can pass &Route where &RouteRef is expected (via Deref)
//! let route = Route::from_static("/learn/");
//! takes_route_ref(&route);  // works!
//! ```

use aliri_braid::braid;

/// Relative path to a source file from the content directory.
/// Example: "learn/_index.md", "learn/showcases/json.md"
#[braid]
pub struct SourcePath;

/// URL route path for a page or section.
/// Always starts and ends with `/`. Example: "/learn/", "/learn/showcases/json/"
#[braid]
pub struct Route;

/// Title of a page or section from frontmatter.
#[braid]
pub struct Title;

/// Rendered HTML body content.
#[braid]
pub struct HtmlBody;

/// Raw source file content (markdown with frontmatter).
#[braid]
pub struct SourceContent;

/// Relative path to a template file from the templates directory.
/// Example: "base.html", "page.html"
#[braid]
pub struct TemplatePath;

/// Raw template file content.
#[braid]
pub struct TemplateContent;

/// Relative path to a Sass/SCSS file from the sass directory.
/// Example: "main.scss", "_variables.scss"
#[braid]
pub struct SassPath;

/// Raw Sass/SCSS file content.
#[braid]
pub struct SassContent;

/// Relative path to a static file from the static directory.
/// Example: "favicon.ico", "images/logo.png"
#[braid]
pub struct StaticPath;

impl Route {
    /// Create the root route "/"
    pub fn root() -> Self {
        Self::from_static("/")
    }

    /// Check if this route is within a section (contains the section name)
    pub fn is_in_section(&self, section: &str) -> bool {
        RouteRef::from_str(self.as_str()).is_in_section(section)
    }

    /// Get the parent route (e.g., "/learn/showcases/" -> "/learn/")
    pub fn parent(&self) -> Option<Route> {
        RouteRef::from_str(self.as_str()).parent()
    }
}

impl RouteRef {
    /// Check if this route is within a section (contains the section name)
    pub fn is_in_section(&self, section: &str) -> bool {
        self.as_str().contains(&format!("{section}/"))
    }

    /// Check if this is an ancestor of another route
    pub fn is_ancestor_of(&self, other: &RouteRef) -> bool {
        other.as_str().starts_with(self.as_str())
    }

    /// Get the parent route (e.g., "/learn/showcases/" -> "/learn/")
    pub fn parent(&self) -> Option<Route> {
        let s = self.as_str().trim_end_matches('/');
        if s.is_empty() || s == "/" {
            return None;
        }
        match s.rfind('/') {
            Some(0) => Some(Route::root()),
            Some(idx) => Some(Route::new(format!("{}/", &s[..idx]))),
            None => Some(Route::root()),
        }
    }
}

impl SourcePath {
    /// Check if this is a section index file (_index.md)
    pub fn is_section_index(&self) -> bool {
        self.as_str().ends_with("_index.md")
    }

    /// Convert source path to URL route.
    /// - "learn/_index.md" -> "/learn/"
    /// - "learn/page.md" -> "/learn/page/"
    /// - "_index.md" -> "/"
    pub fn to_route(&self) -> Route {
        let mut path = self.as_str().to_string();

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
            Route::root()
        } else {
            Route::new(format!("/{path}/"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_path_to_route() {
        assert_eq!(
            SourcePath::from_static("learn/_index.md").to_route(),
            Route::from_static("/learn/")
        );
        assert_eq!(
            SourcePath::from_static("learn/showcases/_index.md").to_route(),
            Route::from_static("/learn/showcases/")
        );
        assert_eq!(
            SourcePath::from_static("_index.md").to_route(),
            Route::root()
        );
        assert_eq!(
            SourcePath::from_static("learn/page.md").to_route(),
            Route::from_static("/learn/page/")
        );
    }

    #[test]
    fn test_route_parent() {
        assert_eq!(
            Route::from_static("/learn/showcases/").parent(),
            Some(Route::from_static("/learn/"))
        );
        assert_eq!(Route::from_static("/learn/").parent(), Some(Route::root()));
        assert_eq!(Route::root().parent(), None);
    }

    #[test]
    fn test_route_is_in_section() {
        let route = Route::from_static("/learn/showcases/json/");
        assert!(route.is_in_section("learn"));
        assert!(route.is_in_section("showcases"));
        assert!(!route.is_in_section("extend"));
    }
}
