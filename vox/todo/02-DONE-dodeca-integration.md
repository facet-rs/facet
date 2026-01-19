# Phase 2: dodeca-devtools Integration

## Goal

Make dodeca-devtools compile for WASM and use roam RPC.

## Prerequisites

- Phase 1 complete (Driver in roam-session)

## Steps

### 1. Fix dodeca-devtools dependencies

Remove `roam-stream` dependency - it's not needed for WASM.

```toml
# dodeca-devtools/Cargo.toml
[dependencies]
roam.workspace = true
roam-session.workspace = true
roam-websocket.workspace = true
# NO roam-stream!
```

### 2. Update state.rs imports

```rust
// Before
use roam_stream::{ConnectionHandle, HandshakeConfig, NoDispatcher, accept_framed};

// After
use roam_session::{ConnectionHandle, HandshakeConfig, NoDispatcher, accept_framed};
```

### 3. Verify WASM compilation

```bash
cargo build -p dodeca-devtools --target wasm32-unknown-unknown
```

### 4. Test end-to-end

1. Run dodeca server
2. Open browser
3. Verify devtools connects and receives events

## Cleanup

Once working, remove legacy protocol types from `dodeca-protocol`:
- `ClientMessage`
- `ServerMessage`

Keep only the roam RPC types:
- `DevtoolsService` trait
- `DevtoolsEvent` enum
