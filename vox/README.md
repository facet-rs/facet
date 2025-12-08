# rapace

A Rust-native RPC system with transport-agnostic service traits, facet-driven encoding,
and zero-copy shared memory as the reference transport.

## What is rapace?

rapace lets you write normal Rust traits:

```rust
#[rapace::service]
trait Hasher {
    async fn sha256(&self, data: &[u8]) -> [u8; 32];
}
```

...and call them transparently across process boundaries:

```rust
// Same process (direct call, no serialization)
let hasher = HasherClient::new_inproc(Box::new(MyHasher));

// Sibling process (shared memory, zero-copy when possible)
let hasher = HasherClient::new(ShmSession::connect("/tmp/hasher.sock")?);

// Remote machine (TCP, full serialization)
let hasher = HasherClient::new(StreamTransport::connect("192.168.1.100:8080")?);

// All three have identical APIs
let hash = hasher.sha256(data).await?;
```

The same trait works on any transport. The transport chooses how to move the data.

## Why rapace?

| vs. | rapace advantage |
|-----|-------------------|
| Unix domain sockets | Zero-copy for large payloads, no kernel transitions on hot path |
| gRPC over loopback | ~10-100× lower latency, no HTTP/2 overhead, SHM for bulk data |
| boost::interprocess | Async-native, built-in RPC semantics, observability, flow control |
| Custom SHM queues | Production-ready: cancellation, deadlines, crash recovery, introspection |

## Core Ideas

### Single API, Multiple Transports

Plugin authors write pure Rust traits. No transport-specific types leak into signatures.
The RPC layer handles serialization, framing, and delivery—differently per transport:

- **In-proc**: Direct trait calls, real borrows, no serialization
- **SHM**: Zero-copy when data is already in shared memory; memcpy otherwise
- **Network**: Full serialization (postcard by default)

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
- Asset processing
- Analysis and diagnostics
- Experimental or untrusted extensions

Each plugin runs in its own process and communicates with the host via rapace.

The existing plugin architecture: [dodeca](https://github.com/bearcove/dodeca)

## Documentation

| Document | Purpose |
|----------|---------|
| [DESIGN.md](DESIGN.md) | Full technical design (transports, RPC, facet, lifetimes) |
| [IMPLEMENTORS.md](IMPLEMENTORS.md) | Rules for contributors (invariants, checklist) |

## Current Status

| Area | Status |
|------|--------|
| Design | ✅ Mostly written down |
| Implementation | 🧪 Starting |
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

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
