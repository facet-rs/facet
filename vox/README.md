# rapace

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/rapace/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/rapace.svg)](https://crates.io/crates/rapace)
[![documentation](https://docs.rs/rapace/badge.svg)](https://docs.rs/rapace)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

A high-performance RPC framework for Rust with support for shared memory, TCP, WebSocket, and in-process transports.

## Features

- **Multiple transports**: Choose the right transport for your use case
  - Shared memory (SHM): Ultra-low latency for local processes
  - TCP/Unix sockets: Network communication
  - WebSocket: Browser and web clients
  - In-memory: Testing and single-process RPC

- **Streaming**: Full support for server and client streaming

- **Code generation**: Write your service interface once with `#[rapace::service]`

- **Type-safe**: Compile-time verification of RPC calls

- **Cross-platform**: Linux, macOS, Windows, and WebAssembly

## Quick Start

```rust
use rapace::service;
use rapace::RpcSession;
use rapace_transport_mem::MemTransport;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
}

// Implement your service...
struct MyCalculator;
impl Calculator for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

// Use it with any transport
let (client_transport, server_transport) = MemTransport::pair();
let session = RpcSession::new(client_transport);
let client = CalculatorClient::new(session);
```

## Documentation

See the [crate documentation](https://docs.rs/rapace) and [examples](https://github.com/bearcove/rapace/tree/main/demos).

## Crates

- **rapace**: Main framework (re-exports transports)
- **rapace-core**: Core types and protocols
- **rapace-macros**: Service macro
- **rapace-registry**: Service metadata
- **Transports**:
  - rapace-transport-mem
  - rapace-transport-stream (TCP/Unix)
  - rapace-transport-websocket
  - rapace-transport-shm
- **rapace-explorer**: Dynamic service discovery
- **rapace-testkit**: Transport conformance tests

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
