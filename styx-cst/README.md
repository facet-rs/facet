# styx-cst

[![crates.io](https://img.shields.io/crates/v/styx-cst.svg)](https://crates.io/crates/styx-cst)
[![documentation](https://docs.rs/styx-cst/badge.svg)](https://docs.rs/styx-cst)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/styx-cst.svg)](./LICENSE)

Lossless Concrete Syntax Tree for the [Styx](https://github.com/bearcove/styx) configuration language. Preserves all whitespace and comments for formatting and refactoring tools.

This crate provides a CST (Concrete Syntax Tree) representation of Styx documents
using the [rowan](https://docs.rs/rowan) library. Unlike an AST, the CST preserves
all source information including whitespace, comments, and exact token positions,
making it ideal for tooling like formatters, refactoring tools, and language servers.

# Features

- **Lossless representation**: Source text can be exactly reconstructed from the CST
- **Cheap cloning**: Syntax nodes use reference counting internally
- **Parent pointers**: Navigate up and down the tree
- **Typed AST layer**: Ergonomic wrappers over raw CST nodes
- **Semantic validation**: Check for issues like duplicate keys and mixed separators

# Example

```
use styx_cst::{parse, ast::{AstNode, Document}};

let source = r#"
host localhost
port 8080
"#;

let parsed = parse(source);
assert!(parsed.is_ok());

let doc = Document::cast(parsed.syntax()).unwrap();
for entry in doc.entries() {
    if let Some(key) = entry.key_text() {
        println!("Found key: {}", key);
    }
}

// Roundtrip: source can be exactly reconstructed
assert_eq!(parsed.syntax().to_string(), source);
```

# Validation

```
use styx_cst::{parse, validation::validate};

let source = "{ a 1, a 2 }"; // Duplicate key
let parsed = parse(source);
let diagnostics = validate(&parsed.syntax());

assert!(!diagnostics.is_empty());
```

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/bearcove/styx/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/styx/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/styx/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
