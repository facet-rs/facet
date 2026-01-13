# Phase 007: styx-schema (Schema Validation)

Schema definition and validation library for Styx. Used by both CLI tools and the LSP server.

## Deliverables

- `crates/styx-schema/src/lib.rs` - Crate root
- `crates/styx-schema/src/types.rs` - Schema type definitions
- `crates/styx-schema/src/parse.rs` - Parse schema from Styx document
- `crates/styx-schema/src/validate.rs` - Validate documents against schema
- `crates/styx-schema/src/error.rs` - Validation error types

## Dependencies

```toml
[dependencies]
styx-parse = { path = "../styx-parse" }
styx-tree = { path = "../styx-tree" }
```

## Schema Format

Schemas are Styx documents with a specific structure:

```styx
// server-config.schema.styx
{
    /// The root type for this schema
    root ServerConfig
    
    types {
        /// Server configuration
        ServerConfig {
            fields {
                /// Server hostname or IP address
                host { type string, required true }
                /// Port number (1-65535)
                port { type int, required true }
                /// TLS configuration (optional)
                tls { type TlsConfig }
                /// List of allowed origins
                origins { type (string) }
            }
        }
        
        TlsConfig {
            fields {
                cert { type string, required true, doc "Path to certificate file" }
                key { type string, required true, doc "Path to private key file" }
                /// Minimum TLS version
                min_version { type TlsVersion, default "1.2" }
            }
        }
        
        /// TLS version enum
        TlsVersion {
            variants (
                @1.2
                @1.3
            )
        }
    }
}
```

## Type System

### Primitive Types

- `string` - Any scalar value
- `int` - Integer (validated via regex or parsing)
- `float` - Floating point number
- `bool` - `true` or `false`
- `any` - Any value (no validation)

### Compound Types

- `TypeName` - Reference to a defined type
- `(T)` - Sequence of T
- `{K: V}` - Map with key type K and value type V (keys are always strings)
- `T?` - Optional T (syntactic sugar for `required false`)

### Tagged Unions (Enums)

```styx
Result {
    variants (
        @Ok { type T }      // Tag with payload
        @Err { type string }
        @Pending            // Tag without payload (unit)
    )
}
```

### Struct Types

```styx
Person {
    fields {
        name { type string, required true }
        age { type int }
        email { type string, pattern r"^[^@]+@[^@]+$" }
    }
    /// Allow additional fields not in schema
    additional_fields true
}
```

## Schema Type Definitions

```rust
/// A complete schema definition.
pub struct Schema {
    /// The root type name.
    pub root: String,
    /// All type definitions.
    pub types: HashMap<String, TypeDef>,
}

/// A type definition.
pub enum TypeDef {
    /// Struct with named fields.
    Struct(StructDef),
    /// Tagged union (enum).
    Enum(EnumDef),
    /// Type alias.
    Alias(TypeRef),
}

/// Struct definition.
pub struct StructDef {
    /// Documentation comment.
    pub doc: Option<String>,
    /// Fields in this struct.
    pub fields: Vec<FieldDef>,
    /// Whether to allow fields not in schema.
    pub additional_fields: bool,
}

/// Field definition.
pub struct FieldDef {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: TypeRef,
    /// Whether this field is required.
    pub required: bool,
    /// Default value (as Styx source).
    pub default: Option<String>,
    /// Documentation comment.
    pub doc: Option<String>,
    /// Validation pattern (for strings).
    pub pattern: Option<String>,
}

/// Enum definition.
pub struct EnumDef {
    /// Documentation comment.
    pub doc: Option<String>,
    /// Enum variants.
    pub variants: Vec<VariantDef>,
}

/// Enum variant.
pub struct VariantDef {
    /// Variant name (tag name without @).
    pub name: String,
    /// Payload type (None for unit variants).
    pub payload: Option<TypeRef>,
    /// Documentation comment.
    pub doc: Option<String>,
}

/// A type reference.
pub enum TypeRef {
    /// Primitive type.
    Primitive(PrimitiveType),
    /// Named type reference.
    Named(String),
    /// Sequence type.
    Sequence(Box<TypeRef>),
    /// Map type.
    Map(Box<TypeRef>),
    /// Optional wrapper.
    Optional(Box<TypeRef>),
}

pub enum PrimitiveType {
    String,
    Int,
    Float,
    Bool,
    Any,
}
```

## Validation API

```rust
/// Validate a document against a schema.
pub fn validate(
    doc: &styx_tree::Document,
    schema: &Schema,
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
pub fn load_schema(source: &str) -> Result<Schema, SchemaError>;

/// Load a schema from a file.
pub fn load_schema_file(path: &Path) -> Result<Schema, SchemaError>;

/// Schema parsing errors.
pub enum SchemaError {
    /// Parse error in schema document.
    Parse(Vec<ParseError>),
    /// Invalid schema structure.
    InvalidStructure { message: String, span: Option<Span> },
    /// Unknown type reference.
    UnknownType { name: String, span: Option<Span> },
    /// Duplicate type definition.
    DuplicateType { name: String, span: Option<Span> },
    /// Cyclic type reference.
    CyclicType { cycle: Vec<String> },
}
```

## Validation Process

1. **Parse schema** - Load and validate the schema document itself
2. **Build type graph** - Resolve all type references, detect cycles
3. **Validate document** - Walk the document tree, checking against schema

```rust
fn validate_value(
    value: &Value,
    ty: &TypeRef,
    schema: &Schema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match (ty, value) {
        (TypeRef::Primitive(PrimitiveType::String), Value::Scalar(_)) => {
            // OK - any scalar is valid as string
        }
        (TypeRef::Primitive(PrimitiveType::Int), Value::Scalar(s)) => {
            if s.parse::<i64>().is_err() {
                errors.push(ValidationError {
                    path: path.to_string(),
                    kind: ValidationErrorKind::TypeMismatch {
                        expected: "int".into(),
                        got: "string".into(),
                    },
                    message: format!("expected integer, got '{}'", s),
                    span: value.span(),
                });
            }
        }
        (TypeRef::Named(name), value) => {
            let type_def = schema.types.get(name).unwrap();
            validate_against_type_def(value, type_def, schema, path, errors);
        }
        (TypeRef::Sequence(inner), Value::Sequence(items)) => {
            for (i, item) in items.iter().enumerate() {
                validate_value(item, inner, schema, &format!("{}[{}]", path, i), errors);
            }
        }
        // ... other cases
    }
}
```

## Schema Discovery

Schemas can be associated with documents via:

1. **File naming convention**: `foo.styx` looks for `foo.schema.styx`
2. **Schema directive**: `// @schema: ./path/to/schema.styx` at top of file
3. **Directory convention**: `.styx-schema` file in directory
4. **Explicit**: Pass schema path to validator

```rust
/// Find schema for a document.
pub fn discover_schema(doc_path: &Path) -> Option<PathBuf> {
    // Try foo.schema.styx
    let schema_path = doc_path.with_extension("schema.styx");
    if schema_path.exists() {
        return Some(schema_path);
    }
    
    // Try schema directive in file
    if let Some(directive) = find_schema_directive(doc_path) {
        return Some(doc_path.parent()?.join(directive));
    }
    
    // Try .styx-schema in directory
    let dir_schema = doc_path.parent()?.join(".styx-schema");
    if dir_schema.exists() {
        return Some(dir_schema);
    }
    
    None
}
```

## Integration with CST

For LSP integration, we also support validation against CST nodes:

```rust
/// Validate a CST node against a schema.
/// Returns diagnostics with precise source locations.
pub fn validate_cst(
    root: &SyntaxNode,
    schema: &Schema,
) -> Vec<Diagnostic>;
```

## Testing

- Schema parsing tests
- Type resolution tests
- Validation tests for each type
- Error message quality tests
- Schema discovery tests
- Integration tests with real-world schemas
