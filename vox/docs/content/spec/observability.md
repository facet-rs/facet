+++
title = "Observability"
description = "Tracing, metrics, and instrumentation"
weight = 90
+++

This document defines observability conventions for Rapace: distributed tracing, metrics, logging, and instrumentation patterns.

## Design Goals

Observability should not significantly impact performance. Implementations should use OpenTelemetry semantic conventions where applicable. Basic metrics should be cheap; detailed tracing may be sampled.

> **Note**: For deployment guidance on observability, see the [Deployment Guide](/guide/deployment/#observability).

## Distributed Tracing

### Trace Context Propagation

Rapace uses the metadata keys defined in [Metadata Conventions](@/spec/metadata.md):

| Key | Purpose |
|-----|---------|
| `rapace.trace_id` | 16-byte trace identifier |
| `rapace.span_id` | 8-byte span identifier |
| `rapace.parent_span_id` | 8-byte parent span ID |
| `rapace.trace_flags` | 1-byte trace flags (sampled, etc.) |
| `rapace.trace_state` | Vendor-specific trace state |

These map directly to [W3C Trace Context](https://www.w3.org/TR/trace-context/).

### Span Structure

Each RPC creates spans with this hierarchy:

```
[client] rapace.client/Service.method
    └── [network] rapace.transport/send
    └── [network] rapace.transport/recv
[server] rapace.server/Service.method
    └── [internal] application logic
```

### Span Naming Convention

| Component | Span Name Format | Example |
|-----------|-----------------|---------|
| Client call | `rapace.client/{Service}.{method}` | `rapace.client/Inventory.getItem` |
| Server handler | `rapace.server/{Service}.{method}` | `rapace.server/Inventory.getItem` |
| Transport send | `rapace.transport/send` | |
| Transport recv | `rapace.transport/recv` | |
| Stream item | `rapace.stream/{Service}.{method}/item` | |

### Span Attributes

#### Client Span Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `rpc.system` | string | Always `"rapace"` |
| `rpc.service` | string | Service name |
| `rpc.method` | string | Method name |
| `rpc.rapace.method_id` | int | Method ID (u32) |
| `rpc.rapace.channel_id` | int | Channel ID |
| `net.peer.name` | string | Server hostname |
| `net.peer.port` | int | Server port |
| `net.transport` | string | `"tcp"`, `"unix"`, `"shm"`, `"websocket"` |

#### Server Span Attributes

All client attributes, plus:

| Attribute | Type | Description |
|-----------|------|-------------|
| `rpc.rapace.priority` | int | Request priority (0-255) |
| `rpc.rapace.deadline_remaining_ms` | int | Remaining deadline |

#### Error Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `otel.status_code` | string | `"ERROR"` on failure |
| `otel.status_message` | string | Error message |
| `rpc.rapace.error_code` | int | Rapace error code |

### Span Events

| Event | When | Attributes |
|-------|------|------------|
| `request_sent` | Client sends request | `payload_size` |
| `response_received` | Client receives response | `payload_size`, `status_code` |
| `request_received` | Server receives request | `payload_size` |
| `response_sent` | Server sends response | `payload_size`, `status_code` |
| `stream_item` | Stream item sent/received | `item_index`, `payload_size` |
| `cancelled` | Request cancelled | `reason` |

### Sampling

Rapace respects the `rapace.trace_flags` sampled bit:

- If bit 0 is set (sampled): Record detailed spans
- If bit 0 is clear: Record only metrics, skip spans

Implementations may override sampling decisions locally based on:
- Error responses (always sample errors)
- High latency (sample slow requests)
- Debug mode (sample everything)

## Metrics

### Metric Naming Convention

All metrics use the `rapace.` prefix:

```
rapace.<component>.<metric>
```

### Client Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `rapace.client.calls` | Counter | calls | Total RPC calls |
| `rapace.client.call_duration` | Histogram | seconds | Call latency |
| `rapace.client.request_size` | Histogram | bytes | Request payload size |
| `rapace.client.response_size` | Histogram | bytes | Response payload size |
| `rapace.client.errors` | Counter | errors | Failed calls |
| `rapace.client.active_calls` | Gauge | calls | In-flight calls |

Labels:
- `service`: Service name
- `method`: Method name
- `status`: `"ok"` or error code
- `transport`: Transport type

### Server Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `rapace.server.calls` | Counter | calls | Total calls handled |
| `rapace.server.call_duration` | Histogram | seconds | Handler latency |
| `rapace.server.request_size` | Histogram | bytes | Request payload size |
| `rapace.server.response_size` | Histogram | bytes | Response payload size |
| `rapace.server.errors` | Counter | errors | Failed calls |
| `rapace.server.active_calls` | Gauge | calls | In-flight calls |
| `rapace.server.shed_calls` | Counter | calls | Load-shed calls |

Additional labels:
- `priority`: Priority bucket (`background`, `low`, `normal`, `high`, `critical`)

### Connection Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `rapace.connections` | Gauge | connections | Active connections |
| `rapace.connection_duration` | Histogram | seconds | Connection lifetime |
| `rapace.channels` | Gauge | channels | Active channels |
| `rapace.bytes_sent` | Counter | bytes | Total bytes sent |
| `rapace.bytes_received` | Counter | bytes | Total bytes received |
| `rapace.frames_sent` | Counter | frames | Total frames sent |
| `rapace.frames_received` | Counter | frames | Total frames received |

### SHM-Specific Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `rapace.shm.slots_total` | Gauge | slots | Total slots in segment |
| `rapace.shm.slots_free` | Gauge | slots | Available slots |
| `rapace.shm.slot_allocations` | Counter | allocations | Slot allocation count |
| `rapace.shm.slot_wait_duration` | Histogram | seconds | Time waiting for slot |
| `rapace.shm.ring_depth` | Gauge | entries | Current ring buffer depth |
| `rapace.shm.ring_capacity` | Gauge | entries | Ring buffer capacity |
| `rapace.shm.zero_copy_ratio` | Gauge | ratio | Fraction of zero-copy payloads |

Labels:
- `segment`: Segment identifier
- `size_class`: Slot size class (for Hub transport)

### Flow Control Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `rapace.credits_granted` | Counter | bytes | Credits granted |
| `rapace.credits_consumed` | Counter | bytes | Credits consumed |
| `rapace.credit_stalls` | Counter | stalls | Times sender blocked on credits |
| `rapace.credit_stall_duration` | Histogram | seconds | Duration of credit stalls |

### Histogram Buckets

Recommended histogram buckets:

**Latency (seconds)**:
```
[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]
```

**Size (bytes)**:
```
[64, 256, 1024, 4096, 16384, 65536, 262144, 1048576, 4194304]
```

## Logging

### Log Levels

| Level | Use |
|-------|-----|
| ERROR | RPC failures, protocol errors |
| WARN | Degraded performance, retries, timeouts |
| INFO | Connection lifecycle, significant events |
| DEBUG | Per-call logging, detailed flow |
| TRACE | Frame-level debugging |

### Structured Log Fields

All logs should include:

| Field | Description |
|-------|-------------|
| `trace_id` | Trace ID (if tracing enabled) |
| `span_id` | Span ID (if tracing enabled) |
| `connection_id` | Connection identifier |
| `channel_id` | Channel ID (if applicable) |
| `method_id` | Method ID (if applicable) |
| `service` | Service name (if applicable) |
| `method` | Method name (if applicable) |

### Log Message Conventions

```
// Connection events
[INFO]  connection established  {peer="192.168.1.1:8080", transport="tcp"}
[INFO]  connection closed       {peer="192.168.1.1:8080", reason="goaway"}

// RPC events
[DEBUG] call started           {service="Inventory", method="getItem", channel_id=1}
[DEBUG] call completed         {service="Inventory", method="getItem", status="ok", duration_ms=12}
[WARN]  call failed            {service="Inventory", method="getItem", error="deadline_exceeded"}

// Protocol events
[TRACE] frame sent             {channel_id=1, flags="DATA|EOS", size=256}
[TRACE] frame received         {channel_id=1, flags="DATA|EOS|RESPONSE", size=128}
```

## Health Checks

### Ping/Pong Liveness

Use Rapace-level Ping/Pong (control verb 5/6):

```rust
// Client health check
async fn is_healthy(&self) -> bool {
    let start = Instant::now();
    match timeout(Duration::from_secs(5), self.ping()).await {
        Ok(Ok(_)) => {
            metrics::histogram!("rapace.health.ping_duration")
                .record(start.elapsed().as_secs_f64());
            true
        }
        _ => false
    }
}
```

### Application Health RPC

Define a health service:

```rust
#[rapace::service]
trait Health {
    async fn check(&self, service: Option<String>) -> HealthResponse;
    async fn watch(&self, service: Option<String>) -> Streaming<HealthResponse>;
}

struct HealthResponse {
    status: ServingStatus,
}

enum ServingStatus {
    Unknown = 0,
    Serving = 1,
    NotServing = 2,
}
```

This follows the [gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md) pattern.

### Health Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rapace.health.ping_duration` | Histogram | Ping/Pong latency |
| `rapace.health.checks` | Counter | Health check count |
| `rapace.health.failures` | Counter | Failed health checks |

## Instrumentation Patterns

### Rust (tracing crate)

```rust
use tracing::{instrument, info_span, Instrument};

#[instrument(
    name = "rapace.server/Inventory.getItem",
    skip(self),
    fields(
        rpc.system = "rapace",
        rpc.service = "Inventory",
        rpc.method = "getItem",
        rpc.rapace.method_id = %method_id,
    )
)]
async fn handle_get_item(&self, request: GetItemRequest) -> Result<Item, RpcError> {
    // Handler implementation
}
```

### Go (OpenTelemetry)

```go
func (s *Server) GetItem(ctx context.Context, req *GetItemRequest) (*Item, error) {
    ctx, span := tracer.Start(ctx, "rapace.server/Inventory.getItem",
        trace.WithAttributes(
            attribute.String("rpc.system", "rapace"),
            attribute.String("rpc.service", "Inventory"),
            attribute.String("rpc.method", "getItem"),
        ),
    )
    defer span.End()

    // Handler implementation
}
```

### TypeScript (OpenTelemetry)

```typescript
async getItem(request: GetItemRequest): Promise<Item> {
    return tracer.startActiveSpan('rapace.server/Inventory.getItem', {
        attributes: {
            'rpc.system': 'rapace',
            'rpc.service': 'Inventory',
            'rpc.method': 'getItem',
        }
    }, async (span) => {
        try {
            // Handler implementation
            return item;
        } finally {
            span.end();
        }
    });
}
```

## Dashboards

### Recommended Panels

**Overview Dashboard**:
- Requests per second (by service, method)
- Error rate (by error code)
- P50/P95/P99 latency
- Active connections
- Active channels

**SHM Dashboard**:
- Slot utilization over time
- Ring buffer depth
- Zero-copy ratio
- Allocation wait time

**Per-Service Dashboard**:
- Call volume by method
- Latency by method
- Error breakdown
- Payload size distribution

## Alerting

### Recommended Alerts

| Alert | Condition | Severity |
|-------|-----------|----------|
| High error rate | `rapace.server.errors / rapace.server.calls > 0.01` for 5m | Warning |
| Very high error rate | `rapace.server.errors / rapace.server.calls > 0.1` for 1m | Critical |
| High latency | P99 latency > 2s for 5m | Warning |
| SHM slot exhaustion | `rapace.shm.slots_free / rapace.shm.slots_total < 0.1` | Warning |
| Connection churn | Connection rate > 100/s | Warning |

## Next Steps

- [Metadata Conventions](@/spec/metadata.md) – Trace context keys
- [Error Handling](@/spec/errors.md) – Error code classification
- [Overload & Draining](@/spec/overload.md) – Overload metrics
