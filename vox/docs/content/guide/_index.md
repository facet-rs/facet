+++
title = "Guide"
description = "Rust implementation guide"
weight = 2
+++

This section covers the **Rust implementation** of rapace—the reference implementation that defines services and types which code generators then use to produce bindings for other languages.

For the formal protocol definition, see the [Specification](/spec/).

## Using rapace in Rust

- [Architecture](architecture.md) — frames, sessions, transports, and how they fit together
- [Design notes](design.md) — invariants and internal constraints
- [Zero-copy deserialization](zero-copy.md) — borrowing data directly from shared memory frames

## Cells (plugin architecture)

- [Cells](cells.md) — building isolated plugin processes with `rapace-cell`
- [Cell Lifecycle](cell-lifecycle.md) — detecting cell death and automatic relaunching

## Background

- [Motivation](motivation.md) — why rapace exists (dodeca's plugin system)
- [Comparisons](comparisons.md) — how this relates to gRPC, Cap'n Proto, etc.

For API details, see the [crate documentation on docs.rs](https://docs.rs/rapace).
