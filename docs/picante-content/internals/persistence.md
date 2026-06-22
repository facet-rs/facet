+++
title = "Persistence"
weight = 13
+++

This page documents picante’s cache persistence layer: what’s stored, how it’s encoded, and the operational knobs for controlling size and corruption handling.

Primary implementation: `crates/picante/src/persist.rs`.

## What persistence stores

A cache file contains:

- a format version
- the runtime’s `current_revision`
- a list of per-ingredient sections (inputs, derived, interned)

Each section is keyed by:

- stable kind id (`QueryKindId` as `u32`)
- kind name (for mismatch detection)
- section type (`Input`, `Derived`, `Interned`)

Each section contains opaque records owned by the ingredient; each record is a `facet-postcard` blob.

### What is *not* stored

Cache files store ingredient state, not “the whole database struct”. In particular:

- custom fields on your db struct (from `#[picante::db]`) are not part of persistence
- the runtime’s dependency graph is not stored as a separate section; it is reconstructed by ingredients during `restore_runtime_state(...)`

## Public API

- `save_cache(path, runtime, ingredients)`
- `load_cache(path, runtime, ingredients) -> PicanteResult<bool>`

Options:

- `CacheSaveOptions` (`max_bytes`, `max_records_per_section`, `max_record_bytes`)
- `CacheLoadOptions` (`max_bytes`, `on_corrupt: OnCorruptCache`)

Corruption policy:

- `OnCorruptCache::Error`
- `OnCorruptCache::Ignore`
- `OnCorruptCache::Delete`

## Example

```rust,noexec
use picante::persist::{load_cache_with_options, save_cache_with_options, CacheLoadOptions, CacheSaveOptions, OnCorruptCache};

// Save with a best-effort size cap.
save_cache_with_options(
    "path/to/picante.bin",
    db.runtime(),
    db.ingredient_registry().persistable_ingredients().as_slice(),
    &CacheSaveOptions {
        max_bytes: Some(10 * 1024 * 1024),
        max_records_per_section: None,
        max_record_bytes: Some(256 * 1024),
    },
).await?;

// Load, deleting corrupt caches automatically.
let loaded = load_cache_with_options(
    "path/to/picante.bin",
    db.runtime(),
    db.ingredient_registry().persistable_ingredients().as_slice(),
    &CacheLoadOptions {
        max_bytes: Some(20 * 1024 * 1024),
        on_corrupt: OnCorruptCache::Delete,
    },
).await?;
```

## Save semantics

`save_cache_with_options(...)`:

- calls `ensure_unique_kinds(...)` to reject duplicate kind ids in the passed ingredient list
- calls each ingredient’s `save_records().await` and packs the resulting blobs into `Section { kind_id, kind_name, section_type, records }`
- applies best-effort size limiting (record dropping / truncation) based on `CacheSaveOptions`
- writes to a `*.tmp` file then `rename`s it into place (best-effort atomic replace)

## Load semantics

`load_cache_with_options(...)` returns:

- `Ok(false)` when the file does not exist
- `Ok(false)` when the file is corrupt and `on_corrupt` is `Ignore` or `Delete`
- `Err(...)` when the file is corrupt and `on_corrupt` is `Error`

Validation behavior:

- `format_version` must match
- each cache section must match a provided ingredient by `kind_id`
  - unknown sections are ignored (warning)
- for known sections, `kind_name` must match exactly (mismatch is an error)
- for known sections, `section_type` must match exactly (mismatch is an error)

Load order (important):

1. `runtime.clear_dependency_graph()`
2. `ingredient.clear()` for every provided ingredient (prevents blending partial state)
3. `ingredient.load_records(...)` for each section found
4. `ingredient.restore_runtime_state(runtime).await` for every ingredient (rebuilds reverse deps, etc.)
5. `runtime.set_current_revision(Revision(cache.current_revision))`

## Restoring runtime-derived state

After ingredients load their records, they may rebuild runtime-side state derived from those records via `PersistableIngredient::restore_runtime_state(...)` (for example, restoring reverse deps so invalidation events work immediately after load).
