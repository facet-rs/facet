+++
title = "roam cross-cell tracing"
description = "Specification for cross-cell tracing over roam RPC"
weight = 60
+++

# Introduction

This document specifies roam-tracing, a system for forwarding tracing events
from cells (sandboxed guest processes) to the host process over roam RPC.

# Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│  HOST                                                       │
│  ┌──────────────────┐    ┌──────────────────────────────┐  │
│  │ HostTracingState │◄───│  mpsc::Receiver<TaggedRecord>│  │
│  │                  │    │  (consumer: TUI/logs/etc.)   │  │
│  └──────────────────┘    └──────────────────────────────┘  │
│         │                                                   │
│         ▼ service_for_peer(id, name)                       │
│  ┌──────────────────┐                                      │
│  │HostTracingService│  ◄── implements HostTracing trait    │
│  │  (one per cell)  │      get_tracing_config()            │
│  └──────────────────┘      emit_tracing(records)           │
└─────────────────────────────────────────────────────────────┘
                             ▲
                             │ RPC calls
                             │
┌─────────────────────────────────────────────────────────────┐
│  CELL                                                       │
│  ┌─────────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │ CellTracingLayer│───►│ LossyBuffer  │───►│drain task │  │
│  │   (Layer<S>)    │    │  (bounded)   │    │(RPC calls)│  │
│  └─────────────────┘    └──────────────┘    └───────────┘  │
│         ▲                                         │         │
│  ┌──────┴──────┐                                  │         │
│  │ tracing::{  │                                  ▼         │
│  │  info!(),   │                     HostTracingClient      │
│  │  #[instrument]                    .emit_tracing(batch)   │
│  │ }           │                                            │
│  └─────────────┘                                            │
└─────────────────────────────────────────────────────────────┘
```

# Design Principles

> r[tracing.design.cell-pushes]
>
> Cells push tracing records to the host. The host does not poll or subscribe.
> This allows tracing to work from the very first moment the cell connects.

> r[tracing.design.lossy]
>
> The system is lossy by design. If the buffer is full, oldest records are
> dropped. Tracing MUST NOT block the application.

> r[tracing.design.batched]
>
> Records are sent in batches to reduce RPC overhead. The drain task collects
> records from the buffer and sends them periodically.

> r[tracing.design.config-query]
>
> Cells query the tracing configuration from the host on startup. The host
> can also push configuration updates to cells at any time.

# Services

Two complementary services enable bidirectional communication:

## HostTracing (Host implements, Cell calls)

> r[tracing.service.host-tracing]
>
> The `HostTracing` service is implemented by the host and called by cells.

```rust
#[service]
pub trait HostTracing {
    /// Get the current tracing configuration.
    async fn get_tracing_config(&self) -> TracingConfig;

    /// Push a batch of tracing records to the host.
    async fn emit_tracing(&self, records: Vec<TracingRecord>);
}
```

> r[tracing.service.host-tracing.get-config]
>
> `get_tracing_config()` returns the host's current filter settings. Cells
> SHOULD call this on startup before emitting records.

> r[tracing.service.host-tracing.emit]
>
> `emit_tracing()` delivers a batch of records to the host. This is
> fire-and-forget — cells do not wait for acknowledgment.

## CellTracing (Cell implements, Host calls)

> r[tracing.service.cell-tracing]
>
> The `CellTracing` service is implemented by cells and called by the host
> to push configuration updates.

```rust
#[service]
pub trait CellTracing {
    /// Update the tracing configuration.
    async fn configure(&self, config: TracingConfig) -> ConfigResult;
}
```

> r[tracing.service.cell-tracing.configure]
>
> `configure()` updates the cell's tracing filter settings. The cell applies
> these settings immediately to its `CellTracingLayer`.

# Types

## TracingConfig

> r[tracing.type.config]
>
> `TracingConfig` controls what records are captured and forwarded.

```rust
pub struct TracingConfig {
    /// Minimum level to emit (records below this are dropped).
    pub min_level: Level,
    /// Target filters (empty = accept all).
    pub filters: Vec<String>,
    /// Whether to include span enter/exit events (verbose).
    pub include_span_events: bool,
}
```

## TracingRecord

> r[tracing.type.record]
>
> `TracingRecord` represents a single tracing event or span lifecycle event.

```rust
pub enum TracingRecord {
    SpanEnter {
        id: SpanId,
        parent: Option<SpanId>,
        target: String,
        name: String,
        level: Level,
        fields: Vec<(String, FieldValue)>,
        timestamp_ns: u64,
    },
    SpanExit { id: SpanId, timestamp_ns: u64 },
    SpanClose { id: SpanId, timestamp_ns: u64 },
    Event {
        parent: Option<SpanId>,
        target: String,
        level: Level,
        message: Option<String>,
        fields: Vec<(String, FieldValue)>,
        timestamp_ns: u64,
    },
}
```

> r[tracing.type.record.span-id]
>
> `SpanId` values are cell-local. The host tags records with `peer_id` to
> distinguish spans from different cells.

> r[tracing.type.record.timestamp]
>
> `timestamp_ns` is a monotonic nanosecond timestamp relative to the cell's
> start time. It is NOT wall-clock time and NOT synchronized across cells.

## TaggedRecord

> r[tracing.type.tagged]
>
> The host wraps each record with peer identification.

```rust
pub struct TaggedRecord {
    pub peer_id: PeerId,
    pub peer_name: Option<String>,
    pub record: TracingRecord,
}
```

# Protocol Flow

## Startup

> r[tracing.flow.startup]
>
> On startup, a cell SHOULD:
> 1. Initialize `CellTracingLayer` and install it as a tracing subscriber layer
> 2. Register `CellTracingDispatcher` with the cell's service dispatcher
> 3. After `establish_guest()`, call `service.spawn_drain(handle)`
> 4. The drain task queries config via `get_tracing_config()`

## Steady State

> r[tracing.flow.steady]
>
> During normal operation:
> 1. Application code emits tracing events (`info!()`, `#[instrument]`, etc.)
> 2. `CellTracingLayer` captures events and pushes to `LossyBuffer`
> 3. Drain task periodically collects batches and calls `emit_tracing()`
> 4. Host receives `TaggedRecord`s via its `mpsc::Receiver`

## Configuration Updates

> r[tracing.flow.config-update]
>
> The host can update a cell's configuration at any time by calling
> `CellTracingClient::configure()` on the cell's connection handle.

# Service Composition

Both cell and host use `RoutedDispatcher` to compose tracing services with
their application-specific services.

## Cell Side

```rust
let (layer, service) = init_cell_tracing(1024);
tracing_subscriber::registry().with(layer).init();

// Compose: CellTracing + user services
let combined = RoutedDispatcher::new(
    CellTracingDispatcher::new(service.clone()),
    user_dispatcher,
);

let (handle, driver) = establish_guest(transport, combined);
service.spawn_drain(handle.clone());
```

## Host Side

```rust
let tracing_state = HostTracingState::new(4096);
let mut records = tracing_state.take_receiver().unwrap();

// For each cell:
let tracing_service = tracing_state.service_for_peer(peer_id, Some(name));
let combined = RoutedDispatcher::new(
    HostTracingDispatcher::new(tracing_service),
    host_service_dispatcher,
);
```

# Implementation Notes

## Buffer Sizing

> r[tracing.impl.buffer-size]
>
> The cell-side buffer size (passed to `init_cell_tracing()`) determines how
> many records can be queued before dropping. Typical values: 1024-4096.

> r[tracing.impl.host-buffer-size]
>
> The host-side buffer size (passed to `HostTracingState::new()`) determines
> how many tagged records can queue before backpressure. Typical: 4096-16384.

## Drain Task Failure

> r[tracing.impl.drain-panic]
>
> If the drain task exits unexpectedly, it MUST panic with a clear message.
> Silent tracing failures are unacceptable — they make debugging impossible.

## Serialization

> r[tracing.impl.serialization]
>
> All types use [Facet](https://facet.rs) for serialization, NOT serde.
> This matches the rest of the roam ecosystem.
