+++
title = "Guide"
sort_by = "weight"
weight = 1
insert_anchor_links = "heading"
+++

Task-oriented documentation. Each page walks you through a workflow or explains concepts in context. Read sequentially or jump to the topic you need.

New here? Start with [Getting Started](@/guide/getting-started.md), then [Why facet?](@/guide/why.md).

**How-to, by task:**

- [JSON](@/guide/json.md) — serialize and deserialize with span-aware errors
- [CLI & config](@/guide/cli.md) — typed args, env vars, and config files with figue
- [Pretty-printing](@/guide/pretty-printing.md) — readable, redacted, colored output
- [Custom defaults](@/guide/facet-default.md) — per-field defaults with `facet-default`
- [Error types](@/guide/facet-error.md) — `thiserror`-style errors from doc comments
- [Validation](@/guide/facet-validate.md) — reject bad data during deserialization
- [Schema & code generation](@/guide/schema-codegen.md) — TypeScript, Zod, JSON Schema
- [Type Support](@/guide/type-support.md) — third-party types that already implement `Facet`

For the full constellation of crates, see the [Ecosystem map](@/ecosystem/_index.md).
