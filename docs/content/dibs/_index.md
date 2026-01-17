+++
title = "dibs"
description = "Postgres toolkit for Rust, powered by facet reflection"
+++

dibs is a Postgres toolkit for Rust that provides database migrations as Rust functions, schema introspection via facet reflection, and query building.

## Features

- **Migrations as Rust code** — Write migrations as async functions, not SQL files
- **Date-based versioning** — `YYYY-MM-DD-slug` format for clear ordering
- **Compile-time registration** — Migrations are collected via inventory
- **Backfill helpers** — Built-in support for batch data operations
- **No syn dependency** — Uses unsynn for fast proc macro compilation

## Quick Start

Add dibs to your project:

```toml
[dependencies]
dibs = "0.1"
```

Define a migration:

```rust
#[dibs::migration("2026-01-17-create-users")]
async fn create_users(ctx: &mut MigrationContext) -> Result<()> {
    ctx.execute("CREATE TABLE users (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL,
        email TEXT UNIQUE NOT NULL
    )").await?;
    Ok(())
}
```

Run migrations:

```rust
let runner = MigrationRunner::new(&client);
runner.migrate().await?;
```

## Links

- [GitHub](https://github.com/bearcove/dibs)
- [crates.io](https://crates.io/crates/dibs)
- [docs.rs](https://docs.rs/dibs)
