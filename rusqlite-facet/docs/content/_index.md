+++
title = "rusqlite-facet"
weight = 35
insert_anchor_links = "heading"
+++

`rusqlite-facet` maps between [rusqlite](https://docs.rs/rusqlite) and
`Facet` types.

It binds SQL parameters from struct fields and projects query rows back into
typed structs, so SQLite boundaries can use the same reflected Rust models as
the rest of a Facet-based application.

```rust
use facet::Facet;
use rusqlite::Connection;
use rusqlite_facet::ConnectionFacetExt;

#[derive(Facet)]
struct UserParams {
    name: String,
}

#[derive(Facet)]
struct User {
    id: i64,
    name: String,
}

let conn = Connection::open_in_memory()?;
conn.execute("create table users (id integer primary key, name text)", [])?;

conn.facet_execute(
    "insert into users (name) values (:name)",
    UserParams {
        name: "Ada".to_owned(),
    },
)?;

let users: Vec<User> = conn.facet_query(
    "select id, name from users where name = :name",
    UserParams {
        name: "Ada".to_owned(),
    },
)?;
```

Source: [`rusqlite-facet`](https://github.com/facet-rs/facet/tree/main/rusqlite-facet)
