+++
title = "Setting up your -queries crate"
description = "Configure the queries workspace"
weight = 6
+++

The **-queries crate** is where you write Styx query definitions and generate typed Rust query functions.

This is optional — you can use raw SQL with `tokio-postgres` if you prefer. But the queries crate gives you:

- Type-safe query parameters and results
- LSP support (completions, go-to-definition, diagnostics)
- Automatic SQL generation from Styx definitions

## Create the crate

```bash
cargo new --lib crates/my-app-queries
```

Add dependencies to `crates/my-app-queries/Cargo.toml`:

```toml
[dependencies]
my-app-db = { path = "../my-app-db" }
dibs-runtime = { git = "https://github.com/bearcove/dibs", branch = "main" }

[build-dependencies]
dibs = { git = "https://github.com/bearcove/dibs", branch = "main" }
my-app-db = { path = "../my-app-db" }
```

## Set up codegen

Create `crates/my-app-queries/build.rs`:

```rust
use dibs::{parse_query_file, generate_rust_code_with_planner};
use std::{env, fs, path::Path};

fn main() {
    println!("cargo::rerun-if-changed=.dibs-queries/queries.styx");

    // Force the linker to include my_app_db's inventory submissions
    // by referencing a type from the crate
    let _ = std::any::TypeId::of::<my_app_db::User>();

    // Collect schema from registered tables via inventory
    let schema = dibs::Schema::collect();

    // Parse and generate
    let queries_path = Path::new(".dibs-queries/queries.styx");
    let source = fs::read_to_string(queries_path)
        .expect("Failed to read .dibs-queries/queries.styx");

    let file = parse_query_file(&source)
        .expect("Failed to parse .dibs-queries/queries.styx");

    let generated = generate_rust_code_with_planner(&file, &schema, None);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("queries.rs");

    fs::write(&dest_path, &generated.code)
        .expect("Failed to write generated queries.rs");
}
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
