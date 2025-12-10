# rapace

> **⚠️ EXPERIMENTAL - DO NOT USE ⚠️**
>
> This is an early-stage research project. The API is unstable, features are incomplete, and there are no performance guarantees. **Do not use this in production or for anything important.**

A Rust-native RPC system with transport-agnostic service traits, facet-driven encoding,
and zero-copy shared memory as the reference transport.

## What is rapace?

rapace lets you write normal Rust traits:

```rust
use rapace::prelude::*;

#[rapace::service]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn range(&self, n: u32) -> Streaming<u32>;  // Server-streaming
}
```

The `#[rapace::service]` macro generates a client stub and server dispatcher:

```rust
// Implement the service
struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn range(&self, n: u32) -> Streaming<u32> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0..n {
                let _ = tx.send(Ok(i)).await;
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

// Create transport pair (in-memory for this example)
let (client_transport, server_transport) = rapace::InProcTransport::pair();

// Wrap each half in an RpcSession and spawn the demux loop once.
let client_transport = Arc::new(client_transport);
let server_transport = Arc::new(server_transport);
let client_session = Arc::new(rapace::RpcSession::with_channel_start(client_transport.clone(), 2));
let server_session = Arc::new(rapace::RpcSession::with_channel_start(server_transport.clone(), 1));
tokio::spawn(client_session.clone().run());
tokio::spawn(server_session.clone().run());

// Server side: plug the generated dispatcher into the session.
server_session.set_dispatcher(|_channel_id, method_id, payload| {
    let server = CalculatorServer::new(CalculatorImpl);
    Box::pin(async move { server.dispatch(method_id, &payload).await })
});

// Client side
let client = CalculatorClient::new(client_session.clone());
let result = client.add(2, 3).await?;  // Returns 5
```

The same trait works on any transport. The transport chooses how to move the data.

> **Why manually wire `RpcSession`?** Each side must own exactly one session per transport, spawn its `run()` loop, and decide which channel IDs it controls (hosts usually start at 1 for odd IDs, plugins at 2 for even IDs). The `#[rapace::service]` macro intentionally stays pure so you can share a single session across many generated clients/servers and integrate with whichever executor you already use.

## Getting Started

Add to your `Cargo.toml`:

```toml
[dependencies]
rapace = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
tokio-stream = "0.1"
```

Run the example:

```bash
cargo run --example basic -p rapace
```

## Why rapace?

rapace is designed for **host ↔ plugin architectures** where plugins run in separate processes on the same machine. The goals:

- **Rust-native**: Define services as normal async traits, no IDL
- **Transport-agnostic**: Same trait works over TCP, WebSocket, or shared memory
- **Streaming**: First-class support for server-streaming RPCs
- **Async**: Built on tokio, no busy polling

> **Status**: Extremely basic and experimental. No benchmarks, no stability guarantees, many missing features. This exists to explore ideas, not to be used.

## Core Ideas

### Single API, Multiple Transports

Plugin authors write pure Rust traits. No transport-specific types leak into signatures.
The RPC layer handles serialization, framing, and delivery—differently per transport:

- **In-proc**: Direct trait calls, real borrows, no serialization
- **SHM**: Zero-copy when data is already in shared memory; memcpy otherwise
- **Network**: Full serialization (postcard only, via facet-postcard)

### Facet at the Center

All types implement [facet](https://github.com/bearcove/facet). This gives us:

- Transport-specific encoding (postcard for wire, slot references for SHM)
- Service registry schemas
- Runtime introspection
- Future: diff/patch for `&mut T` across transports

### Channels, Not Just Request/Response

Everything is a channel. Unary RPC is sugar on top.

```
Client                          Server
  │                                │
  │── DATA ────────────────────────>│
  │<────────────────────── DATA ───│
  │── DATA ────────────────────────>│  (bidirectional streaming)
  │<────────────────────── DATA ───│
  │── EOS ─────────────────────────>│
  │<─────────────────────── EOS ───│
```

Client-streaming, server-streaming, and bidirectional streaming come "for free."

### Async-Native

No busy polling. eventfd doorbells integrate cleanly with tokio.

### Crash-Aware

- Generation counters detect stale references after recovery
- Heartbeats for peer liveness
- Explicit cleanup rules after peer death

## rapace is NOT

- A general-purpose network RPC framework
- A language-agnostic wire protocol (Rust-native by design)
- A capability-secure sandbox
- A replacement for gRPC / HTTP APIs for internet services

It is a **Rust-native RPC spine optimized for host ↔ plugin architectures**.

## Intended Use

rapace is being designed as the IPC layer for plugin systems where plugins may provide:

- HTML or AST diffing
- Template rendering
- Asset processing (image processing, format conversion)
- Analysis and diagnostics
- Experimental or untrusted extensions

Each plugin runs in its own process and communicates with the host via rapace.

### Zero-Copy for Large Payloads

For same-machine communication, rapace's SHM transport provides **true zero-copy** for large
payloads. By allocating data directly in shared memory, you can send images, documents, or
other large blobs without any memcpy:

```rust
// Enable with: rapace-transport-shm = { features = ["allocator"] }
use rapace_transport_shm::{ShmAllocator, shm_vec};

// Allocate directly in SHM
let alloc = ShmAllocator::new(session.clone());
let shm_png = shm_vec(&alloc, &png_bytes);

// When encoded, the encoder detects "hey, this is already in SHM!"
// and just records (slot, offset, len) — no copy!
```

See `demos/shm-image/` for a complete demonstration with metrics.

Rapace is primarily motivated by [dodeca](https://github.com/bearcove/dodeca), a static site generator
that implements most of its functionality as plugins. Dodeca is the main "real" application that
drives rapace's design.

## Documentation

| Document | Purpose |
|----------|---------|
| [DESIGN.md](DESIGN.md) | Full technical design (transports, RPC, facet, lifetimes) |
| [IMPLEMENTORS.md](IMPLEMENTORS.md) | Rules for contributors (invariants, checklist) |

## Current Status

| Area | Status |
|------|--------|
| Design | ✅ Documented (DESIGN.md) |
| Core types | ✅ Frame, Transport, RpcError, Streaming |
| Proc macros | ✅ `#[rapace::service]` generates client/server |
| In-memory transport | ✅ For testing |
| Stream transport | ✅ TCP/Unix sockets |
| WebSocket transport | ✅ For browser clients |
| SHM transport | ✅ Zero-copy, allocator, metrics |
| Session layer | ✅ Flow control, cancellation |
| Browser tests | ✅ Playwright + wasm-pack |
| Conformance tests | ✅ Shared test scenarios |
| API stability | ❌ Not stable |
| Benchmarks | ❌ None yet |
| Production readiness | ❌ Not ready |

## Quick Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│ Service Traits                                           │
│   #[rapace::service] trait Foo { ... }                  │
├─────────────────────────────────────────────────────────┤
│ Facet Reflection                                         │
│   Schema, encoding, introspection                        │
├─────────────────────────────────────────────────────────┤
│ RPC Framing                                              │
│   Channels, flow control, deadlines, errors              │
├─────────────────────────────────────────────────────────┤
│ Transport-Specific Encoding                              │
│   In-proc: pass-through | SHM: slots | Stream: postcard │
├─────────────────────────────────────────────────────────┤
│ Transport Implementation                                 │
│   SHM rings | TCP streams | WebSocket frames            │
└─────────────────────────────────────────────────────────┘
```

A method call flows through all layers:

```
Trait → Facet → Encoder → Transport → [wire] → Transport → Decoder → Facet → Trait
```

## The Giants Whose Shoulders We Stand On

- **gRPC / HTTP/2** — Streaming RPC, status codes, deadlines, and the basic "service trait ↔ wire method" mental model.
- **tonic / tower (Rust)** — Ergonomic async service traits and middleware-style layering between "transport" and "RPC semantics".
- **Cap'n Proto / FlatBuffers** — Schema-driven, zero-copy thinking and the idea of treating messages as structured views over bytes.
- **ZeroMQ / nanomsg / NNG** — Patterned messaging, separation of transport mechanics from higher-level semantics.
- **Aeron / Disruptor / SPSC ring literature** — Cache-line-aware descriptor rings, single-producer/single-consumer queues, and careful publication barriers.
- **Boost.Interprocess / POSIX shared memory patterns** — Practical shared-memory layouts, generation counters, and crash-resilient resource ownership.
- **Linux I/O stack (io_uring, epoll, eventfd)** — Event-driven, async I/O and doorbell primitives that inspired the SHM + eventfd design.
- **Tracing / OpenTelemetry ecosystem** — The emphasis on trace/span IDs, structured telemetry, and observability as first-class concerns.
- **facet & schema-first systems (protobuf, Smithy, etc.)** — Reflection-driven schemas, type shapes, and the idea of using a single schema layer to power encoding, registry, and tooling.

## Sponsors

CI and browser tests run on generously provisioned runners provided by Depot:

<p><a href="https://depot.dev?utm_source=rapace">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/rapace/raw/main/static/depot-dark.svg">
<img src="https://github.com/bearcove/rapace/raw/main/static/depot-light.svg" height="40" alt="Depot">
</picture>
</a></p>

Their support makes it feasible to run heavy test suites (Miri, fuzzing, browser/WebSocket tests) on
every change, which directly improves the safety and reliability of this project.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
