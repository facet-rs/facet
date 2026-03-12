# facet-typescript

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Generate TypeScript type definitions from facet type metadata.

This crate uses facet’s reflection capabilities to generate TypeScript
interfaces and types from any type that implements `Facet`.

## Example

```rust
use facet::Facet;
use facet_typescript::to_typescript;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let ts = to_typescript::<User>();
assert!(ts.contains("export interface User"));
```

<!-- cargo-reedme: end -->
