# facet-miette

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-miette/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-miette.svg)](https://crates.io/crates/facet-miette)
[![documentation](https://docs.rs/facet-miette/badge.svg)](https://docs.rs/facet-miette)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-miette.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-miette

Derive [`miette::Diagnostic`](https://docs.rs/miette/latest/miette/trait.Diagnostic.html) for your error types using facet's plugin system. Get rich error reporting with source spans, error codes, help text, and more.

## Usage

```rust
use facet::Facet;
use facet_miette as diagnostic;
use miette::SourceSpan;

#[derive(Facet, Debug)]
#[facet(derive(Error, facet_miette::Diagnostic))]
pub enum ParseError {
    /// Unexpected token in input
    #[facet(diagnostic::code = "parse::unexpected_token")]
    #[facet(diagnostic::help = "Check for typos or missing delimiters")]
    UnexpectedToken {
        #[facet(diagnostic::source_code)]
        src: String,
        #[facet(diagnostic::label = "this token was unexpected")]
        span: SourceSpan,
    },

    /// End of file reached unexpectedly
    #[facet(diagnostic::code = "parse::unexpected_eof")]
    UnexpectedEof,
}
```

## Attributes

### Container/Variant Level

- `#[facet(diagnostic::code = "my_lib::error_code")]` - Error code for this diagnostic
- `#[facet(diagnostic::help = "Helpful message")]` - Help text shown to user
- `#[facet(diagnostic::url = "https://...")]` - URL for more information
- `#[facet(diagnostic::severity = "warning")]` - Severity: `"error"`, `"warning"`, or `"advice"`

### Field Level

- `#[facet(diagnostic::source_code)]` - Field containing the source text (must impl `SourceCode`)
- `#[facet(diagnostic::label = "description")]` - Field is a span to highlight with label
- `#[facet(diagnostic::related)]` - Field contains related diagnostics (iterator)

## Integration with facet-error

Typically you'll use both `Error` and `Diagnostic` together:

```rust
use facet::Facet;
use facet_miette as diagnostic;

#[derive(Facet, Debug)]
#[facet(derive(Error, facet_miette::Diagnostic))]
pub enum MyError {
    /// Something went wrong while processing
    #[facet(diagnostic::code = "my_error::processing")]
    #[facet(diagnostic::help = "Try again with different input")]
    ProcessingError,
}
```

The `Error` derive (from `facet-error`) generates `Display` and `Error` implementations from doc comments, while `Diagnostic` adds miette's rich error reporting features.

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
