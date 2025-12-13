+++
title = "Architecture"
description = "How rapace is put together internally"
+++

This page is about how rapace actually works: what gets sent over the wire, how shared memory is laid out, and how RPC calls are mapped onto frames and channels. It is descriptive, not a recommendation to use rapace.

## Layers at a glance

From the bottom up, the main pieces look like this:

- **Transport layer** ([`rapace_core::Transport`](https://docs.rs/rapace-core/latest/rapace_core/trait.Transport.html))
  - Moves opaque frames between two peers (processes or tasks).
  - Implemented by in‑memory, stream, WebSocket, and SHM transports.
- **Session layer** ([`rapace_core::RpcSession`](https://docs.rs/rapace-core/latest/rapace_core/struct.RpcSession.html))
  - Manages channels, flow control, and dispatch based on method IDs.
  - Owns a `Transport` and a demux loop.
- **Service layer** ([`#[rapace::service]`](https://docs.rs/rapace-macros/latest/rapace_macros/attr.service.html))
  - Code‑first service traits generate client and server types.
  - Uses [facet](https://facets.rs) and [`facet-postcard`](https://docs.rs/facet-postcard/latest/facet_postcard/) for arguments and return values.

The rest of this page walks through each part in a bit more detail.

## Service definitions and encoding

Service APIs start as Rust traits annotated with [`#[rapace::service]`](https://docs.rs/rapace-macros/latest/rapace_macros/attr.service.html):

```rust,noexec
use rapace::prelude::*;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
}
```

The macro generates:

- a `CalculatorClient<T>` that knows how to encode requests and decode responses;
- a `CalculatorServer<S>` that knows how to decode requests and dispatch to an implementation `S`;
- method IDs and some helper code for registration.

Under the hood:

- argument and result types derive `Facet` (via `facet::Facet` from the prelude);
- [facet](https://facets.rs) produces type shapes for those types;
- [`facet-postcard`](https://docs.rs/facet-postcard/latest/facet_postcard/) uses the shapes to serialize/deserialize values into postcard payloads.

That combination means rapace does not have a separate IDL file: the service trait and its `Facet` types are the "schema", and tools like the explorer can inspect them via the shapes in `rapace-registry`.

## Frames and messages

At the transport boundary, everything is a [`Frame`](https://docs.rs/rapace-core/latest/rapace_core/struct.Frame.html) from `rapace-core`.

A frame contains:

- a **message descriptor** (`MsgDescHot`/`MsgDescCold`), which encodes things like:
  - which channel the message belongs to;
  - the method ID (for the first frame of a call);
  - flags (control vs data, error codes, end‑of‑stream, etc.);
- a **header** (`MsgHeader`) with length and encoding information;
- a **payload slice**, which is usually a postcard‑encoded message or a chunk in a stream.

The encoding layer sees `Frame` as "opaque bytes plus metadata". It takes a `Facet` value, uses `facet-postcard` to produce postcard bytes, and then wraps those bytes into one or more frames according to size limits and streaming semantics.

On the receiving side, [`FrameView`](https://docs.rs/rapace-core/latest/rapace_core/struct.FrameView.html) gives read‑only access into the underlying storage so the decoder can reconstruct values without copying more than necessary.

## Sessions, channels, and dispatch

[`RpcSession<T>`](https://docs.rs/rapace/latest/rapace/struct.RpcSession.html) sits on top of a [`Transport`](https://docs.rs/rapace-core/latest/rapace_core/trait.Transport.html) and does three main jobs:

1. **Channel management**
   - Each logical RPC call lives on a channel (identified by a small integer).
   - Different channels can be active at the same time, allowing concurrency.
   - Peers typically pick disjoint ranges (e.g., odd vs even) to avoid collisions.

2. **Flow control and lifetime**
   - The session reads frames from the transport in a loop (`run()`), demuxes them by channel and method ID, and routes them to the right client or server side.
   - When a stream is finished, flags in the frame header mark the end‑of‑stream, so the session can tear down that channel’s state.

3. **Dispatch**
   - On the server side, the session exposes `set_dispatcher`, which takes a function of the form `(channel_id, method_id, payload_bytes) -> Future<Frame>`.
   - Generated servers (`CalculatorServer` and friends) use this to decode, call the implementation, and encode replies.

Transports themselves do not know about methods, only frames. All method awareness and routing lives in the session and service layers.

## Shared memory transport layout

The `rapace-transport-shm` crate provides a shared‑memory transport with an explicit memory layout for a segment:

```text
┌─────────────────────────────────────────────────────────────────────┐
│  Segment Header (64 bytes)                                         │
├─────────────────────────────────────────────────────────────────────┤
│  A→B Descriptor Ring                                               │
├─────────────────────────────────────────────────────────────────────┤
│  B→A Descriptor Ring                                               │
├─────────────────────────────────────────────────────────────────────┤
│  Data Segment (slab allocator)                                     │
└─────────────────────────────────────────────────────────────────────┘
```

Each segment represents a session between exactly two peers (A and B). For each direction there is:

- a **descriptor ring** (SPSC) that contains small descriptors for in‑flight frames;
- a **data segment** that holds the actual payload bytes, managed by a slab‑style allocator.

The descriptor ring points into the data segment and also carries flags, generation counters, and other metadata.

## SHM allocator and zero‑copy

With the `allocator` feature enabled, `rapace-transport-shm` exposes a `ShmAllocator`. This lets callers allocate buffers directly in the shared memory region:

- `shm_vec(&alloc, &bytes)` copies existing data into SHM‑backed storage;
- `shm_vec_with_capacity(&alloc, n)` creates an empty SHM‑backed buffer of capacity `n`.

When such a buffer is passed through the encoder, it is detected as already living in SHM. Instead of copying the bytes again, the encoder arranges for the descriptor ring to reference that existing slot. From the service code’s point of view, it is still just sending a `Vec<u8>`; the zero‑copy aspect is entirely a transport concern.

The allocator is optional. Service traits remain transport‑agnostic: they do not know or care where the underlying bytes live.

## Other transports

The same `Transport` trait is implemented by several transports:

- **InProcTransport** (`rapace-transport-mem`) – an in‑memory SPSC channel pair used for tests and examples.
- **Stream transport** (`rapace-transport-stream`) – wraps TCP/Unix‑style streams.
- **WebSocket transport** (`rapace-transport-websocket`) – wraps WebSocket connections (used for browser‑side tools).
- **SHM transport** (`rapace-transport-shm`) – the shared memory layout described above.

Service code is written against `#[rapace::service]` traits and clients/servers; it generally does not care which transport sits underneath, as long as both ends pick the same one.
