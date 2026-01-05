A high-performance RPC framework with implementations in **Rust**, **Swift**, and **TypeScript**.

## Implementations

| Language | Directory | Status | Description |
|----------|-----------|--------|-------------|
| **Rust** | [`rust/`](./rust/) | In progress | New implementation targeting `docs/content/` |
| **Rust (legacy)** | [`rust-legacy/`](./rust-legacy/) | Stable | Legacy implementation with all transports |
| **TypeScript** | [`typescript/`](./typescript/) | Stable | WebSocket client for browsers & Node.js |
| **Swift** | [`swift/`](./swift/) | WIP | Native client for iOS/macOS |

## Features

- **Multiple transports**: Choose the right transport for your use case
  - Shared memory (SHM): Ultra-low latency for local processes
  - TCP/Unix sockets: Network communication
  - WebSocket: Browser and web clients
  - In-memory: Testing and single-process RPC

- **Streaming**: Full support for server and client streaming

- **Code generation**: Define services once in Rust, generate clients for all languages

- **Type-safe**: Compile-time verification of RPC calls

- **Cross-platform**: Linux, macOS, Windows, iOS, and WebAssembly

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

## Rust Crates

- **rapace**: Main framework (re-exports transports)
- **rapace-core**: Core types and protocols
- **rapace-macros**: Service macro
- **rapace-registry**: Service metadata
- **Transports**: mem, stream (TCP/Unix), websocket, shm
- **rapace-explorer**: Dynamic service discovery

## TypeScript

- **@bearcove/rapace**: WebSocket client with postcard serialization ([npm](https://www.npmjs.com/package/@bearcove/rapace))

## Swift

- **Rapace**: TCP client with async/await
- **Postcard**: Binary serialization compatible with Rust
