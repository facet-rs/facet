+++
title = "Integrate with Rust"
weight = 2
slug = "integrate-rust"
insert_anchor_links = "heading"
+++

Add Styx to your Rust application using [Facet](https://github.com/facet-rs/facet) for type-safe deserialization.

## Installation

Add `facet` and `facet-styx` to your `Cargo.toml`:

```bash
cargo add facet facet-styx
```

## Basic usage

Define your configuration struct and derive `Facet`:

```rust
use facet::Facet;

#[derive(Debug, Facet)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = r#"
        host localhost
        port 8080
        debug true
    "#;

    let config: Config = facet_styx::from_str(input)?;
    println!("Listening on {}:{}", config.host, config.port);
    Ok(())
}
```

## Loading from a file

```rust
use facet::Facet;
use std::fs;

#[derive(Debug, Facet)]
struct Config {
    host: String,
    port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string("config.styx")?;
    let config: Config = facet_styx::from_str(&content)?;
    Ok(())
}
```

Or use `include_str!` for compile-time embedding:

```rust
let config: Config = facet_styx::from_str(include_str!("config.styx"))?;
```

## Error handling

Styx errors include source spans for precise error reporting:

```rust
use facet::Facet;

#[derive(Debug, Facet)]
struct Config {
    port: u16,
}

fn main() {
    let input = "port not-a-number";

    match facet_styx::from_str::<Config>(input) {
        Ok(config) => println!("Port: {}", config.port),
        Err(e) => {
            // Error includes span information for nice diagnostics
            eprintln!("Configuration error: {}", e);
        }
    }
}
```

## Alternative: serde_styx

If you're already using serde and don't want to switch to Facet, `serde_styx` provides a serde-based alternative:

```bash
cargo add serde serde_styx
```

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    host: String,
    port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Config = serde_styx::from_str("host localhost\nport 8080")?;
    println!("{}:{}", config.host, config.port);
    Ok(())
}
```

## Next steps

- See [Rust type mappings](/reference/bindings/rust) for how Rust types correspond to Styx syntax
- Learn about [schemas](/reference/spec/schema) for validation
- Explore the [primer](/guide/primer) for Styx syntax basics
