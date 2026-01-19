# Phase 1: Move Driver to roam-session

## Goal

Move the generic protocol machinery from `roam-stream` to `roam-session` so WASM can use it.

## What to Move

From `roam-stream/src/driver.rs` to `roam-session/src/driver.rs`:

- `Driver<T, D>` struct
- `HandshakeConfig`
- `ConnectionError`
- `Negotiated`
- `NoDispatcher`
- `accept_framed()` function
- `connect_framed()` function
- `FramedClient`
- `MessageConnector` trait
- `RetryPolicy`

## What Stays in roam-stream

- `CobsFramed` (byte-stream framing)
- `accept()` / `connect()` convenience wrappers that use CobsFramed
- `Connector` trait (byte-stream specific)

## Steps

1. Create `roam-session/src/driver.rs`
2. Move the types listed above
3. Update imports to use `crate::runtime::*` instead of `tokio::*`
4. Update `roam-stream` to re-export from `roam-session` for backwards compat
5. Update `roam-websocket` to import from `roam-session`
6. Run tests: `cargo nextest run -p roam-session -p roam-stream -p roam-websocket`

## Key Change: Replace tokio with runtime abstraction

```rust
// Before (in roam-stream)
use tokio::sync::mpsc;
use tokio::spawn;

// After (in roam-session)
use crate::runtime::{channel, spawn, Sender, Receiver};
```

## Verification

```bash
# All native tests pass
cargo nextest run -p roam-session -p roam-stream -p roam-websocket

# WASM compiles
cargo build -p roam-websocket --target wasm32-unknown-unknown
```
