# Phase 009: Tracing Across Cells (Roam-Native Replacement for rapace_tracing)

## Goal

Replace the `rapace_tracing` + `rapace_cell` tracing plumbing used by dodeca
with a roam-native tracing story that works over roam-shm.

Minimum target: cells can emit tracing events/spans that the host can collect
and present (or forward to a subscriber), with backpressure and bounded memory.

## Current State

- ✅ **IMPLEMENTED** in `rust/roam-tracing/` crate
- Works with both stream and SHM transports

## Target API

Host side:

```rust
// Host installs a collector and optionally pushes config to the cell.
let mut tracing = roam_tracing::TracingHost::new(4096);
let mut records = tracing.take_receiver().unwrap();
tracing.register_peer(peer_id, Some("cell-name".into()), &handle).await?;

// Consume records
while let Some(tagged) = records.recv().await {
    println!("[{}] {:?}", tagged.peer_name.unwrap_or_default(), tagged.record);
}
```

Cell side:

```rust
// Cell installs a layer that forwards to host.
let (layer, service) = roam_tracing::init_cell_tracing(1024);

tracing_subscriber::registry()
    .with(layer)
    .init();

// Register service with dispatcher
let tracing_dispatcher = CellTracingDispatcher::new(service);
```

## Design

### Protocol Shape

Implemented as a single service:

- `CellTracing` service (implemented by cell):
  - `configure(config: TracingConfig) -> ConfigResult` - host pushes config
  - `subscribe(sink: Tx<TracingRecord>)` - host establishes record stream

### Record Format

```rust
pub enum TracingRecord {
    SpanEnter { id, parent, target, name, level, fields, timestamp_ns },
    SpanExit { id, timestamp_ns },
    SpanClose { id, timestamp_ns },
    Event { parent, target, level, message, fields, timestamp_ns },
}
```

### Key Constraints (all met)

- ✅ **Lossy buffering** - Cell-side `LossyBuffer` drops oldest; host-side drops newest under backpressure
- ✅ **Never blocks** - Layer pushes to local buffer; async drain to host
- ✅ **Bounded memory** - Configurable buffer size at both cell and host
- ✅ **Transport agnostic** - Works over stream and SHM via `Tx<TracingRecord>`

## Implementation

### Crate Structure

```
rust/roam-tracing/
├── Cargo.toml
└── src/
    ├── lib.rs          # Public API, init_cell_tracing()
    ├── record.rs       # TracingRecord, Level, FieldValue, SpanId
    ├── service.rs      # CellTracing service trait
    ├── host.rs         # TracingHost implementation
    ├── cell.rs         # CellTracingLayer, CellTracingService
    └── buffer.rs       # LossyBuffer<T>
```

### Components

1. **TracingRecord** (`record.rs`) - Compact event/span schema using facet
2. **CellTracing service** (`service.rs`) - RPC service trait with `#[roam::service]`
3. **LossyBuffer** (`buffer.rs`) - Bounded ring buffer, drops oldest on overflow
4. **CellTracingLayer** (`cell.rs`) - `tracing_subscriber::Layer` implementation
5. **CellTracingService** (`cell.rs`) - Implements `CellTracing` trait
6. **TracingHost** (`host.rs`) - Collects from multiple peers, tags with peer_id

## Tasks

- [x] Decide crate layout (`roam-tracing` vs `roam-tracing-proto`) → single crate
- [x] Define record format (events + spans + fields)
- [x] Implement host sink server + optional config client
- [x] Implement cell forwarding layer + bounded buffering behavior
- [x] Add tests (in-process layer tests)
- [ ] Add integration tests over SHM transport (requires test harness)
- [ ] Integration with dodeca spawn lifecycle

## Notes

- Keep this independent of any specific UI (TUI/web); the host should expose a
  minimal API to subscribe/consume records.
- If we need "crash last N logs", prefer a ring buffer in host memory.
