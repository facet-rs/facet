+++
title = "facet-cargo-toml"
weight = 25
insert_anchor_links = "heading"
+++

`facet-cargo-toml` provides typed Rust models for `Cargo.toml` manifests and
`Cargo.lock` files, parsed through Facet-powered TOML support.

It is useful when tools need to inspect Cargo workspaces without treating
manifests and lockfiles as unstructured TOML tables.

## Example

```rust
use facet_cargo_toml::{CargoLock, CargoToml};

let manifest = CargoToml::from_path("Cargo.toml")?;
let lockfile = CargoLock::from_path("Cargo.lock")?;

if let Some(package) = manifest.package {
    if let Some(name) = package.name {
        println!("package: {}", name.value);
    }
}

println!("locked packages: {}", lockfile.packages.len());
# Ok::<_, facet_cargo_toml::Error>(())
```

## Source

`facet-cargo-toml` now lives in the Facet monorepo:

- [`facet-cargo-toml`](https://github.com/facet-rs/facet/tree/main/facet-cargo-toml)
