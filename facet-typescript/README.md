# facet-typescript

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-typescript/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-typescript.svg)](https://crates.io/crates/facet-typescript)
[![documentation](https://docs.rs/facet-typescript/badge.svg)](https://docs.rs/facet-typescript)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-typescript.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-typescript

Generate TypeScript type definitions from facet type metadata.

This crate uses facet's reflection capabilities to generate TypeScript interfaces
and types from any type that implements `Facet`. Unlike going through JSON Schema,
this generates TypeScript directly, preserving:

- Exact optional field semantics
- Union types for enums
- Literal types for discriminated unions
- Proper `readonly` modifiers

## Usage

```rust
use facet::Facet;
use facet_typescript::to_typescript;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let ts = to_typescript::<User>();
println!("{}", ts);
```

## Output

```typescript
export interface User {
  name: string;
  age: number;
  email?: string;
}
```

## Multiple Types

Generate types for multiple related types at once:

```rust
use facet_typescript::TypeScriptGenerator;

let mut gen = TypeScriptGenerator::new();
gen.add_type::<User>();
gen.add_type::<Post>();
gen.add_type::<Comment>();

let ts = gen.finish();
```


## LLM contribution policy

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
