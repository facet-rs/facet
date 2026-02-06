# figue

[![crates.io](https://img.shields.io/crates/v/figue.svg)](https://crates.io/crates/figue)
[![documentation](https://docs.rs/figue/badge.svg)](https://docs.rs/figue)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/figue.svg)](../../LICENSE-MIT)

figue (pronounced 'fig', like the fruit) provides configuration parsing from
CLI arguments, environment variables, and config files, a bit like
[figment](https://docs.rs/figment/latest/figment/) but based on facet
reflection:

```rust
use facet_pretty::FacetPretty;
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

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let args: Args = figue::from_slice(&["--verbose", "-j", "14", "example.rs"])?;
eprintln!("args: {}", args.pretty());
Ok(())
# }
```

The entry point of figue is [`builder`] — let yourself be guided from there.

## Color

Color is enabled by default if the terminal supports it. It is disabled when the
[`NO_COLOR`](https://no-color.org) environment variable is set.

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/bearcove/figue/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/figue/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/figue/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
