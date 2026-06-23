+++
title = "CLI and config"
description = "Build typed CLI arguments, environment variables, and config files from one Facet type."
weight = 1
insert_anchor_links = "heading"
+++

`figue` turns a `Facet` type into a small, typed interface for command-line
arguments, environment variables, config files, and defaults. Reach for it when
you want one Rust shape to describe how an app is configured, from quick CLIs to
layered service configuration.

## Install

Add `facet` and `figue` to your crate.

## Minimal example

```rust
use facet::Facet;
use figue::{self as args, FigueBuiltins};

#[derive(Facet, Debug)]
struct Cli {
    /// Enable more detailed output
    #[facet(args::named, args::short = 'v', default)]
    verbose: bool,

    /// File to process
    #[facet(args::positional)]
    input: String,

    /// Adds --help, --version, and --completions
    #[facet(flatten)]
    builtins: FigueBuiltins,
}

let cli: Cli = figue::from_slice(&["--verbose", "input.txt"]).unwrap();
assert!(cli.verbose);
assert_eq!(cli.input, "input.txt");
```

Use `figue::from_std_args()` for the real process arguments, and
`figue::from_slice(&["..."])` when you want the input to be explicit in tests or
examples. Field doc comments become help text, so the type stays useful to both
the compiler and the person at the terminal.

## Attribute vocabulary

Attributes live in the `args::` namespace when you import
`figue::{self as args, ...}`:

| Attribute | Effect |
|-----------|--------|
| `args::positional` | Bare positional argument |
| `args::named` | `--flag` style option |
| `args::short = 'v'` | Add a short alias such as `-v` |
| `args::counted` | Count repeats such as `-vvv` |
| `args::subcommand` | Enum field selects a subcommand |
| `args::config` | Field is a layered config struct |
| `args::env_prefix = "MYAPP"` | Read environment variables for that config |
| `rename`, `default`, `flatten` | Common facet attributes |

`FigueBuiltins` contributes the standard help, version, completion, and schema
switches, so the everyday CLI niceties stay boring in the best way.

## Layered configuration

For applications that need more than CLI parsing, build a driver. CLI values can
sit on top of environment variables, config files, and Rust defaults:

```rust
use facet::Facet;
use figue::{self as args, Driver, builder};

#[derive(Facet, Debug)]
struct Args {
    #[facet(args::config, args::env_prefix = "MYAPP")]
    config: ServerConfig,
}

#[derive(Facet, Debug)]
struct ServerConfig {
    #[facet(default = 8080)]
    port: u16,

    #[facet(default = "localhost")]
    host: String,
}

let config = builder::<Args>()
    .unwrap()
    .cli(|cli| cli.args(["--config.port", "3000"]))
    .build();

let output = Driver::new(config).run().into_result().unwrap();
assert_eq!(output.value.config.port, 3000);
assert_eq!(output.value.config.host, "localhost");
```

## Related

- [facet-default](/facet-default/guide/) — derive richer `Default` impls for config structs
- [facet-json](/facet-json/guide/) — read and write config-shaped JSON
- [facet-error](/facet-error/guide/) — model friendly CLI and config errors
- [Ecosystem](/ecosystem/) — the rest of the facet crates
