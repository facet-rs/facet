+++
title = "Setting up your -db crate"
description = "Create the schema crate and configure dibs"
weight = 1
+++

The **-db crate** is where you define your database schema as Rust structs. dibs reads this crate to generate migrations and power the <abbr title="Text User Interface">TUI</abbr>.

## Workspace structure

A typical dibs workspace looks like this:

```
my-app/
  .config/dibs.styx    # dibs configuration
  crates/
    my-app-db/         # schema + migrations (this guide)
    my-app-queries/    # query definitions (covered later)
    my-app/            # your application
```

## Create the -db crate

```bash
cargo new --lib crates/my-app-db
```

Add dependencies to `crates/my-app-db/Cargo.toml`:

```toml
[dependencies]
# dibs/facet are currently developed rapidly; for now, the recommended setup is to use git deps:
dibs = { git = "https://github.com/facet-rs/facet", branch = "main" }
facet = { git = "https://github.com/facet-rs/facet", branch = "main" }

[[bin]]
name = "my-app-db"
path = "src/main.rs"
```

## Add the service binary

Create `crates/my-app-db/src/main.rs`:

```rust
fn main() {
    // Force the linker to keep this crate's table submissions. Must be a real
    // symbol reference (a function call) — a `type_name`/`TypeId::of` reference
    // is a const intrinsic and does NOT link the crate's inventory statics, so
    // dibs would discover zero tables.
    my_app_db::ensure_linked();

    dibs::run_service();
}
```

This binary is spawned by the dibs CLI to answer schema requests. You don't run it directly.

The `ensure_linked()` call ensures your table types are included in the binary so dibs can discover them via inventory. Define it once in your crate as an empty `pub fn ensure_linked() {}`.

## Configure dibs

Create `.config/dibs.styx` at the workspace root:

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
}
```

This tells dibs which crate contains your schema.

## Set up the database

Create a `.env` file at the workspace root (and add it to `.gitignore`):

```
DATABASE_URL=postgres://user:pass@localhost/mydb
```

dibs reads this when connecting to your database for migrations and diffs.

On macOS, [Postgres.app](https://postgresapp.com/) is a nice way to run Postgres locally.

## Verify the setup

```bash
dibs schema
```

You should see "No tables defined" since we haven't created any yet.
