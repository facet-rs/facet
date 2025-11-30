use crate::types::{
    HtmlBody, Route, SassContent, SassPath, SourceContent, SourcePath, TemplateContent,
    TemplatePath, Title,
};
use tracing::debug;

/// The Salsa database trait for dodeca
#[salsa::db]
pub trait Db: salsa::Database {}

/// The concrete database implementation
#[salsa::db]
#[derive(Default, Clone)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {
    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        use salsa::EventKind;

        // Use tracing for Salsa events - filtering handled by subscriber
        let event = event();
        match event.kind {
            EventKind::WillExecute { database_key } => {
                debug!(target: "salsa", "execute: {:?}", database_key);
            }
            EventKind::DidValidateMemoizedValue { database_key } => {
                debug!(target: "salsa", "reuse: {:?}", database_key);
            }
            _ => {}
        }
    }
}

#[salsa::db]
impl Db for Database {}

impl Database {
    /// Create a new database
    pub fn new() -> Self {
        Self::default()
    }
}

/// Input: A source file with its content
#[salsa::input]
pub struct SourceFile {
    /// The path to this file (relative to content dir)
    #[return_ref]
    pub path: SourcePath,

    /// The raw content of the file
    #[return_ref]
    pub content: SourceContent,
}

/// Input: A template file with its content
#[salsa::input]
pub struct TemplateFile {
    /// The path to this file (relative to templates dir)
    #[return_ref]
    pub path: TemplatePath,

    /// The raw content of the template
    #[return_ref]
    pub content: TemplateContent,
}

/// Input: A Sass/SCSS file with its content
#[salsa::input]
pub struct SassFile {
    /// The path to this file (relative to sass dir)
    #[return_ref]
    pub path: SassPath,

    /// The raw content of the Sass file
    #[return_ref]
    pub content: SassContent,
}

/// Interned template registry - allows Salsa to track template set as a whole
#[salsa::interned]
pub struct TemplateRegistry<'db> {
    #[return_ref]
    pub templates: Vec<TemplateFile>,
}

/// Interned sass registry - allows Salsa to track sass file set as a whole
#[salsa::interned]
pub struct SassRegistry<'db> {
    #[return_ref]
    pub files: Vec<SassFile>,
}

/// Interned source registry - allows Salsa to track all source files as a whole
#[salsa::interned]
pub struct SourceRegistry<'db> {
    #[return_ref]
    pub sources: Vec<SourceFile>,
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
