+++
title = "figue"
weight = 34
insert_anchor_links = "heading"
+++

`figue` builds typed configuration from CLI arguments, environment variables,
config files, and code defaults.

It uses `Facet` metadata to derive the shape of your configuration type, then
applies layers in a predictable order: CLI arguments override environment
variables, environment variables override config files, and config files
override defaults.

```rust
use facet::Facet;
use figue as args;

#[derive(Facet)]
struct Config {
    #[facet(args::positional)]
    path: String,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,
}

let config: Config = figue::from_slice(&["--verbose", "config.json"])?;
```

Source: [`figue`](https://github.com/facet-rs/facet/tree/main/figue)
