+++
title = "Setting up your -queries crate"
description = "Configure the queries workspace"
weight = 6
+++

The **-queries crate** is where you write Styx query definitions and generate typed Rust query functions.

This is optional — you can use raw SQL with `tokio-postgres` if you prefer. But the queries crate gives you:

- Type-safe query parameters and results
- <abbr title="Language Server Protocol">LSP</abbr> support (completions, go-to-definition, diagnostics)
- Automatic SQL generation from Styx definitions

## Create the crate

```bash
cargo new --lib crates/my-app-queries
```

Add dependencies to `crates/my-app-queries/Cargo.toml`:

```toml
[dependencies]
my-app-db = { path = "../my-app-db" }
# See db-crate setup for why we use git deps
dibs-runtime = { git = "https://github.com/facet-rs/facet", branch = "main" }

[build-dependencies]
dibs = { git = "https://github.com/facet-rs/facet", branch = "main" }
my-app-db = { path = "../my-app-db" }
```

## Set up codegen

Create `crates/my-app-queries/build.rs`:

```rust
fn main() {
    // Force the linker to include my_app_db's inventory submissions.
    // This MUST be a real symbol reference (a function call). A
    // `std::any::TypeId::of::<my_app_db::User>()` or `type_name` reference is a
    // const intrinsic and does NOT pull the crate's `inventory::submit!` statics
    // into the build — the schema would come back empty and codegen would fall
    // back to wrong column types.
    my_app_db::ensure_linked();

    // Collects the schema from inventory, parses the query file, generates the
    // Rust code, and writes it to OUT_DIR. Panics with a helpful message if the
    // schema is empty (i.e. you forgot the `ensure_linked()` call above).
    dibs::build_queries(".dibs-queries/queries.styx");
}
```

Your `my-app-db` crate needs to expose `ensure_linked()` — an empty `pub fn`
whose call forces the linker to keep the crate's table submissions:

```rust
// in my-app-db/src/lib.rs
/// Call this from build scripts so the linker keeps this crate's
/// `#[facet(dibs::table)]` inventory submissions.
pub fn ensure_linked() {}
```

Create `crates/my-app-queries/src/lib.rs`:

```rust
include!(concat!(env!("OUT_DIR"), "/queries.rs"));
```

## Create the queries file

Create `.dibs-queries/queries.styx` at the workspace root:

```styx
@schema {id crate:dibs-queries@1, cli dibs}

# Queries will go here
```

## Verify the setup

```bash
cargo build -p my-app-queries
```

It should compile successfully (with no queries defined yet).
