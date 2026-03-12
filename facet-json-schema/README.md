# facet-json-schema

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Generate JSON Schema from facet type metadata.

This crate uses facet’s reflection capabilities to generate JSON Schema definitions
from any type that implements `Facet`.

## Example

```rust
use facet::Facet;
use facet_json_schema::to_schema;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let schema = to_schema::<User>();
println!("{}", schema);
```

<!-- cargo-reedme: end -->
