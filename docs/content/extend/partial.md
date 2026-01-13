+++
title = "Partial"
weight = 5
insert_anchor_links = "heading"
+++

`Partial` builds values incrementally. It has two frame-management modes:

- **Strict (default):** A frame must be fully initialized before you `end()` it. Best for inputs where nested data is contiguous (JSON/YAML objects).
- **Deferred:** Frames can be popped and stored, then re-entered later. Validation happens when you call `finish_deferred()`. Use this when nested fields arrive out of order or interleaved.

## When to use deferred
- TOML dotted keys: `inner.x = 1`, later `inner.y = 3`.
- Flattened structs where child fields are mixed with parent fields.
- Any format that lets you jump in and out of nested paths.

## API sketch

```rust,noexec
use facet_reflect::{Partial, Resolution};
use facet_solver::ResolutionBuilder; // or other way to obtain Resolution

let resolution: Resolution = /* describe the shape (fields, paths) */;

let partial = Partial::alloc::<Config>()?
    .begin_deferred()?   // enter deferred mode
    .begin_field("inner")?
        .set_field("x", 1i32)?
    .end()?                        // frame stored, not yet validated
    .set_field("count", 2u32)?
    .begin_field("inner")?         // re-enter; restores stored frame
        .set_field("y", 3i32)?
    .end()?
    .finish_deferred()?            // validates all stored frames
    .build()?;                     // produce the final value
```

## Rules of thumb
- Call `begin_deferred(resolution)` once; it errors if already in deferred mode.
- Always call `finish_deferred()` before `build()`; missing fields are validated there.
- Paths are tracked relative to where deferred began; re-entering a path restores its stored frame and state.
- Strict mode remains the safest default; prefer deferred only when the input order demands it.

More examples coming soon (collections, enums, nested flatten).
