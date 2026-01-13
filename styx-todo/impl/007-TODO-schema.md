# Phase 007: styx-schema (Schema Validation)

Schema definition and validation library for Styx. Used by both CLI tools and the LSP server.

## Key Design Principle: Eat Your Own Dog Food

**The meta-schema (from `docs/content/spec/schema.md`) must be bundled into styx-schema and
used to validate schema files via facet-styx deserialization.**

This means:
1. The meta-schema is embedded in the crate (via `include_str!` or const)
2. Schema files are deserialized into proper Rust structs using `facet-styx`
3. No hand-written schema parsing logic - if facet-styx can't deserialize the meta-schema,
   that's a bug in the deserialization stack that needs fixing

This approach:
- Ensures the spec and implementation stay in sync
- Tests the full deserialization pipeline with a complex real-world schema
- Catches gaps in facet-styx early

**If deserialization issues are encountered:**
- Implementation gaps → fix them in facet-styx/styx-parse
- Design issues → stop and escalate (spec might need adjustment)

## Deliverables

- `crates/styx-schema/src/lib.rs` - Crate root
- `crates/styx-schema/src/types.rs` - Schema type definitions (derived via facet)
- `crates/styx-schema/src/meta.rs` - Bundled meta-schema
- `crates/styx-schema/src/validate.rs` - Validate documents against schema
- `crates/styx-schema/src/error.rs` - Validation error types

## Dependencies

```toml
[dependencies]
styx-parse = { path = "../styx-parse" }
styx-tree = { path = "../styx-tree" }
facet-styx = { path = "../facet-styx" }
facet = { version = "0.42" }
```

## Schema Types (via facet derive)

These types are derived from the meta-schema specification. They use `#[derive(Facet)]`
for automatic deserialization from Styx.

```rust
use facet::Facet;

/// A complete schema file.
#[derive(Facet)]
pub struct SchemaFile {
    /// Schema metadata (required).
    pub meta: Meta,
    /// External schema imports (optional).
    pub imports: Option<HashMap<String, String>>,
    /// Type definitions: @ for document root, strings for named types.
    pub schema: HashMap<SchemaKey, Schema>,
}

/// Schema metadata.
#[derive(Facet)]
pub struct Meta {
    /// Unique identifier for the schema (URL recommended).
    pub id: String,
    /// Schema version (date or semver).
    pub version: String,
    /// Human-readable description.
    pub description: Option<String>,
}

/// Key in the schema map: either a type name (String) or @ (unit key for root).
#[derive(Facet, Hash, Eq, PartialEq)]
pub enum SchemaKey {
    TypeName(String),
    Root, // represents @
}

/// A type constraint (the Schema union from the meta-schema).
#[derive(Facet)]
pub enum Schema {
    /// Literal value constraint (scalar).
    Literal(String),
    /// Type reference (any tag with unit payload like @string, @MyType).
    TypeRef(String),
    /// Object schema: {field @type}
    Object(HashMap<ObjectKey, Schema>),
    /// Sequence schema: (@type)
    Sequence(Box<Schema>),
    /// Union: @union(@A @B)
    Union(Vec<Schema>),
    /// Optional: @optional(@T)
    Optional(Box<Schema>),
    /// Enum: @enum{a, b {x @type}}
    Enum(HashMap<String, EnumVariant>),
    /// Map: @map(@V) or @map(@K @V)
    Map(MapSchema),
    /// Flatten: @flatten(@Type)
    Flatten(String),
}

#[derive(Facet)]
pub enum ObjectKey {
    Field(String),
    AdditionalFields, // represents @ key
}

#[derive(Facet)]
pub enum EnumVariant {
    Unit,
    Payload(HashMap<String, Schema>),
}

#[derive(Facet)]
pub enum MapSchema {
    /// @map(@V) - string keys
    ValueOnly(Box<Schema>),
    /// @map(@K @V) - explicit key type
    KeyValue(Box<Schema>, Box<Schema>),
}
```

## Bundled Meta-Schema

```rust
// src/meta.rs

/// The meta-schema for validating schema files.
/// This is the schema from docs/content/spec/schema.md
pub const META_SCHEMA_SOURCE: &str = include_str!("../../../docs/content/spec/schema.md");

// Extract the schema block from the markdown (between ```styx and ```)
// Or alternatively, have a separate meta-schema.styx file

/// Load the meta-schema as a Schema struct.
pub fn meta_schema() -> &'static SchemaFile {
    static META: OnceLock<SchemaFile> = OnceLock::new();
    META.get_or_init(|| {
        let source = extract_schema_from_spec(META_SCHEMA_SOURCE);
        facet_styx::from_str(&source)
            .expect("meta-schema must deserialize - this is a bug in the stack")
    })
}
```

## Validation API

```rust
/// Validate a document against a schema.
pub fn validate(
    doc: &styx_tree::Document,
    schema: &SchemaFile,
) -> ValidationResult;

/// Validation result.
pub struct ValidationResult {
    /// Whether validation passed.
    pub is_valid: bool,
    /// Validation errors.
    pub errors: Vec<ValidationError>,
    /// Validation warnings.
    pub warnings: Vec<ValidationWarning>,
}

/// A validation error.
pub struct ValidationError {
    /// Path to the error (e.g., "server.tls.cert").
    pub path: String,
    /// Span in the source document.
    pub span: Option<Span>,
    /// Error kind.
    pub kind: ValidationErrorKind,
    /// Human-readable message.
    pub message: String,
}

pub enum ValidationErrorKind {
    /// Missing required field.
    MissingField { field: String },
    /// Unknown field (when additional_fields is false).
    UnknownField { field: String },
    /// Type mismatch.
    TypeMismatch { expected: String, got: String },
    /// Invalid value for type.
    InvalidValue { reason: String },
    /// Pattern validation failed.
    PatternMismatch { pattern: String },
    /// Unknown type reference.
    UnknownType { name: String },
    /// Invalid enum variant.
    InvalidVariant { expected: Vec<String>, got: String },
}
```

## Schema Loading

```rust
/// Load a schema from a Styx source string.
pub fn load_schema(source: &str) -> Result<SchemaFile, SchemaError> {
    // First validate the schema file against the meta-schema
    let meta = meta_schema();
    
    // Then deserialize into our types
    facet_styx::from_str(source).map_err(SchemaError::from)
}

/// Load a schema from a file.
pub fn load_schema_file(path: &Path) -> Result<SchemaFile, SchemaError> {
    let source = std::fs::read_to_string(path)?;
    load_schema(&source)
}

/// Schema loading errors.
pub enum SchemaError {
    /// IO error reading file.
    Io(std::io::Error),
    /// Deserialization error.
    Deserialize(facet_styx::Error),
    /// Validation error (schema doesn't match meta-schema).
    Validation(Vec<ValidationError>),
}
```

## Schema Discovery

Schemas can be associated with documents via:

1. **Schema declaration in document**: `@ https://example.com/schema.styx` or `@ { schema { ... } }`
2. **File naming convention**: `foo.styx` looks for `foo.schema.styx`
3. **Directory convention**: `.styx-schema` file in directory

```rust
/// Find schema for a document.
pub fn discover_schema(doc_path: &Path) -> Option<PathBuf> {
    // Try foo.schema.styx
    let schema_path = doc_path.with_extension("schema.styx");
    if schema_path.exists() {
        return Some(schema_path);
    }
    
    // Try .styx-schema in directory
    let dir_schema = doc_path.parent()?.join(".styx-schema");
    if dir_schema.exists() {
        return Some(dir_schema);
    }
    
    None
}
```

## Testing

1. **Meta-schema self-validation**: The meta-schema must deserialize successfully
2. **Round-trip tests**: Parse schema → serialize → parse should be equivalent
3. **Validation tests**: Test each constraint type
4. **Error message quality**: Ensure errors have good spans and messages
5. **Integration tests**: Validate real-world schema files
