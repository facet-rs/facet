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

## Service Routing: Automatic Service Name Metadata

**Key insight:** the client already knows which service it wants (it's the
type parameter). The service name should be sent automatically as metadata,
and the server should route based on it.

### Client side

Generated clients know their service name from the `ServiceDescriptor`.
When `connect::<FooClient>(addr)` or `session.open::<FooClient>()` is
called, the service name (`"Foo"`) is sent automatically:

- **Root connections:** service name sent as handshake metadata
- **Virtual connections:** service name sent as `ConnectionOpen` metadata

```rust
// Root — sends "Hello" as service name automatically
let client: HelloClient = vox::connect(addr).await?;

// Virtual — sends "Chat" as service name automatically
let chat: ChatClient = client.session.unwrap().open::<ChatClient>().await?;
```

This requires a trait on generated clients that exposes the service name.
`FromVoxSession` could carry this, or a separate `VoxService` trait.

### Server side

A factory closure receives a `ConnectionContext` with the requested
service name and returns the appropriate dispatcher:

```rust
vox::serve(listener, |cx: &ConnectionContext| match cx.service_name() {
    "Hello" => Some(HelloDispatcher::new(HelloService)),
    "Chat" => Some(ChatDispatcher::new(ChatService)),
    _ => None, // reject unknown services
}).await?;
```

The same factory handles both root and virtual connections.

### Everything is metadata

The `vox-` prefix is already reserved for internal metadata keys:
`vox-session-key`, `vox-retry-support`, `vox-operation-id`, etc.

Service routing and transport info use the same mechanism:

| Key | Value | Set by |
|-----|-------|--------|
| `vox-service` | Service name (e.g. `"Hello"`) | Client, automatically from type param |
| `vox-transport` | Transport type (`"tcp"`, `"local"`, `"shm"`, `"ws"`) | Transport layer |
| `vox-peer-addr` | Remote address (e.g. `"192.168.1.1:4000"`) | TCP/WS transport |
| `vox-peer-pid` | Peer process ID | Unix socket transport |
| `vox-connection-kind` | `"root"` or `"virtual"` | Session layer |

The factory just reads metadata — no special enum for transport info,
no separate `ConnectionContext` struct. It's all `Metadata`:

```rust
vox::serve(listener, |metadata: &Metadata| {
    let service = metadata.get_str("vox-service")?;
    match service {
        "Hello" => Some(HelloDispatcher::new(HelloService)),
        "Chat" => Some(ChatDispatcher::new(ChatService)),
        _ => None,
    }
}).await?;
```

If the factory needs transport details (for auth, logging, etc), it
reads more metadata:

```rust
vox::serve(listener, |metadata: &Metadata| {
    let service = metadata.get_str("vox-service")?;
    let peer = metadata.get_str("vox-peer-addr").unwrap_or("unknown");
    log::info!("new {service} connection from {peer}");
    // ...
}).await?;
```

### Why this replaces `ConnectionAcceptor`

Today, `ConnectionAcceptor` is a trait that handles virtual connection
setup. With the factory model, root and virtual connections go through
the same routing — both just present metadata to the factory.
`ConnectionAcceptor` becomes unnecessary.

### User metadata

Service name and transport info are injected automatically. Users can
still attach additional metadata for auth tokens, routing hints, etc:

```rust
let chat: ChatClient = session
    .open::<ChatClient>()
    .metadata(vec![auth_token_entry])
    .await?;
```

The factory sees everything — `vox-*` internal metadata and user metadata
together.

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
8. ✅ Service routing: automatic service name metadata + server factory
   - Client sends service name automatically (from type parameter)
   - `session.open::<FooClient>()` for typed virtual connections
   - `ErasedHandler` trait for object-safe boxed handlers
   - `vox-service` metadata auto-injected, `metadata_get_str`/`metadata_get_u64` helpers
   - `vox-connection-kind` metadata injected (`"root"` or `"virtual"`)
9. ✅ Metadata ergonomics
   - `MetadataEntry::str(key, value)`, `MetadataEntry::u64(key, value)` constructors
   - `MetadataBuilder` via `metadata_build().str(k, v).u64(k, v).done()`
10. ✅ Connect timeout
    - `connect_timeout`: per-attempt timeout (default 5s in `vox::connect()`)
    - `recovery_timeout`: total recovery window before giving up
    - `SessionConfig` refactored — helpers pass config as whole struct, not individual fields
11. ✅ Unified `ConnectionAcceptor` + `PendingConnection`
    - `ConnectionRequest` wraps metadata with typed getters
      (`service()`, `transport()`, `peer_addr()`, `is_root()`, `is_virtual()`)
    - `ConnectionAcceptor::accept(&self, req: &ConnectionRequest, conn: PendingConnection)
       -> Result<(), Metadata<'static>>`
    - `PendingConnection`: `handle_with()`, `proxy_to()`, `into_handle()`
    - Blanket impl: any `Handler<DriverReplySink> + Clone` is a `ConnectionAcceptor`
    - `acceptor_fn()` wrapper for closure-based factories
    - `AcceptedConnection`, `ConnectionSetup` removed
    - Root and virtual connections go through the same acceptor
    - `establish()` no longer takes a `handler` parameter
    - `proxy_connections()` returns `Result`, checks parity mismatch
12. ✅ `vox::serve(listener, acceptor)` — symmetric to `vox::connect()`
    - `VoxListener` trait implemented for `TcpListener`, `LocalLinkAcceptor`
    - Accepts connections in a loop, spawns sessions
    - Single-service: `vox::serve(listener, EchoDispatcher::new(EchoService))`
    - Multi-service factory: `vox::serve(listener, acceptor_fn(|req, conn| { ... }))`
13. ✅ Facade re-export hygiene + crate documentation
    - Replaced `pub use vox_core::*` with curated, grouped explicit re-exports
    - Rewrote crate-level doc comment with `connect()` / `serve()` examples
    - Organized vox-core re-exports by category: session builders, connection management,
      driver/caller, conduits, stable conduit, memory links, handshake, transport, operations
14. Multi-listener support
    - Serve on multiple transports simultaneously (e.g. TCP + Unix socket + SHM)
    - Compose multiple `VoxListener`s into one

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

## Connect Timeout ✅

**Done.** Two knobs:

- `connect_timeout`: bounds each individual connection attempt (transport
  + handshake). Default 5s in `vox::connect()`. Applied to both initial
  `establish()` and each reconnection attempt in `BareSourceRecoverer`.
- `recovery_timeout`: bounds total recovery time after a link breaks. If
  the session can't reconnect within this window, it gives up and the
  session dies. No default (unlimited retries unless set).

`SessionConfig` refactored: helpers pass config as a whole struct instead
of destructuring into 8+ individual fields.

## Open Questions

- How to stage behavior changes for root caller-drop semantics safely
