+++
title = "Use Styx in Your App"
weight = 1
+++

<!-- TODO: This page needs content -->

Add Styx as a configuration or data language in your application.

## Rust

The fastest path — derive `Facet` and deserialize directly.

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Config = facet_styx::from_str(include_str!("config.styx"))?;
    println!("Listening on {}:{}", config.host, config.port);
    Ok(())
}
```

### With Schema Validation

<!-- TODO: Example with explicit schema -->

### Error Handling

<!-- TODO: How to display errors nicely -->

## JavaScript / TypeScript

<!-- TODO: npm package, usage example -->

```typescript
import { parse } from "@bearcove/styx";
import schema from "./config.schema.styx";

const config = parse(input, schema);
```

## Python

<!-- TODO: Python bindings -->

## Go

<!-- TODO: Go bindings -->

## Other Languages

Styx has a simple grammar. If bindings don't exist for your language yet:

1. Use the tree-sitter grammar for parsing
2. Implement scalar interpretation per [the spec](/reference/spec/scalars)
3. Consider contributing bindings back!
