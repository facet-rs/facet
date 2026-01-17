+++
title = "TOML"
weight = 3
slug = "toml"
insert_anchor_links = "heading"
+++

TOML is popular for Rust project configuration (Cargo.toml). Styx offers
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
  {host alpha, port 8080}
  {host beta, port 8081}
)
```

## Inline tables

```compare
/// toml
point = { x = 1, y = 2 }
/// styx
point {x 1, y 2}
```

TOML 1.1 allows multi-line inline tables with trailing commas. Styx comma-separated objects must be single-line.

## Reopening sections

```compare
/// toml
[server]
host = "localhost"

[database]
url = "postgres://..."

[server]  # reopening server
port = 8080
/// styx
server {
  host localhost
  port 8080
}
database {
  url postgres://...
}
```

TOML allows reopening sections. In Styx, each key appears exactly once.
