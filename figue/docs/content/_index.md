+++
title = "figue"
description = "Type-safe CLI arguments, config files, and environment variables — powered by Facet reflection."
insert_anchor_links = "heading"
+++

**figue** (pronounced 'fig', like the fruit) parses configuration from CLI
arguments, environment variables, and config files — a bit like
[figment](https://docs.rs/figment), but built on facet reflection.

```rust
use facet::Facet;
use figue as args;

#[derive(Facet)]
struct Args {
    #[facet(args::positional)]
    path: String,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(args::named, args::short = 'j')]
    concurrency: usize,
}

let args: Args = figue::from_slice(&["--verbose", "-j", "14", "example.rs"])?;
```

The entry point is `builder` — let yourself be guided from there.

## Color

Color is enabled by default when the terminal supports it, and disabled when the
[`NO_COLOR`](https://no-color.org) environment variable is set.

[Source on GitHub](https://github.com/bearcove/figue) · [docs.rs](https://docs.rs/figue)
