# facet-perf-shootout

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-perf-shootout/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-perf-shootout.svg)](https://crates.io/crates/facet-perf-shootout)
[![documentation](https://docs.rs/facet-perf-shootout/badge.svg)](https://docs.rs/facet-perf-shootout)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-perf-shootout.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-perf-shootout

Performance benchmarks comparing facet format implementations against their reference counterparts.

## Formats

- **JSON**: `facet-json` vs `serde_json`
- **Postcard**: `facet-postcard` vs `postcard`
- More formats coming soon...

## Running Benchmarks

```bash
# Run all benchmarks with JIT
cargo bench -p facet-perf-shootout --features jit

# Run specific format
cargo bench -p facet-perf-shootout --features jit -- json::
cargo bench -p facet-perf-shootout --features jit -- postcard::

# Run specific benchmark
cargo bench -p facet-perf-shootout --features jit -- json::simple_struct
```

## Benchmark Definitions

Benchmarks are defined in YAML files under `benches/`:
- `json.yaml` - JSON format benchmarks
- `postcard.yaml` - Postcard format benchmarks

To regenerate benchmark code after editing YAML files:
```bash
cargo xtask gen-benchmarks
```

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
