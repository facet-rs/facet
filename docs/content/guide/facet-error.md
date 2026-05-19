+++
title = "Error types (facet-error)"
weight = 9
insert_anchor_links = "heading"
+++

[`facet-error`](https://docs.rs/facet-error) is a `thiserror`-style derive built
on facet: your doc comments *are* the error messages, with field interpolation
and an automatic `Error::source()`.

## Setup

```bash
cargo add facet facet-error
```

```rust,noexec
use facet::Facet;

#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
enum MyError {
    /// something went wrong
    Unknown,

    /// invalid value: {0}
    InvalidValue(String),

    /// invalid header (expected {expected}, found {found})
    InvalidHeader { expected: String, found: String },
}

let e = MyError::InvalidValue("nope".into());
assert_eq!(e.to_string(), "invalid value: nope");
```

The derive generates `Display` and `Error` impls. Messages come straight from
the doc comment on each variant:

- Tuple variants interpolate positionally: `{0}`, `{1}`.
- Struct variants interpolate by name: `{expected}`, `{found}`.

## Wrapping a source error

Mark the wrapped error so `From` and `source()` are generated for you:

```rust,noexec
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
enum MyError {
    /// data store disconnected
    #[facet(error::from)]
    Disconnect(std::io::Error),

    /// unknown error
    Unknown,
}

fn read() -> Result<String, MyError> {
    let s = std::fs::read_to_string("config.toml")?; // io::Error → MyError
    Ok(s)
}
```

Use `#[facet(error::source)]` instead of `error::from` when you want
`source()` chaining but *not* an implicit `From` conversion.

Because the error type also derives `Facet`, it can be serialized,
pretty-printed, or sent over the wire like any other facet type — handy for
structured logging and RPC.

## Related

- [Ecosystem](@/ecosystem/_index.md) — derive plugins overview
- [Pretty-printing](@/guide/pretty-printing.md) — render errors with context
