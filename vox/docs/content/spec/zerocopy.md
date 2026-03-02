+++
title = "Zero-copy"
sort_by = "weight"
weight = 6
insert_anchor_links = "left"
+++

# Zero-copy data flow

> r[zerocopy]
>
> Roam aims to minimize copies at every layer. This document specifies how
> payloads flow through the system — from user code, through serialization
> and transport, to deserialization on the other side — and which link types
> enable zero-copy at each stage.

## Backing storage

> r[zerocopy.backing]
>
> A **Backing** is an owned handle that keeps a region of bytes alive. The
> receive path deserializes borrowing from a Backing, producing values that
> may contain `&str` or `&[u8]` pointing into the original buffer.

> r[zerocopy.backing.boxed]
>
> **Boxed** — a heap-allocated `Box<[u8]>`. Used by stream (TCP) and
> WebSocket links after reading into an owned buffer.

> r[zerocopy.backing.bipbuf]
>
> **BipBuf (copy-out)** — for small messages that fit inline in the
> BipBuffer ring, the receiver copies the payload bytes into a
> heap-allocated `Box<[u8]>` and immediately releases the ring region.
> This is intentional: the BipBuffer consumer must release regions in
> FIFO order, so holding a borrow would block all subsequent receives
> until that particular message is dropped. Copying out allows multiple
> in-flight messages to be processed concurrently.

> r[zerocopy.backing.bipbuf.pool]
>
> A future optimization is to copy into buffers drawn from a pool rather
> than allocating a fresh `Box<[u8]>` per message, amortizing allocation
> cost for small inline messages.

> r[zerocopy.backing.varslot]
>
> **VarSlot** — a slot in a shared-memory VarSlotPool. For medium messages,
> the sender writes into a variable-size slot and the receiver borrows from
> it. The slot is returned to the pool when the Backing is dropped.

> r[zerocopy.backing.mmap]
>
> **Mmap** — a memory-mapped region for large payloads. The sender writes a
> file (or anonymous mapping), the receiver maps it and borrows from the
> mapping.

## Send path

> r[zerocopy.send]
>
> On the send side, user code provides a value to serialize. The value may
> borrow from the calling context.

### Borrowed arguments

> r[zerocopy.send.borrowed]
>
> A call like `client.method(&buf[..12]).await` passes a borrowed slice.
> The future returned by `method()` captures the reference. The caller's
> `.await` keeps the borrow alive for the future's entire lifetime —
> including across yield points (e.g. `reserve()` for backpressure).

> r[zerocopy.send.borrowed-in-struct]
>
> A call like `client.method(Context { name: &my_string }).await` passes a
> struct that borrows from the calling scope. This is valid for the same
> reason: the future captures the struct, which holds the borrow, and the
> caller's `.await` keeps everything alive until the future completes.

> r[zerocopy.send.lifetime]
>
> The send future is not `'static` — it borrows from the caller's scope.
> This means it cannot be spawned on executors that require `'static`
> futures (e.g. `tokio::spawn`). It is driven to
> completion by the caller's `.await`, which guarantees all borrows remain
> valid. The actual sequence is:
>
> 1. The future calls `reserve().await` — yields until the link has capacity
> 2. The future calls `alloc(len)` — obtains a write slot
> 3. The future serializes the borrowed value into the write slot
> 4. The future calls `commit()` — publishes the bytes
>
> Serialization happens in step 3, after the backpressure yield. The
> borrowed data is valid because the caller is still awaiting.

### Link-specific send behavior

> r[zerocopy.send.stream]
>
> **Stream links (TCP):** serialize into a write buffer, flush to socket.
> One copy (value → write buffer).

> r[zerocopy.send.websocket]
>
> **WebSocket links:** serialize into a message buffer, send as a WebSocket
> frame. One copy (value → message buffer).

> r[zerocopy.send.shm]
>
> **SHM links:** the send path depends on payload size:
>
> - **Small (`8 + payload_len <= inline_threshold`):** `LinkTx::alloc`
>   returns a `WriteSlot` backed by a heap-allocated `Vec<u8>`.
>   Serialization writes into this buffer, then `commit()` copies the
>   bytes into the BipBuffer ring. One copy (heap buffer → ring). The
>   ring cannot be written to directly because the producer reservation
>   must be held as briefly as possible to avoid blocking the receiver.
> - **Medium (`8 + payload_len > inline_threshold` and
>   `payload_len <= mmap_threshold`):** serialize
>   into a VarSlot. `LinkTx::alloc` allocates a slot and returns a
>   `WriteSlot` pointing directly into shared memory. Zero copies for
>   the payload bytes. A slot-ref frame is written to the BipBuffer to
>   notify the receiver.
> - **Large (`payload_len > mmap_threshold`):** serialize into a memory-mapped
>   region. A reference to the mapping is sent through the BipBuffer.
>
> `mmap_threshold` is defined by the SHM transport as the largest payload
> that fits in any VarSlotPool class (see [SHM spec](@/spec/shm.md)).

## Receive path

> r[zerocopy.recv]
>
> On the receive side, `LinkRx::recv` returns a `Backing` that keeps raw
> bytes alive. For heap-backed links the Backing owns the buffer; for SHM
> links it holds a handle (BipBuf region, VarSlot, or mmap) that keeps the
> underlying shared memory valid until dropped. The conduit
> deserializes borrowing from this backing, producing a `SelfRef<T>` that
> pairs the decoded value with its backing.

> r[zerocopy.recv.selfref]
>
> `SelfRef<T>` guarantees correct drop order: the decoded value is dropped
> before its backing storage. This allows the value to contain references
> (`&str`, `&[u8]`) pointing into the backing without use-after-free.

### Link-specific receive behavior

> r[zerocopy.recv.stream]
>
> **Stream links (TCP):** `recv` reads a length-prefixed frame into a
> `Box<[u8]>`. One copy (socket → heap). Deserialization borrows from the
> box.

> r[zerocopy.recv.websocket]
>
> **WebSocket links:** `recv` receives a complete message as `bytes::Bytes`,
> converted to `Box<[u8]>`. One copy at the Roam link boundary (transport-
> internal buffering is not counted). Deserialization borrows from the box.

> r[zerocopy.recv.shm.inline]
>
> **SHM links (inline):** for messages where
> `8 + payload_len <= inline_threshold`, `recv`
> copies the payload out of the BipBuffer ring into a `Box<[u8]>` and
> releases the ring region immediately (see `zerocopy.backing.bipbuf`).
> One copy (ring → heap). Deserialization borrows from the box.

> r[zerocopy.recv.shm.slotref]
>
> **SHM links (slot-ref):** for messages where
> `8 + payload_len > inline_threshold` and `payload_len <= mmap_threshold`,
> `recv` returns a Backing
> that borrows from a VarSlot. Zero copies — deserialization reads from the
> slot, which is returned to the pool when the Backing drops.

> r[zerocopy.recv.shm.mmap]
>
> **SHM links (mmap):** for messages where `payload_len > mmap_threshold`,
> `recv` maps the region and
> returns a Backing that owns the mapping. Zero copies — deserialization
> reads from the mapping.

## Framing layers

> r[zerocopy.framing]
>
> A message passes through three layers of framing between user code and
> the physical wire. Each layer has a distinct responsibility.

### Layer 1: Value encoding

> r[zerocopy.framing.value]
>
> The user's Rust value is serialized using postcard (via facet-postcard).
> Postcard produces a compact binary encoding that supports zero-copy
> deserialization — string and byte slice fields can borrow directly from
> the input buffer.
>
> The output of this layer is a contiguous byte sequence representing the
> serialized value.

> r[zerocopy.framing.value.opaque]
>
> For `Message<'payload>` payload fields marked as opaque, value encoding is
> the boundary where erased payload behavior is applied:
>
> - **Outgoing (`Message<'call>`):** the opaque adapter maps the payload to
>   `(PtrConst, Shape, Option<TypePlanCore>)`, and postcard serializes that
>   mapped value.
> - **Incoming (`Message<'static>` inside `SelfRef`):** postcard decodes the
>   payload byte sequence and materializes deferred payload state as either a
>   borrowed byte slice (when input backing is stable) or owned bytes.
>
> Conduit framing and link framing do not change this mapping contract; they
> only add/remove their own framing around the same encoded payload bytes.

### Layer 2: Conduit framing

> r[zerocopy.framing.conduit]
>
> The conduit wraps the serialized value bytes depending on the conduit
> type:

> r[zerocopy.framing.conduit.bare]
>
> **BareConduit** — no additional framing. The serialized value bytes are
> passed directly to the link. Suitable for transports where reliability
> is inherent or unnecessary (shared memory, in-process, localhost).

> r[zerocopy.framing.conduit.stable]
>
> **StableConduit** — serializes a `Frame<T>` instead of a bare `T`. The
> Frame struct contains:
>
> - `seq: u32` — monotonically increasing sequence number
> - `ack: Option<u32>` — highest sequence number received from the peer
> - `item: T` — the actual value
>
> The entire `Frame<T>` is serialized in one postcard pass — there is no
> separate header serialization step. The conduit framing fields are just
> the first fields of the serialized output. The conduit maintains a
> replay buffer of serialized frame bytes for retransmission after
> reconnection. Required for transports that may drop the underlying
> connection (TCP, WebSocket).

### Layer 3: Link framing

> r[zerocopy.framing.link]
>
> The link adds transport-specific framing to preserve message boundaries:

> r[zerocopy.framing.link.stream]
>
> **Stream links (TCP, Unix sockets):** 4-byte little-endian length prefix
> followed by the payload bytes: `[len: u32 LE][payload]`.

> r[zerocopy.framing.link.websocket]
>
> **WebSocket links:** each message is sent as a single binary WebSocket
> frame. The WebSocket protocol preserves message boundaries natively.

> r[zerocopy.framing.link.shm]
>
> **SHM links:** 8-byte frame header (total_len + flags + reserved)
> followed by one of:
>
> - inline payload bytes (padded to 4-byte alignment),
> - a 12-byte slot-ref body pointing into the VarSlotPool (`SLOT_REF`), or
> - a 24-byte mmap-ref body (`MMAP_REF`) containing mapping identifier,
>   generation, offset, and payload length.
>
> See the
> [SHM spec](@/spec/shm.md) for details.

> r[zerocopy.framing.link.memory]
>
> **Memory links (in-process):** no framing. Messages are `Vec<u8>` passed
> through an MPSC channel. Used for testing and in-process communication.

### Framing combinations

> r[zerocopy.framing.combinations]
>
> Not all conduit × link combinations are valid or useful:
>
> | Conduit       | Stream | WebSocket | SHM  | Memory |
> |---------------|--------|-----------|------|--------|
> | BareConduit   | —      | —         | yes  | yes    |
> | StableConduit | yes    | yes       | —    | —      |
>
> BareConduit is used with links that don't lose connections (SHM, memory).
> StableConduit is used with links that may disconnect (TCP, WebSocket) and
> need seq/ack for replay on reconnect.

### End-to-end pipeline and lifetimes

> r[zerocopy.framing.pipeline]
>
> The runtime pipeline is:
>
> 1. **Link layer** receives/sends framed transport bytes.
> 2. **Conduit layer** removes/applies conduit framing (`T` vs `Frame<T>`).
> 3. **Value layer** decodes/encodes `Message<'payload>` fields, including
>    opaque payload handling.

> r[zerocopy.framing.pipeline.incoming]
>
> Incoming path:
>
> 1. `LinkRx::recv` yields `Backing` containing one message payload.
> 2. Conduit deframes and deserializes into `SelfRef<Message<'static>>`.
> 3. Driver/dispatch reads `method_id`, resolves concrete args shape/plan, and
>    maps `SelfRef<Message<'static>>` to `SelfRef<ConcreteArgs>` using the same
>    backing.

> r[zerocopy.framing.pipeline.outgoing]
>
> Outgoing path:
>
> 1. Driver builds `Message<'call>` borrowing from call scope as needed.
> 2. Opaque payload mapping happens during value serialization.
> 3. Conduit applies its framing (`T` or `Frame<T>`), then link applies
>    transport framing at commit/send time.

### Serialization timing

> r[zerocopy.framing.single-pass]
>
> Despite the three logical layers, serialization happens exactly once.
> The conduit is generic over the value type `T`, so:
>
> - **BareConduit** serializes `T` directly into the write slot.
> - **StableConduit** serializes `Frame<T>` directly into the write slot.
>
> In both cases, postcard writes the output into the buffer provided by
> `LinkTx::alloc`. There is no intermediate buffer between layers — the
> value encoding and conduit framing are a single serialization pass, and
> the link framing (length prefix, SHM frame header, etc.) is applied by
> the link at `commit()` time around the already-written bytes.

> r[zerocopy.framing.no-double-serialize]
>
> The conduit MUST NOT serialize the value into a temporary buffer and
> then copy it into the write slot. The conduit serializes the value (or
> `Frame<T>`) directly into the link's write slot in one pass.

### Scatter/gather serialization

> r[zerocopy.scatter]
>
> Serializing directly into the write slot requires knowing the total
> encoded size before calling `LinkTx::alloc(len)`. Postcard's encoding
> is sequential and deterministic, so the serializer can compute the
> exact output size and collect copy instructions without writing to a
> final destination buffer.

> r[zerocopy.scatter.plan]
>
> The serializer performs a single walk over the value and produces a
> **scatter plan**: a staging buffer plus an ordered list of segments.
> Each segment is either:
>
> - **Staged** — a byte range within the staging buffer (structural
>   bytes: varints, enum tags, length prefixes, fixed-size fields), or
> - **Reference** — a pointer and length into the original value's memory
>   (blob fields: `&[u8]`, `&str`).
>
> The staging buffer contains only the structural bytes. Blob payloads
> are never copied into it — they remain at their original addresses.

> r[zerocopy.scatter.plan.size]
>
> The total encoded size is the sum of all segment lengths (staged +
> referenced). This is known after the walk completes, before any bytes
> are written to the destination.

> r[zerocopy.scatter.write]
>
> To write the scatter plan into a destination buffer:
>
> 1. Call `LinkTx::alloc(total_size)` to obtain a `WriteSlot`.
> 2. Walk the segment list in order. For each segment, `memcpy` its bytes
>    (from staging buffer or from the referenced source) into the write
>    slot at the current offset.
> 3. Call `commit()` on the write slot.
>
> This is the only point where bytes are copied into the link's buffer.
> Blob data goes directly from the caller's memory to the write slot —
> one copy total.

> r[zerocopy.scatter.lifetime]
>
> The scatter plan borrows from the original value. The plan MUST be
> consumed (written into a slot) before the borrows expire. In practice,
> the conduit's `send` method builds the plan and writes it within the
> same call, while the caller's `.await` keeps all borrows alive (see
> `zerocopy.send.lifetime`).

> r[zerocopy.scatter.replay]
>
> For StableConduit, the replay buffer needs an owned copy of the
> serialized frame bytes. After writing the scatter plan into the write
> slot, the conduit copies the slot's byte range into the replay buffer.
> This is one additional `memcpy` (slot → replay buffer) that is
> unavoidable for reliability — but there is no intermediate `Vec`
> between serialization and the write slot.

## Payload representation

> r[zerocopy.payload]
>
> `Payload` represents a value ready for serialization. Its variants
> reflect the different ownership situations:

> r[zerocopy.payload.borrowed]
>
> **Borrowed** — a type-erased pointer to caller-owned memory (stack,
> heap, arena, etc.) plus its Shape. Used on the send path when the value
> is reachable for the borrow lifetime.

> r[zerocopy.payload.bytes]
>
> **Bytes** — a contiguous byte buffer that is already serialized (e.g.
> when forwarding a message without deserializing, or when the link
> provides raw bytes). Paired with a Backing to keep the buffer alive.

## Copy count summary

> r[zerocopy.copies]
>
> Copy counts are measured at the Roam link boundary — copies internal to
> the transport library (e.g. TLS decryption, WebSocket frame assembly) are
> not included.
>
> | Direction | Stream (TCP) | WebSocket | SHM (inline) | SHM (slot-ref) | SHM (mmap) |
> |-----------|-------------|-----------|--------------|-----------------|------------|
> | Send      | 1           | 1         | 1            | 0               | 0          |
> | Receive   | 1           | 1         | 1            | 0               | 0          |
