+++
title = "Implementing Facet for third-party types"
weight = 6
insert_anchor_links = "heading"
+++

This guide is for contributing `Facet` implementations to the facet repository. If you just want to use a type that doesn't implement `Facet`, see [When a type doesn't implement Facet](@/guide/ecosystem.md#when-a-type-doesn-t-implement-facet).

## Why we implement from the facet side

In Rust, you can only implement a trait in one of two places:
1. The crate that defines the trait
2. The crate that defines the type

Ideally, crates like `chrono` or `uuid` would implement `Facet` for their types directly. But facet isn't stable yet — the `Facet` trait and `Shape` structure are still evolving.

So we implement `Facet` for third-party types from the facet side, using optional features in `facet-core` (re-exported through `facet`). When facet stabilizes, crate authors can implement `Facet` themselves, and we'll deprecate our implementations.

## Adding support for a new crate

1. Add the dependency to `facet-core/Cargo.toml`:
   ```toml
   [dependencies]
   my-crate = { version = "1.0", optional = true }

   [features]
   my-crate = ["dep:my-crate"]
   ```

2. Create `facet-core/src/impls/crates/my_crate.rs`

3. Add to `facet-core/src/impls/crates/mod.rs`:
   ```rust,noexec
   #[cfg(feature = "my-crate")]
   mod my_crate;
   ```

4. Re-export the feature from `facet/Cargo.toml`:
   ```toml
   [features]
   my-crate = ["facet-core/my-crate"]
   ```

## Implementing Facet

Most third-party types are scalars (atomic values like UUIDs, timestamps, paths).

Look at existing implementations in `facet-core/src/impls/crates/` for patterns:
- `uuid.rs` — simple scalar
- `chrono.rs` — multiple related types
- `camino.rs` — path types with borrowed variants
- `bytes.rs` — byte buffer types

## Testing

Add tests in the same file or create a test file. Make sure to test:
- Round-trip through at least one format (JSON is easiest)
- Edge cases for the type (empty values, max values, etc.)
