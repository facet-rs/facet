//! Tree-sitter external scanner inputs.

use facet::Facet;

#[cfg(feature = "tree-sitter-import")]
use crate::diagnostic::ImportError;
use crate::{grammar::RawRuleJson, source::SourceFile};

/// Ordinal of an external token in Tree-sitter's `externals` array.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalTokenOrdinal(u32);

impl ExternalTokenOrdinal {
    #[cfg(feature = "tree-sitter-import")]
    pub(crate) fn from_index(index: usize) -> Result<Self, ImportError> {
        let value = u32::try_from(index)
            .map_err(|_| ImportError::ExternalTokenOrdinalOverflow { index })?;
        Ok(Self(value))
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

/// Imported raw external token declaration preserving Tree-sitter scanner ordinal.
///
/// This is package-provenance data. Runtime scanner semantics are derived from
/// validated grammar and lexical facts, not from this raw DTO.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ExternalTokenDecl {
    ordinal: ExternalTokenOrdinal,
    rule: RawRuleJson,
    name: Option<ExternalTokenName>,
}

impl ExternalTokenDecl {
    #[cfg(feature = "tree-sitter-import")]
    fn new(ordinal: ExternalTokenOrdinal, rule: RawRuleJson) -> Self {
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

    /// Ordinal in the `externals` array.
    pub const fn ordinal(&self) -> ExternalTokenOrdinal {
        self.ordinal
    }

    /// Full raw external rule for import/provenance diagnostics.
    pub const fn rule(&self) -> &RawRuleJson {
        &self.rule
    }

    /// Optional symbolic name for diagnostics.
    pub const fn name(&self) -> Option<&ExternalTokenName> {
        self.name.as_ref()
    }
}

/// External tokens in grammar order; index is the scanner ordinal.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
#[facet(transparent)]
pub struct ExternalTokenTable(Vec<ExternalTokenDecl>);

impl ExternalTokenTable {
    /// Build an external token table from raw grammar externals.
    #[cfg(feature = "tree-sitter-import")]
    pub(crate) fn from_rules(rules: &[RawRuleJson]) -> Result<Self, ImportError> {
        let mut tokens = Vec::with_capacity(rules.len());
        for (index, rule) in rules.iter().cloned().enumerate() {
            tokens.push(ExternalTokenDecl::new(
                ExternalTokenOrdinal::from_index(index)?,
                rule,
            ));
        }
        Ok(Self(tokens))
    }

    /// Number of external tokens.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no external tokens.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get an external token by ordinal index.
    pub fn get(&self, index: usize) -> Option<&ExternalTokenDecl> {
        self.0.get(index)
    }

    /// Iterate external tokens in scanner ordinal order.
    pub fn iter(&self) -> impl Iterator<Item = &ExternalTokenDecl> {
        self.0.iter()
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
    pub externals: ExternalTokenTable,
}
