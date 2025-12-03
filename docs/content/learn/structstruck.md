+++
title = "structstruck + facet"
weight = 6
insert_anchor_links = "heading"
+++

Turn sample JSON/YAML into Rust types with [`structstruck`](https://crates.io/crates/structstruck), then derive `Facet` to get multi-format I/O and diagnostics for free. structstruck generates nested structs/enums at **compile time** from example data, so you avoid hand-writing boilerplate while keeping everything type-checked. citeturn0search0

## Why pair them?
- **Faster modeling:** structstruck infers shapes from example payloads; facet then exposes those shapes to every format crate.
- **Better errors:** facet’s diagnostics highlight bad fields/values in user input; combining them makes “copy a sample, paste an error” workflows tight.
- **One derive:** add `Facet` to the generated types and you’re ready for JSON/YAML/TOML/KDL/MsgPack/Postcard.

## Quick start
```toml
[dependencies]
structstruck = "0.4"         # sample-driven type generation
facet = "1"
facet-json = "1"
```

```rust
structstruck::strike! {
    // Apply derives to every generated type
    #[structstruck::each[derive(facet::Facet, Debug, Clone)]]
    #[structstruck::each[field_attribute(#[facet(default)])]]
    pub struct Config {
        name: String,
        port: u16,
        limits: struct {
            connections: u32,
        },
        features: Option<struct { tracing: bool }>,
    }
}

fn main() -> miette::Result<()> {
    let cfg: Config = facet_json::from_str(r#"{ "name": "app", "port": 8080 }"#)?;
    println!("{cfg:?}");
    Ok(())
}
```
- `each[derive(...)]` attaches `Facet` to every generated struct/enum.  
- `each[field_attribute(#\[facet(default)\])]` makes Option fields truly optional in facet (missing values default to `None`).  

## Tips
- Feed the smallest realistic sample that still covers optional fields; structstruck treats absent fields as optional but facet still needs `#[facet(default)]` as shown.
- If you need renames or tagging, add facet attributes via `each[field_attribute(...)]` too.
- Regenerate types whenever the sample changes; this is a compile-time macro, not runtime inference.

## When not to use
- Highly dynamic schemas that change per-request—hand-written types or `facet_value::Value` may fit better.
- Performance-critical hot loops where serde’s monomorphized code still wins.

## Next steps
- Add format crates as needed (`facet-yaml`, `facet-toml`, `facet-kdl`, `facet-msgpack`, `facet-postcard`).
- Run your test inputs through `facet_json::from_str` and snapshot the diagnostics (`miette::Report`) to lock errors.
