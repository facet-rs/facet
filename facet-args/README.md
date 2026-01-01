# facet-args

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-args/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-args.svg)](https://crates.io/crates/facet-args)
[![documentation](https://docs.rs/facet-args/badge.svg)](https://docs.rs/facet-args)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-args.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Provides CLI argument parsing (WIP).

```rust
use facet_pretty::FacetPretty;
use facet::Facet;
use facet_args as args;

#[derive(Facet)]
struct Args {
    #[facet(args::positional)]
    path: String,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(args::named, args::short = 'j')]
    concurrency: usize,
}

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let args: Args = facet_args::from_slice(&["--verbose", "-j", "14", "example.rs"])?;
eprintln!("args: {}", args.pretty());
Ok(())
# }
```

## Behavior

The behavior of facet-args is still in flux, but here are the broad strokes:

  * We're always parsing to a struct (not an enum, vec etc.)
  * The struct we're parsing to is always owned — no borrowing happening here, it
    gets too complicated with `&'slice [&'text str]`
  * Arguments are either `positional` or `named` — fields lacking either annotation are ignored
  * Accepted syntaxes for short flags are: `args::short = 'v'` and `args::short = "v"` (where v can be any letter)
  * `positional` args of type `Vec` (or anything that has a `Def::List`) will soak up all the positional
    arguments — if followed by `positional` arguments of type `String` for example, those will never
    get filled
  * After parsing every available argument, uninitialized struct fields are filled with their default value
    if they have `facet(default)` set: this includes `Vec`.

## LLM contribution policy

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
