+++
title = "rapace"
description = "A Rust-centric RPC protocol with cross-language code generation"
+++

Rapace is a binary RPC protocol built around the Rust type system. There is no IDL—services and types are defined in Rust using [Facet](https://facet.rs) for reflection, and code generators produce client bindings for other languages.

## Design philosophy

**Rust as the source of truth.** Service definitions are Rust traits annotated with `#[rapace::service]`. Types use `#[derive(Facet)]` for compile-time introspection. The Facet shapes power both serialization and cross-language code generation.

**No IDL files.** Instead of maintaining separate `.proto` or `.thrift` files, you write Rust code. The schema is the code. Code generators read from a runtime registry of Facet shapes to produce Swift, TypeScript, and other bindings.

**Postcard on the wire.** All payloads use [Postcard](https://postcard.jamesmunns.com/), a compact non-self-describing binary format. Schema compatibility is verified at handshake time via structural hashing—peers must agree on type shapes before exchanging messages.

## Example

```rust,noexec
use rapace::prelude::*;

#[derive(Facet)]
pub struct Point { pub x: i32, pub y: i32 }

#[rapace::service]
pub trait Canvas {
    async fn draw(&self, p: Point) -> Result<(), String>;
    async fn clear(&self);
}
```

This generates:
- `CanvasClient` and `CanvasServer` for Rust
- Method IDs (FNV-1a hashes of `"Canvas.draw"`, `"Canvas.clear"`)
- Registry entries with Facet shapes for codegen

From the registry, code generators produce Swift and TypeScript clients that encode/decode the same wire format.

## Transports

Rapace separates the protocol from the transport. The same service works over:

- **Shared memory** — zero-copy IPC for same-machine communication
- **WebSocket** — browser and cross-network, works on native and WASM
- **TCP/Unix streams** — traditional socket-based transport
- **In-memory channels** — for testing

## Cross-language support

| Language | Status | Generated artifacts |
|----------|--------|---------------------|
| **Rust** | Complete | Client, server, registry |
| **TypeScript** | Complete | Client, encoder/decoder |
| **Swift** | Complete | Client, encoder/decoder |
| Go | Planned | — |
| Java | Planned | — |

Code generators read Facet shapes from the Rust service registry and emit idiomatic code for each target language. See [Language Mappings](/spec/language-mappings/) for type conversion details.

## Documentation

- **[Specification](/spec/)** — Formal protocol definition with normative rules
- **[Rust Guide](/guide/)** — Implementation details for the Rust crates
- **[API docs](https://docs.rs/rapace)** — Crate documentation on docs.rs

## Crates

| Crate | Purpose |
|-------|---------|
| [`rapace`](https://docs.rs/rapace) | Main crate with service macro and prelude |
| [`rapace-core`](https://docs.rs/rapace-core) | Frames, transports, sessions |
| [`rapace-cell`](https://docs.rs/rapace-cell) | High-level runtime for SHM-based plugins |
| [`rapace-registry`](https://docs.rs/rapace-registry) | Service registry for codegen |
| [`rapace-tracing`](https://docs.rs/rapace-tracing) | Tracing subscriber that forwards over rapace |
