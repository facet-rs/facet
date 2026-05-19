+++
title = "CLI & config (figue)"
weight = 3
insert_anchor_links = "heading"
+++

[`figue`](https://docs.rs/figue) (formerly `facet-args`) builds a typed CLI,
environment-variable reader, and config-file loader from one `#[derive(Facet)]`
struct. One type, one layered source of truth.

## Setup

```bash
cargo add facet figue
```

## A minimal CLI

```rust,noexec
use facet::Facet;
use figue::{self as args, FigueBuiltins};

#[derive(Facet, Debug)]
struct Args {
    /// Enable verbose output
    #[facet(args::named, args::short = 'v', default)]
    verbose: bool,

    /// Input file to process
    #[facet(args::positional)]
    input: String,

    /// --help / --version / shell completions
    #[facet(flatten)]
    builtins: FigueBuiltins,
}

fn main() {
    let args: Args = figue::from_std_args().unwrap();
    println!("processing {} (verbose={})", args.input, args.verbose);
}
```

`from_std_args()` reads the real process arguments; `from_slice(&["..."])`
takes them explicitly, which is what you use in tests. The doc comment on each
field becomes its `--help` text automatically.

## The attribute vocabulary

Attributes live in the `args::` namespace (via `use figue as args;`):

| Attribute | Effect |
|-----------|--------|
| `args::positional` | Bare positional argument |
| `args::named` | `--flag` style option |
| `args::short = 'v'` | Add a short alias (`-v`) |
| `args::counted` | Count repeats (`-vvv` → 3) |
| `args::subcommand` | Enum field selects a subcommand |
| `args::config` | Field is a layered config struct |
| `args::env_prefix = "MYAPP"` | Read env vars for that config |
| `rename` / `default` / `flatten` | As elsewhere in facet |

`FigueBuiltins` contributes `--help`, `--version`, `--completions <shell>`, and
JSON-schema export, so you don't hand-roll them.

## Layered configuration

For real apps, merge CLI flags over environment variables over config-file
values over defaults:

```rust,noexec
use figue::{builder, Driver};
use figue as args;

#[derive(Facet, Debug)]
struct Config {
    #[facet(args::config, args::env_prefix = "MYAPP")]
    server: ServerConfig,
}

#[derive(Facet, Debug)]
struct ServerConfig {
    #[facet(default = 8080u16)]
    port: u16,
    #[facet(default = "localhost")]
    host: String,
}

let config = builder::<Config>()?
    .cli(|cli| cli.args(["--server.port", "3000"]))
    .build();

let out = Driver::new(config).run().unwrap();
assert_eq!(out.value.server.port, 3000);   // from CLI
assert_eq!(out.value.server.host, "localhost"); // from default
```

The full surface — subcommands, completions, layering precedence — is on
[docs.rs/figue](https://docs.rs/figue).

## Related

- [facet-default](@/guide/facet-default.md) — richer default values
- [Ecosystem](@/ecosystem/_index.md) — the rest of the constellation
