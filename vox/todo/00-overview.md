# WASM Support for roam-websocket

## Status: COMPLETE (2025-01-19)

**What's been achieved:**
- Runtime abstraction in `roam-session/src/runtime/` (tokio + wasm)
- WASM WsTransport in `roam-websocket/src/wasm.rs`
- MessageTransport trait in `roam-session/src/transport.rs`
- Driver, accept_framed, initiate_framed moved to `roam-session`
- dodeca-devtools compiles for WASM without roam-stream dependency

## Completed Phases

1. [01-DONE-move-driver.md](./01-DONE-move-driver.md) - Moved Driver to roam-session
2. [02-DONE-dodeca-integration.md](./02-DONE-dodeca-integration.md) - dodeca-devtools WASM compilation

## Key Changes

### roam-session
- New `driver.rs` with `Driver`, `HandshakeConfig`, `NoDispatcher`, `accept_framed`, `initiate_framed`, etc.
- Uses `crate::runtime::*` (Mutex, channel, spawn, etc.) for WASM portability
- Added `futures-util` dependency for both native and WASM

### roam-stream
- Kept byte-stream specific code: `Connector`, `Client`, `accept`, `connect`, `CobsFramed`
- Re-exports moved types from roam-session for backwards compatibility

### dodeca-devtools
- Removed roam-stream dependency
- Uses roam-session directly for `ConnectionHandle`, `HandshakeConfig`, `NoDispatcher`, `accept_framed`

## Next Steps (if needed)

- Test end-to-end: run dodeca, verify devtools connects in browser
- Clean up legacy protocol types in dodeca-protocol (ClientMessage, ServerMessage)
