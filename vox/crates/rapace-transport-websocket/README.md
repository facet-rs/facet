# rapace-transport-websocket

[![crates.io](https://img.shields.io/crates/v/rapace-transport-websocket.svg)](https://crates.io/crates/rapace-transport-websocket)
[![documentation](https://docs.rs/rapace-transport-websocket/badge.svg)](https://docs.rs/rapace-transport-websocket)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-websocket.svg)](./LICENSE)

WebSocket transport for rapace RPC.

Enable RPC communication over WebSocket connections for browser clients and web servers.

## Features

- **Browser support**: WebAssembly clients in the browser
- **Multiple WebSocket backends**: Support for tokio-tungstenite and axum
- **Cross-platform**: Works on both native and WASM targets

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `tungstenite` | Yes | Support for `tokio-tungstenite::WebSocketStream` |
| `axum` | No | Support for `axum::extract::ws::WebSocket` |

## Usage

### With tokio-tungstenite (default)

```rust
use rapace::RpcSession;
use rapace_transport_websocket::TungsteniteTransport;

// Accept a WebSocket connection with tokio-tungstenite
let ws_stream = tokio_tungstenite::accept_async(tcp_stream).await?;
let transport = TungsteniteTransport::new(ws_stream);
let session = RpcSession::new(transport);
```

### With axum

Enable the `axum` feature in your `Cargo.toml`:

```toml
[dependencies]
rapace-transport-websocket = { version = "0.4", features = ["axum"] }
```

Then use `AxumTransport` in your WebSocket handler:

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use rapace::RpcSession;
use rapace_transport_websocket::AxumTransport;

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let transport = AxumTransport::new(socket);
    let session = RpcSession::new(transport);
    // Use session...
}
```

### WASM client

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
