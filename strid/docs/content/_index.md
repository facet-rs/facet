+++
title = "strid"
description = "Strongly-typed strings with less boilerplate — a facet-aware fork of aliri_braid."
insert_anchor_links = "heading"
+++

**strid** (string id) makes it painless to create strongly-typed wrappers around
string values, so you use them the right way every time. It's a fork of
[aliri_braid](https://github.com/neoeinstein/aliri_braid) brought up to Rust
edition 2024 with [facet](https://crates.io/crates/facet) support.

Attach `#[braid]` to a struct and the macro wraps a string and generates the
borrowed form of the strong type:

```rust
use strid::braid;

#[braid]
pub struct DatabaseName;
```

Custom string types are supported too, as long as they implement the expected
traits:

```rust
use strid::braid;
use compact_str::CompactString as String;

#[braid]
pub struct UserId;
```

Once created, strids pass around as strongly-typed, immutable strings — or untyped
for stringly-typed interfaces:

```rust
let owned = DatabaseName::new(String::from("mongo"));
borrow_strong_string(&owned);   // &DatabaseNameRef
take_raw_string(owned.take());  // String
```

[Source on GitHub](https://github.com/bearcove/strid) · [docs.rs](https://docs.rs/strid)
