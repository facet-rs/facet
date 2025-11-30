use crate::types::{
    HtmlBody, Route, SassContent, SassPath, SourceContent, SourcePath, StaticPath, TemplateContent,
    TemplatePath, Title,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The Salsa database trait for dodeca
#[salsa::db]
pub trait Db: salsa::Database {}

/// Statistics about query execution
#[derive(Debug, Default)]
pub struct QueryStats {
    /// Number of queries that were executed (cache miss)
    pub executed: AtomicUsize,
    /// Number of queries that were reused (cache hit)
    pub reused: AtomicUsize,
}

impl QueryStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn executed(&self) -> usize {
        self.executed.load(Ordering::Relaxed)
    }

    pub fn reused(&self) -> usize {
        self.reused.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.executed() + self.reused()
    }
}

/// The concrete database implementation
#[salsa::db]
#[derive(Clone)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {}

impl Default for Database {
    fn default() -> Self {
        Self::new_without_stats()
    }
}

impl Database {
    /// Create a new database without stats tracking
    pub fn new_without_stats() -> Self {
        Self {
            storage: salsa::Storage::new(None),
        }
    }

    /// Create a new database with stats tracking
    pub fn new_with_stats(stats: Arc<QueryStats>) -> Self {
        let callback = Box::new(move |event: salsa::Event| {
            use salsa::EventKind;
            match event.kind {
                EventKind::WillExecute { .. } => {
                    stats.executed.fetch_add(1, Ordering::Relaxed);
                }
                EventKind::DidValidateMemoizedValue { .. } => {
                    stats.reused.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        });

        Self {
            storage: salsa::Storage::new(Some(callback)),
        }
    }

    /// Create a new database (alias for default)
    pub fn new() -> Self {
        Self::default()
    }
}

/// Input: A source file with its content
#[salsa::input]
pub struct SourceFile {
    /// The path to this file (relative to content dir)
    #[returns(ref)]
    pub path: SourcePath,

    /// The raw content of the file
    #[returns(ref)]
    pub content: SourceContent,
}

/// Input: A template file with its content
#[salsa::input]
pub struct TemplateFile {
    /// The path to this file (relative to templates dir)
    #[returns(ref)]
    pub path: TemplatePath,

    /// The raw content of the template
    #[returns(ref)]
    pub content: TemplateContent,
}

/// Input: A Sass/SCSS file with its content
#[salsa::input]
pub struct SassFile {
    /// The path to this file (relative to sass dir)
    #[returns(ref)]
    pub path: SassPath,

    /// The raw content of the Sass file
    #[returns(ref)]
    pub content: SassContent,
}

/// Interned template registry - allows Salsa to track template set as a whole
#[salsa::interned]
pub struct TemplateRegistry<'db> {
    #[returns(ref)]
    pub templates: Vec<TemplateFile>,
}

/// Interned sass registry - allows Salsa to track sass file set as a whole
#[salsa::interned]
pub struct SassRegistry<'db> {
    #[returns(ref)]
    pub files: Vec<SassFile>,
}

/// Input: A static file with its binary content
#[salsa::input]
pub struct StaticFile {
    /// The path to this file (relative to static dir)
    #[returns(ref)]
    pub path: StaticPath,

    /// The binary content of the file
    #[returns(ref)]
    pub content: Vec<u8>,
}

/// Interned static file registry - allows Salsa to track static files as a whole
#[salsa::interned]
pub struct StaticRegistry<'db> {
    #[returns(ref)]
    pub files: Vec<StaticFile>,
}

/// Interned source registry - allows Salsa to track all source files as a whole
#[salsa::interned]
pub struct SourceRegistry<'db> {
    #[returns(ref)]
    pub sources: Vec<SourceFile>,
}

/// Interned character set for font subsetting
/// Using a sorted Vec<char> for deterministic hashing
#[salsa::interned]
pub struct CharSet<'db> {
    #[returns(ref)]
    pub chars: Vec<char>,
}

/// A heading extracted from page/section content
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Heading {
    /// The heading text
    pub title: String,
    /// The anchor ID (for linking)
    pub id: String,
    /// The heading level (1-6)
    pub level: u8,
}

/// A section in the site tree (corresponds to _index.md files)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Section {
    pub route: Route,
    pub title: Title,
    pub weight: i32,
    pub body_html: HtmlBody,
    /// Headings extracted from content
    pub headings: Vec<Heading>,
}

/// A page in the site tree (non-index .md files)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Page {
    pub route: Route,
    pub title: Title,
    pub weight: i32,
    pub body_html: HtmlBody,
    pub section_route: Route,
    /// Headings extracted from content
    pub headings: Vec<Heading>,
}

/// The complete site tree - sections and pages
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteTree {
    pub sections: std::collections::BTreeMap<Route, Section>,
    pub pages: std::collections::BTreeMap<Route, Page>,
}

/// Rendered HTML output for a page or section
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderedHtml(pub String);

/// Output of parsing: contains all the data needed for tree building
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedData {
    /// Source path relative to content dir
    pub source_path: SourcePath,
    /// URL route (e.g., "/learn/showcases/json/")
    pub route: Route,
    /// Parsed title
    pub title: Title,
    /// Weight for sorting
    pub weight: i32,
    /// Body HTML
    pub body_html: HtmlBody,
    /// Is this a section index (_index.md)?
    pub is_section: bool,
    /// Headings extracted from content
    pub headings: Vec<Heading>,
}

/// A single output file to be written to disk
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OutputFile {
    /// HTML page output: route -> html content
    Html { route: Route, content: String },
    /// CSS output from compiled SASS (path includes cache-bust hash)
    Css { path: StaticPath, content: String },
    /// Static file: relative path -> binary content (path includes cache-bust hash)
    Static { path: StaticPath, content: Vec<u8> },
}

/// Complete site output - all files that need to be written
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteOutput {
    pub files: Vec<OutputFile>,
}

/// Result of checking an external link
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalLinkStatus {
    /// Link is valid (2xx or 3xx response)
    Ok,
    /// Link returned an error status code
    Error(u16),
    /// Network or other error
    Failed(String),
}

/// Rendered OG image output (SVG + PNG)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OgImageOutput {
    /// SVG content (preferred format, smaller)
    pub svg: String,
    /// PNG content (fallback format)
    pub png: Vec<u8>,
}

/// Input for OG template (optional - uses default if not provided)
#[salsa::input]
pub struct OgTemplateFile {
    /// The Typst template content
    #[returns(ref)]
    pub content: String,
}
