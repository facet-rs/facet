#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Tree-sitter-compatible grammar package and parser runtime foundations.
//!
//! Snark keeps the Tree-sitter compatibility boundary separate from the
//! validated grammar and runtime layers. The current crate can import and
//! preserve Tree-sitter package artifacts; lowering into Snark's resolved
//! grammar IR is the next layer.

pub mod corpus;
pub mod diagnostic;
pub mod grammar;
pub mod query;
pub mod runtime_input;
pub mod scanner;
pub mod source;
#[cfg(feature = "tree-sitter-import")]
pub mod tree_sitter;

pub use corpus::{CorpusFixture, CorpusKind, CorpusSource};
pub use diagnostic::{ImportError, JsonDocumentKind};
pub use grammar::{
    LanguageName, PrecedenceValue, RawGrammarJson, RawRuleJson, ReservedSetTable, RuleName,
    RuleTable,
};
pub use query::{QueryBundle, QuerySource, WellKnownQuery};
pub use runtime_input::{
    ByteOffset, ByteRange, IncludedRange, InputEdit, PointBytes, PointRange, Row, Utf8ColumnBytes,
};
pub use scanner::{
    ExternalTokenDecl, ExternalTokenName, ExternalTokenOrdinal, ScannerSource, TreeSitterScanner,
    TreeSitterScannerKind,
};
pub use source::{PackageRelativePath, PackageRoot, SourceFile, SourceId, TreeSitterConfigJson};
#[cfg(feature = "tree-sitter-import")]
pub use tree_sitter::{ImportedPackage, NodeTypesJson, ParserC, TreeSitterPackageImporter};
