# facet-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-diff.svg)](https://crates.io/crates/facet-diff)
[![documentation](https://docs.rs/facet-diff/badge.svg)](https://docs.rs/facet-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Provides diffing capabilities for Facet types.

## Architecture

facet-diff computes structural differences between two values using a multi-phase
pipeline. Here's how it works, using HTML diffing as a concrete example.

### Example: Diffing HTML Documents

```text
Old HTML: <div>hello<p>world</p></div>
New HTML: <div><p>WORLD</p></div>

Expected patches:
1. Delete text node "hello"
2. Update text "world" → "WORLD"
```

### Phase 1: Tree Building (`build_tree`)

Both values are converted to generic trees using `Peek` (facet's reflection API).

```text
Input:  Peek<Html>  ──────►  Tree<NodeKind, NodeLabel>
```

Each tree node contains:
- **hash**: Content hash (for detecting identical subtrees)
- **kind**: Type info like `Struct("Div")`, `List("children")`, `Scalar("String")`
- **label**: Path from root (e.g., `body.children[0].Div.children[1]`)

The hash captures content identity, the kind enables structural matching,
and the path allows navigating back to the original value.

### Phase 2: Tree Matching (cinereus)

The [cinereus](https://docs.rs/cinereus) crate implements the GumTree algorithm
to find which nodes in tree A correspond to which nodes in tree B.

```text
Input:  Tree A, Tree B  ──────►  Matching (A↔B node pairs)
```

Two-phase matching:
1. **Top-down**: Match subtrees with identical hashes (exact matches)
2. **Bottom-up**: Match remaining nodes by kind + position (structural matches)

### Phase 3: Edit Script Generation (cinereus/chawathe)

Given the matching, generate a minimal edit script using the Chawathe algorithm.

```text
Input:  Tree A, Tree B, Matching  ──────►  Vec<CinereusEditOp>
```

Operations reference NodeIds (valid only while both trees exist):
- `Update { node_a, node_b }` - Content changed
- `Insert { node_b, parent_b, position }` - New node
- `Delete { node_a }` - Node removed
- `Move { node_a, node_b, new_parent, new_position }` - Node relocated

### Phase 4: Path Translation (`convert_ops_with_shadow`)

Convert cinereus ops (with NodeIds) to facet-diff's public `EditOp` (with paths).

```text
Input:  CinereusEditOp[], Tree A, Tree B, Peek A, Peek B
Output: Vec<EditOp>  (self-contained, no tree references)
```

Uses a "shadow tree" that starts as a clone of tree A and gets modified as
operations are processed. This ensures paths account for structural changes
(insertions shift indices, deletions shift indices, etc.).

**Critical**: `EditOp::Update` must include the actual new value (not just a hash),
because consumers won't have access to the original trees.

### Phase 5: Consumer Application

Consumers receive `Vec<EditOp>` and translate to domain-specific patches.

```text
Input:  Vec<EditOp>  ──────►  Domain patches (e.g., DOM mutations)
```

For HTML: `EditOp::Update` on a text node → `SetText { path, value }`

The consumer does NOT have access to the original values. Each `EditOp` must
be self-contained with all information needed to apply it.

### Data Flow Summary

```text
┌─────────────────────────────────────────────────────────────────────┐
│  Peek A, Peek B  (original values with full data)                   │
└──────────────┬──────────────────────────────────────────────────────┘
               │ build_tree()
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Tree A, Tree B  (hashes + kinds + paths, no values)                │
└──────────────┬──────────────────────────────────────────────────────┘
               │ compute_matching() + generate_edit_script()
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  CinereusEditOp[]  (references NodeIds, needs both trees)           │
└──────────────┬──────────────────────────────────────────────────────┘
               │ convert_ops_with_shadow(ops, trees, peeks)
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  EditOp[]  (self-contained with paths + values)                     │
└──────────────┬──────────────────────────────────────────────────────┘
               │ consumer translation
               ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Domain patches  (e.g., DOM mutations, SQL updates, etc.)           │
└─────────────────────────────────────────────────────────────────────┘
```

### Key Invariant

**EditOp must be self-contained.** After phase 4, the original `Peek` values
are no longer available. Each operation must include everything needed to
apply it:

- `Update`: path + **new value** (not just hash!)
- `Insert`: path + **value to insert**
- `Delete`: path (value not needed)
- `Move`: old_path + new_path

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
