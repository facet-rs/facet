# facet-assert

[![codecov](https://codecov.io/gh/facet-rs/facet/graph/badge.svg)](https://codecov.io/gh/facet-rs/facet)
[![crates.io](https://img.shields.io/crates/v/facet-assert.svg)](https://crates.io/crates/facet-assert)
[![documentation](https://docs.rs/facet-assert/badge.svg)](https://docs.rs/facet-assert)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-assert.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)


Pretty assertions for [Facet](https://github.com/facet-rs/facet) types.

## What makes this different?

### No `PartialEq` required

Standard Rust assertions need `PartialEq`:

```text
assert_eq!(a, b); // Requires: PartialEq + Debug
```

`facet-assert` uses **structural comparison via reflection**:

```text
assert_same!(a, b); // Requires: Facet (that's it!)
```

This works because Facet gives us full introspection into any type's structure.

### Type inference works naturally

Unlike some reflection-based comparison macros, `assert_same!` supports full
type inference:

```ignore
let x: Option<Option<i32>> = Some(None);
assert_same!(x, Some(None)); // Type of Some(None) inferred from x
```

### Cross-type comparison with `assert_sameish!`

Need to compare values of different types? Use `assert_sameish!`:

```ignore
#[derive(Facet)]
struct PersonV1 { name: String, age: u32 }

#[derive(Facet)]
struct PersonV2 { name: String, age: u32 }

let a = PersonV1 { name: "Alice".into(), age: 30 };
let b = PersonV2 { name: "Alice".into(), age: 30 };

assert_sameish!(a, b); // Passes! Same structure, same values.
```

This is useful for:
- Comparing DTOs across API versions
- Testing serialization roundtrips (JSON → struct → JSON)
- Comparing values parsed from different formats (YAML vs TOML vs JSON)

### Smart structural diffs

When values differ, you get a **structural diff** — not just line-by-line text
comparison. We know which fields changed:

```text
.host:
  - localhost
  + prod.example.com
.port:
  - 8080
  + 443
.tags[1] (only in left):
  - api
```

Instead of a wall of red/green like traditional diff tools.

### Render diffs in your format

Want the diff in JSON or XML so another tool can consume it? Call
`check_same_report` to get a `SameReport`. When values differ you receive a
`DiffReport` that can render the change set in Rust, JSON, or XML layouts with
or without ANSI colors.

```ignore
use facet_assert::{SameReport, check_same_report};

let report = match check_same_report(&c_output, &rust_output) {
    SameReport::Different(report) => report,
    SameReport::Same => return,
    SameReport::Opaque { type_name } => panic!("opaque type {type_name}"),
};

let rust_view = report.legacy_string();
let json_view = report.render_plain_json();
let xml_view = report.render_plain_xml();
```

For full control, use `render_with_options` and pass your own `BuildOptions`,
`RenderOptions`, or even a custom `DiffFlavor` implementation.

### Opaque types fail clearly

If a type cannot be inspected (opaque), the assertion fails with a clear message
rather than silently giving wrong results:

```text
assertion `assert_same!(left, right)` failed: cannot compare opaque type `SomeOpaqueType`
```

## Usage

```ignore
use facet::Facet;
use facet_assert::assert_same;

#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

#[test]
fn test_config_parsing() {
    let from_json: Config = parse_json("...");
    let from_yaml: Config = parse_yaml("...");

    assert_same!(from_json, from_yaml);
}
```

## Macros

### Same-type comparison (the common case)

- `assert_same!(a, b)` — panics if `a` and `b` are not structurally same
- `assert_same!(a, b, "message {}", x)` — with custom message
- `assert_same_with!(a, b, options)` — with custom comparison options
- `debug_assert_same!(...)` — only in debug builds

### Cross-type comparison (for migrations, etc.)

- `assert_sameish!(a, b)` — compare values of different types
- `assert_sameish_with!(a, b, options)` — with custom comparison options
- `debug_assert_sameish!(...)` — only in debug builds

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
