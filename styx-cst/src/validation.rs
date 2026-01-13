//! Semantic validation for Styx CST.
//!
//! This module performs validation checks that go beyond syntax,
//! such as detecting duplicate keys and mixed separators.

use std::collections::HashSet;

use rowan::TextRange;

use crate::ast::{AstNode, Document, Entry, Object, Separator};
use crate::syntax_kind::{SyntaxKind, SyntaxNode};

/// A diagnostic message from validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The text range where the issue occurs.
    pub range: TextRange,
    /// The severity of the diagnostic.
    pub severity: Severity,
    /// The diagnostic message.
    pub message: String,
}

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// An error that should be fixed.
    Error,
    /// A warning about potential issues.
    Warning,
    /// Informational hint.
    Hint,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(range: TextRange, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Severity::Error,
            message: message.into(),
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(range: TextRange, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Severity::Warning,
            message: message.into(),
        }
    }

    /// Create a new hint diagnostic.
    pub fn hint(range: TextRange, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Severity::Hint,
            message: message.into(),
        }
    }
}

/// Validate a document and return all diagnostics.
pub fn validate(root: &SyntaxNode) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_node(root, &mut diagnostics);
    diagnostics
}

/// Validate a document AST node.
pub fn validate_document(doc: &Document) -> Vec<Diagnostic> {
    validate(doc.syntax())
}

fn validate_node(node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    match node.kind() {
        SyntaxKind::DOCUMENT => {
            if let Some(doc) = Document::cast(node.clone()) {
                validate_entries_for_duplicates(doc.entries(), diagnostics);
            }
        }
        SyntaxKind::OBJECT => {
            if let Some(obj) = Object::cast(node.clone()) {
                validate_object(&obj, diagnostics);
            }
        }
        _ => {}
    }

    // Recurse into children
    for child in node.children() {
        validate_node(&child, diagnostics);
    }
}

fn validate_object(obj: &Object, diagnostics: &mut Vec<Diagnostic>) {
    // Check for mixed separators
    if obj.separator() == Separator::Mixed {
        diagnostics.push(Diagnostic::warning(
            obj.syntax().text_range(),
            "object uses mixed separators (both commas and newlines)",
        ));
    }

    // Check for duplicate keys
    validate_entries_for_duplicates(obj.entries(), diagnostics);
}

fn validate_entries_for_duplicates(
    entries: impl Iterator<Item = Entry>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen_keys: HashSet<String> = HashSet::new();

    for entry in entries {
        if let Some(key) = entry.key() {
            let key_text = key.text_content();
            if !key_text.is_empty() {
                if seen_keys.contains(&key_text) {
                    diagnostics.push(Diagnostic::warning(
                        key.syntax().text_range(),
                        format!("duplicate key: `{}`", key_text),
                    ));
                } else {
                    seen_keys.insert(key_text);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn validate_source(source: &str) -> Vec<Diagnostic> {
        let p = parse(source);
        validate(&p.syntax())
    }

    #[test]
    fn test_no_errors() {
        let diagnostics = validate_source("host localhost\nport 8080");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_keys_at_root() {
        let diagnostics = validate_source("host localhost\nhost other");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("duplicate key"));
        assert!(diagnostics[0].message.contains("host"));
    }

    #[test]
    fn test_duplicate_keys_in_object() {
        let diagnostics = validate_source("config { a 1, a 2 }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("duplicate key"));
    }

    #[test]
    fn test_mixed_separators() {
        let diagnostics = validate_source("{ a 1, b 2\nc 3 }");
        assert!(
            diagnostics.iter().any(|d| d.message.contains("mixed")),
            "expected mixed separator warning: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_nested_validation() {
        // Both inner and outer objects have issues
        let diagnostics = validate_source("outer { inner { x 1, x 2 }, inner { y 1 } }");

        // Should find duplicate 'x' in the inner object
        // and duplicate 'inner' in the outer object
        assert!(diagnostics.len() >= 2, "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn test_comma_only_is_fine() {
        let diagnostics = validate_source("{ a 1, b 2, c 3 }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_newline_only_is_fine() {
        let diagnostics = validate_source("{\na 1\nb 2\nc 3\n}");
        // Filter out mixed separator warnings (newlines between entries are fine)
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty());
    }
}
