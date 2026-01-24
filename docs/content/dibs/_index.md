+++
title = "dibs"
description = "Schema as Rust, queries as Styx"
+++

dibs is a Postgres toolkit for Rust with two pillars:

- **Schema as Rust** (facet): your Rust types are the source of truth for tables/columns/constraints.
- **Queries as Styx**: you write queries in a small DSL, get LSP support, and generate typed Rust + SQL.

From those, dibs generates migrations (also Rust), so you can do backfills and data fixes without switching mental models.

## Start here

- [How dibs works](/guide/model/) (model + workspace layout + deployment)
- [Getting started](/guide/getting-started/)
- [Queries](/guide/queries/)
- [Migrations](/guide/migrations/)

## Links

- [GitHub](https://github.com/bearcove/dibs)
- [crates.io](https://crates.io/crates/dibs)
- [docs.rs](https://docs.rs/dibs)
