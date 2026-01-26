# figue-attrs

[![crates.io](https://img.shields.io/crates/v/figue-attrs.svg)](https://crates.io/crates/figue-attrs)
[![documentation](https://docs.rs/figue-attrs/badge.svg)](https://docs.rs/figue-attrs)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/figue-attrs.svg)](./LICENSE)

Attribute macros for [figue](https://crates.io/crates/figue) CLI argument parsing.

**Note:** This is an internal crate. Users should depend on `figue` directly, which
re-exports everything from this crate.

## Why a separate crate?

This crate exists to work around Rust's restriction on accessing macro-expanded
`#[macro_export]` macros by absolute paths within the same crate
([rust-lang/rust#52234](https://github.com/rust-lang/rust/issues/52234)).

By defining the attribute grammar macros in a separate crate, the main `figue` crate
can reference them via an external crate path.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/figue/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/figue/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
