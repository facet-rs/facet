+++
title = "facet-axum"
weight = 30
insert_anchor_links = "heading"
+++

`facet-axum` exposes [axum](https://docs.rs/axum) extractors and response
types backed by Facet format crates.

Use it when your request and response types already derive `Facet` and you want
the web boundary to use the same schema as the rest of the Facet ecosystem.

```rust
use axum::{
    Router,
    routing::{get, post},
};
use facet::Facet;
use facet_axum::{Json, Query};

#[derive(Facet)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Facet)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Facet)]
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

The default features enable JSON responses/extractors and URL-encoded form/query
extractors. Optional features expose YAML, TOML, XML, MessagePack, and Postcard
types when the corresponding Facet format crates are enabled.

Source: [`facet-axum`](https://github.com/facet-rs/facet/tree/main/facet-axum)
