# Connect/Serve API Simplification With Per-Connection Factory

Design notes for simplifying the public Rust API while preserving advanced
session features (virtual connections, resume, middleware, etc).

## Context

Current setup is powerful but awkward:

- session establishment uses builder-heavy APIs
- generated clients currently expose only service methods (plus a few utility methods)
- server lifetime can be coupled to root caller drop behavior
- virtual connections have per-open metadata, but root connection does not
- `SessionHandle` control plumbing currently exposes multiple internal channels

Related issue: <https://github.com/bearcove/vox/issues/280>

## Goals

- Keep the common client path extremely small
- Keep advanced features available without making common usage harder
- Make server behavior consistent between root and virtual connections
- Support per-connection service selection (factory model)
- Avoid generated-client method-name collisions

## Non-goals (for first iteration)

- Full rewrite of session internals
- Breaking all existing builder APIs at once
- Solving every control-plane cleanup item in the same PR

## Proposed Client API

Common case:

```rust
#[vox::service]
trait Hello {
    async fn say_hello(&self) -> String;
}

let client: HelloClient = vox::connect(addr).await?;
let msg = client.say_hello().await?;
```

`connect(...)` implies the client-only path. Duplex convenience helpers are
explicitly deferred for now; advanced bidirectional cases continue using the
existing lower-level APIs.

## Generated Client: Public Fields, No Traits

**Problem:** Generated clients need access to the caller (for connection
lifecycle) and session handle (for virtual connections). Previous attempts
used traits (`VoxClient`, `HasSessionHandle`, `VoxSessionCarrier`) and
free functions (`vox::closed()`, `vox::session_handle()`). This created
a mess: `DriverCaller` and generated clients used different traits,
free functions only accepted one kind, and the whole thing was confusing.

**Solution:** Public fields. Fields don't clash with methods.

```rust
pub struct HelloClient {
    pub caller: Caller,
    pub session: Option<SessionHandle>,
    // ... generated service methods
}
```

Usage:

```rust
client.caller.closed().await;       // wait for connection close
client.caller.is_connected();       // check liveness
client.session.as_ref();            // access session handle
client.say_hello("world").await?;   // service method — no clash
```

This eliminates:
- `VoxClient` trait
- `HasSessionHandle` trait
- `FromVoxSession` trait
- `vox::closed()` free function
- `vox::is_connected()` free function
- `vox::session_handle()` free function

### `Caller` as a concrete type

`Caller` becomes a single concrete struct — no trait. Today there's a
`Caller` trait implemented by `DriverCaller`, `ErasedCaller`, and
`MiddlewareCaller`, but `DriverCaller` is the only real implementation.
The others just wrap it.

The new `Caller` struct owns the connection state directly (what
`DriverCaller` has today) plus an optional middleware chain (what
`ErasedCaller` adds). One type, inside and outside.

Methods: `closed()`, `is_connected()`, `with_middleware()`, and the
internal `call()` used by generated client methods.

**Next steps:**

1. Merge `DriverCaller` + `ErasedCaller` + middleware into one `Caller` struct
2. Kill the `Caller` trait, `VoxClient`, `HasSessionHandle`, `FromVoxSession`
3. Update macro: generated clients get `pub caller: Caller` and
   `pub session: Option<SessionHandle>` fields
4. Update `establish()` to construct clients with the new fields
5. Update all examples and tests

**Open:** naming — should it be `Caller`? Should `SessionHandle` remain
separate or merge into `Caller`? To be decided after implementation.

This avoids polluting the service method namespace while keeping advanced
session operations available.

## Virtual Connections Should Stay Typed

When users need sub-connections, the path should stay typed (not raw low-level calls):

```rust
let client: RootClient = vox::connect(addr).await?;
let sub: ChatClient = client.session.unwrap()
    .open_typed::<ChatClient>(settings, metadata).await?;
```

The common path remains tiny; advanced flows remain explicit.

## Proposed Server API Direction

Current API mostly requires passing a root handler directly to `establish`.
For better ergonomics and consistency, move toward a per-connection factory.

High-level idea:

```rust
vox::serve(listener)
    .factory(my_factory)
    .await?;
```

Where the factory receives per-connection context and returns a handler/dispatcher.

## Unified Factory Context (Root + Virtual)

We want the same selection mechanism for root and virtual connections.

Sketch:

```rust
enum ConnectionKind {
    Root,
    Virtual {
        id: vox::ConnectionId,
        open_metadata: vox::Metadata<'static>,
        peer_settings: vox::ConnectionSettings,
    },
}

struct ConnectionContext {
    kind: ConnectionKind,
    link_info: LinkInfo, // tcp/local/shm/ws specific details
    peer_settings: vox::ConnectionSettings,
    metadata: vox::Metadata<'static>, // root or virtual open metadata
}
```

Factory sees one context shape and can branch only when needed.

## Important Gap To Close: Root Metadata

Today, virtual connections have explicit metadata via `ConnectionOpen.metadata`.
Root connection does not currently have an equivalent metadata channel in the
handshake, and builder-level root `.metadata(...)` is currently not wired through
to handshake/session establishment.

To support a truly uniform factory model, root connection metadata must become
first-class (or be replaced with a clearly defined equivalent).

## Lifetime Footgun To Fix

Current behavior can stop serving when the last root caller is dropped.
That is surprising for server use.

Desired direction:

- server lifetime should be explicit (shutdown/error), not accidental caller drop
- root and virtual liveness semantics should be documented clearly

## Resumable-By-Default Footgun ✅

**Fixed.** Both `SessionSourceInitiatorBuilder` and
`SessionTransportAcceptorBuilder` now default `resumable: false`.
Users opt in with `.resumable()`.

## Facade Crate Re-export Hygiene

`rust/vox/src/lib.rs` currently does `pub use vox_core::*`, which dumps
every public symbol from `vox-core` into the `vox` namespace. This
includes internal types that shouldn't be part of the public API.

The `runtime` cargo feature that gates this re-export is also
questionable — it's in `default` features, so it's always on unless
someone explicitly opts out, and anyone who only needs types can depend
on `vox-types` directly.

The result is that `docs.rs` for the `vox` crate shows a flat namespace
with ~100 items mixing genuine public API (`connect`, `service`,
`SessionHandle`) with internal machinery (`BareConduitPermit`,
`DriverChannelSink`, `MessagePlan`, `ConnectionState`, `Session`,
`recv_client_hello`, `exhausted_source`, etc). This makes the crate
effectively undiscoverable.

**Desired direction:** replace the glob re-export with an explicit,
curated list of public API symbols. Consider removing the `runtime`
feature entirely.

## Migration Approach

1. ✅ `vox::connect(addr)` — TCP, local, WebSocket, SHM
2. ✅ `SessionHandle` stored in client
3. ✅ `resumable` defaults to `false`
4. ✅ SHM bootstrap: removed SID, 4 FDs, `ShmLinkSource`
5. ✅ Concrete `Caller` type, public fields on clients
   - Killed: `Caller` trait, `ErasedCaller`, `ErasedCallerDyn`,
     `MiddlewareCaller`, `VoxClient`, `HasSessionHandle`, old `NoopCaller`
   - Killed: `vox::closed()`, `vox::is_connected()`, `vox::session_handle()`
   - `Caller` is a concrete struct in `vox-core` wrapping `Arc<DriverCaller>`
   - Generated clients: `pub caller: Caller`, `pub session: Option<SessionHandle>`
   - `NoopClient` for liveness-only connections
   - `FromVoxSession` simplified: takes `Caller` instead of `DriverCaller`
   - Driver tests ported to `vox/tests/` using `#[vox::service]` generated clients
   - Internal tests use `caller.driver()` escape hatch for raw protocol testing
6. ✅ `establish()` returns `Client` directly (not `(Client, SessionHandle)`)
7. ✅ `SessionConfig` struct deduplicates shared fields across 5 builder types
8. Typed virtual connection helpers
9. Server factory API with `ConnectionContext`
10. Root metadata support
11. Facade re-export hygiene
12. Connect timeout
13. Keep existing builders as lower-level escape hatch

## SHM Transport in `connect()` ✅

**Done.** The SHM bootstrap protocol (in `shm-primitives/src/bootstrap.rs`)
now sends 4 FDs (doorbell, segment, mmap_rx, mmap_tx) over SCM_RIGHTS,
eliminating the need for FD inheritance. The SID field was removed from
the wire format (it was vixen-specific). Magic renamed from RSH0/RSP0
to VSH1/VSP1.

`ShmLinkSource` (in `vox-shm/src/bootstrap.rs`) performs the full
guest-side bootstrap on each `next_link()` call:

1. Connect to Unix control socket
2. Send 4-byte magic (`VSH1`)
3. Receive response + 4 FDs
4. Attach segment, claim peer slot, build `ShmLink`

Usage: `vox::connect("shm:///path/to/control.sock")`

## Connect Timeout

`vox::connect()` currently has no overall timeout. The individual
transports have their own timeouts (TCP has configurable resolve/connect
timeouts), but the vox handshake that follows can stall indefinitely.

**Desired direction:** `connect()` should accept an optional timeout
(or have a sensible default) that covers the entire operation: transport
setup + vox handshake.

## Open Questions

- Exact wire format for root metadata (handshake extension vs separate open-like step)
- Exact `LinkInfo` shape across TCP/local/SHM/websocket
- Whether factory creation can be fallible with structured rejection metadata
- Whether factory should be async trait, closure, or both
- How to stage behavior changes for root caller-drop semantics safely
