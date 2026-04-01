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

## Generated Client: Session Access Without Name Clashes

We should not add arbitrary inherent methods to generated `*Client` types,
because service methods can clash with any added method names.

Use trait + helper function:

```rust
pub trait VoxSessionCarrier {
    fn __vox_session_handle(&self) -> &vox::SessionHandle;
}

pub fn session_handle<T: VoxSessionCarrier + ?Sized>(value: &T) -> &vox::SessionHandle {
    value.__vox_session_handle()
}
```

Generated clients implement `VoxSessionCarrier`.

This avoids polluting the service method namespace while keeping advanced
session operations available.

## Virtual Connections Should Stay Typed

When users need sub-connections, the path should stay typed (not raw low-level calls):

```rust
let client: RootClient = vox::connect(addr).await?;
let sh = vox::session_handle(&client);
let sub: ChatClient = sh.open_typed::<ChatClient>(settings, metadata).await?;
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

## Migration Approach

1. Add `connect(...).await?` happy path returning typed client via inference
2. Add `VoxSessionCarrier` + `vox::session_handle(&client)` helper
3. Add typed virtual connection helper(s)
4. Introduce server factory API with `ConnectionContext`
5. Add root metadata support so root and virtual selection are symmetric
6. Keep existing builders as lower-level escape hatch during migration

## Open Questions

- Exact wire format for root metadata (handshake extension vs separate open-like step)
- Exact `LinkInfo` shape across TCP/local/SHM/websocket
- Whether factory creation can be fallible with structured rejection metadata
- Whether factory should be async trait, closure, or both
- How to stage behavior changes for root caller-drop semantics safely
