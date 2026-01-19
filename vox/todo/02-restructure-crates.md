# Phase 2: Restructure Crates

## Goal

Move generic protocol machinery out of `roam-stream` into `roam-session`,
leaving `roam-stream` as purely byte-stream (COBS) specific.

## Current State

```
roam-stream/
├── driver.rs       # Driver, accept, connect, HandshakeConfig ← MOVE
├── framing.rs      # CobsFramed ← KEEP
├── transport.rs    # MessageTransport trait ← MOVE
└── lib.rs          # Re-exports

roam-session/
├── lib.rs          # ConnectionHandle, Caller, channels, dispatchers
└── (tunnel stuff)  # tunnel_stream, Tunnel ← KEEP (SHM-specific)
```

## Target State

```
roam-session/
├── lib.rs
├── runtime.rs       # NEW: runtime abstraction (Phase 1)
├── transport.rs     # MOVED: MessageTransport trait
├── driver.rs        # MOVED: Driver, HandshakeConfig
├── connection.rs    # ConnectionHandle, Caller (maybe split out)
└── (existing)       # channels, dispatchers, tunnel stuff

roam-stream/
├── framing.rs       # CobsFramed
├── lib.rs           # Re-exports CobsFramed, convenience wrappers
└── (thin)           # Maybe accept/connect that wrap CobsFramed + roam_session::accept_framed
```

## Migration Steps

### Step 1: Move MessageTransport trait

```bash
# From roam-stream/src/transport.rs
# To roam-session/src/transport.rs
```

Update imports in:
- roam-stream (now imports from roam-session)
- roam-websocket (now imports from roam-session)

### Step 2: Move Driver and related types

Move from `roam-stream/src/driver.rs` to `roam-session/src/driver.rs`:
- `Driver<T, D>`
- `HandshakeConfig`
- `ConnectionError`
- `Negotiated`
- `NoDispatcher`
- `accept_framed()`
- `connect_framed()`
- `FramedClient`
- `MessageConnector` trait
- `Connector` trait (maybe keep in roam-stream since it's for byte streams?)
- `RetryPolicy`

### Step 3: Update roam-stream

`roam-stream` becomes thin:
- Exports `CobsFramed`
- Maybe provides `accept()` / `connect()` convenience functions that:
  - Wrap a byte stream in CobsFramed
  - Call `roam_session::accept_framed()`

### Step 4: Update roam-websocket

Change imports:
```rust
// Before
use roam_stream::{MessageTransport, Driver, accept_framed, ...};

// After
use roam_session::{MessageTransport, Driver, accept_framed, ...};
```

## Dependency Changes

```
Before:
  roam-websocket → roam-stream → roam-session

After:
  roam-websocket → roam-session
  roam-stream → roam-session (for CobsFramed to use accept_framed)
```

## Breaking Changes

External users of `roam-stream` will need to update imports.
This is acceptable since roam is pre-1.0.
