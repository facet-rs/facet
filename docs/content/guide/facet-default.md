+++
title = "Custom defaults (facet-default)"
weight = 8
insert_anchor_links = "heading"
+++

[`facet-default`](https://docs.rs/facet-default) derives `Default`, but with
per-field control: literal defaults, function-computed defaults, and a chosen
default enum variant. It's the `#[derive(Default)]` you wished std had.

## Setup

```bash
cargo add facet facet-default
```

Opt a type in with `#[facet(derive(Default))]`:

```rust,noexec
use facet::Facet;
use facet_default as default;

#[derive(Facet, Debug)]
#[facet(derive(Default))]
struct Config {
    #[facet(default::value = "localhost")]
    host: String,

    #[facet(default::value = 8080u16)]
    port: u16,

    #[facet(default::func = "default_timeout")]
    timeout: std::time::Duration,

    // No attribute → falls back to Default::default()
    debug: bool,
}

fn default_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(30)
}

let c = Config::default();
assert_eq!(c.host, "localhost");
assert_eq!(c.port, 8080);
```

## Defaults

- `#[facet(default::value = <literal>)]` — a literal (use a type suffix like
  `8080u16` where the type is ambiguous).
- `#[facet(default::func = "path")]` — call a function returning the field type.
- No attribute — the field's own `Default::default()`.

## Enums

Mark which variant `Default` should produce:

```rust,noexec
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

This pairs naturally with [figue](@/guide/cli.md) config structs, where defaults
fill in whatever the user didn't supply.

## Related

- [Ecosystem](@/ecosystem/_index.md) — other derive plugins (`facet-error`, `facet-validate`)
