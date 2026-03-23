# SHM Exact-Layout Fast Path

## Status

This is a design note for making the local shared-memory path use an
exact-layout fast path when peers have identical schemas.

It is intentionally narrower than the general schema-evolution story:

- exact-layout mode is SHM-only
- exact-layout mode requires exact schema equality
- all other cases continue to use the normal postcard path

The goal is not to invent a second RPC model. The goal is to let the
existing request/response/channel/session model run with much lower overhead
when peers are local and exactly aligned.

## Goal

The current SHM transport removes some copying, but it still pays for:

- postcard encode/decode
- materializing control messages as owned values
- repeated traversal of variable-length postcard layouts
- per-hop serialization work even when every peer is local

For local VFS/store-style traffic, this is still too expensive.

The fast path described here keeps the same RPC semantics but changes the
representation used on SHM:

- exact-layout `Message`
- exact-layout service payloads
- ordinary postcard fallback when exact layout is not admitted

## Non-goals

This design does not try to:

- replace schema evolution
- replace remote transports
- make mixed-version SHM peers work in exact-layout mode
- make every heap object in every process shared-memory-resident from birth

The general postcard path remains the compatibility path.

## Core model

There are two execution modes for SHM connections.

### 1. Evolving mode

This is the current general mode:

- schema exchange is active
- receivers build translation plans
- payloads are postcard
- SHM only changes where bytes live, not how values are represented

This mode works for:

- mixed versions
- schema evolution
- any transport

### 2. Exact-layout mode

This mode is SHM-only and requires exact schema equality:

- `Message` uses exact layout
- service args and responses use exact layout
- channel items use exact layout
- no translation plans are built
- postcard is not used for those values on that connection

The semantic model stays the same:

- same handshake/session rules
- same connections and virtual connections
- same requests/responses
- same channel behavior
- same retry semantics

Only the representation changes.

## Admission rule

Exact-layout mode is admitted only when both of these are true:

1. the connection is running over SHM
2. the peers' schemas are exactly equal for the relevant roots

There is no separate ABI compatibility negotiation beyond schema equality in
this design. Instead, the schema itself must carry enough layout information
to make equality meaningful for exact layout.

So:

- semantic schema equality is enough for evolving mode
- semantic plus layout schema equality is required for exact-layout mode

If the equality check fails at any point, the connection stays on the normal
postcard path.

## What schemas must carry

Today schemas describe semantic shape. For exact-layout mode they must also
describe the chosen ABI layout.

That means schemas need optional fast-path metadata such as:

- size
- alignment
- field offsets
- enum discriminant representation
- enum payload layout
- niche usage, if any
- container/layout mode for standard special forms

This is not a separate ABI file. It is the same schema system gaining an
additional exact-layout layer.

That gives one source of truth:

- evolving mode uses the semantic layer
- exact-layout mode uses the semantic plus layout layer

## Scope of exact layout

Exact layout is not only for service payloads.

It covers every value that currently flows through postcard on SHM:

- `Message`
- request bodies
- response bodies
- channel messages
- channel items
- service args
- service returns
- payloads on virtual connections

This matters because a partial fast path would leave control-plane overhead in
the hottest path and complicate the implementation boundary.

## Enum story

Enums are the main wrinkle.

### User-defined enums

Facet already requires explicit representation on enums. That makes user
enums much more tractable:

- discriminants are explicit
- schema can reflect the chosen layout
- Swift can generate helpers from the same layout facts

### Standard special enums

The hard cases are:

- `Option<T>`
- `Result<T, E>`
- `VoxError<E>`

These matter because Rust may use niche optimizations and other layout
choices that are invisible at the semantic level.

The design here is:

- Rust code continues to use standard `Option` and `Result`
- exact-layout schemas reflect the actual chosen boundary layout
- boundary machinery may normalize or adapt these representations if needed
- Swift gets generated helpers for reading and constructing them

This keeps ordinary Rust service code idiomatic:

- `?` still works
- handlers still return `Result`
- ecosystem code still composes normally

The cost is pushed into the runtime boundary instead of the handler surface.

## Rust ergonomics

The point of this fast path is not to replace Rust values with protocol-view
objects everywhere.

Rust service code should remain ordinary Rust as much as possible:

- ordinary structs
- ordinary enums
- ordinary `Option`
- ordinary `Result`

The fast-path machinery lives at the boundary:

- reading exact-layout values
- writing exact-layout values
- validating schema/layout equality
- adapting standard wrapper enums where needed

That means there may still be small conversions at the boundary for some
special cases, but the design avoids poisoning the whole handler surface.

## Swift ergonomics

Swift is more constrained for exact layout, especially around enums.

The design does not require Swift native enum layout to match Rust.
Instead:

- layout is described by schema
- Swift codegen produces exact-layout helpers
- those helpers read and write the schema-defined representation

This means Swift may need generated accessors for some enum-heavy cases, but
the contract is still driven by schema, not by hand-maintained parallel ABI
definitions.

## Relationship to the shared-payload broker

The exact-layout control and payload fast path is independent from a future
shared-payload broker.

The broker is still valuable for:

- large immutable blobs
- large VFS/store payloads
- cross-peer forwarding in one local handle space

But the exact-layout mode stands on its own:

- it reduces control-plane overhead
- it reduces ordinary payload encode/decode overhead
- it does not require every payload to be broker-managed

So the expected layering is:

1. SHM transport
2. schema exchange and mode selection
3. exact-layout values when admitted
4. optional brokered shared payloads for large buffers

## Mode selection flow

The session still starts with the normal handshake.

After schema exchange, peers decide per connection whether exact layout is
admitted:

- `Message` schema equality gates exact-layout control traffic
- service root schema equality gates exact-layout payload traffic
- if either side cannot prove equality, postcard remains in use

This means one session can still behave coherently:

- local exact-layout SHM when possible
- normal postcard behavior otherwise

No separate semantic protocol exists.

## Implementation phases

### Phase 1: exact-layout `Message` on SHM

First target:

- make `Message` schema carry layout metadata
- admit exact-layout control traffic on SHM when `Message` schemas match
- keep service payloads on postcard

This gives an initial cut that exercises:

- handshake admission
- exact-layout framing on SHM
- control-plane correctness

### Phase 2: exact-layout service payload roots

Then extend the same mechanism to:

- method args
- method returns
- channel items

This uses the same admission logic, just on additional roots.

### Phase 3: brokered large payloads

After exact layout is working, add the brokered shared-payload path for large
immutable buffers.

This remains orthogonal:

- exact layout improves representation
- brokered payloads improve bulk byte movement

## Why this is worth doing

For same-host daemon stacks, the expected wins are substantial:

- less encode/decode work
- fewer owned control-plane allocations
- fewer materialized intermediate values
- less repeated traversal of variable-length postcard layouts
- better odds of truly low-copy local RPC on the hot path

The complexity is real, but it is concentrated exactly where the performance
pressure is highest: local SHM traffic between same-build peers.

## Summary

The design is:

- one RPC semantic model
- one schema system
- postcard for the general path
- exact layout for SHM when schemas match exactly
- optional shared-payload brokering on top for large immutable buffers

This keeps the general system flexible while finally letting the local SHM
path become meaningfully faster than "shared memory, but still mostly the
same serialization work".
