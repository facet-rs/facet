+++
title = "Appendix"
weight = 6
slug = "appendix"
insert_anchor_links = "heading"
+++

## Usage patterns (non-normative)

### Dynamic access

Parse into a generic document tree and interpret values on demand:

```rust
let doc: styx::Document = styx::parse(r#"
    server {
        host localhost
        port 8080
        timeout 30s
    }
"#)?;

// Caller decides how to interpret each scalar
let host = doc["server"]["host"].as_str()?;
let port = doc["server"]["port"].as_u16()?;
let timeout = doc["server"]["timeout"].as_duration()?;
```

This approach is useful for:
- Tools that process arbitrary STYX documents
- Exploratory parsing where the schema is unknown
- Gradual migration from other formats

### Typed deserialization

Deserialize directly into concrete types. The type system guides scalar interpretation:

```rust
use std::time::Duration;

#[derive(Facet)]
struct Config {
    server: Server,
}

#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
    timeout: Duration,
}

let config: Config = styx::from_str(r#"
    server {
        host localhost
        port 8080
        timeout 30s
    }
"#)?;

assert_eq!(config.server.port, 8080);
assert_eq!(config.server.timeout, Duration::from_secs(30));
```

### Enum deserialization

Enums use tag syntax. The tag names the variant; the payload follows.

```rust
#[derive(Facet)]
enum Status {
    Ok,
    Pending,
    Err { message: String, code: Option<i32> },
}

#[derive(Facet)]
struct Response {
    status: Status,
}

// Unit variant
let r: Response = styx::from_str("status @ok")?;

// Variant with payload
let r: Response = styx::from_str(r#"
    status @err{
        message "connection timeout"
        code 504
    }
"#)?;

// Using attribute syntax for payload
let r: Response = styx::from_str(r#"
    status @err{message="timeout" code=504}
"#)?;
```

## Design invariants (non-normative)

STYX enforces the following invariants:

- **No implicit merges**: Objects are never merged. Each key appears exactly once.
- **No reopening**: Once an object is closed, it cannot be extended with additional keys.
- **No indentation-based structure**: All structure is explicit via `{}` and `()`.
- **No semantic interpretation during parsing**: The parser produces opaque scalars; meaning is assigned during deserialization.
- **All structure is explicit**: Braces and parentheses define nesting, not whitespace or conventions.
- **Commas in objects only**: Commas are optional separators in objects (interchangeable with newlines). Sequences use whitespace only.
- **Explicit unit value**: `@` is the unit value, distinct from `()` (empty sequence). Keys without values implicitly produce `@`. This enables concise enum variants (`@ok`) and flag-like entries (`enabled`).

## Type-to-schema mapping (non-normative)

STYX schemas are typically generated from strongly-typed languages like Rust. This section
shows how Rust types with Facet derive map to STYX schemas.

### Primitives

| Rust Type | STYX Schema |
|-----------|-------------|
| `String`, `&str` | `@string` |
| `bool` | `@boolean` |
| `u8` | `@u8` |
| `u16` | `@u16` |
| `u32` | `@u32` |
| `u64` | `@u64` |
| `u128` | `@u128` |
| `usize` | `@usize` |
| `i8` | `@i8` |
| `i16` | `@i16` |
| `i32` | `@i32` |
| `i64` | `@i64` |
| `i128` | `@i128` |
| `isize` | `@isize` |
| `f32` | `@f32` |
| `f64` | `@f64` |
| `Duration` | `@duration` |
| `()` | `@unit` |

### Optional fields

`Option<T>` maps to an optional field with `?` suffix:

```rust
#[derive(Facet)]
struct Server {
    host: String,           // required
    port: u16,              // required
    timeout: Option<Duration>, // optional
}
```

```styx
Server {
  host @string
  port @u16
  timeout? @duration
}
```

### Sequences

`Vec<T>` and other sequence types map to `(@T)`:

```rust
#[derive(Facet)]
struct Config {
    hosts: Vec<String>,
    ports: Vec<u16>,
}
```

```styx
Config {
  hosts (@string)
  ports (@u16)
}
```

### Maps

`HashMap<K, V>` maps to `@map(@K @V)`:

```rust
#[derive(Facet)]
struct Config {
    env: HashMap<String, String>,
    ports: HashMap<String, u16>,
}
```

```styx
Config {
  env @map(@string)           // shorthand for @map(@string @string)
  ports @map(@string @u16)
}
```

### Structs

Structs map to object schemas:

```rust
#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
    tls: TlsConfig,
}

#[derive(Facet)]
struct TlsConfig {
    cert: String,
    key: String,
    enabled: Option<bool>,
}
```

```styx
Server {
  host @string
  port @u16
  tls @TlsConfig
}

TlsConfig {
  cert @string
  key @string
  enabled? @boolean
}
```

### Enums

Enums map to `@enum{ ... }`:

```rust
#[derive(Facet)]
enum Status {
    Ok,
    Pending,
    Err { message: String, code: Option<i32> },
}
```

```styx
Status @enum{
  ok
  pending
  err { message @string, code? @i32 }
}
```

Note: Rust enum variants are PascalCase by convention, but STYX uses lowercase
for enum variant names. Schema generators should apply case conversion.

### Flatten

The `#[facet(flatten)]` attribute maps to `@flatten`:

```rust
#[derive(Facet)]
struct User {
    name: String,
    email: String,
}

#[derive(Facet)]
struct Admin {
    #[facet(flatten)]
    user: User,
    permissions: Vec<String>,
}
```

```styx
User {
  name @string
  email @string
}

Admin {
  user @flatten(@User)
  permissions (@string)
}
```

## Comparison with JSON (non-normative)

JSON is the lingua franca of data interchange. STYX is designed for human authoring,
not machine interchange, which leads to different trade-offs.

### What STYX removes

| JSON | STYX | Rationale |
|------|------|-----------|
| Mandatory quotes on keys | Bare keys | `name alice` vs `"name": "alice"` — less noise |
| Colons between key/value | Whitespace | `host localhost` vs `"host": "localhost"` |
| Commas between elements | Newlines or commas | Trailing comma errors eliminated |
| `null` | `@` (unit) | Structural absence, not a value |
| No comments | `//` comments | Configuration needs explanation |

### What STYX adds

| Feature | JSON equivalent | Example |
|---------|-----------------|---------|
| Heredocs | Escaped strings | `<<EOF` multiline content `EOF` |
| Raw strings | Escaped strings | `r#"no \"escaping\" needed"#` |
| Tagged values | Convention-based | `rgb(255 0 0)` vs `{"$type": "rgb", ...}` |
| Dotted paths | Nested objects | `server.host localhost` |
| Attribute syntax | Verbose objects | `labels app=web tier=frontend` |
| Duration literals | Strings + parsing | `30s` vs `"30s"` |
| Schemas | JSON Schema (separate) | Inline `@schema { ... }` |

### Data model differences

JSON has seven types: object, array, string, number, boolean, null.
STYX has six value types: object, sequence, scalar, tagged object, tagged sequence, unit.

Key differences:

- **Scalars are opaque**: STYX doesn't distinguish strings from numbers at parse time.
  `42` and `"42"` both produce scalars containing the text `42`. The deserializer
  interprets based on target type.

- **Tagged values are first-class**: `rgb(255 0 0)` is a tagged sequence, not an
  object with magic keys. This enables cleaner enum representation.

- **Unit vs null**: JSON's `null` is a value. STYX's `@` represents structural absence.
  Keys without values implicitly get `@`: `enabled` means `enabled @`.

### Example: Complex configuration

```json
{
  "server": {
    "host": "localhost",
    "port": 8080,
    "tls": {
      "enabled": true,
      "cert": "/path/to/cert.pem",
      "key": "/path/to/key.pem"
    }
  },
  "database": {
    "url": "postgres://localhost/mydb",
    "pool": {
      "min": 5,
      "max": 20,
      "timeout": "30s"
    }
  },
  "features": ["auth", "logging", "metrics"]
}
```

```styx
server {
  host localhost
  port 8080
  tls {
    enabled true
    cert /path/to/cert.pem
    key /path/to/key.pem
  }
}

database {
  url postgres://localhost/mydb
  pool {
    min 5
    max 20
    timeout 30s
  }
}

features (auth logging metrics)
```

The STYX version is 40% shorter and easier to scan.

## Comparison with YAML (non-normative)

YAML is the most common human-authored configuration format. STYX addresses several
YAML pain points while preserving readability.

### The Norway problem

YAML's implicit typing causes famous bugs:

```yaml
countries:
  - GB    # string "GB"
  - NO    # boolean false (!)
  - IE    # string "IE"
```

STYX scalars are opaque — `NO` is always the text `NO`. The deserializer interprets
it based on target type. If you're deserializing into `Vec<String>`, you get `"NO"`.
If you're deserializing into `Vec<bool>`, you get an error (not a silent conversion).

### Indentation-based structure

YAML uses indentation to define structure:

```yaml
server:
  host: localhost
  port: 8080
    extra: oops  # is this a child of port or server?
```

STYX uses explicit delimiters:

```styx
server {
  host localhost
  port 8080
  extra oops    // clearly a child of server
}
```

Mixed tabs and spaces, invisible trailing whitespace, and copy-paste errors that
break indentation are not possible in STYX.

### Multi-document streams

YAML supports multiple documents in one file with `---` separators.
STYX does not — each file is one document. For multiple configurations,
use multiple files or a sequence at the root.

### Anchors and aliases

YAML supports references:

```yaml
defaults: &defaults
  timeout: 30s
  retries: 3

production:
  <<: *defaults
  timeout: 60s
```

STYX does not support references. Use your application's configuration
merging logic, or schema-level defaults. This keeps the format simple and
prevents circular reference bugs.

### Example: Kubernetes-style config

```yaml
apiVersion: v1
kind: Service
metadata:
  name: my-service
  labels:
    app: web
    tier: frontend
spec:
  ports:
    - port: 80
      targetPort: 8080
  selector:
    app: web
```

```styx
apiVersion v1
kind Service

metadata {
  name my-service
  labels app=web tier=frontend
}

spec {
  ports ({
    port 80
    targetPort 8080
  })
  selector app=web
}
```

### What YAML has that STYX doesn't

| YAML feature | STYX alternative |
|--------------|------------------|
| Anchors/aliases | Application-level merging |
| Multi-document | Multiple files or root sequence |
| Flow/block choice for all types | Sequences always use `()`, objects use `{}` |
| Implicit typing | Explicit types via schema |
| Custom tags (`!ruby/object`) | Tagged values with schema interpretation |

### What STYX has that YAML doesn't

| STYX feature | YAML workaround |
|--------------|-----------------|
| Heredocs with indent control | Literal blocks (`|`) with fixed behavior |
| Raw strings | Quoted strings with escaping |
| Attribute syntax | Verbose nested objects |
| Inline schemas | External JSON Schema |
| Tagged sequences/objects | Custom tags (implementation-dependent) |

## Comparison with KDL (non-normative)

KDL (Kdl Document Language) is a modern document format with similar goals to STYX.
Both reject YAML's indentation-sensitivity and JSON's verbosity.

### Structural differences

KDL uses a node-based model where each line is a node with optional arguments and properties:

```kdl
server {
    host "localhost"
    port 8080
}
```

STYX uses a key-value model:

```styx
server {
  host localhost
  port 8080
}
```

The difference is subtle but significant for complex structures.

### Arguments vs values

KDL nodes can have positional arguments:

```kdl
person "Alice" age=30
```

In STYX, the equivalent uses explicit keys or tagged values:

```styx
person { name Alice, age 30 }
// or with a tagged sequence for positional data:
person("Alice" 30)
```

### Properties syntax

KDL uses `key=value` for properties on a node. STYX uses `key=value` for attribute
objects, which are syntactic sugar for block objects:

```kdl
// KDL: properties on a node
node key=value other="thing"
```

```styx
// STYX: attribute object (sugar for nested object)
node key=value other=thing
// expands to:
node {
  key value
  other thing
}
```

### Type annotations

KDL uses parenthesized type annotations:

```kdl
port (u16)8080
timeout (duration)"30s"
```

STYX uses schema-defined types, not inline annotations:

```styx
// Schema
port @u16
timeout @duration

// Document
port 8080
timeout 30s
```

This keeps documents clean and centralizes type information.

### Null/empty handling

KDL uses `null` as a keyword. STYX uses `@` for unit:

```kdl
optional null
```

```styx
optional @
```

KDL's `null` is a value; STYX's `@` represents structural absence.

### Example: Package manifest

```kdl
package {
    name "my-app"
    version "1.0.0"

    dependencies {
        serde "1.0" features=["derive"]
        tokio "1.0" features=["full"] optional=true
    }
}
```

```styx
package {
  name my-app
  version 1.0.0

  dependencies {
    serde {
      version 1.0
      features (derive)
    }
    tokio {
      version 1.0
      features (full)
      optional true
    }
  }
}
```

STYX is slightly more verbose here because it doesn't have positional arguments,
but the structure is more uniform and easier to query programmatically.

### Key differences summary

| Aspect | KDL | STYX |
|--------|-----|------|
| Model | Node with args + props | Key-value pairs |
| Positional data | Node arguments | Tagged sequences |
| Type annotations | Inline `(type)` | Schema-defined |
| Strings | Always quoted | Bare or quoted |
| Empty/null | `null` keyword | `@` unit value |
| Comments | `//` and `/* */` | `//` only |
| Multiline strings | `"...\n..."` | Heredocs `<<EOF` |

## Comparison with TOML (non-normative)

TOML is popular for Rust project configuration (Cargo.toml). STYX offers
different trade-offs for deeply nested structures.

### Flat vs nested

TOML uses section headers that implicitly define nesting:

```toml
[package]
name = "my-app"
version = "1.0.0"

[dependencies]
serde = "1.0"

[dependencies.tokio]
version = "1.0"
features = ["full"]
```

STYX uses explicit nesting:

```styx
package {
  name my-app
  version 1.0.0
}

dependencies {
  serde 1.0
  tokio {
    version 1.0
    features (full)
  }
}
```

### Arrays of tables

TOML's `[[array]]` syntax:

```toml
[[servers]]
host = "alpha"
port = 8080

[[servers]]
host = "beta"
port = 8081
```

STYX uses sequences:

```styx
servers (
  { host alpha, port 8080 }
  { host beta, port 8081 }
)
```

### Inline tables

TOML inline tables cannot span lines and cannot have trailing commas:

```toml
point = { x = 1, y = 2 }  # must be on one line
```

STYX inline objects can span lines and allow trailing commas:

```styx
point { x 1, y 2, }   // trailing comma OK
point {
  x 1,
  y 2,              // multiline OK
}
```

### What TOML has that STYX doesn't

| TOML feature | STYX alternative |
|--------------|------------------|
| `[[array]]` headers | Sequence of objects |
| Datetime literals | `@timestamp` with schema |
| Reopening sections | Use block objects |
| Bare integers/floats | Opaque scalars + schema |

### What STYX has that TOML doesn't

| STYX feature | TOML workaround |
|--------------|-----------------|
| Tagged values | Magic key conventions |
| Heredocs | Multi-line basic strings |
| Raw strings | Literal strings `'...'` |
| Inline schemas | External validation |
| Deeply nested inline | Awkward table headers |
