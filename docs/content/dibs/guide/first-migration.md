+++
title = "Your first migration"
description = "Generate and run migrations"
weight = 4
+++

Now that you've defined a table, let's generate a migration to create it in the database.

Migrations in dibs are **generated Rust files** based on schema changes you make to your structs.

## Generate the migration

Since you defined a `User` table but haven't created it in the database yet, dibs will detect this difference:

```bash
dibs diff
```

You should see output showing that the `users` table needs to be created.

Now generate a migration from that diff:

```bash
dibs generate-from-diff create-users
```

This creates a new `.rs` file in `crates/my-app-db/src/migrations/` with the SQL already written. It also prints the `mod ...;` line you need to add to `migrations/mod.rs`.

## Set up the migrations module

Create `crates/my-app-db/src/migrations/mod.rs`:

```rust
pub mod m2026_01_24_120000_create_users;
```

(Use the actual module name that `generate-from-diff` printed)

Then add this to `crates/my-app-db/src/lib.rs`:

```rust
pub mod migrations;
```

## The generated migration

The generated migration file looks like this:

```rust
use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute("CREATE TABLE users (
        id BIGINT PRIMARY KEY,
        email TEXT UNIQUE NOT NULL,
        display_name TEXT NOT NULL
    )").await?;
    Ok(())
}
```

## Run the migration

```bash
dibs migrate
```

This applies the migration to your database. You can verify it worked with:

```bash
dibs status
```

## Adding data migrations

Since migrations are Rust functions, you can add backfills, data transformations, or any logic you need:

```rust
#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    // Schema change
    ctx.execute("ALTER TABLE users ADD COLUMN bio TEXT").await?;

    // Data backfill
    ctx.execute("UPDATE users SET bio = 'No bio yet' WHERE bio IS NULL").await?;

    Ok(())
}
```

For large backfills, prefer batching so you don't lock tables:

```rust
loop {
    let rows_affected = ctx.execute(
        "UPDATE users SET bio = 'No bio yet'
         WHERE bio IS NULL
         LIMIT 1000"
    ).await?;

    if rows_affected == 0 {
        break;
    }
}
```

## Running migrations

```bash
export DATABASE_URL=postgres://user:pass@localhost/mydb
dibs migrate
dibs status
```

## Creating a blank migration

If you need a data-only migration (no schema changes), you can create an empty skeleton:

```bash
dibs generate backfill-user-data
```
