+++
title = "TOML"
weight = 4
insert_anchor_links = "heading"
+++

[`facet-toml`](https://docs.rs/facet-toml) provides TOML serialization and deserialization, built on [`toml_edit`](https://docs.rs/toml_edit). Perfect for configuration files.

## Basic Usage

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    name: String,
    version: String,
    debug: bool,
}

// Deserialize
let toml = r#"
name = "myapp"
version = "1.0.0"
debug = true
"#;
let config: Config = facet_toml::from_str(toml)?;

// Serialize
let output = facet_toml::to_string(&config)?;
```

## Nested Tables

TOML tables map to nested structs:

```rust
#[derive(Facet)]
struct Package {
    name: String,
    version: String,
}

#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
}

#[derive(Facet)]
struct Config {
    package: Package,
    server: Server,
}

let toml = r#"
[package]
name = "myapp"
version = "1.0.0"

[server]
host = "localhost"
port = 8080
"#;

let config: Config = facet_toml::from_str(toml)?;
```

## Arrays

TOML arrays work with `Vec<T>`:

```rust
#[derive(Facet)]
struct Dependency {
    name: String,
    version: String,
}

#[derive(Facet)]
struct Config {
    dependencies: Vec<Dependency>,
}

let toml = r#"
[[dependencies]]
name = "serde"
version = "1.0"

[[dependencies]]
name = "tokio"
version = "1.0"
"#;

let config: Config = facet_toml::from_str(toml)?;
assert_eq!(config.dependencies.len(), 2);
```

## Inline Tables

Inline tables in TOML are handled automatically:

```toml
# Both forms work:
server = { host = "localhost", port = 8080 }

# Or:
[server]
host = "localhost"
port = 8080
```

## Optional Fields

Use `Option<T>` for optional configuration:

```rust
#[derive(Facet)]
struct Database {
    host: String,
    port: Option<u16>,  // Optional in TOML
    #[facet(default = 10)]
    pool_size: u32,     // Defaults to 10 if missing
}
```

## DateTime Support

TOML has native datetime support. Enable the appropriate feature:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["chrono"] }
# or "time" or "jiff02"
```

```rust
use chrono::{DateTime, Utc};

#[derive(Facet)]
struct Event {
    name: String,
    timestamp: DateTime<Utc>,
}

let toml = r#"
name = "deploy"
timestamp = 2024-01-15T10:30:00Z
"#;

let event: Event = facet_toml::from_str(toml)?;
```

## Error Messages

facet-toml provides helpful error messages with source locations:

```
Error: missing field `port`
  ┌─ config.toml:2:1
  │
2 │ [server]
  │ ^^^^^^^^ expected field `port` here
  │
```

## Common Patterns

### Cargo.toml-style configs

```rust
#[derive(Facet)]
struct Manifest {
    package: Package,
    #[facet(default)]
    dependencies: HashMap<String, Dependency>,
}

#[derive(Facet)]
struct Package {
    name: String,
    version: String,
    #[facet(default)]
    authors: Vec<String>,
}

#[derive(Facet)]
#[facet(untagged)]
enum Dependency {
    Simple(String),  // "1.0"
    Detailed {
        version: String,
        #[facet(default)]
        features: Vec<String>,
    },
}
```

### Environment-specific configs

```rust
#[derive(Facet)]
struct Config {
    #[facet(default)]
    development: Option<ServerConfig>,
    #[facet(default)]
    production: Option<ServerConfig>,
}

#[derive(Facet)]
struct ServerConfig {
    host: String,
    port: u16,
    #[facet(default)]
    tls: bool,
}
```

## Serialization

Serialize to TOML:

```rust
let config = Config {
    package: Package {
        name: "myapp".into(),
        version: "1.0.0".into(),
    },
    server: Server {
        host: "localhost".into(),
        port: 8080,
    },
};

let toml = facet_toml::to_string(&config)?;
println!("{}", toml);
```

Output:

```toml
[package]
name = "myapp"
version = "1.0.0"

[server]
host = "localhost"
port = 8080
```

## Next Steps

- See [Ecosystem](@/guide/ecosystem.md) for third-party type support (chrono, time, etc.)
- Check [Attributes Reference](@/guide/attributes.md) for all available attributes
- Read [Error handling](@/guide/errors.md) for more on diagnostics
