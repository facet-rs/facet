//! Imported Tree-sitter corpus and highlight fixture files.

use facet::Facet;

use crate::source::SourceFile;

/// Raw Tree-sitter corpus or highlight fixture source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusSource(pub String);

/// Imported corpus or highlight fixture.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusFixture {
    /// Fixture kind.
    pub kind: CorpusKind,
    /// Fixture source file.
    pub source: SourceFile<CorpusSource>,
}

/// Supported fixture categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum CorpusKind {
    /// Tree-sitter parse corpus fixture from `test/corpus`.
    Parse,
    /// Highlight fixture from `test/highlight` or legacy `test/highlights`.
    Highlight,
}
