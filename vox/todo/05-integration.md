# Phase 5: Integration and Testing

## Goal

Verify everything works end-to-end for both native and WASM.

## Test Matrix

| Transport | Runtime | Test |
|-----------|---------|------|
| CobsFramed (TCP) | tokio | Existing tests |
| WsTransport (tungstenite) | tokio | Existing tests |
| WsTransport (web_sys) | WASM | New tests |

## Native Tests

Existing tests should continue to pass:

```bash
cargo nextest run -p roam-session
cargo nextest run -p roam-stream
cargo nextest run -p roam-websocket
```

## WASM Tests

### Option A: wasm-pack test

```bash
cd rust/roam-websocket
wasm-pack test --headless --firefox
```

Requires test server running to connect to.

### Option B: Integration test with peer-server

The repo already has `typescript/peer-server` for browser tests.
Could add a Rust WASM test that connects to it.

### Test Scenario

1. Start native WebSocket server (peer-server or custom)
2. WASM client connects via `WsTransport::connect()`
3. Perform Hello handshake (via Driver)
4. Make RPC call
5. Verify response

```rust
// tests/wasm.rs (run with wasm-pack test)
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_websocket_rpc() {
    let transport = WsTransport::connect("ws://localhost:9000").await.unwrap();
    let (handle, driver) = accept_framed(transport, HandshakeConfig::default(), NoDispatcher)
        .await
        .unwrap();

    roam_session::runtime::spawn(async move {
        let _ = driver.run().await;
    });

    // Make a call
    let response = handle.call_raw(METHOD_ECHO, b"hello".to_vec()).await.unwrap();
    assert_eq!(response, b"hello");
}
```

## dodeca Integration

Once roam WASM support is complete:

1. Update `dodeca-devtools` to use `roam-websocket::WsTransport`
2. Define `DevtoolsService` trait
3. Host implements it
4. cell-http uses `ForwardingDispatcher` to proxy browser → host
5. Remove old hand-rolled `ClientMessage`/`ServerMessage`

## Checklist

- [ ] Phase 1: Runtime abstraction implemented
- [ ] Phase 2: Crates restructured
- [ ] Phase 3: WASM runtime implemented
- [ ] Phase 4: WASM WsTransport implemented
- [ ] Native tests pass
- [ ] WASM tests pass
- [ ] dodeca-devtools migrated
- [ ] Old protocol removed
