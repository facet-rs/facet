+++
title = "TOML"
weight = 3
slug = "toml"
insert_anchor_links = "heading"
+++

TOML is popular for Rust project configuration (Cargo.toml). STYX offers
different trade-offs for deeply nested structures.

## Simple object

```compare
/// toml
name = "my-app"
version = "1.0.0"
/// styx
name my-app
version 1.0.0
```

## Nested sections

```compare
/// toml
[package]
name = "my-app"
version = "1.0.0"

[dependencies]
serde = "1.0"

[dependencies.tokio]
version = "1.0"
features = ["full"]
/// styx
package {
  name my-app
  version 1.0.0
}
dependencies {
  serde 1.0
  tokio {
    version 1.0
    features (full)
  }
}
```

## Arrays of tables

```compare
/// toml
[[servers]]
host = "alpha"
port = 8080

[[servers]]
host = "beta"
port = 8081
/// styx
servers (
  { host alpha, port 8080 }
  { host beta, port 8081 }
)
```

## Inline tables

```compare
/// toml
# must be single line, no trailing comma
point = { x = 1, y = 2 }
/// styx
# can span lines, trailing comma OK
point {
  x 1,
  y 2,
}
```

## Key differences

| TOML | STYX |
|------|------|
| Section headers `[foo]` | Explicit nesting `foo { }` |
| `[[array]]` syntax | Sequences `(...)` |
| Inline tables single-line only | Objects can span lines |
| Reopening sections allowed | Each key appears once |
