# facet-python

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Generate Python type definitions from facet type metadata.

This crate uses facet’s reflection capabilities to generate Python
type hints and TypedDicts from any type that implements `Facet`.

## Example

```rust
use facet::Facet;
use facet_python::to_python;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let py = to_python::<User>(false);
assert!(py.contains("class User(TypedDict"));
```

<!-- cargo-reedme: end -->
