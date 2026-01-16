+++
title = "Getting Started"
weight = 0
insert_anchor_links = "heading"
+++

Start from a fresh binary crate, add facet, and ship your first JSON round-trip with helpful errors.

## Prerequisites
- Rust stable (latest recommended)
- `cargo` available on your PATH

## Create a project

```bash
cargo new facet-hello
cd facet-hello
```

## Add dependencies

Pick at least one format crate. JSON is a good default:

```toml
# Cargo.toml
[dependencies]
facet = "{{ data.versions.facet }}"
facet-json = "{{ data.versions.facet }}"
```

If you also need YAML/TOML/etc., add `facet-yaml`, `facet-toml`, `facet-msgpack`, etc.

### Optional feature flags
- Time/UUID: enable the matching features on the format crate (check the crate docs).
- `doc`: include doc comments in generated shapes (needed for CLI help text with facet-args). To strip docs in release builds while keeping them in debug, add `--cfg facet_no_doc` to your release rustflags.
- `no_std`: use `facet-core` with `alloc`; most format crates require `std`.

## Derive `Facet` on your types

```rust,noexec
// src/main.rs
use facet::Facet;
use facet_json::{from_str, to_string};

#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    #[facet(default = "info".to_string())]
    level: String,
}

fn main() {
    let json = r#"{ "name": "app", "port": 8080 }"#;

    // Deserialize
    let cfg: Config = from_str(json).unwrap();

    // Serialize back out
    let out = to_string(&cfg);
    println!("Round trip: {out}");
}
```

Run it:

```bash
cargo run
```

## Common tweaks
- **Require strict inputs:** add `#[facet(deny_unknown_fields)]` to your structs.
- **Hide secrets:** mark sensitive fields `#[facet(sensitive)]`; tools like `facet-pretty` will redact.
- **Provide defaults:** `#[facet(default)]` or `#[facet(default = some_fn())]`.
- **Rename fields:** `#[facet(rename = "serverPort")]` or `#[facet(rename_all = "camelCase")]`.

## Next steps
- Browse the [Attributes Reference](@/reference/attributes/_index.md) for all available attributes.
- Check the [Format Support Matrix](@/reference/format-crate-matrix/_index.md) if you use multiple formats.
- Read the [FAQ](@/guide/faq.md) for common questions.
