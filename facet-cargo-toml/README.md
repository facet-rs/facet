# facet-cargo-toml

Typed Cargo.toml and Cargo.lock parser using [facet](https://github.com/facet-rs/facet).

## Features

- **Complete Cargo.toml parsing**: All manifest fields supported with proper types
- **Cargo.lock parsing**: Full lockfile support with dependency resolution
- **Type-safe**: Uses facet's derive macros for automatic parsing and validation
- **Well-tested**: Validated against hundreds of real-world Cargo.toml files

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
facet-cargo-toml = "0.1"
```

### Parse a Cargo.toml

```rust
use facet_cargo_toml::CargoManifest;

let manifest = CargoManifest::from_path("Cargo.toml")?;

if let Some(package) = &manifest.package {
    println!("Package name: {:?}", package.name);
    println!("Version: {:?}", package.version);
}

for (name, dep) in manifest.dependencies.unwrap_or_default() {
    println!("Dependency: {name} = {dep:?}");
}
```

### Parse a Cargo.lock

```rust
use facet_cargo_toml::Lockfile;

let lockfile = Lockfile::from_path("Cargo.lock")?;

for package in &lockfile.package {
    println!("Locked package: {} {}", package.name, package.version);
}
```

## API Overview

### `CargoManifest`

Complete typed representation of Cargo.toml including:
- Package metadata
- Dependencies (regular, dev, build, target-specific)
- Workspace configuration
- Build targets (lib, bin, test, bench, example)
- Features
- Profiles
- Lints
- Patches

### `Lockfile`

Typed representation of Cargo.lock including:
- Package list with versions and checksums
- Dependency resolution
- Source information (registry, git, path)

## Why facet?

This crate uses the [facet](https://github.com/facet-rs/facet) serialization framework which provides:
- **Derive macros**: Automatic parsing from TOML with `#[derive(Facet)]`
- **Better error messages**: Uses miette for beautiful parse error reporting
- **Span information**: Track the source location of every parsed value
- **Type safety**: Strong typing throughout with enums for variants

## License

Licensed under either of:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
