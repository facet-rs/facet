+++
title = "Extension Attributes"
+++

Extension attributes allow third-party crates to attach custom metadata to types, fields, variants, and more. This enables crates like `facet-kdl`, `facet-args`, or your own custom crate to define domain-specific attributes without conflicting with facet's built-in attributes.

## Using Extension Attributes

Extension attributes use a namespaced syntax:

```rust
#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    server: Server,

    #[facet(kdl::property)]
    name: String,
}
```

The namespace (`kdl`) is typically the name you import the crate as:

```rust
use facet_kdl as kdl;  // Now kdl:: attributes work
```

### Available Namespaces

Different crates provide different namespaces:

| Crate | Namespace | Example |
|-------|-----------|---------|
| `facet-kdl` | `kdl` | `#[facet(kdl::child)]` |
| `facet-args` | `args` | `#[facet(args::short = 'v')]` |

### Attribute Arguments

Extension attributes can have arguments:

```rust
// Simple marker (no arguments)
#[facet(kdl::child)]

// With parenthesized arguments
#[facet(orm::index(unique, name = "idx_email"))]

// With equals-style arguments
#[facet(args::short = 'v')]
```

## KDL Extension Attributes

The `facet-kdl` crate provides these attributes for KDL serialization/deserialization:

### `kdl::argument`

Marks a field as a KDL positional argument (the unnamed values after a node name):

```rust
#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]
    name: String,  // Comes from: server "my-server" ...
    host: String,
    port: u16,
}
```

KDL input:
```kdl
server "my-server" host="localhost" port=8080
```

### `kdl::arguments`

Marks a `Vec` field to collect all positional arguments:

```rust
#[derive(Facet)]
struct Matrix {
    #[facet(kdl::arguments)]
    data: Vec<i32>,  // Collects all positional args
}
```

KDL input:
```kdl
matrix 1 2 3 4 5
```

### `kdl::property`

Marks a field as a KDL property (key=value pairs):

```rust
#[derive(Facet)]
struct Server {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}
```

KDL input:
```kdl
server host="localhost" port=8080
```

**Note:** Fields without any KDL attribute default to being properties.

### `kdl::child`

Marks a field as a KDL child node:

```rust
#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    database: Database,
}

#[derive(Facet)]
struct Database {
    host: String,
    port: u16,
}
```

KDL input:
```kdl
database host="localhost" port=5432
```

### `kdl::children`

Marks a `Vec` field to collect repeated child nodes:

```rust
#[derive(Facet)]
struct Config {
    #[facet(kdl::children)]
    servers: Vec<Server>,
}
```

KDL input:
```kdl
server host="web1" port=8080
server host="web2" port=8081
```

### `kdl::node_name`

Captures the KDL node name into a field:

```rust
#[derive(Facet)]
struct Dependency {
    #[facet(kdl::node_name)]
    name: String,
    version: String,
}
```

KDL input:
```kdl
serde version="1.0"
tokio version="1.28"
```

Here `name` will be `"serde"` or `"tokio"`.

## Creating Extension Attributes (For Crate Authors)

If you're building a crate that needs custom attributes, here's how to define and query them.

### Querying Extension Attributes at Runtime

Extension attributes are stored in the type's shape and can be queried at runtime:

```rust
use facet_core::{Field, FieldAttribute};

fn process_field(field: &Field) {
    // Check if an attribute exists
    if field.has_extension_attr("kdl", "child") {
        // This field is a KDL child node
    }

    // Get the attribute with its arguments
    if let Some(ext) = field.get_extension_attr("kdl", "property") {
        // Access raw token arguments
        let args = ext.args;  // &[Token]

        // Or parse into structured form
        let parsed = ext.parse_args().unwrap();
        for (name, value) in parsed.named {
            // Handle named arguments like `rename = "foo"`
        }
    }
}
```

### The ExtensionAttr Structure

```rust
pub struct ExtensionAttr {
    /// The namespace (e.g., "kdl" in `#[facet(kdl::child)]`)
    pub ns: &'static str,

    /// The key (e.g., "child" in `#[facet(kdl::child)]`)
    pub key: &'static str,

    /// Raw token arguments from the attribute
    pub args: &'static [Token],

    /// Function to get typed data (for advanced use)
    pub get: fn() -> &'static (dyn Any + Send + Sync),
}
```

### Parsing Arguments

For attributes with arguments like `#[facet(orm::index(unique, name = "idx"))]`:

```rust
let ext = field.get_extension_attr("orm", "index").unwrap();
let args = ext.parse_args().unwrap();

// Positional arguments (like `unique`)
for arg in &args.positional {
    match arg {
        ParsedValue::Ident(name) => println!("Flag: {}", name),
        ParsedValue::Literal(lit) => println!("Value: {}", lit),
    }
}

// Named arguments (like `name = "idx"`)
for (key, value) in &args.named {
    println!("{} = {:?}", key, value);
}
```

### Where Extension Attributes Can Appear

Extension attributes can be attached to:

- **Shapes** (structs, enums): via `ShapeAttribute::Extension`
- **Fields**: via `FieldAttribute::Extension`
- **Variants**: via `VariantAttribute::Extension`

Example checking shape-level attributes:

```rust
use facet_core::{Shape, ShapeAttribute};

fn check_shape(shape: &Shape) {
    for attr in shape.attributes {
        if let ShapeAttribute::Extension(ext) = attr {
            if ext.ns == "orm" && ext.key == "table" {
                // This type has #[facet(orm::table)]
            }
        }
    }
}
```

## Current Limitations

Extension attributes are validated at **runtime**, not compile time. This means:

- A typo like `#[facet(kdl::chld)]` (instead of `child`) will compile successfully
- The error only appears at runtime when the attribute is queried and not found
- Error messages may not clearly indicate the typo

We're working on compile-time validation with helpful error messages.
