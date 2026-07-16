+++
title = "Setting up your -db crate"
description = "Define schema and migrations as a library linked by your application"
weight = 1
+++

The **-db crate** is the library where database schema and migrations live. It
is not a deployment executable. Your application links it and exposes explicit
execution modes for serving, migration, and optional Dibs tooling.

## Workspace structure

```text
my-app/
  .config/dibs.styx    # local Dibs tooling endpoint
  crates/
    my-app-db/         # schema + migrations
    my-app-queries/    # generated typed queries
    my-app/            # the only application binary
```

## Create the library

```bash
cargo new --lib crates/my-app-db
```

Add Dibs and Facet to `crates/my-app-db/Cargo.toml`:

```toml
[dependencies]
dibs = { git = "https://github.com/facet-rs/facet", branch = "main" }
facet = { git = "https://github.com/facet-rs/facet", branch = "main" }
```

Define a real symbol that the application can call:

```rust
// crates/my-app-db/src/lib.rs
pub fn ensure_linked() {}
```

The call forces the linker to retain the table and migration inventory from
the library. Merely naming one of its types through `type_name` or `TypeId` does
not retain inventory submissions.

## Link it into the application

```toml
# crates/my-app/Cargo.toml
[dependencies]
my-app-db = { path = "../my-app-db" }
dibs = { git = "https://github.com/facet-rs/facet", branch = "main" }
```

The application calls `my_app_db::ensure_linked()` and implements at least two
modes:

- `serve` starts the application;
- `migrate` connects to Postgres and calls `dibs::MigrationRunner` directly.

The [production deployment guide](deployment/) shows the full shape. Both
modes run from the same binary and image.

## Configure local tooling

The Dibs TUI and schema-authoring commands use an explicitly enabled endpoint
inside the application. Configure where the CLI should connect:

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
    endpoint "127.0.0.1:7764"
}
```

Add a development-only application mode that calls
`dibs::serve("127.0.0.1:7764".parse()?).await`. Start that mode yourself, then
run `dibs schema`, `dibs diff`, or the TUI. The CLI never searches `PATH`, runs
Cargo, or launches an application binary on your behalf.

## Set up the database

Set `DATABASE_URL` in your development environment. Dibs reads it for commands
that compare with or migrate a live database.

```bash
export DATABASE_URL=postgres://user:pass@localhost/mydb
```

On macOS, [Postgres.app](https://postgresapp.com/) is a convenient local
Postgres installation.
