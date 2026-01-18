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
    facet_styx::generate_schema::<myapp::Config>("schema.styx");
}
```

```rust
// src/main.rs
styx_embed::embed_file!(concat!(env!("OUT_DIR"), "/schema.styx"));
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
