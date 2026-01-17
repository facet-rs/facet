+++
title = "Schema Distribution"
weight = 10
insert_anchor_links = "heading"
+++

# Schema Distribution

Styx schemas can be published to [crates.io](https://crates.io) for versioned, immutable distribution. This page explains the system and its guarantees.

## The problem with URLs

Traditional schema URLs have several failure modes:

- **Domain expiration** — owners forget to renew, lose access
- **Hosting changes** — migrations break old URLs
- **No versioning** — same URL might serve different content over time
- **Network dependency** — CI and editors need internet access

## Schemas as crates

Styx schemas are published to crates.io as regular Rust crates. This gives you:

- **Immutability** — once published, a version cannot be modified (only yanked)
- **Versioning** — semver built in, with dependency resolution
- **Governance** — crates.io is maintained by the Rust Foundation
- **Longevity** — likely to exist for decades
- **Familiar tooling** — `cargo publish`, ownership transfer, teams

A schema crate is minimal — just a `Cargo.toml` and your schema file:

```
myapp-schema/
├── Cargo.toml
└── schema.styx
```

## Declaring a schema

Config files declare their schema with the `@schema` directive, pinning only the **major version**:

```styx
@schema crate:myapp@2

host localhost
port 8080
timeout 30s
```

This means "any 2.x version of the myapp schema." Tooling fetches the latest compatible version.

### Why major version only?

Semver promises that minor and patch versions are backwards compatible:

- **2.0.0** → initial release
- **2.1.0** → new optional field added (compatible)
- **2.2.0** → new scalar type supported (compatible)
- **3.0.0** → field removed or type narrowed (breaking)

Your config written for 2.0 keeps working through 2.1, 2.47, 2.193. It only breaks at 3.0, when the app itself has breaking changes and you'd need to update your config anyway.

## Semver enforcement

The `styx publish` command enforces semantic versioning by diffing schemas:

```bash
$ styx publish myapp-schema
error: breaking change detected

  removed field `timeout` from `Config`

  ┌─ schema.styx:4:3
  │
4 │   timeout @duration
  │   ^^^^^^^ this field existed in 1.3.0

This requires a major version bump.
Current version: 1.3.0
Minimum allowed: 2.0.0
```

### What counts as breaking?

**Breaking changes** (require major bump):
- Removing a field
- Renaming a field
- Changing a field's type to something incompatible
- Making an optional field required
- Narrowing allowed values (e.g., `@int` → `@int{ min 0 }`)

**Non-breaking changes** (minor bump):
- Adding a new optional field
- Adding a new type to a union
- Widening allowed values
- Adding new enum variants (if schema allows unknown variants)

**Patch changes**:
- Documentation updates
- Whitespace/formatting

## Schema resolution

The `@schema` directive tells tooling where to find the schema. The full form specifies both the crate identity and the CLI binary name:

```styx
@schema {source crate:myapp-config@2, cli myapp}
```

When the crate name matches the CLI name, use the short form:

```styx
@schema crate:myapp@2
```

For local development, you can use a file path:

```styx
@schema file:./schema.styx
```

### Resolution order

Given `@schema {source crate:myapp-config@2, cli myapp}`, tooling:

1. **Try CLI** — run `myapp @dump-styx-schema`, verify version is 2.x
2. **Fallback to crates.io** — fetch and cache `crate:myapp-config@2`

This means:
- Developers with the app installed get validation matching their version (works offline)
- CI can fetch the published schema without installing the app
- The crate identity is canonical; the CLI is just a fresher source

## CLI schema discovery

If your CLI tool supports the `@dump-styx-schema` argument, editors can discover your schema automatically:

```bash
$ myapp @dump-styx-schema
@meta {
  crate myapp-config
  version 2.3.1
  bin myapp
}

/// Server configuration.
Config @object {
  /// Hostname or IP address to bind to.
  host @default(localhost @string)

  /// Port number (1-65535).
  port @default(8080 @int{ min 1, max 65535 })

  /// Request timeout.
  timeout @default(30s @duration)

  /// TLS configuration (optional).
  tls @optional(TlsConfig)
}

/// TLS certificate configuration.
TlsConfig @object {
  /// Path to certificate file.
  cert @string
  /// Path to private key file.
  key @string
}
```

This is the best experience for users who have your tool installed:
- Works offline
- Always matches their installed version
- Zero configuration

### Implementing discovery

First, derive `Facet` on your configuration types. Doc comments become schema documentation, and attributes define constraints:

```rust
use std::path::PathBuf;
use std::time::Duration;
use facet::Facet;

#[derive(Facet)]
/// Server configuration.
struct Config {
    /// Hostname or IP address to bind to.
    host: String,

    /// Port number (1-65535).
    #[facet(min = 1, max = 65535)]
    port: u16,

    /// Request timeout.
    timeout: Duration,

    /// TLS configuration (optional).
    tls: Option<TlsConfig>,
}

#[derive(Facet)]
/// TLS certificate configuration.
struct TlsConfig {
    /// Path to certificate file.
    cert: PathBuf,
    /// Path to private key file.
    key: PathBuf,
}
```

Then handle the `@dump-styx-schema` argument in your main function:

```rust
use facet_styx::StyxSchema;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("@dump-styx-schema") {
        let schema = StyxSchema::builder()
            .crate_name("myapp-config")
            .version(env!("CARGO_PKG_VERSION"))
            .bin("myapp")
            .root::<Config>()
            .build();
        println!("{schema}");
        return;
    }

    // ... rest of your app
}
```

This outputs a complete schema with metadata:

```styx
@meta {
  crate myapp-config
  version 1.0.0
  bin myapp
}

/// Server configuration.
Config @object {
  /// Hostname or IP address to bind to.
  host @string

  /// Port number (1-65535).
  port @int{ min 1, max 65535 }

  /// Request timeout.
  timeout @duration

  /// TLS configuration (optional).
  tls @optional(TlsConfig)
}

/// TLS certificate configuration.
TlsConfig @object {
  /// Path to certificate file.
  cert @string
  /// Path to private key file.
  key @string
}
```

## Publishing workflow

### 1. Generate schema from types

```bash
$ cargo run --bin myapp -- @dump-styx-schema > myapp-schema/schema.styx
```

Or use `facet-styx` directly in your build.

### 2. Check for breaking changes

```bash
$ styx check myapp-schema/
Comparing against crate:myapp-schema@1.3.0...

Changes detected:
  + added field `retry_count` to `Config` (optional, non-breaking)

Suggested version: 1.4.0
```

### 3. Publish

```bash
$ cd myapp-schema
$ cargo publish
```

The schema is now available at `crate:myapp-schema@1`.

## Caching

The Styx LSP and CLI cache schemas aggressively:

- **Crates.io schemas** — cached indefinitely per version (they're immutable)
- **CLI schemas** — cached per binary mtime
- **Local files** — watched for changes

Cache location: `~/.cache/styx/schemas/`

## Migration from URLs

If you have existing configs with URL schemas:

```styx
// Old
@schema https://example.com/myapp/v1.schema.styx

// New
@schema crate:myapp@1
```

The URL form continues to work but is discouraged for new projects.
