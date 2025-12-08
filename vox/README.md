# rapace

rapace is a design-driven Rust crate for a shared-memory RPC / IPC system, intended primarily to support out-of-process plugin systems running on the same machine.

At the moment, rapace is a design and implementation-in-progress, not a finished transport. The focus so far is on defining clear invariants, correct concurrency primitives, and a coherent type-driven API, before worrying about benchmarks or polish.

## Motivation

Plugin systems often need:

- **Isolation** — crashy or slow plugins shouldn't take down the host
- **Low overhead** — IPC on local workloads is often the bottleneck
- **Structured, typed APIs** — not stringly-typed messages
- **Observability** — debugging plugins is hard enough already

Existing solutions tend to trade one of these off against the others.

rapace explores an alternative design:

- Shared memory for data transfer
- Event-driven wakeups (eventfd) instead of polling
- Channel-based RPC instead of request–reply-only APIs
- Strong typing at the edges, not just byte-level framing

## Scope

rapace is intentionally narrow in scope.

**What it is trying to be:**

- A Rust-native IPC transport
- Optimized for same-machine, trusted peers
- Suitable as the runtime layer for a plugin system
- Explicit about correctness, failure modes, and recovery

**What it is not trying to be:**

- A general network protocol
- A cross-machine RPC system
- A C ABI or language-agnostic interface
- A drop-in replacement for gRPC or Cap'n Proto
- "Fast at all costs" before correctness is understood

## Core ideas

These are design goals, not yet stable APIs.

### Sessions over shared memory

- One session = one SHM segment between exactly two peers
- Two SPSC rings (A→B, B→A) for message descriptors

### Descriptors + data slabs

- Rings carry fixed-size descriptors
- Payloads live in a separate slab allocator
- Inline payloads for small messages

### Channels as the primitive

- Unary calls are a special case of channels
- Client-streaming, server-streaming, bidi come "for free"
- Cancellation and deadlines are first-class

### Async integration

- No busy polling
- eventfd doorbells integrate cleanly with tokio

### Crash-aware

- Generation counters on reusable memory
- Heartbeats for peer liveness
- Explicit cleanup rules after peer death

## Relationship with facet

rapace itself operates below the serialization layer: it moves frames, not Rust types.

However, the intended use of rapace is with [facet](https://github.com/bearcove/facet)-based APIs:

- RPC request / response types implement `Facet`
- Payloads are serialized with facet-postcard
- Services expose schemas and metadata via reflection
- Introspection and tooling become possible without hand-written IDLs

In practice:

- The transport layer does not depend on facet
- The RPC layer built on top of rapace does
- Plugin-facing APIs are typed, reflected, and self-describing

This separation keeps the transport simple while allowing rich semantics above it.

## Intended use

rapace is being designed as the IPC layer for a plugin system used in a static site generator, where plugins may provide:

- HTML or AST diffing
- Template rendering
- Asset processing
- Analysis and diagnostics
- Experimental or untrusted extensions

Each plugin runs in its own process and communicates with the host via rapace.

The existing plugin architecture can be seen here: [dodeca](https://github.com/bearcove/dodeca)

rapace aims to eventually replace or underpin the current IPC mechanism used there.

## Current status

| Area | Status |
|------|--------|
| Design | ✅ Mostly written down |
| Implementation | 🧪 Starting |
| API stability | ❌ Not stable |
| Benchmarks | ❌ None yet |
| Production readiness | ❌ Not ready |

This repository currently reflects thinking and direction, not a production-ready crate.

## Philosophy

rapace is deliberately designed as if correctness matters.

Instead of relying on *"be careful"*, it aims to rely on **types, invariants, and explicit states**.

Especially compared to C-style shared memory systems, the goal is to make:

- Illegal states unrepresentable
- Race conditions structurally harder to express
- Crash recovery a design concern, not an afterthought

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
