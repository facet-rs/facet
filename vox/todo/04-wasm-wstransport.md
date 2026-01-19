# Phase 4: WASM WsTransport

## Goal

Implement `MessageTransport` for `web_sys::WebSocket` so the same Driver
works in the browser.

## Challenge

`web_sys::WebSocket` is callback-based, not async. Need to bridge to async.

## Design

```rust
/// WASM WebSocket transport.
pub struct WsTransport {
    ws: WebSocket,
    // Receiver for incoming messages (fed by onmessage callback)
    incoming_rx: mpsc::UnboundedReceiver<Result<Vec<u8>, WsError>>,
    // Keep callback closures alive
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    // Last decoded bytes for error detection
    last_decoded: Vec<u8>,
}
```

## Implementation

### Constructor

```rust
impl WsTransport {
    pub async fn connect(url: &str) -> Result<Self, WsError> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let (incoming_tx, incoming_rx) = mpsc::unbounded();
        let (open_tx, open_rx) = oneshot::channel();

        // Set up callbacks...
        let on_open = Closure::once(move |_| {
            let _ = open_tx.send(Ok(()));
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let on_message = Closure::new(move |event: MessageEvent| {
            // Extract binary data, send to incoming_tx
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Wait for connection
        open_rx.await??;

        Ok(Self { ws, incoming_rx, ... })
    }
}
```

### MessageTransport impl

```rust
impl MessageTransport for WsTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let bytes = facet_postcard::to_vec(msg)?;
        let array = Uint8Array::from(bytes.as_slice());
        self.ws.send_with_array_buffer(&array.buffer())
            .map_err(|e| io::Error::other(format!("{:?}", e)))?;
        Ok(())
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        match self.incoming_rx.next().await {
            Some(Ok(bytes)) => {
                self.last_decoded = bytes.clone();
                let msg = facet_postcard::from_slice(&bytes)?;
                Ok(Some(msg))
            }
            Some(Err(e)) => Err(e.into()),
            None => Ok(None), // Channel closed
        }
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        match crate::runtime::timeout(timeout, self.recv()).await {
            Some(result) => result,
            None => Ok(None),
        }
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}
```

## File Structure

```
roam-websocket/src/
├── lib.rs           # Conditional exports
├── native.rs        # tokio-tungstenite impl (existing)
└── wasm.rs          # web_sys::WebSocket impl (new)
```

### lib.rs

```rust
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

// Common re-exports
pub use roam_session::{Message, Hello};
```

## Dependencies

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
js-sys = "0.3"
web-sys = { version = "0.3", features = [
    "WebSocket", "BinaryType", "MessageEvent",
    "ErrorEvent", "CloseEvent", "console"
] }
futures-channel = "0.3"
futures-util = { version = "0.3", default-features = false }
```

## Usage in Browser

```rust
use roam_websocket::WsTransport;
use roam_session::{accept_framed, HandshakeConfig, NoDispatcher};

let transport = WsTransport::connect("ws://localhost:9000").await?;
let (handle, driver) = accept_framed(transport, HandshakeConfig::default(), NoDispatcher).await?;

// Spawn driver (uses runtime abstraction)
roam_session::runtime::spawn(async move {
    driver.run().await;
});

// Use handle
let client = MyServiceClient::new(handle);
```
