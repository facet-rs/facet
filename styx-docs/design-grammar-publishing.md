# Grammar/Schema Publishing Design

## Goal

The `styx` CLI should be able to package a schema as a Rust crate and publish it to crates.io (or staging.crates.io for testing).

## Staging Registry

For testing, use staging.crates.io:

```bash
cargo publish --index sparse+https://index.staging.crates.io/
```

Authentication:
- Get a token from https://staging.crates.io (separate account from production)
- Pass via `--token <TOKEN>` or set `CARGO_REGISTRIES_STAGING_TOKEN`

Note: staging.crates.io has fewer crates than production, so dependencies may not exist there.

## Generated Crate Structure

```
myapp-schema/
├── Cargo.toml
├── src/
│   └── lib.rs
└── schema.styx  (optional, for reference)
```

### Cargo.toml

```toml
[package]
name = "myapp-schema"
version = "1.0.0"
edition = "2024"
license = "MIT OR Apache-2.0"
description = "Styx schema for myapp configuration"
categories = ["config"]
keywords = ["styx", "schema", "myapp"]

# No dependencies - pure data crate
```

### lib.rs

```rust
//! Styx schema for myapp configuration.
//!
//! This crate provides the schema definition for validating myapp config files.
//!
//! ## Usage
//!
//! ```rust
//! let schema = myapp_schema::SCHEMA;
//! // Pass to styx validation APIs
//! ```

/// The styx schema content.
pub const SCHEMA: &str = include_str!("../schema.styx");
```

Benefits:
- Schema file is visible on crates.io and docs.rs
- No escaping issues
- Simple API: `myapp_schema::SCHEMA`

## CLI Subcommands

### `styx @package` - Generate a publishable crate

```bash
# Generate crate in ./myapp-schema/
styx @package schema.styx --name myapp-schema --version 1.0.0

# Custom output directory
styx @package schema.styx --name myapp-schema --version 1.0.0 --output ./dist/myapp-schema

# Then user runs cargo publish themselves
cd myapp-schema && cargo publish
```

### `styx @diff` - Compare schemas for semver

Fetches the latest published version and compares against local schema:

```bash
# Compare local schema against latest published version
styx @diff schema.styx --crate myapp-schema

# Compare against specific version
styx @diff schema.styx --crate myapp-schema --baseline 1.3.0

# Use staging registry
styx @diff schema.styx --crate myapp-schema --registry staging
```

Output:

```
Comparing against myapp-schema@1.3.0...

Breaking changes (require major bump):
  - removed field `timeout` from Config

Additive changes (require minor bump):
  + added optional field `retry_count` to Config

Current version: 1.3.0
Minimum allowed: 2.0.0
Suggested: 2.0.0
```

### Diff implementation

1. **Fetch baseline**: Download crate from registry, extract `schema.styx`
2. **Parse both**: Parse into `SchemaFile` (from `styx-schema/src/types.rs`)
3. **Compute diff**: Walk both schemas, categorize changes
4. **Suggest version**: Based on current version + change category

### Change categorization rules

**Breaking changes** (major bump required):

| Change | Example |
|--------|---------|
| Remove type definition | `Config` existed, now gone |
| Remove required field | `@object{ host @string }` → `@object{}` |
| Make optional field required | `@optional(@string)` → `@string` |
| Narrow type | `@union(@int @string)` → `@int` |
| Tighten constraints | `@int` → `@int{ min 0 }` |
| Change field type incompatibly | `@string` → `@int` |
| Remove enum variant | `@enum{ a @unit, b @unit }` → `@enum{ a @unit }` |

**Additive changes** (minor bump):

| Change | Example |
|--------|---------|
| Add new type definition | New `TlsConfig` type |
| Add optional field | `@object{}` → `@object{ timeout @optional(@int) }` |
| Add field with default | `@object{}` → `@object{ port @default(8080 @int) }` |
| Widen type | `@int` → `@union(@int @string)` |
| Loosen constraints | `@int{ min 0 }` → `@int` |
| Add enum variant | `@enum{ a @unit }` → `@enum{ a @unit, b @unit }` |
| Add union member | `@union(@int)` → `@union(@int @string)` |

**Patch changes** (patch bump):

| Change | Example |
|--------|---------|
| Documentation changes | Doc comments only |
| Whitespace/formatting | No semantic change |
| Deprecation added | `@string` → `@deprecated("use X" @string)` |

### Type compatibility matrix

For determining if type A can be replaced with type B:

```
A → B compatible if:
  - A == B (identical)
  - B is @any (widens to accept anything)
  - B is @union containing A (widens)
  - B is @optional(A) (widens - adds absence case)
  - A and B are both @int/@float and B's constraints are looser
  - A and B are both @object and all of A's required fields exist in B with compatible types
```

### Registry fetching

To fetch a crate from crates.io/staging:

```bash
# Download crate tarball
curl -L "https://static.crates.io/crates/myapp-schema/1.3.0/download" -o crate.tar.gz

# For staging:
curl -L "https://static.staging.crates.io/crates/myapp-schema/1.3.0/download" -o crate.tar.gz
```

Or use `cargo download` / parse the sparse index to find latest version.

## Decisions

- **lib.rs format**: `include_str!("../schema.styx")` - keeps schema visible on crates.io/docs.rs
- **Schema file**: Included in crate root as `schema.styx`
- **Versioning**: Yes, via `styx @diff` command
- **Two-step publish**: `styx @package` generates crate, user runs `cargo publish`

## Open Questions

1. **Dependencies** - Should the generated crate depend on anything (e.g., `styx-schema` for validation)?

2. **Feature flags** - Should there be optional features like `validate` that pull in dependencies?

3. **Naming convention** - Should we enforce a suffix like `-schema` or `-styx`?

4. **Latest version discovery** - How to find the latest published version for `@diff`?
   - Parse sparse index at `https://index.crates.io/my/ap/myapp-schema`
   - Use `cargo search` output
   - Call crates.io API: `https://crates.io/api/v1/crates/myapp-schema`
