+++
title = "Ecosystem"
weight = 1
insert_anchor_links = "heading"
+++

One `#[derive(Facet)]` describes your type once. Everything below reads that
description to do something useful — serialize it, diff it, generate a schema,
build a CLI, pretty-print it. You almost never depend on these crates directly:
you derive `Facet`, add the crate that does the job, and call it.

This page is the map. Crate names link to **docs.rs**; the **Source** column
points at the repository.

> Looking for *which standard/third-party Rust types already implement `Facet`*
> (`Uuid`, `DateTime`, `Utf8PathBuf`, …)? That's the
> [Type Support](@/guide/type-support.md) page.

## Core & reflection

The foundation. `facet` is the one crate every user depends on; the rest is the
machinery it re-exports.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet`](https://docs.rs/facet) | The umbrella crate — `#[derive(Facet)]` and the `Facet` trait. Start here. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-core`](https://docs.rs/facet-core) | `Shape` metadata, the `Def` tree, type-erased pointers. The vocabulary everything else speaks. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-reflect`](https://docs.rs/facet-reflect) | Build and read values of arbitrary shapes at runtime, safely — `Peek` and `Partial`. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-macros`](https://docs.rs/facet-macros) | The derive macro itself, powered by [unsynn](https://docs.rs/unsynn) for fast compiles. | [facet-rs/facet](https://github.com/facet-rs/facet) |

## Data formats

Serialize and deserialize derived types. Same type, any format — pick the crate,
call `to_string` / `from_str`.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-json`](https://docs.rs/facet-json) | JSON, with a tiered JIT deserializer. The flagship — [start here](@/guide/json.md). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-json) |
| [`facet-toml`](https://docs.rs/facet-toml) | TOML serialization and deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-toml) |
| [`facet-yaml`](https://docs.rs/facet-yaml) | YAML serialization and deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-yaml) |
| [`facet-msgpack`](https://docs.rs/facet-msgpack) | MessagePack binary format. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-msgpack) |
| [`facet-postcard`](https://docs.rs/facet-postcard) | Postcard binary format, with tiered JIT deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-postcard) |
| [`facet-csv`](https://docs.rs/facet-csv) | CSV serialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-csv) |
| [`facet-asn1`](https://docs.rs/facet-asn1) | ASN.1 DER/BER serialization and deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-asn1) |
| [`facet-xdr`](https://docs.rs/facet-xdr) | XDR binary format serialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-xdr) |
| [`facet-urlencoded`](https://docs.rs/facet-urlencoded) | `application/x-www-form-urlencoded` form data. | [facet-rs/facet](https://github.com/facet-rs/facet) |

### The XML family

XML uses a tree (DOM) architecture rather than streaming, so it lives in its own
workspace alongside formats built on top of it.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-xml`](https://docs.rs/facet-xml) | XML serialization and deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-xml) |
| [`facet-dom`](https://docs.rs/facet-dom) | Tree-based (DOM) deserializer shared by HTML and XML. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-dom) |
| [`facet-svg`](https://docs.rs/facet-svg) | Strongly-typed SVG documents on top of `facet-xml`. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-svg) |
| [`facet-atom`](https://docs.rs/facet-atom) | Atom Syndication Format (RFC 4287) types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-atom) |

## Schema & code generation

Project your Rust types into other type systems — keep a frontend, an API
contract, or another language in sync from one source of truth.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-typescript`](https://docs.rs/facet-typescript) | Generate TypeScript type definitions. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-typescript) |
| [`facet-zod`](https://github.com/facet-rs/facet/tree/main/facet-zod) | Generate [Zod](https://zod.dev) schemas (runtime validation + inferred TS types). *Unreleased — landing on crates.io soon.* | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-zod) |
| [`facet-json-schema`](https://docs.rs/facet-json-schema) | Generate JSON Schema documents. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-json-schema) |
| [`facet-python`](https://docs.rs/facet-python) | Generate Python type definitions. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-python) |

See the [Schema codegen guide](@/guide/schema-codegen.md) for a full-stack workflow.

## Diagnostics & derive plugins

Day-to-day ergonomics: better output, better errors, less boilerplate.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-pretty`](https://docs.rs/facet-pretty) | Colored, structured pretty-printing with sensitive-field redaction. [Guide](@/guide/pretty-printing.md). | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`rediff`](https://docs.rs/rediff) | Structural diff and pretty assertions for any `Facet` type — no `PartialEq` required. | [bearcove/rediff](https://github.com/bearcove/rediff) |
| [`facet-default`](https://docs.rs/facet-default) | Derive `Default` with per-field custom defaults. [Guide](@/guide/facet-default.md). | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-error`](https://docs.rs/facet-error) | A `thiserror` replacement — derive `Error` from doc comments. [Guide](@/guide/facet-error.md). | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-validate`](https://docs.rs/facet-validate) | Validation attributes checked during deserialization. [Guide](@/guide/facet-validate.md). | [facet-rs/facet](https://github.com/facet-rs/facet) |

## Configuration & CLI

| Crate | What it does | Source |
|-------|--------------|--------|
| [`figue`](https://docs.rs/figue) | Type-safe CLI args, environment variables, and config files in one layered model. [Guide](@/guide/cli.md). | [bearcove/figue](https://github.com/bearcove/figue) |
| [`facet-cargo-toml`](https://docs.rs/facet-cargo-toml) | A fully-typed `Cargo.toml` / `Cargo.lock` parser. [Guide](@/ecosystem/facet-cargo-toml/_index.md). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-cargo-toml) |
| [`rediff`](https://docs.rs/rediff) | Structural diffs and assertions for Facet values. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/rediff) |

## Database

| Crate | What it does | Source |
|-------|--------------|--------|
| [`rusqlite-facet`](https://docs.rs/rusqlite-facet) | Bind query parameters and map SQLite rows using Facet reflection. [Guide](@/ecosystem/rusqlite-facet/_index.md). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/rusqlite-facet) |

## Web & UI

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-axum`](https://docs.rs/facet-axum) | [axum](https://docs.rs/axum) extractors and responses backed by facet instead of serde. [Guide](@/ecosystem/facet-axum/_index.md). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-axum) |
| [`facet-egui`](https://docs.rs/facet-egui) | An [egui](https://www.egui.rs) inspector/editor widget for any `Facet` type — live, type-driven UI straight from a `Shape`. *Community-maintained.* | [Erik1000/facet-egui](https://github.com/Erik1000/facet-egui) |

## Building blocks

Lower-level pieces you'll meet when writing your own format crate or tooling.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-value`](https://docs.rs/facet-value) | A memory-efficient dynamic value type — JSON-like data plus bytes. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-value) |
| [`facet-solver`](https://docs.rs/facet-solver) | Constraint solver that resolves type shapes from field names. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-path`](https://docs.rs/facet-path) | Path tracking for navigating nested `Facet` structures. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`strid`](https://docs.rs/strid) | Strongly-typed string identifiers with Facet integration. [Guide](@/ecosystem/strid/_index.md). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/strid) |

Writing a new format crate? The [Extend](@/extend/_index.md) section walks
through `Peek`, `Partial`, and the format-crate architecture.

---

Building something facet-adjacent? Open a PR against the
[website](https://github.com/facet-rs/facet/tree/main/docs) and we'll add it to
the map.
