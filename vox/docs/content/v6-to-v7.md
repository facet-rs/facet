+++
title = "Migrating from v6 to v7"
description = "Practical API migration notes for Rust users moving to roam v7"
weight = 16
+++

This page maps the old v6 Rust API surface to the current v7 API.

## High-level changes

1. Service impls no longer receive `&Context`; generated methods now:
   - return values directly for owned success returns, or
   - receive `call: impl roam::Call<T, E>` for explicit `'roam` borrowed success returns.
2. The generated service trait is `{Service}`.
3. Generated client calls return `T` for owned returns, and `SelfRef<T>` for explicit `'roam` borrowed returns.
4. `ConnectionHandle` is no longer a `Caller`; clients are built from `driver.caller()`.
5. Session setup moved from `accept_framed` / `initiate_framed` to `session::acceptor` / `session::initiator` builders.
6. Channels are now `Tx<T, N>` / `Rx<T, N>` with const-generic initial credit (`N`, default `16`).
7. Codegen input moved from `*_service_detail()` to `*_service_descriptor()`.

## Service macro changes

v6 style:

```rust
#[roam::service]
trait Adder {
    async fn add(&self, a: i32, b: i32) -> i32;
}

impl Adder for MyAdder {
    async fn add(&self, _cx: &roam::Context, a: i32, b: i32) -> i32 {
        a + b
    }
}
```

v7 style (owned return):

```rust
#[roam::service]
trait Adder {
    async fn add(&self, a: i32, b: i32) -> i32;
}

impl Adder for MyAdder {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
```

For explicit `'roam` borrowed success returns, v7 methods use `call` and reply
explicitly with `call.ok(...)`, `call.err(...)`, or `call.reply(...)`.

## Client return type changes

v6 call sites often expected bare values:

```rust
let sum: i32 = client.add(3, 5).await?;
```

In v7:

```rust
let sum: i32 = client.add(3, 5).await?;
```

Borrowed return payloads use explicit `'roam` and are returned as `SelfRef<T>`:

```rust
// service method: async fn hash(&self, payload: Vec<u8>) -> &'roam [u8];
let hash: roam::SelfRef<&'roam [u8]> = client.hash(bytes).await?;
```

Generated Rust clients do not expose response metadata in return types.

## Session and driver setup

v6 used handshake helpers returning a handle/driver pair:

```rust
use roam_session::{accept_framed, initiate_framed, HandshakeConfig, NoDispatcher};
let (handle, _incoming, driver) = initiate_framed(transport, HandshakeConfig::default(), NoDispatcher).await?;
```

v7 setup is explicit:

```rust
let (mut session, handle, _session_handle) = roam_core::session::initiator(conduit)
    .establish()
    .await?;

let mut driver = roam_core::Driver::new(handle, dispatcher_or_handler, roam_types::Parity::Odd);
let caller = driver.caller();
```

Server side is analogous via `roam_core::session::acceptor(conduit).establish().await?`.

## Codegen build script migration (Swift example)

Your old v6 pattern used:

```rust
let service = vixenfs_proto::vfs_service_detail();
let swift = roam_codegen::targets::swift::generate_service_with_bindings(
    &service,
    roam_codegen::targets::swift::SwiftBindings::Client,
);
```

In v7, switch to the generated service descriptor function:

```rust
let service = vixenfs_proto::vfs_service_descriptor();
let swift = roam_codegen::targets::swift::generate_service_with_bindings(
    service,
    roam_codegen::targets::swift::SwiftBindings::Client,
);
```

Notes:

1. `generate_service_with_bindings` still exists in v7.
2. The main migration point is `*_service_detail()` -> `*_service_descriptor()`.
3. Descriptor functions return `&'static ServiceDescriptor`, so pass it directly.

## Old-to-new symbol map

| v6 | v7 | Notes |
|---|---|---|
| `impl MyService for Handler` | `impl MyService for Handler` | Generated service trait remains `{Service}` |
| `fn method(&self, cx: &Context, ...) -> T` | `fn method(&self, ...) -> Future<Output = T>` | Owned return path |
| `fn method(&self, cx: &Context, ...) -> &'a U` | `fn method<'roam>(&self, call: impl Call<&'roam U, E>, ...) -> Future<Output = ()>` | Borrowed reply path |
| `MyServiceClient::new(connection_handle)` | `MyServiceClient::new(driver.caller())` | `ConnectionHandle` is no longer a `Caller` |
| `accept_framed` / `initiate_framed` | `session::acceptor` / `session::initiator` + `.establish()` | New session builders |
| `Tx<T>` / `Rx<T>` | `Tx<T, N>` / `Rx<T, N>` | `N` is initial credit (default `16`) |
| `client.method(...).with_metadata(...)` | no generated call-builder equivalent | Use lower-level request construction if needed |
| `*_service_detail()` | `*_service_descriptor()` | Used by `roam-codegen` targets |

## Migration checklist

1. Keep service impls on `{Service}`.
2. Replace `&Context` parameters:
   - for owned returns, remove context and return values directly;
   - for explicit `'roam` borrowed returns, use `call: impl roam::Call<Ok, Err>`.
3. For borrowed-return methods, reply with `call.ok(...)` / `call.err(...)`.
4. Update client call sites for `'roam`-borrowing methods to handle `SelfRef<T>`.
5. Update bootstrap code to `session::{initiator,acceptor}` and instantiate `Driver`.
6. Switch client construction from connection handles to `driver.caller()`.
7. If needed, annotate channel credit explicitly with `Tx<T, N>` / `Rx<T, N>`.
8. Update custom codegen scripts from `*_service_detail()` to `*_service_descriptor()`.

## Virtual connections in v7 (Rust)

v7 Rust exposes virtual connections directly on `SessionHandle`:

```rust
let (mut session, root_handle, session_handle) = roam_core::session::initiator(conduit)
    .establish()
    .await?;

let mut root_driver = roam_core::Driver::new(root_handle, root_dispatcher, roam_types::Parity::Odd);
let root_caller = root_driver.caller();
```

Open a new virtual connection from the existing session:

```rust
let vconn_handle = session_handle
    .open_connection(
        roam_types::ConnectionSettings {
            parity: roam_types::Parity::Odd,
            max_concurrent_requests: 64,
        },
        vec![],
    )
    .await?;

// This virtual connection can use a different handler/dispatcher and caller
// than the root connection.
let mut vconn_driver = roam_core::Driver::new(vconn_handle, vconn_dispatcher, roam_types::Parity::Odd);
let vconn_caller = vconn_driver.caller();
```

Accepting inbound virtual connections is opt-in on both `initiator(...)` and
`acceptor(...)` builders:

```rust
let (mut session, root_handle, _session_handle) = roam_core::session::acceptor(conduit)
    .on_connection(my_acceptor)
    .establish()
    .await?;
```

If `.on_connection(...)` is not configured, inbound virtual `ConnectionOpen`
messages are rejected by default.

## SHM hosting API migration notes

If you were using older host orchestration symbols such as:

- `roam_shm::ShmHost`
- `roam_shm::bootstrap`
- `roam_shm::driver`
- `roam_shm::AddPeerOptions`
- `roam_shm::MultiPeerHostDriver`

those are not part of the current v7 `roam-shm` API in this repository.

Current v7 `roam-shm` exposes lower-level building blocks instead:

1. `roam_shm::segment::Segment` and `SegmentConfig` for segment lifecycle.
2. `Segment::{reserve_peer, claim_peer, attach_peer, detach_peer, recover_crashed_peer}` for peer lifecycle.
3. `roam_shm::ShmLink::{for_host, for_guest}` for one host-guest link.
4. `roam_core::{BareConduit, session::{initiator,acceptor}, Driver}` for RPC/session runtime on top of that link.

To reduce migration friction, v7 now also ships a thin Unix orchestration module:

- `roam_shm::host::HostHub` and `HostHub::prepare_peer()`
- `roam_shm::host::GuestSpawnTicket`
- `roam_shm::host::{guest_link_from_raw, guest_link_from_ticket}`

This is intentionally smaller than the old monolithic host API: SHM stays a
transport, while orchestration helpers package only spawn/peer wiring.
