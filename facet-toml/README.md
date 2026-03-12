# facet-toml

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

TOML serialization for facet using the new format architecture.

This is the successor to `facet-toml`, using the unified `facet-format` traits.

## Deserialization

```rust
use facet::Facet;
use facet_toml::from_str;

#[derive(Facet, Debug)]
struct Config {
    name: String,
    port: u16,
}

let toml = r#"
name = "my-app"
port = 8080
"#;

let config: Config = from_str(toml).unwrap();
assert_eq!(config.name, "my-app");
assert_eq!(config.port, 8080);
```

<!-- cargo-reedme: end -->
