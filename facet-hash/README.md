# facet-hash

[![crates.io](https://img.shields.io/crates/v/facet-hash.svg)](https://crates.io/crates/facet-hash)
[![documentation](https://docs.rs/facet-hash/badge.svg)](https://docs.rs/facet-hash)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-hash.svg)](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT)

`facet-hash` provides structural hashing for any type that derives `Facet`. Rather than
relying on a hand-written `Hash` impl — which must be kept in sync with the type and may
accidentally hash padding bytes — it lowers the shape of a type into a compact bytecode
program once, then interprets that program to hash values. Only real field data is ever
fed to the hasher; padding bytes between fields are invisible to the hash stream.

Because the shape is lowered once into a reusable [`HashPlan`], hashing many values of
the same type is cheap: no repeated reflection walks, no repeated vtable lookups. Two
modes are available — `Value` (the default) hashes only field contents, while `Structural`
additionally mixes in type ids, struct kind, and field names so that hashes from different
shapes cannot collide.

```rust
use facet::Facet;
use facet_hash::HashPlan;

#[derive(Facet)]
struct Point {
    x: i32,
    y: i32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Lower Point's shape once.
    let plan = HashPlan::<Point>::build()?;

    let a = Point { x: 10, y: -4 };
    let b = Point { x: 10, y: 0 };

    // Hash with the standard DefaultHasher.
    let ha = plan.hash64(&a)?;
    let hb = plan.hash64(&b)?;
    assert_ne!(ha, hb);

    // Or drive any Hasher you like.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    plan.hash(&a, &mut hasher)?;

    Ok(())
}
```

The bytecode interpreter is provided by [`weavy`](https://docs.rs/weavy).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
