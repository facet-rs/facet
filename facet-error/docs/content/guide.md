+++
title = "Error types"
description = "Derive Display, Error, source, and From implementations from Facet error enums."
weight = 1
insert_anchor_links = "heading"
+++

`facet-error` is a `thiserror`-style derive built on facet: doc comments become
error messages, fields can be interpolated, and source errors can be wired in
with attributes. Reach for it when an error enum should be readable to humans
and still carry the same `Facet` shape as the rest of your protocol or config.

## Install

Add `facet` and `facet-error` to your crate.

## Minimal example

```rust
use facet::Facet;

#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
enum ConfigError {
    /// missing field: {0}
    MissingField(String),

    /// invalid header (expected {expected}, found {found})
    InvalidHeader { expected: String, found: String },
}

let error = ConfigError::MissingField("host".to_string());
assert_eq!(error.to_string(), "missing field: host");
```

The derive generates `Display` and `std::error::Error`. Tuple variants
interpolate positionally with `{0}`, `{1}`, and struct variants interpolate by
field name, such as `{expected}`.

## Error sources

Use `#[facet(error::from)]` on a field to mark it as the source and generate a
`From` impl. Use `#[facet(error::source)]` when you want `source()` chaining
without an implicit conversion.

```rust
use facet::Facet;
use facet_error as error;
use std::{error::Error, fmt};

#[derive(Facet, Debug)]
struct SourceError {
    message: String,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source error: {}", self.message)
    }
}

impl Error for SourceError {}

#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
enum ConfigError {
    /// config file could not be read
    Read(#[facet(error::from)] SourceError),

    /// unknown error
    Unknown,
}

let error: ConfigError = SourceError {
    message: "permission denied".to_string(),
}
.into();

assert_eq!(error.to_string(), "config file could not be read");
```

Because the error type also derives `Facet`, it can be serialized,
pretty-printed, or sent over RPC like any other facet value.

## Related

- [facet-pretty](/facet-pretty/guide/) — render errors and context as structured text
- [facet-json](/facet-json/guide/) — serialize facet-shaped errors when that is part of your protocol
- [figue](/figue/guide/) — pair typed config with typed failures
- [Ecosystem](/ecosystem/) — derive plugins and companion crates
