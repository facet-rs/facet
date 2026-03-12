# facet-yaml

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

YAML parser and serializer using facet-format.

This crate provides YAML support via the `FormatParser` trait,
using saphyr-parser for streaming event-based parsing.

## Example

```rust
use facet::Facet;
use facet_yaml::{from_str, to_string};

#[derive(Facet, Debug, PartialEq)]
struct Config {
    name: String,
    port: u16,
}

let yaml = "name: myapp\nport: 8080";
let config: Config = from_str(yaml).unwrap();
assert_eq!(config.name, "myapp");
assert_eq!(config.port, 8080);

let output = to_string(&config).unwrap();
assert!(output.contains("name: myapp"));
```

<!-- cargo-reedme: end -->
