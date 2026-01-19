//! Schema validation for the LSP.
//!
//! This module handles loading schema files and validating documents against them.

use std::path::{Path, PathBuf};

use facet_styx::{Schema, SchemaFile, ValidationResult, validate};
use styx_tree::Value;
use tower_lsp::lsp_types::Url;

use crate::cache;

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

/// Reference to a schema (before resolution).
#[derive(Debug, Clone)]
pub enum SchemaRef {
    /// External schema file path: @schema path/to/schema.styx
    External(String),
    /// Embedded schema from binary: @schema {id ..., cli <binary>}
    Embedded { cli: String },
    /// Explicit opt-out: @schema @ (no schema validation)
    None,
}

impl SchemaRef {
    /// Load the schema source text.
    ///
    /// Returns `Err` for `SchemaRef::None` since there's no source to load.
    pub fn load_source(&self, document_uri: &Url) -> Result<String, String> {
        match self {
            SchemaRef::External(path) => {
                let resolved = resolve_schema_path(path, document_uri)
                    .ok_or_else(|| format!("could not resolve schema path '{}'", path))?;
                std::fs::read_to_string(&resolved).map_err(|e| {
                    format!("failed to read schema file '{}': {}", resolved.display(), e)
                })
            }
            SchemaRef::Embedded { cli } => extract_embedded_schema_source(cli),
            SchemaRef::None => Err("schema validation explicitly disabled".to_string()),
        }
    }

    /// Load and parse the schema file.
    ///
    /// Returns `Err` for `SchemaRef::None` since there's no schema to load.
    pub fn load_schema(&self, document_uri: &Url) -> Result<SchemaFile, String> {
        match self {
            SchemaRef::External(path) => {
                let resolved = resolve_schema_path(path, document_uri)
                    .ok_or_else(|| format!("could not resolve schema path '{}'", path))?;
                load_schema_file(&resolved)
            }
            SchemaRef::Embedded { cli } => extract_embedded_schema(cli),
            SchemaRef::None => Err("schema validation explicitly disabled".to_string()),
        }
    }

    /// Get the URI for this schema reference.
    ///
    /// For embedded schemas, caches the source to disk first.
    /// Returns `Err` for `SchemaRef::None`.
    pub fn to_uri(&self, document_uri: &Url, source: &str) -> Result<Url, String> {
        match self {
            SchemaRef::External(path) => {
                let resolved = resolve_schema_path(path, document_uri)
                    .ok_or_else(|| format!("could not resolve schema path '{}'", path))?;
                Url::from_file_path(&resolved)
                    .map_err(|_| format!("could not create URI for '{}'", resolved.display()))
            }
            SchemaRef::Embedded { cli } => {
                // Cache the schema to disk and return a file:// URI
                if let Some(cache_path) = cache::cache_embedded_schema(cli, source) {
                    Url::from_file_path(&cache_path)
                        .map_err(|_| format!("could not create URI for '{}'", cache_path.display()))
                } else {
                    // Fallback to virtual URI if caching fails
                    Url::parse(&format!("{}://{}/schema.styx", EMBEDDED_SCHEMA_SCHEME, cli))
                        .map_err(|e| format!("could not create embedded schema URI: {}", e))
                }
            }
            SchemaRef::None => Err("schema validation explicitly disabled".to_string()),
        }
    }

    /// Returns true if this is an explicit opt-out (`@schema @`).
    #[cfg(test)]
    pub fn is_none(&self) -> bool {
        matches!(self, SchemaRef::None)
    }
}

/// A fully resolved schema with source text and location.
///
/// This is the single source of truth for schema information in the LSP.
/// All features (hover, completion, diagnostics, etc.) should use this.
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    /// The raw source text (for doc comments, field lookup, parsing)
    pub source: String,
    /// URI for the schema location:
    /// - `file://` for external schema files
    /// - `styx-embedded://<cli>/schema.styx` for embedded schemas
    pub uri: Url,
}

/// Find the schema declaration in a document.
///
/// Looks for:
/// - `@schema @` - explicit opt-out (no schema)
/// - `@schema "path/to/schema.styx"` - external schema file
/// - `@schema {id ..., cli <binary>}` - embedded schema from binary
pub fn find_schema_declaration(value: &Value) -> Option<SchemaRef> {
    let obj = value.as_object()?;

    for entry in &obj.entries {
        if entry.key.is_schema_tag() {
            // @schema @ (explicit opt-out)
            if entry.value.is_unit() {
                return Some(SchemaRef::None);
            }

            // @schema path/to/schema.styx
            if let Some(path) = entry.value.as_str() {
                return Some(SchemaRef::External(path.to_string()));
            }

            // @schema {id ..., cli ...}
            if let Some(schema_obj) = entry.value.as_object()
                && let Some(cli_value) = schema_obj.get("cli")
                && let Some(cli_name) = cli_value.as_str()
            {
                return Some(SchemaRef::Embedded {
                    cli: cli_name.to_string(),
                });
            }
        }
    }

    None
}

/// Resolve a schema path relative to the document URI.
fn resolve_schema_path(schema_path: &str, document_uri: &Url) -> Option<PathBuf> {
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
fn load_schema_file(path: &Path) -> Result<SchemaFile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read schema file '{}': {}", path.display(), e))?;

    facet_styx::from_str(&content)
        .map_err(|e| format!("failed to parse schema file '{}': {}", path.display(), e))
}

/// Strip schema declaration keys from a document before validation.
pub fn strip_schema_declaration(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        let filtered_entries: Vec<_> = obj
            .entries
            .iter()
            .filter(|e| !e.key.is_schema_tag())
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
fn extract_embedded_schema(cli_name: &str) -> Result<SchemaFile, String> {
    let source = extract_embedded_schema_source(cli_name)?;
    facet_styx::from_str(&source).map_err(|e| format!("failed to parse embedded schema: {}", e))
}

/// Extract schema source text from a binary with embedded styx schemas.
fn extract_embedded_schema_source(cli_name: &str) -> Result<String, String> {
    let binary_path =
        which::which(cli_name).map_err(|_| format!("binary '{}' not found in PATH", cli_name))?;

    let schemas = styx_embed::extract_schemas_from_file(&binary_path).map_err(|e| {
        format!(
            "failed to extract schema from '{}': {}",
            binary_path.display(),
            e
        )
    })?;

    if schemas.is_empty() {
        return Err(format!(
            "no embedded schemas found in '{}'",
            binary_path.display()
        ));
    }

    Ok(schemas.into_iter().next().unwrap())
}

/// The URI scheme for embedded schemas (fallback if caching fails).
pub const EMBEDDED_SCHEMA_SCHEME: &str = "styx-embedded";

/// Resolve a schema reference to a fully loaded ResolvedSchema.
///
/// This is the main entry point for getting schema information.
/// Returns `Err` if no schema declaration, if schema is `@schema @`, or if loading fails.
pub fn resolve_schema(value: &Value, document_uri: &Url) -> Result<ResolvedSchema, String> {
    let schema_ref =
        find_schema_declaration(value).ok_or_else(|| "no schema declaration found".to_string())?;

    let source = schema_ref.load_source(document_uri)?;

    // Validate that the source is a valid schema
    let _: SchemaFile =
        facet_styx::from_str(&source).map_err(|e| format!("failed to parse schema: {}", e))?;

    let uri = schema_ref.to_uri(document_uri, &source)?;

    Ok(ResolvedSchema { source, uri })
}

/// Load and validate a document against its declared schema.
///
/// Returns validation errors, or an error message if schema can't be loaded.
/// Returns `Err` for `@schema @` (explicit opt-out).
pub fn validate_against_schema(
    value: &Value,
    document_uri: &Url,
) -> Result<ValidationResult, String> {
    let schema_ref =
        find_schema_declaration(value).ok_or_else(|| "no schema declaration found".to_string())?;

    let schema_file = schema_ref.load_schema(document_uri)?;

    // Strip schema declaration before validation
    let value_for_validation = strip_schema_declaration(value);

    Ok(validate(&value_for_validation, &schema_file))
}

/// Find a value in the tree by path (e.g., "server.tls.cert").
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
pub fn get_error_span(value: &Value, error_path: &str) -> Option<(usize, usize)> {
    let target = find_value_by_path(value, error_path)?;
    let span = target.span?;
    Some((span.start as usize, span.end as usize))
}

/// Get all fields from the root schema object.
pub fn get_schema_fields(schema_file: &SchemaFile) -> Vec<SchemaField> {
    let mut fields = Vec::new();

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
        Schema::Flatten(flatten) => {
            collect_object_fields(&flatten.0.0, fields);
        }
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
            let default_value = def.0.0.to_string();
            let (optional, _, inner) = unwrap_field_modifiers(*def.0.1);
            (optional, Some(default_value), inner)
        }
        Schema::Deprecated(dep) => unwrap_field_modifiers(*dep.0.1),
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
        Schema::Default(def) => def.0.0.to_string(),
        Schema::Deprecated(dep) => generate_placeholder(&dep.0.1),
        Schema::Union(u) => {
            u.0.first()
                .map(generate_placeholder)
                .unwrap_or_else(|| "@".to_string())
        }
        Schema::Enum(e) => {
            e.0.keys()
                .next()
                .map(|k| format!("@{}", k))
                .unwrap_or_else(|| "@".to_string())
        }
        Schema::Flatten(f) => generate_placeholder(&f.0.0),
        Schema::Literal(lit) => lit.clone(),
        Schema::Type { name } => name
            .as_ref()
            .map(|n| format!("@{}", n))
            .unwrap_or_else(|| "@".to_string()),
    }
}

/// Load schema for a document and return the SchemaFile.
pub fn load_document_schema(value: &Value, document_uri: &Url) -> Result<SchemaFile, String> {
    let schema_ref =
        find_schema_declaration(value).ok_or_else(|| "no schema declaration found".to_string())?;

    schema_ref.load_schema(document_uri)
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
pub fn find_object_at_offset(value: &Value, offset: usize) -> Option<ObjectContext> {
    find_object_at_offset_recursive(value, offset, Vec::new())
}

fn find_object_at_offset_recursive(
    value: &Value,
    offset: usize,
    path: Vec<String>,
) -> Option<ObjectContext> {
    let obj = value.as_object()?;

    if let Some(span) = obj.span
        && (offset < span.start as usize || offset > span.end as usize)
    {
        return None;
    }

    for entry in &obj.entries {
        if let Some(val_span) = entry.value.span
            && offset >= val_span.start as usize
            && offset <= val_span.end as usize
            && let Some(nested_obj) = entry.value.as_object()
        {
            let mut nested_path = path.clone();
            if let Some(key) = entry.key.as_str() {
                nested_path.push(key.to_string());
            }
            if let Some(deeper) =
                find_object_at_offset_recursive(&entry.value, offset, nested_path.clone())
            {
                return Some(deeper);
            }
            return Some(ObjectContext {
                path: nested_path,
                object: nested_obj.clone(),
                span: entry.value.span,
            });
        }
    }

    Some(ObjectContext {
        path,
        object: obj.clone(),
        span: obj.span,
    })
}

/// Get the schema for a given path within a schema file.
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
            let field_schema = obj.0.get(&Some(field_name.clone()))?;
            get_schema_at_path_recursive(field_schema, rest, schema_file)
        }
        Schema::Optional(opt) => get_schema_at_path_recursive(&opt.0.0, path, schema_file),
        Schema::Default(def) => get_schema_at_path_recursive(&def.0.1, path, schema_file),
        Schema::Deprecated(dep) => get_schema_at_path_recursive(&dep.0.1, path, schema_file),
        Schema::Type {
            name: Some(type_name),
        } => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_schema_declaration_none() {
        // No @schema declaration
        let value = styx_tree::parse("foo bar").unwrap();
        assert!(find_schema_declaration(&value).is_none());
    }

    #[test]
    fn test_find_schema_declaration_external() {
        let value = styx_tree::parse(r#"@schema "path/to/schema.styx""#).unwrap();
        let decl = find_schema_declaration(&value).expect("should find declaration");
        assert!(matches!(decl, SchemaRef::External(path) if path == "path/to/schema.styx"));
    }

    #[test]
    fn test_find_schema_declaration_embedded() {
        let value = styx_tree::parse("@schema {id crate:foo@1, cli foo}").unwrap();
        let decl = find_schema_declaration(&value).expect("should find declaration");
        assert!(matches!(decl, SchemaRef::Embedded { cli } if cli == "foo"));
    }

    #[test]
    fn test_find_schema_declaration_opt_out() {
        // @schema @ means "no schema, stop asking"
        let value = styx_tree::parse("@schema @").unwrap();
        let decl = find_schema_declaration(&value).expect("should find declaration");
        assert!(matches!(decl, SchemaRef::None));
        assert!(decl.is_none());
    }

    #[test]
    fn test_schema_ref_none_returns_error() {
        let schema_ref = SchemaRef::None;
        let uri = Url::parse("file:///test.styx").unwrap();

        // All methods should return errors for SchemaRef::None
        assert!(schema_ref.load_source(&uri).is_err());
        assert!(schema_ref.load_schema(&uri).is_err());
        assert!(schema_ref.to_uri(&uri, "").is_err());
    }

    #[test]
    fn test_opt_out_prevents_schema_hints() {
        // With @schema @, find_schema_declaration returns Some(SchemaRef::None)
        // This means "a schema declaration exists" so hints should not appear
        let value = styx_tree::parse("@schema @\nfoo bar").unwrap();
        let decl = find_schema_declaration(&value);
        assert!(
            decl.is_some(),
            "@schema @ should be detected as a declaration"
        );
    }
}
