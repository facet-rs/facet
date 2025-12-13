# strid

[![Build Status](https://github.com/bearcove/strid/actions/workflows/rust.yml/badge.svg?branch=main&event=push)](https://github.com/bearcove/strid)

**strid** (string id) is a fork of [aliri_braid](https://github.com/neoeinstein/aliri_braid) brought up to speed with Rust edition 2024 and adding support for [facet](https://crates.io/crates/facet).

Improve and strengthen your strings

Strongly-typed APIs reduce errors and confusion over passing around un-typed strings.
Strid helps in that endeavor by making it painless to create wrappers around your
string values, ensuring that you use them in the right way every time.

Examples of the documentation and implementations provided for strids are available
below and in the [`strid-examples`] crate documentation.

[`strid-examples`]: https://docs.rs/strid-examples/

## Usage

A strid is created by attaching `#[braid]` to a struct definition. The macro will take
care of automatically updating the representation of the struct to wrap a string and
generate the borrowed form of the strong type.

```rust
use strid::braid;

#[braid]
pub struct DatabaseName;
```

Strids of custom string types are also supported, so long as they implement a set of
expected traits. If not specified, the type named `String` in the current namespace
will be used.

```rust
use strid::braid;
use compact_str::CompactString as String;

#[braid]
pub struct UserId;
```

Once created, strids can be passed around as strongly-typed, immutable strings.

```rust
fn take_strong_string(n: DatabaseName) {}
fn borrow_strong_string(n: &DatabaseNameRef) {}

let owned = DatabaseName::new(String::from("mongo"));
borrow_strong_string(&owned);
take_strong_string(owned);
```

A strid can also be untyped for use in stringly-typed interfaces.

```rust
fn take_raw_string(s: String) {}
fn borrow_raw_str(s: &str) {}

let owned = DatabaseName::new(String::from("mongo"));
borrow_raw_str(owned.as_str());
take_raw_string(owned.take());
```

For more information, see the [documentation on docs.rs](https://docs.rs/strid).

## Acknowledgments

This project is a fork of [aliri_braid](https://github.com/neoeinstein/aliri_braid) by Marcus Griep.
