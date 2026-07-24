+++
title = "Ecosystem"
description = "A scannable map of the facet crates, tools, integrations, and adjacent projects."
weight = 1
insert_anchor_links = "heading"
+++

One `#[derive(Facet)]` describes your type once. Everything below reads that
description to do something useful: serialize it, diff it, generate a schema,
build a CLI, pretty-print it, or connect it to another system.

## Start here

- New to facet? Read the [guide](/guide/) first.
- Need JSON? Start with [`facet-json`](/facet-json/guide/).
- Looking for a crate? Use the tables below; local guides are linked whenever this repo has one.
- Looking for standard and third-party Rust types that already implement `Facet`
  (`Uuid`, `DateTime`, `Utf8PathBuf`, ...)? See [type support](/guide/type-support/).

## Core and reflection

The foundation. `facet` is the crate most users add directly; the others provide
the reflection machinery it re-exports or builds on.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet`](https://docs.rs/facet) | Umbrella crate with `#[derive(Facet)]`, the `Facet` trait, and the usual public entry points. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-core`](https://docs.rs/facet-core) | Defines `Shape`, the `Def` tree, type metadata, and pointer vocabulary for reflection. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-reflect`](https://docs.rs/facet-reflect) | Reads and builds values of arbitrary reflected shapes with `Peek` and `Partial`. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-macros`](https://docs.rs/facet-macros) | Implements the `Facet` derive macro, powered by [unsynn](https://docs.rs/unsynn). | [facet-rs/facet](https://github.com/facet-rs/facet) |

## Data formats

Serialize and deserialize derived types. Same Rust shape, different wire format.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-json`](/facet-json/guide/) | Serializes and deserializes JSON with span-aware diagnostics. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-json) |
| [`facet-toml`](https://docs.rs/facet-toml) | Serializes and deserializes TOML for Facet types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-toml) |
| [`facet-yaml`](https://docs.rs/facet-yaml) | Serializes and deserializes YAML for Facet types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-yaml) |
| [`facet-msgpack`](https://docs.rs/facet-msgpack) | Serializes and deserializes MessagePack for Facet types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-msgpack) |
| [`facet-postcard`](https://docs.rs/facet-postcard) | Serializes and deserializes Postcard for compact binary data. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-postcard) |
| [`facet-csv`](https://docs.rs/facet-csv) | Serializes rows and records as CSV. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-csv) |
| [`facet-asn1`](https://docs.rs/facet-asn1) | Serializes and deserializes ASN.1 DER/BER data. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-asn1) |
| [`facet-xdr`](https://docs.rs/facet-xdr) | Serializes and deserializes XDR binary data. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-xdr) |
| [`facet-urlencoded`](https://docs.rs/facet-urlencoded) | Parses and emits `application/x-www-form-urlencoded` form data. | [facet-rs/facet](https://github.com/facet-rs/facet) |

## XML family

XML uses a tree architecture rather than the usual streaming format path, so
these crates live together.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-xml`](https://docs.rs/facet-xml) | Serializes and deserializes XML for Facet types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-xml) |
| [`facet-dom`](https://docs.rs/facet-dom) | Provides the tree-based DOM layer shared by HTML and XML support. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-dom) |
| [`facet-svg`](https://docs.rs/facet-svg) | Models strongly typed SVG documents on top of `facet-xml`. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-svg) |
| [`facet-atom`](https://docs.rs/facet-atom) | Models Atom Syndication Format (RFC 4287) documents. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-atom) |

## Schema and code generation

Project Rust types into other type systems so frontends, APIs, and generated
clients stay aligned with the reflected source type.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-typescript`](https://docs.rs/facet-typescript) | Generates TypeScript type definitions from Facet shapes. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-typescript) |
| [`facet-zod`](https://github.com/facet-rs/facet/tree/main/facet-zod) | Generates [Zod](https://zod.dev) schemas for runtime validation and inferred TypeScript types. *Unreleased.* | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-zod) |
| [`facet-json-schema`](https://docs.rs/facet-json-schema) | Generates JSON Schema documents from Facet shapes. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-json-schema) |
| [`facet-python`](https://docs.rs/facet-python) | Generates Python type definitions from Facet shapes. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-python) |

See the [schema codegen guide](/guide/schema-codegen/) for a full-stack workflow.

## Diagnostics and derive plugins

Day-to-day ergonomics: better output, better errors, and less boilerplate.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-pretty`](/facet-pretty/guide/) | Pretty-prints Facet values with structure, color, and sensitive-field redaction. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`rediff`](/rediff/) | Diffs Facet values structurally and reports path-aware differences. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/rediff) |
| [`facet-default`](/facet-default/guide/) | Derives `Default` with per-field custom default values. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-error`](/facet-error/guide/) | Derives `Error` implementations from enum variants and doc comments. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-validate`](/facet-validate/guide/) | Runs validation attributes during deserialization. | [facet-rs/facet](https://github.com/facet-rs/facet) |

## Configuration and CLI

Turn reflected Rust types into configuration and command-line interfaces.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`figue`](/figue/guide/) | Layers CLI args, environment variables, config files, and defaults into one typed model. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/figue) |
| [`facet-styx`](https://docs.rs/facet-styx) | Serializes and deserializes Styx documents for Facet types. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-styx) |
| [`styx-cli`](https://docs.rs/styx-cli) | Provides Styx validation, formatting, schema generation, and language-server tooling. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/styx-cli) |
| [`facet-cargo-toml`](/facet-cargo-toml/) | Parses `Cargo.toml` manifests and `Cargo.lock` files into typed Rust models. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-cargo-toml) |

## Database

Move between database rows and reflected Rust types without writing the same
mapping twice.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`rusqlite-facet`](/rusqlite-facet/) | Binds SQLite query parameters and maps rows through Facet-reflected structs. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/rusqlite-facet) |

## Runtime and incremental computation

Reusable runtime pieces for incremental systems and lowered programs.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`picante`](/picante/) | Runs Tokio-first incremental queries with memoization and dependency tracking. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/picante) |
| [`fable`](/fable/) | Evaluates a tiny typed language over Facet-reflected Rust values. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/fable) |
| [`weavy`](/weavy/) | Provides a lowered-program substrate for interpreters and copy-and-patch backends. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/weavy) |

## Observability

Emit and inspect telemetry from reflected Rust types.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`tracing-wide`](https://docs.rs/tracing-wide) | Logs and catalogues typed *wide events* for [`tracing`](https://docs.rs/tracing) with opt-in Facet support. *Community-maintained.* | [yawn/tracing-wide](https://github.com/yawn/tracing-wide) |

## Web, RPC, and UI

Use Facet shapes at application boundaries: HTTP, RPC, and interactive tools.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-axum`](/facet-axum/) | Adds Facet-backed extractors and responses for [axum](https://docs.rs/axum). | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-axum) |
| [`vox`](/vox/) | Provides Rust-native RPC with cross-language codegen and multiple transport backends. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/vox) |
| [`facet-egui`](https://docs.rs/facet-egui) | Provides an [egui](https://www.egui.rs) inspector and editor widget for any `Facet` type. *Community-maintained.* | [Erik1000/facet-egui](https://github.com/Erik1000/facet-egui) |

## Building blocks

Lower-level pieces you may meet while writing a format crate, integration, or
tooling around reflection.

| Crate | What it does | Source |
|-------|--------------|--------|
| [`facet-value`](https://docs.rs/facet-value) | Stores dynamic Facet values as JSON-like data plus bytes. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/facet-value) |
| [`facet-solver`](https://docs.rs/facet-solver) | Resolves type shapes from field names and constraints. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`facet-path`](https://docs.rs/facet-path) | Tracks paths through nested Facet structures. | [facet-rs/facet](https://github.com/facet-rs/facet) |
| [`strid`](/strid/) | Defines strongly typed string identifiers with Facet integration. | [facet-rs/facet](https://github.com/facet-rs/facet/tree/main/strid) |

Writing a new format crate? The [extend](/extend/) section walks through
`Peek`, `Partial`, and the format-crate architecture.

Building something facet-adjacent? Open a PR against the
[website](https://github.com/facet-rs/facet/tree/main/docs), and we'll add it
to the map.
