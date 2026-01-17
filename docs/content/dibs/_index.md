+++
title = "dibs"
description = "Postgres toolkit for Rust, powered by facet reflection"
+++

dibs is a Postgres toolkit for Rust that lets you define your schema as Rust structs, detect schema drift, and generate migrations automatically.

## Features

- **Schema as code** — Define tables as Rust structs with facet attributes
- **Schema diffing** — Detect differences between your code and your database
- **Generated migrations** — Auto-generate migration skeletons from diffs
- **Migrations as Rust** — Complex data migrations with real logic, not just SQL
- **No syn dependency** — Uses unsynn for fast proc macro compilation

## Quick Start

Define your schema:

```rust
use dibs::prelude::*;
use facet::Facet;

#[derive(Facet)]
#[facet(dibs::table = "users")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,
    
    #[facet(dibs::unique)]
    pub email: String,
    
    pub name: String,
    
    #[facet(dibs::fkey = tenants::id)]
    pub tenant_id: i64,
}
```

Diff against your database:

```
$ dibs diff
Changes detected:

  users:
    + email_normalized: TEXT (nullable)
    ~ name: VARCHAR(100) -> TEXT
```

Generate and run migrations:

```
$ dibs generate add-email-normalized
Created: migrations/2026-01-17-add-email-normalized.rs

$ dibs migrate
Applied 2026-01-17-add-email-normalized (32ms)
```

## Why Rust Migrations?

Because sometimes you need to do more than just DDL:

```rust
#[dibs::migration("2026-01-17-normalize-emails")]
async fn normalize_emails(ctx: &mut MigrationContext) -> Result<()> {
    ctx.execute("ALTER TABLE users ADD COLUMN email_normalized TEXT").await?;
    
    // Backfill in batches (don't lock the table)
    ctx.backfill(|tx| async move {
        tx.execute(
            "UPDATE users SET email_normalized = LOWER(TRIM(email)) 
             WHERE email_normalized IS NULL LIMIT 1000", &[]
        ).await
    }).await?;
    
    ctx.execute(
        "ALTER TABLE users ADD CONSTRAINT users_email_normalized_unique 
         UNIQUE (email_normalized)"
    ).await?;
    
    Ok(())
}
```

## Links

- [GitHub](https://github.com/bearcove/dibs)
- [crates.io](https://crates.io/crates/dibs)
- [docs.rs](https://docs.rs/dibs)
