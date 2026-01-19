# WASM Support for roam-websocket

## Goal

Make `roam-websocket` work seamlessly in both native (tokio) and WASM (browser) environments,
sharing the same Driver, ConnectionHandle, and protocol logic.

## Current Problem

```
roam-stream depends on tokio for:
  - Driver (spawn, mpsc, oneshot, timeout, sleep)
  - CobsFramed (AsyncRead/AsyncWrite)
  - MessageTransport trait

roam-session depends on tokio for:
  - mpsc channels
  - oneshot channels
  - spawn for dispatch handlers
  - tunnel_stream (AsyncRead/AsyncWrite)
```

The tokio dependencies fall into two buckets:

1. **WASM-portable** (has equivalents): spawn, mpsc, oneshot, timeout, sleep
2. **NOT WASM-portable** (byte-stream I/O): AsyncRead, AsyncWrite, CobsFramed, tunnel_stream

## Solution

Restructure so that:
- Core protocol logic uses abstract runtime traits (portable)
- Byte-stream specific code stays separate (not portable, not needed for WebSocket)
- WebSocket transport implements MessageTransport for both native and WASM

## Phases

1. [01-runtime-abstraction.md](./01-runtime-abstraction.md) - Create runtime abstraction layer
2. [02-restructure-crates.md](./02-restructure-crates.md) - Move Driver/MessageTransport to roam-session
3. [03-wasm-runtime.md](./03-wasm-runtime.md) - Implement WASM runtime
4. [04-wasm-wstransport.md](./04-wasm-wstransport.md) - Add WASM WsTransport
5. [05-integration.md](./05-integration.md) - Test and integrate
