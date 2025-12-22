---
name: use-facet-crates
description: Guidelines for using facet crates (facet-json, facet-toml, facet-args) instead of serde-based alternatives for consistent dogfooding
---

# Use Facet Crates Instead of Serde Ecosystem

When writing code in this workspace, prefer facet-based crates over serde-based ones. This project is building facet as a replacement for serde, so we should dogfood our own libraries.

## Crate Replacements

| Instead of         | Use                | Notes                                    |
|--------------------|--------------------|-----------------------------------------|
| `serde`            | `facet`            | Core derive and traits                  |
| `serde_json`       | `facet-json`       | JSON serialization/deserialization      |
| `toml`             | `facet-toml`       | TOML parsing (or consider `facet-kdl`)  |
| `serde_yaml`       | `facet-yaml`       | YAML support                            |
| `clap`             | `facet-args`       | CLI argument parsing                    |
| `serde_derive`     | `facet` (derive)   | `#[derive(Facet)]` replaces Serialize/Deserialize |

## When to Use Which

### Use facet-json for:
- New code in this workspace
- Internal tools (like benchmark-analyzer)
- Anything that doesn't need serde compatibility

### serde_json is acceptable for:
- Interop with external crates that require serde
- Benchmarks comparing facet vs serde performance
- Code that specifically tests serde compatibility

## Quick Example

```rust
// OLD (serde)
use serde::{Serialize, Deserialize};
use serde_json;

#[derive(Serialize, Deserialize)]
struct Config {
    name: String,
}

let config: Config = serde_json::from_str(json)?;

// NEW (facet)
use facet::Facet;
use facet_json as json;

#[derive(Facet)]
struct Config {
    name: String,
}

let config: Config = json::from_str(json)?;
```

## Checking Dependencies

When adding new dependencies or reviewing code, check Cargo.toml for serde ecosystem crates and consider if facet alternatives exist.

## TODO for This Workspace

The benchmark-analyzer currently uses `serde_json` for JSON serialization in chart data. This should be migrated to `facet-json` for consistency (eating our own dogfood).

Location: `tools/benchmark-analyzer/src/report.rs` - uses `serde_json::to_string()` for chart labels/data.
