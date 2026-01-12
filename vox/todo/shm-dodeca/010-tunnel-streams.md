# Phase 010: Tunnel Streams (Tx/Rx<Vec<u8>> Replacement for rapace::TunnelStream)

## Goal

Support dodeca's "TCP tunneling" use case (host accepts connections, cell
handles bytes) in the roam stack over roam-shm.

## Current State

- Dodeca uses `rapace::TunnelStream<AnyTransport>` (and friends) to tunnel bytes.
- Roam already supports streaming `Vec<u8>` payloads (`roam_wire::Value::Bytes(Vec<u8>)`)
  and stream handles (`roam_session::Tx<T>` / `roam_session::Rx<T>`), so "tunnel"
  can just be a pair of `Tx<Vec<u8>>`/`Rx<Vec<u8>>`.

## Implementation (DONE)

Generic `AsyncRead`/`AsyncWrite` tunnel adapters are now implemented in `roam-session`
and re-exported from `roam`. The implementation is generic over any async stream type,
not just `TcpStream`.

### API

```rust
use roam::{Tunnel, tunnel_pair, tunnel_stream, DEFAULT_TUNNEL_CHUNK_SIZE};

// Create connected tunnel pair
let (local, remote) = tunnel_pair();

// Bridge an async stream to a tunnel (spawns two tasks)
let (read_handle, write_handle) = tunnel_stream(socket, local, DEFAULT_TUNNEL_CHUNK_SIZE);

// Or use the lower-level pump functions directly:
use roam::{pump_read_to_tx, pump_rx_to_write};
```

### Types

- `Tunnel` - Bidirectional byte tunnel (`tx: Tx<Vec<u8>>`, `rx: Rx<Vec<u8>>`)
- `tunnel_pair()` - Create connected tunnel pair
- `tunnel_stream()` - Bridge `AsyncRead + AsyncWrite` to tunnel (spawns tasks)
- `pump_read_to_tx()` - Pump bytes from reader to channel
- `pump_rx_to_write()` - Pump bytes from channel to writer
- `DEFAULT_TUNNEL_CHUNK_SIZE` - 32KB default chunk size

### Files Changed

- `Cargo.toml` - Added `io-util` feature to tokio workspace dep
- `rust/roam-session/src/lib.rs` - Added tunnel types and pump functions
- `rust/roam/src/lib.rs` - Re-exported tunnel types

## Tasks

- [x] Implement host pumps (generic `AsyncRead`/`AsyncWrite` <-> `Tx/Rx<Vec<u8>>`)
- [x] Implement cell pumps (`Tx/Rx<Vec<u8>>` <-> cell logic) - same functions work both sides
- [x] Add tests (loopback with `tokio::io::duplex`)

## Notes

- Chunk size is configurable via `chunk_size` parameter (default 32KB)
- Close semantics come naturally from channel close (drop `Tx` → `Rx::recv()` returns `None`)
- Pump functions are intentionally simple - backpressure comes from the channel
