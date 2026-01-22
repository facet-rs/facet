# facet-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-diff.svg)](https://crates.io/crates/facet-diff)
[![documentation](https://docs.rs/facet-diff/badge.svg)](https://docs.rs/facet-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-diff

Structural diffing for Facet types using the GumTree/Chawathe algorithm.

## Overview

facet-diff computes the minimal edit script to transform one value into another.
It works on any type that implements `Facet`, using reflection to traverse the
structure without requiring manual diff implementations.

```rust,ignore
use facet_diff::tree_diff;

let old = MyStruct { name: "alice", count: 1 };
let new = MyStruct { name: "alice", count: 2 };

let ops = tree_diff(&old, &new);
// ops contains Update for the count field
```

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│  Peek A, Peek B  (original values with full data)                   │
└──────────────┬──────────────────────────────────────────────────────┘
               │ build_tree()
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Tree A, Tree B  (hashes + kinds + paths, no values)                │
└──────────────┬──────────────────────────────────────────────────────┘
               │ cinereus: compute_matching() + generate_edit_script()
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  CinereusEditOp[]  (references NodeIds, needs both trees)           │
└──────────────┬──────────────────────────────────────────────────────┘
               │ convert_ops_with_shadow()
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  EditOp[]  (self-contained with paths + values)                     │
└──────────────┬──────────────────────────────────────────────────────┘
               │ consumer translation (e.g., facet-html-diff)
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Domain patches  (DOM mutations, database updates, etc.)            │
└─────────────────────────────────────────────────────────────────────┘
```

## Pipeline Phases

### 1. Tree Building

Both values are converted to generic trees using `Peek` (facet's reflection API).
Each node contains a content hash for identity, a kind for structural matching,
and a path for navigation back to the original value.

### 2. Tree Matching (cinereus)

The [cinereus](../cinereus) crate implements the GumTree algorithm:
- **Top-down**: Match subtrees with identical hashes (exact matches)
- **Bottom-up**: Match remaining nodes by kind + Dice similarity

### 3. Edit Script Generation

Given the matching, the Chawathe algorithm generates a minimal edit script.
Operations include Update, Insert, Delete, and Move.

**Important**: Insert and Move follow [Chawathe semantics][chawathe] - they
displace existing nodes into slots rather than shifting siblings. See
[`cinereus/CHAWATHE_SEMANTICS.md`](../cinereus/CHAWATHE_SEMANTICS.md) for details.

[chawathe]: https://dl.acm.org/doi/10.1145/235968.233366

### 4. Path Translation

Cinereus operations reference NodeIds (only valid while both trees exist).
This phase converts them to self-contained `EditOp` values with paths and
actual values, using a "shadow tree" to track structural changes.

## Key Invariant

**EditOp must be self-contained.** After path translation, the original values
are no longer available. Each operation includes everything needed to apply it:

- `Update`: path + new value
- `Insert`: path + value to insert  
- `Delete`: path
- `Move`: old path + new path

## Example: HTML Diffing

```text
Old: <div>hello<p>world</p></div>
New: <div><p>WORLD</p></div>

Pipeline:
1. build_tree() → Tree with Div, Text("hello"), P, Text("world") nodes
2. matching → Text("world") ↔ Text("WORLD"), P ↔ P, etc.
3. edit script → Delete(Text("hello")), Update(Text → "WORLD")
4. path translation → EditOp with paths like body.children[0].Div.children[0]
5. facet-html-diff → SetText { path: [0,0], text: "WORLD" }
```

## LLM contribution policy

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
