//! Schema validation for the LSP.
//!
//! This module handles loading schema files and validating documents against them.

use std::path::{Path, PathBuf};

use styx_schema::{SchemaFile, ValidationResult, validate};
use styx_tree::Value;
use tower_lsp::lsp_types::Url;

/// Reference to a schema - either external file or inline definition.
pub enum SchemaRef {
    /// External schema file path.
    External(String),
    /// Inline schema definition.
    Inline(Value),
}

/// Find the schema declaration in a document.
///
/// Looks for a unit key (`@`) in the root object with either:
/// - A string value (external schema path)
/// - An object value (inline schema)
pub fn find_schema_declaration(value: &Value) -> Option<SchemaRef> {
    let obj = value.as_object()?;

    for entry in &obj.entries {
        if entry.key.is_unit() {
            // Found @ key
            if let Some(path) = entry.value.as_str() {
                return Some(SchemaRef::External(path.to_string()));
            } else if entry.value.as_object().is_some() {
                return Some(SchemaRef::Inline(entry.value.clone()));
            }
        }
    }

    None
}

/// Resolve a schema path relative to the document URI.
pub fn resolve_schema_path(schema_path: &str, document_uri: &Url) -> Option<PathBuf> {
    // If it's a URL, not supported yet
    if schema_path.starts_with("http://") || schema_path.starts_with("https://") {
        return None;
    }

    let path = Path::new(schema_path);

    // If absolute, return as-is
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }

    // Resolve relative to document's directory
    let doc_path = document_uri.to_file_path().ok()?;
    let parent = doc_path.parent()?;
    Some(parent.join(schema_path))
}

/// Load a schema file from disk.
pub fn load_schema_file(path: &Path) -> Result<SchemaFile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read schema file '{}': {}", path.display(), e))?;

    facet_styx::from_str(&content)
        .map_err(|e| format!("failed to parse schema file '{}': {}", path.display(), e))
}

/// Parse an inline schema from a Value.
pub fn parse_inline_schema(value: &Value) -> Result<SchemaFile, String> {
    // Convert value back to string and re-parse as schema
    let content = styx_format::format_value(value, styx_format::FormatOptions::default());
    facet_styx::from_str(&content).map_err(|e| format!("failed to parse inline schema: {}", e))
}

/// Strip the schema declaration (@ key) from a document before validation.
pub fn strip_schema_declaration(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        let filtered_entries: Vec<_> = obj
            .entries
            .iter()
            .filter(|e| !e.key.is_unit())
            .cloned()
            .collect();
        Value {
            tag: value.tag.clone(),
            payload: Some(styx_tree::Payload::Object(styx_tree::Object {
                entries: filtered_entries,
                separator: obj.separator,
                span: obj.span,
            })),
            span: value.span,
        }
    } else {
        value.clone()
    }
}

/// Load and validate a document against its declared schema.
///
/// Returns validation errors, or a schema loading error message.
pub fn validate_against_schema(
    value: &Value,
    document_uri: &Url,
) -> Result<ValidationResult, String> {
    let schema_ref =
        find_schema_declaration(value).ok_or_else(|| "no schema declaration found".to_string())?;

    let schema_file = match schema_ref {
        SchemaRef::External(path) => {
            let resolved = resolve_schema_path(&path, document_uri)
                .ok_or_else(|| format!("could not resolve schema path '{}'", path))?;
            load_schema_file(&resolved)?
        }
        SchemaRef::Inline(schema_value) => parse_inline_schema(&schema_value)?,
    };

    // Strip schema declaration before validation
    let value_for_validation = strip_schema_declaration(value);

    Ok(validate(&value_for_validation, &schema_file))
}

/// Find a value in the tree by path (e.g., "server.tls.cert").
///
/// Returns the value and its span if found.
pub fn find_value_by_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }

    let obj = value.as_object()?;

    // Split path on first dot
    let (segment, rest) = match path.find('.') {
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => (path, ""),
    };

    // Handle array index notation [n]
    if segment.starts_with('[') && segment.ends_with(']') {
        let index: usize = segment[1..segment.len() - 1].parse().ok()?;
        let seq = value.as_sequence()?;
        let item = seq.items.get(index)?;
        return find_value_by_path(item, rest);
    }

    // Find the entry with matching key
    for entry in &obj.entries {
        if let Some(key_str) = entry.key.as_str()
            && key_str == segment {
                return find_value_by_path(&entry.value, rest);
            }
    }

    None
}

/// Get the span for a validation error path.
///
/// Returns (start_offset, end_offset) or None if not found.
pub fn get_error_span(value: &Value, error_path: &str) -> Option<(usize, usize)> {
    let target = find_value_by_path(value, error_path)?;
    let span = target.span?;
    Some((span.start as usize, span.end as usize))
}
