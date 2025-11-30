use crate::types::{HtmlBody, Route, SourceContent, SourcePath, Title};

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
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {
        // Optional: log events for debugging
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
}
