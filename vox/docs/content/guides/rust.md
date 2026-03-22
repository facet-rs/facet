+++
title = "Rust Guide"
description = "Use roam + a transport crate to define services, run drivers, and call methods with channels."
weight = 21
+++

The best way to learn the Rust API is to run the examples in order, from simplest to most complex.

## 1) `borrowed_and_channels` (smallest complete RPC)

- Source: [rust-examples/examples/borrowed_and_channels.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/borrowed_and_channels.rs)
- Run: `cargo run -p rust-examples --example borrowed_and_channels`
- Learn: borrowed args, borrowed returns, and `Rx<T>`/`Tx<T>` channel args.

> ```rust
> async fn is_short(&self, word: &str) -> bool;
> async fn classify(&self, word: String) -> &'roam str;
> async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
> ```

## 2) `virtual_connections` (multiple services on one session)

- Source: [rust-examples/examples/virtual_connections.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/virtual_connections.rs)
- Run: `cargo run -p rust-examples --example virtual_connections`
- Learn: `open_connection`, metadata-based accept, and independent per-vconn drivers.

> ```rust
> match requested_service(metadata) {
>     Some("counter") => { ... }
>     Some("string") => { ... }
>     _ => Err(...),
> }
> ```

## 3) `stable_conduit_reconnect` (reconnect + preserved state/channels)

- Source: [rust-examples/examples/stable_conduit_reconnect.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/stable_conduit_reconnect.rs)
- Run: `cargo run -p rust-examples --example stable_conduit_reconnect`
- Learn: forced link cuts with `StableConduit`, automatic re-establish, service state continuity, and channel continuity across reconnect.

> ```rust
> println!("[demo] intentionally cutting physical link #1 mid-channel");
> ...
> assert_eq!(transformed_count, 3);
> assert_eq!(second, 2);
> ```

## 4) `memory_proxying` (connection-level proxying)

- Source: [rust-examples/examples/memory_proxying.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/memory_proxying.rs)
- Run: `cargo run -p rust-examples --example memory_proxying`
- Learn: host bridges one virtual connection to another without service-specific forwarding code.

> ```rust
> roam::proxy_connections(incoming_handle, upstream_conn).await;
> ```

## 5) `shm_host_two_guests` (most complex: host + multiple guest processes)

- Source: [rust-examples/examples/shm_host_two_guests.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/shm_host_two_guests.rs)
- Run (Unix): `cargo run -p rust-examples --example shm_host_two_guests`
- Learn: one host process launching two guest processes, SHM bootstrap, and serving different services from each guest.

> ```rust
> println!("[host] launching guest: Adder");
> println!("[host] launching guest: StringReverser");
> ```

## Practical API pattern

Most application code only needs `roam` + one transport crate.

```toml
[dependencies]
roam = "7.0.0"
roam-stream = "7.0.0"
tokio = { version = "1", features = ["rt", "net"] }
eyre = "0.6"
```

Define a service with `#[roam::service]`, implement it, and establish on each side:

```rust
let (server_guard, _) = roam::acceptor(StreamLink::tcp(server_socket))
    .establish::<WordLabClient>(WordLabDispatcher::new(WordLabService))
    .await?;

let (client, _session_handle) = roam::initiator(StreamLink::tcp(client_socket))
    .establish::<WordLabClient>(())
    .await?;
```

For borrowed returns, implementations receive a `Call` sink:

```rust
async fn classify<'roam>(
    &self,
    call: impl roam::Call<'roam, &'roam str, std::convert::Infallible>,
    word: String,
) {
    call.ok("short").await;
}
```

For non-Rust bindings, generate code from service descriptors with `roam-codegen`.
