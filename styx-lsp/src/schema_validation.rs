//! Schema validation for the LSP.
//!
//! This module handles loading schema files and validating documents against them.

use std::path::{Path, PathBuf};

use styx_schema::{Schema, SchemaFile, ValidationResult, validate};
use styx_tree::Value;
use tower_lsp::lsp_types::Url;

/// A field from a schema with its name and type info.
#[derive(Debug, Clone)]
pub struct SchemaField {
    /// Field name
    pub name: String,
    /// Whether the field is optional
    pub optional: bool,
    /// Default value if specified in schema
    pub default_value: Option<String>,
    /// The schema type (for generating placeholder values)
    pub schema: Schema,
}

/// Reference to a schema - either external file or inline definition.
#[derive(Debug)]
pub enum SchemaRef {
    /// External schema file path.
    External(String),
    /// Inline schema definition.
    Inline(Value),
    /// Embedded schema from binary: @schema {source ..., cli <binary>}
    Embedded { cli: String },
}

/// Find the schema declaration in a document.
///
/// Looks for:
/// - `@schema {source ..., cli <binary>}` - embedded schema from binary
/// - `@ "path/to/schema.styx"` - external schema file
/// - `@ { inline schema }` - inline schema definition
pub fn find_schema_declaration(value: &Value) -> Option<SchemaRef> {
    let obj = value.as_object()?;

    for entry in &obj.entries {
        // Check for @schema {source ..., cli ...} directive
        if entry.key.is_schema_tag() {
            if let Some(schema_obj) = entry.value.as_object() {
                if let Some(cli_value) = schema_obj.get("cli") {
                    if let Some(cli_name) = cli_value.as_str() {
                        return Some(SchemaRef::Embedded {
                            cli: cli_name.to_string(),
                        });
                    }
                }
            }
            // @schema directive without valid cli field - ignore
            continue;
        }

        // Check for @ (unit key) with path or inline schema
        if entry.key.is_unit() {
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

/// Strip schema declaration keys from a document before validation.
/// Both `@` (unit key) and `@schema` (tagged unit key) are schema metadata.
pub fn strip_schema_declaration(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        let filtered_entries: Vec<_> = obj
            .entries
            .iter()
            .filter(|e| !e.key.is_unit() && !e.key.is_schema_tag())
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

/// Extract schema from a binary with embedded styx schemas.
///
/// Uses the `which` crate to find the binary in PATH, then extracts
/// embedded schemas using `styx_embed::extract_schemas_from_file`.
fn extract_embedded_schema(cli_name: &str) -> Result<SchemaFile, String> {
    // Find the binary in PATH
    let binary_path = which::which(cli_name)
        .map_err(|_| format!("binary '{}' not found in PATH", cli_name))?;

    // Extract schemas from the binary (zero-execution, memory-mapped scan)
    let schemas = styx_embed::extract_schemas_from_file(&binary_path)
        .map_err(|e| format!("failed to extract schema from '{}': {}", binary_path.display(), e))?;

    // We expect at least one schema
    if schemas.is_empty() {
        return Err(format!("no embedded schemas found in '{}'", binary_path.display()));
    }

    // Parse the first schema
    facet_styx::from_str(&schemas[0])
        .map_err(|e| format!("failed to parse embedded schema: {}", e))
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
        SchemaRef::Embedded { cli } => extract_embedded_schema(&cli)?,
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
            && key_str == segment
        {
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

/// Get all fields from the root schema object.
pub fn get_schema_fields(schema_file: &SchemaFile) -> Vec<SchemaField> {
    let mut fields = Vec::new();

    // Get the root schema (key = None)
    let Some(root_schema) = schema_file.schema.get(&None) else {
        return fields;
    };

    collect_object_fields(root_schema, &mut fields);
    fields
}

/// Recursively collect fields from a schema, handling wrappers like @object.
fn collect_object_fields(schema: &Schema, fields: &mut Vec<SchemaField>) {
    match schema {
        Schema::Object(obj) => {
            for (key, field_schema) in &obj.0 {
                // Skip the catch-all @ field
                let Some(name) = key else { continue };

                let (optional, default_value, inner_schema) =
                    unwrap_field_modifiers(field_schema.clone());

                fields.push(SchemaField {
                    name: name.clone(),
                    optional,
                    default_value,
                    schema: inner_schema,
                });
            }
        }
        // Handle flatten - inline fields from another type
        Schema::Flatten(flatten) => {
            collect_object_fields(&flatten.0.0, fields);
        }
        // Handle type references - look them up in the schema
        Schema::Type {
            name: Some(_type_name),
        } => {
            // Would need schema_file to resolve - skip for now
        }
        _ => {}
    }
}

/// Unwrap field modifiers like @optional and @default to get the inner type.
fn unwrap_field_modifiers(schema: Schema) -> (bool, Option<String>, Schema) {
    match schema {
        Schema::Optional(opt) => {
            let (_, default, inner) = unwrap_field_modifiers(*opt.0.0);
            (true, default, inner)
        }
        Schema::Default(def) => {
            let default_value = def.0.0.clone();
            let (optional, _, inner) = unwrap_field_modifiers(*def.0.1);
            (optional, Some(default_value), inner)
        }
        Schema::Deprecated(dep) => {
            // Still include deprecated fields but unwrap
            unwrap_field_modifiers(*dep.0.1)
        }
        other => (false, None, other),
    }
}

/// Generate a placeholder value for a schema type.
pub fn generate_placeholder(schema: &Schema) -> String {
    match schema {
        Schema::String(_) => "\"\"".to_string(),
        Schema::Int(_) => "0".to_string(),
        Schema::Float(_) => "0.0".to_string(),
        Schema::Bool => "false".to_string(),
        Schema::Unit => "@".to_string(),
        Schema::Any => "@".to_string(),
        Schema::Seq(_) => "[]".to_string(),
        Schema::Map(_) => "{}".to_string(),
        Schema::Object(_) => "{}".to_string(),
        Schema::Optional(opt) => generate_placeholder(&opt.0.0),
        Schema::Default(def) => def.0.0.clone(), // Use the default value
        Schema::Deprecated(dep) => generate_placeholder(&dep.0.1),
        Schema::Union(u) => {
            // Use first variant as placeholder
            u.0.first()
                .map(generate_placeholder)
                .unwrap_or_else(|| "@".to_string())
        }
        Schema::Enum(e) => {
            // Use first variant name as placeholder
            e.0.keys()
                .next()
                .map(|k| format!("@{}", k))
                .unwrap_or_else(|| "@".to_string())
        }
        Schema::Flatten(f) => generate_placeholder(&f.0.0),
        Schema::Literal(lit) => lit.clone(),
        Schema::Type { name } => {
            // For custom types, use a tagged unit as placeholder
            name.as_ref()
                .map(|n| format!("@{}", n))
                .unwrap_or_else(|| "@".to_string())
        }
    }
}

/// Load schema for a document and return the SchemaFile.
pub fn load_document_schema(value: &Value, document_uri: &Url) -> Result<SchemaFile, String> {
    let schema_ref =
        find_schema_declaration(value).ok_or_else(|| "no schema declaration found".to_string())?;

    match schema_ref {
        SchemaRef::External(path) => {
            let resolved = resolve_schema_path(&path, document_uri)
                .ok_or_else(|| format!("could not resolve schema path '{}'", path))?;
            load_schema_file(&resolved)
        }
        SchemaRef::Inline(schema_value) => parse_inline_schema(&schema_value),
        SchemaRef::Embedded { cli } => extract_embedded_schema(&cli),
    }
}

/// Get the existing field names from a document.
pub fn get_document_fields(value: &Value) -> Vec<String> {
    let mut fields = Vec::new();
    if let Some(obj) = value.as_object() {
        for entry in &obj.entries {
            if let Some(name) = entry.key.as_str() {
                fields.push(name.to_string());
            }
        }
    }
    fields
}

/// Info about an object at a cursor position
#[derive(Debug, Clone)]
pub struct ObjectContext {
    /// Path to this object (e.g., ["server", "tls"] for nested)
    pub path: Vec<String>,
    /// The object value itself
    pub object: styx_tree::Object,
    /// Span of the object in the source
    pub span: Option<styx_tree::Span>,
}

/// Find the innermost object containing the given offset.
/// Returns the path to that object and the object itself.
pub fn find_object_at_offset(value: &Value, offset: usize) -> Option<ObjectContext> {
    find_object_at_offset_recursive(value, offset, Vec::new())
}

fn find_object_at_offset_recursive(
    value: &Value,
    offset: usize,
    path: Vec<String>,
) -> Option<ObjectContext> {
    let obj = value.as_object()?;

    // Check if offset is within this object's span
    if let Some(span) = obj.span
        && (offset < span.start as usize || offset > span.end as usize)
    {
        return None;
    }

    // Check each entry to see if we're inside a nested object
    for entry in &obj.entries {
        if let Some(val_span) = entry.value.span
            && offset >= val_span.start as usize
            && offset <= val_span.end as usize
        {
            // We're inside this value - check if it's a nested object
            if let Some(nested_obj) = entry.value.as_object() {
                let mut nested_path = path.clone();
                if let Some(key) = entry.key.as_str() {
                    nested_path.push(key.to_string());
                }
                // Recurse into the nested object
                if let Some(deeper) =
                    find_object_at_offset_recursive(&entry.value, offset, nested_path.clone())
                {
                    return Some(deeper);
                }
                // We're in this nested object but not deeper
                return Some(ObjectContext {
                    path: nested_path,
                    object: nested_obj.clone(),
                    span: entry.value.span,
                });
            }
        }
    }

    // We're in this object but not in any nested object
    Some(ObjectContext {
        path,
        object: obj.clone(),
        span: obj.span,
    })
}

/// Get the schema for a given path within a schema file.
/// For example, path ["server", "tls"] would look up the schema for the tls field
/// inside the server field.
pub fn get_schema_at_path(schema_file: &SchemaFile, path: &[String]) -> Option<Schema> {
    let root_schema = schema_file.schema.get(&None)?;
    get_schema_at_path_recursive(root_schema, path, schema_file)
}

fn get_schema_at_path_recursive(
    schema: &Schema,
    path: &[String],
    schema_file: &SchemaFile,
) -> Option<Schema> {
    if path.is_empty() {
        return Some(schema.clone());
    }

    let field_name = &path[0];
    let rest = &path[1..];

    match schema {
        Schema::Object(obj) => {
            // Look up the field in this object
            let field_schema = obj.0.get(&Some(field_name.clone()))?;
            get_schema_at_path_recursive(field_schema, rest, schema_file)
        }
        Schema::Optional(opt) => {
            // Unwrap optional and continue
            get_schema_at_path_recursive(&opt.0.0, path, schema_file)
        }
        Schema::Default(def) => {
            // Unwrap default and continue
            get_schema_at_path_recursive(&def.0.1, path, schema_file)
        }
        Schema::Deprecated(dep) => {
            // Unwrap deprecated and continue
            get_schema_at_path_recursive(&dep.0.1, path, schema_file)
        }
        Schema::Type {
            name: Some(type_name),
        } => {
            // Look up the named type in schema definitions
            let type_schema = schema_file.schema.get(&Some(type_name.clone()))?;
            get_schema_at_path_recursive(type_schema, path, schema_file)
        }
        _ => None,
    }
}

/// Get fields for a schema at a specific path.
pub fn get_schema_fields_at_path(schema_file: &SchemaFile, path: &[String]) -> Vec<SchemaField> {
    let Some(schema) = get_schema_at_path(schema_file, path) else {
        return Vec::new();
    };

    let mut fields = Vec::new();
    collect_object_fields(&schema, &mut fields);
    fields
}
