+++
title = "Errors & diagnostics"
weight = 3
insert_anchor_links = "heading"
+++

Facet format crates return errors that implement [`miette::Diagnostic`](https://docs.rs/miette), so you get source spans, labels, and hints out of the box. This page shows how to print them well and what the common messages mean.

## Printing errors nicely

```rust
use facet::Facet;
use facet_json::from_str;
use miette::{IntoDiagnostic, Result};

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
}

fn main() -> Result<()> {
    let input = r#"{ "name": "Ada", "agge": 36 }"#;
    let user: User = from_str(input).into_diagnostic()?; // will error
    println!("{user:?}");
    Ok(())
}
```

Run with `cargo run` and you’ll see:

```text
× unknown field `agge`, expected one of: ["name", "age"]
  ┌─ <stdin>:1:20
  │
1 │ { "name": "Ada", "agge": 36 }
  │                    ──── did you mean `age`?
```

Another common one:

```text
× invalid type: string "bad", expected u16
  ┌─ <stdin>:1:23
  │
1 │ { "name": "app", "port": "bad" }
  │                       ──── invalid type
```

### Tips
- Use `miette = { version = "7", features = ["fancy"] }` for color + unicode boxes.
- If you parse from a file, pass a named reader so spans show filenames (e.g., `from_reader` on a `File`).
- In tests, snapshot `miette::Report::new(err)` to lock error text and spans.

## Common errors and fixes

| Message (example) | What it means | Fix |
|-------------------|---------------|-----|
| `unknown field "foo"` | Input contains a field not in the struct | Add the field, rename with `#[facet(rename)]`, or allow it; use `deny_unknown_fields` to make this fatal |
| `invalid type: string "bad", expected u16` | Type mismatch while deserializing | Fix the input type or change the field type |
| `missing field "name"` | Required field absent | Provide the field or add `#[facet(default)]` |
| `cannot match enum; no variants matched` | For untagged/flattened enums, none of the variants validated | Check tags/content fields or add disambiguating data |
| `duplicate field "x"` | Field provided twice in inputs that disallow it | Remove the duplicate or enable merging logic upstream |

## Getting stricter or looser
- **Strict mode:** `#[facet(deny_unknown_fields)]` rejects unknown inputs.
- **Optional fields:** `Option<T>` for nullable/absent fields, **and** add `#[facet(default)]` (or a custom default) so missing values initialize cleanly. Pair with `skip_serializing_if = Option::is_none` to omit on output.
- **Defaults:** `#[facet(default)]` uses `Default::default()`, or provide a function/literal via `#[facet(default = ...)]`.

## Debugging tricky cases
- **Flattened structures:** When using `#[facet(flatten)]`, ensure nested structs don’t share clashing field names or tags.
- **Enum tagging:** For `tag`/`content`, confirm the field names match your inputs exactly (case-sensitive).
- **Extension attributes:** If you see “unknown attribute” errors, ensure the namespace crate is imported (e.g., `use facet_kdl as kdl;`).

## Next steps
- Start with the [Getting Started](@/learn/getting-started.md) guide.
- Browse [Showcases](@/learn/showcases/_index.md) for real inputs/outputs.
- Consult the [Attributes Reference](@/learn/attributes.md) for all error-related knobs.
