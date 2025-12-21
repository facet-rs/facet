# rapace

[![crates.io](https://img.shields.io/crates/v/rapace.svg)](https://crates.io/crates/rapace)
[![documentation](https://docs.rs/rapace/badge.svg)](https://docs.rs/rapace)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace.svg)](./LICENSE)

High-performance RPC framework with support for multiple transports (shared memory, TCP, WebSocket) and streaming.

## Features

- **Multiple transports**: In-process (mem), TCP/Unix socket (stream), WebSocket, and shared memory (SHM)
- **Streaming**: Full support for server and client streaming RPC calls
- **Code generation**: Automatic client and server code generation from trait definitions
- **Zero-copy**: Shared memory transport for ultra-low latency
- **Cross-platform**: Works on Linux, macOS, Windows, and WebAssembly

## Quick Start

```rust
use rapace::service;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, a: i32, b: i32) -> i32;
}
```

See the [examples](https://github.com/bearcove/rapace/tree/main/demos) for more.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
