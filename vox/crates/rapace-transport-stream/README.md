# rapace-transport-stream

[![crates.io](https://img.shields.io/crates/v/rapace-transport-stream.svg)](https://crates.io/crates/rapace-transport-stream)
[![documentation](https://docs.rs/rapace-transport-stream/badge.svg)](https://docs.rs/rapace-transport-stream)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-stream.svg)](./LICENSE)

TCP and Unix socket transport for rapace RPC.

Network transport for local and remote communication via TCP or Unix domain sockets.

## Features

- **TCP**: `tcp://localhost:9000` - remote communication, cross-machine
- **Unix sockets**: `unix:///tmp/rapace.sock` - efficient local IPC on Unix-like systems
- **Secure**: Use TLS for encrypted communication

## Usage

```rust
use rapace::RpcSession;
use rapace_transport_stream::TcpTransport;

let transport = TcpTransport::connect("127.0.0.1:9000").await?;
let session = RpcSession::new(transport);
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
