//! Imported Tree-sitter query files.

use facet::Facet;

use crate::source::SourceFile;

/// Raw Tree-sitter query source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QuerySource(pub String);

/// Well-known Tree-sitter query categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum WellKnownQuery {
    /// Highlight query.
    Highlights,
    /// Locals query.
    Locals,
    /// Injections query.
    Injections,
    /// Tags query.
    Tags,
}

impl WellKnownQuery {
    /// Default filename used by Tree-sitter packages.
    pub const fn filename(self) -> &'static str {
        match self {
            Self::Highlights => "highlights.scm",
            Self::Locals => "locals.scm",
            Self::Injections => "injections.scm",
            Self::Tags => "tags.scm",
        }
    }
}

/// Imported query files. Unknown query files are preserved.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
pub struct QueryBundle {
    /// Query source files with category resolution.
    pub files: Vec<QueryFile>,
}

/// Imported query source file with Tree-sitter category metadata.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QueryFile {
    /// Well-known category, when this file was resolved through category semantics.
    pub category: Option<WellKnownQuery>,
    /// Whether the file came from `tree-sitter.json` rather than fallback discovery.
    pub configured: bool,
    /// Query source file.
    pub source: SourceFile<QuerySource>,
}

impl QueryBundle {
    /// Get a well-known query file by default filename.
    pub fn well_known(&self, query: WellKnownQuery) -> Option<&SourceFile<QuerySource>> {
        self.files
            .iter()
            .find(|file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate well-known query files in configured order.
    pub fn well_known_files(
        &self,
        query: WellKnownQuery,
    ) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files
            .iter()
            .filter(move |file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate all query files.
    pub fn iter(&self) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files.iter().map(|file| &file.source)
    }

    /// Iterate all query files with category metadata.
    pub fn iter_files(&self) -> impl Iterator<Item = &QueryFile> {
        self.files.iter()
    }
}
