//! Tree-sitter external scanner import artifacts.

use facet::Facet;

use crate::{grammar::RawRuleJson, source::SourceFile};

/// Ordinal of an external token in Tree-sitter's `externals` array.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalTokenOrdinal(u32);

impl ExternalTokenOrdinal {
    /// Create an external token ordinal.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric ordinal.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Best-effort external token name for diagnostics.
#[derive(Debug, Clone, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalTokenName(String);

impl ExternalTokenName {
    /// Create an external token name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the token name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// External token declaration preserving Tree-sitter scanner ordinal.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ExternalTokenDecl {
    /// Ordinal in the `externals` array.
    pub ordinal: ExternalTokenOrdinal,
    /// Full raw external rule.
    pub rule: RawRuleJson,
    /// Optional symbolic name for diagnostics.
    pub name: Option<ExternalTokenName>,
}

impl ExternalTokenDecl {
    /// Build an external token declaration from a raw grammar rule.
    pub fn new(ordinal: ExternalTokenOrdinal, rule: RawRuleJson) -> Self {
        let name = match &rule {
            RawRuleJson::Symbol { name } => Some(ExternalTokenName::new(name.clone())),
            _ => None,
        };
        Self {
            ordinal,
            rule,
            name,
        }
    }
}

/// Original Tree-sitter scanner source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ScannerSource(pub String);

/// Tree-sitter scanner source language.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum TreeSitterScannerKind {
    /// `src/scanner.c`.
    C,
    /// `src/scanner.cc`.
    Cpp,
}

/// Imported Tree-sitter scanner source plus the external-token order it must follow.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterScanner {
    /// Scanner source language.
    pub kind: TreeSitterScannerKind,
    /// Scanner source file.
    pub source: SourceFile<ScannerSource>,
    /// External tokens in grammar order; index is the scanner ordinal.
    pub externals: Vec<ExternalTokenDecl>,
}
