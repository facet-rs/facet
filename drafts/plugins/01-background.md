# Background

## Problem: Extensibility without Double-Parsing

Facet wants to support extensions like `facet-error` (thiserror replacement), `facet-builder`,
`facet-partial`, etc. The challenge: how do these extensions generate code without each one
parsing the struct independently?

## Failed Approach: Shared Dylib Registry (PR #1143)

The idea was to have proc-macros share state via a common dylib dependency. This failed because:

> "There is no guarantee on proc-macro load order. Rust-analyzer loads them in whatever order,
> while rustc is currently working on parallelization of macro expansion."
> — @bjorn3

## Promising Approach: Macro Expansion Handshake

[proc-macro-handshake](https://github.com/alexkirsz/proc-macro-handshake) demonstrates a
different pattern: use macro expansion itself as the communication channel.

The key insight: instead of sharing runtime state, we chain macro expansions:

```
register!(::plugin)
    → plugin::instantiate!(::plugin, ::registry)
        → registry::register_plugin!(::plugin, "data")
            → stores in static REGISTRY
```

This is deterministic because expansion order follows the call graph.
