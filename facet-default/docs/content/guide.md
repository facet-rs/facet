+++
title = "Custom defaults"
description = "Derive Default for Facet types with per-field values, functions, and enum variants."
weight = 1
insert_anchor_links = "heading"
+++

`facet-default` derives `Default` through the `Facet` plugin chain, with
per-field values, function calls, and explicit enum default variants. Reach for
it when plain `#[derive(Default)]` is close, but your config or domain type needs
defaults written next to the fields they affect.

## Install

Add `facet` and `facet-default` to your crate.

## Minimal example

```rust
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
#[facet(derive(Default))]
struct Config {
    #[facet(default = "localhost")]
    host: String,

    #[facet(default = 8080u16)]
    port: u16,

    #[facet(default = default_timeout())]
    timeout_secs: u64,

    debug: bool,
}

fn default_timeout() -> u64 {
    30
}

let config = Config::default();
assert_eq!(config.host, "localhost");
assert_eq!(config.port, 8080);
assert_eq!(config.timeout_secs, 30);
assert!(!config.debug);
```

The field defaults are real Rust expressions. String literals are converted for
`String` fields, and omitted fields fall back to that field type's own
`Default::default()`.

## Field defaults

Use `#[facet(default = ...)]` for a literal or expression, and
`#[facet(default)]` when you want to be explicit about using
`Default::default()` for that field. Type suffixes such as `8080u16` are useful
when the expression would otherwise be ambiguous.

## Enum defaults

For enums, mark the variant that `Default::default()` should return:

```rust
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
#[facet(derive(Default))]
#[repr(u8)]
enum Status {
    #[facet(default::variant)]
    Pending,
    Active,
    Done,
}

assert_eq!(Status::default(), Status::Pending);
```

Default variants can also have fields; those fields use their own facet default
attributes or their type's `Default` implementation.

## Related

- [figue](/figue/guide/) — layer CLI, env, files, and defaults into one config type
- [facet-json](/facet-json/guide/) — let missing JSON fields use facet defaults
- [facet-validate](/facet-validate/guide/) — pair defaults with constraints
- [Ecosystem](/ecosystem/) — other facet derive plugins and format crates
