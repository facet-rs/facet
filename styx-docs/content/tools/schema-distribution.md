+++
title = "Schema Distribution"
weight = 10
insert_anchor_links = "heading"
+++

The recommended way to distribute your schema:

1. **Reserve a crate name** on crates.io (e.g., `myapp-config`)
2. **Embed the schema** in your binary
3. **Provide an `init` command** that generates starter config

## 1. Reserve your crate

Create a minimal crate to reserve the name. You'll publish your schema here later:

```
myapp-config/
├── Cargo.toml
└── src/lib.rs   # empty for now
```

## 2. Embed the schema

Use `styx_embed` to bake your schema into the binary:

```rust
// build.rs
fn main() {
    facet_styx::GenerateSchema::<myapp::Config>::new()
        .crate_name("myapp-config")
        .version("1")
        .cli("myapp")
        .write("schema.styx");
}
```

```rust
// src/main.rs
styx_embed::embed_outdir_file!("schema.styx");
```

This lets tooling discover your schema by scanning the binary—no execution needed.

## 3. Provide an init command

Help users get started with a valid config file:

```rust
fn main() {
    if std::env::args().nth(1).as_deref() == Some("init") {
        print!(r#"@schema {{source crate:myapp-config@1, cli myapp}}

host localhost
port 8080
"#);
        return;
    }
    // ...
}
```

Usage:

```bash
$ myapp init > config.styx
```

The generated config declares its schema, so editors and the CLI can validate it immediately.

## How tooling resolves schemas

Given `@schema {source crate:myapp-config@1, cli myapp}`:

1. **Scan binary** — extract embedded schema from `myapp` (zero-execution, memory-mapped)
2. **Fetch crate** — download from crates.io (future, when you publish)

The binary is located via `PATH` and scanned for the magic `STYX_SCHEMAS_V1` marker. No code is executed—this is safe even with untrusted binaries.

Users get instant validation. You get a path to versioned distribution later.

## Schema cache

Tooling caches resolved schemas on disk for editor integration (go-to-definition, hover links) and offline access.

### Cache location

| Platform | Path |
|----------|------|
| Linux, macOS | `$XDG_CACHE_HOME/styx/schemas/` (default: `~/.cache/styx/schemas/`) |
| Windows | `%LOCALAPPDATA%\styx\cache\schemas\` |

### Cache structure

```
~/.cache/styx/schemas/
├── embedded/
│   └── <cli-name>/
│       └── <content-hash>.styx    # extracted from binary
├── crates/
│   └── <crate-name>/
│       └── <full-version>/
│           └── schema.styx        # fetched from crates.io (e.g., 1.2.3/)
└── resolve.json                   # maps major versions to resolved full versions
```

### Embedded schema caching

When a schema is extracted from a binary:

1. Compute content hash (SHA-256, truncated to 16 hex chars)
2. Write to `embedded/<cli-name>/<hash>.styx`
3. Return the cache path for editor features

The content hash ensures the cache stays fresh when the binary is rebuilt. Old entries can be garbage-collected periodically.

### Crate schema caching

When a schema reference like `crate:myapp-config@1` is resolved:

1. Query crates.io for the latest version matching `1.x.x` (e.g., `1.2.3`)
2. Record the resolution in `resolve.json`
3. Download the crate tarball for `1.2.3`
4. Extract `schema.styx` from the crate root
5. Write to `crates/myapp-config/1.2.3/schema.styx`

The `resolve.json` file maps version constraints to resolved versions:

```json
{
  "myapp-config": {
    "1": { "resolved": "1.2.3", "timestamp": "2025-01-19T12:00:00Z" }
  }
}
```

Individual schema files are immutable once cached (a published crate version never changes). The resolution mapping may be refreshed to pick up newer compatible versions.

### Cache invalidation

- **Embedded schemas**: Invalidated by content hash mismatch. Stale entries (not accessed in 30 days) may be pruned.
- **Crate schemas**: Never invalidated (immutable). The resolution mapping may be refreshed to pick up newer compatible versions.

Tooling should handle missing cache gracefully by re-extracting or re-fetching.

### CLI commands

**`styx cache`** — Show cache configuration and statistics:

```bash
$ styx cache
Cache directory: /home/user/.cache/styx/schemas/
Embedded schemas: 3 (12 KB)
Crate schemas: 5 (48 KB)
```

**`styx cache --open`** — Open the cache directory in the system file explorer:

```bash
$ styx cache --open
# Opens ~/.cache/styx/schemas/ in Finder/Nautilus/Explorer
```

**`styx cache --clear`** — Remove all cached schemas:

```bash
$ styx cache --clear
Cleared 8 cached schemas (60 KB)
```
