# facet-axum

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-axum/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-axum.svg)](https://crates.io/crates/facet-axum)
[![documentation](https://docs.rs/facet-axum/badge.svg)](https://docs.rs/facet-axum)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-axum.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-axum

Axum integration for Facet - extractors and responses using Facet's serialization.

This crate provides Axum extractors and response types that use Facet's
serialization instead of serde. This allows you to use Facet-derived types
directly in your Axum handlers without needing serde derives.

## Quick start

```rust
use axum::{routing::{get, post}, Router};
use facet::Facet;
use facet_axum::{Json, Query};

#[derive(Debug, Facet)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Debug, Facet)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Facet)]
struct SearchParams {
    q: String,
    page: u64,
}

async fn create_user(Json(payload): Json<CreateUser>) -> Json<User> {
    Json(User {
        id: 1,
        name: payload.name,
        email: payload.email,
    })
}

async fn search(Query(params): Query<SearchParams>) -> String {
    format!("Searching for '{}' on page {}", params.q, params.page)
}

let app = Router::new()
    .route("/users", post(create_user))
    .route("/search", get(search));
```

## Feature flags

- `json` (default): Enables `Json<T>` extractor/response using `facet-json`
- `form` (default): Enables `Form<T>` and `Query<T>` extractors using `facet-urlencoded`
- `yaml`: Enables `Yaml<T>` extractor/response using `facet-yaml`
- `toml`: Enables `Toml<T>` extractor/response using `facet-toml`
- `xml`: Enables `Xml<T>` extractor/response using `facet-xml`
- `msgpack`: Enables `MsgPack<T>` extractor/response using `facet-msgpack`
- `postcard`: Enables `Postcard<T>` extractor/response using `facet-postcard`
- `all`: Enables all format features

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
