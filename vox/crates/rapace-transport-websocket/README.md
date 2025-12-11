# rapace-transport-websocket

[![crates.io](https://img.shields.io/crates/v/rapace-transport-websocket.svg)](https://crates.io/crates/rapace-transport-websocket)
[![documentation](https://docs.rs/rapace-transport-websocket/badge.svg)](https://docs.rs/rapace-transport-websocket)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-websocket.svg)](./LICENSE)

WebSocket transport for rapace RPC.

Enable RPC communication over WebSocket connections for browser clients and web servers.

## Features

- **Browser support**: WebAssembly clients in the browser
- **Server-side WebSocket**: Accept WebSocket connections from web clients
- **Cross-platform**: Works on both native and WASM targets

## Usage

Native server:
```rust
use rapace::RpcSession;
use rapace_transport_websocket::WebSocketTransport;

// Accept WebSocket connections...
```

WASM client:
```rust
use rapace::RpcSession;
use rapace_transport_websocket::WebSocketTransport;

let transport = WebSocketTransport::connect("ws://localhost:9000").await?;
let session = RpcSession::new(transport);
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
