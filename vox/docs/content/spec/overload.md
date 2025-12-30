+++
title = "Overload & Draining"
description = "Graceful degradation and server shutdown"
weight = 85
+++

This document defines how Rapace handles overload conditions, load shedding, and graceful shutdown (draining).

## Design Goals

1. **Graceful degradation**: Overloaded servers shed load predictably
2. **Zero-downtime deploys**: Servers drain existing connections before shutdown
3. **Client awareness**: Clients know when to reconnect elsewhere
4. **Backpressure**: Flow control prevents runaway memory usage

## Overload Detection

### Server-Side Indicators

Servers should monitor metrics like pending requests, memory usage, CPU utilization, and request latency to detect overload. See [Deployment Guide](/guide/deployment/#server-side-metrics) for recommended thresholds.

### Limit Violation Responses

r[overload.limits.response]
When negotiated limits are exceeded, the server MUST respond as follows:

| Limit | Context | Response |
|-------|---------|----------|
| `max_channels` | `OpenChannel` received | `CancelChannel { reason: ResourceExhausted }` |
| `max_pending_calls` | Request received on CALL | `CallResult { status: RESOURCE_EXHAUSTED }` |
| `max_payload_size` | Any frame | Protocol error; close connection (see [Transport Bindings](@/spec/transport-bindings.md)) |

**Rationale**:
- `max_channels` is checked at `OpenChannel` time, before any request data. Use `CancelChannel` since no CallResult envelope exists yet.
- `max_pending_calls` is checked when a request arrives on an already-open CALL channel. Use `CallResult` so clients can retry with backoff.
- Payload size violations are protocol errors (the peer violated negotiated limits).

### Client-Side Indicators

Clients SHOULD detect server overload from:

- Increasing latency (adaptive timeout)
- `RESOURCE_EXHAUSTED` errors
- `UNAVAILABLE` errors
- `GoAway` control message

## Load Shedding

### Shedding Strategies

When overloaded, servers should shed load progressively: reject new connections first, then new calls, then low-priority requests, then deadline-based shedding. See [Deployment Guide](/guide/deployment/#load-shedding-order) for details.

### RESOURCE_EXHAUSTED Response

When shedding a request:

```rust
CallResult {
    status: Status {
        code: ErrorCode::ResourceExhausted as u32,
        message: "server overloaded".into(),
        details: vec![],  // Optional: structured overload info
    },
    trailers: vec![
        ("rapace.retryable".into(), vec![1]),
        ("rapace.retry_after_ms".into(), 100u32.to_le_bytes().to_vec()),
    ],
    body: None,
}
```

### Priority-Based Shedding

When shedding based on priority:

1. Calculate current load (e.g., pending_requests / max_pending_calls)
2. Compute priority threshold: `shed_below = load * 255`
3. Reject requests with `priority < shed_below`

Example:
- 50% load: shed priority < 128 (background/low)
- 80% load: shed priority < 204 (background + low + most normal)
- 95% load: shed priority < 242 (almost everything except critical)

### Admission Control

Servers MAY implement admission control before processing:

```rust
fn should_admit(request: &Request, load: f64) -> bool {
    let priority = request.metadata.get("rapace.priority")
        .map(|v| v[0])
        .unwrap_or(128);
    
    // Probabilistic admission based on priority
    let admit_probability = (priority as f64 / 255.0).powf(1.0 / (1.0 - load));
    rand::random::<f64>() < admit_probability
}
```

## Graceful Shutdown (Draining)

### GoAway Control Message

To initiate graceful shutdown, send a `GoAway` message on channel 0:

```rust
GoAway {
    reason: GoAwayReason,
    last_channel_id: u32,  // Last channel ID the server will process
    message: String,        // Human-readable reason
    metadata: Vec<(String, Vec<u8>)>,  // Extension data
}

enum GoAwayReason {
    Shutdown = 1,           // Planned shutdown
    Maintenance = 2,        // Maintenance window
    Overload = 3,           // Server overloaded
    ProtocolError = 4,      // Client misbehaving
}
```

### Control Verb

| method_id | Verb | Payload |
|-----------|------|---------|
| 7 | `GoAway` | `{ reason, last_channel_id, message, metadata }` |

### GoAway Semantics

r[overload.goaway.existing]
When a peer sends `GoAway`, calls on `channel_id <= last_channel_id` MUST be allowed to proceed normally.

r[overload.goaway.new-rejected]
After receiving `GoAway`, `OpenChannel` requests with `channel_id > last_channel_id` MUST receive `CancelChannel { reason: ResourceExhausted }`.

r[overload.goaway.no-new]
After sending `GoAway`, a peer MUST NOT open new channels.

r[overload.goaway.drain]
The sender MUST close the connection after a grace period.

### Drain Sequence

```
Server                                      Client
   │                                           │
   │  GoAway(reason=Shutdown, last=123)        │
   ├──────────────────────────────────────────▶│
   │                                           │
   │                    [client stops new calls]│
   │                                           │
   │  [finish pending calls on ch 1..123]      │
   │                                           │
   │  [wait for grace period]                  │
   │                                           │
   │  [close transport]                        │
   ├──────────────────────────────────────────▶│
   │                                           │
```

### Client Behavior on GoAway

r[overload.goaway.client.stop]
When receiving `GoAway`, clients MUST stop sending new calls on this connection and route new RPCs elsewhere.

r[overload.goaway.client.complete]
Clients MUST allow pending in-flight calls to complete.

r[overload.goaway.client.reconnect]
Clients MUST establish a new connection to the same or a different server proactively.

r[overload.goaway.client.respect]
Clients MUST NOT flood with retries; they MUST respect the drain window.

Clients should use exponential backoff if reconnecting to the same server and load balance to different servers if available. See [Deployment Guide](/guide/deployment/#goaway-client-behavior) for details.

### Grace Period

r[overload.drain.grace-period]
The draining peer SHOULD wait a grace period before closing:

```
grace_period = max(latest_pending_deadline - now(), 30 seconds)
```

Where `latest_pending_deadline` is the furthest deadline among all in-flight calls on this connection. If no calls have explicit deadlines, implementations SHOULD use a 30-second default.

r[overload.drain.after-grace]
After the grace period, implementations MUST:

1. Cancel any remaining in-flight calls with `DeadlineExceeded`
2. Send `CloseChannel` for all open channels
3. Close the transport connection

### Bidirectional GoAway

Both peers can send `GoAway` independently:

- Server sends `GoAway` for shutdown
- Client sends `GoAway` if it's also shutting down

When both peers have sent `GoAway`, the connection closes after both have no pending work.

## Backpressure via Flow Control

Credit-based flow control prevents memory exhaustion without load shedding.

### Credit Starvation

When a sender exhausts credits:

1. **Block**: Wait for `GrantCredits` (may cause deadlock if both sides wait)
2. **Buffer locally**: Buffer frames until credits arrive (memory risk)
3. **Fail the stream**: Cancel with `RESOURCE_EXHAUSTED`

Recommendation: **Time-bounded wait** with fallback to cancel:

```rust
match timeout(Duration::from_secs(5), wait_for_credits()).await {
    Ok(credits) => send_frame(),
    Err(_) => cancel_stream(ResourceExhausted),
}
```

### Receiver Backpressure

When a receiver can't keep up:

1. **Withhold credits**: Don't grant credits until buffers drain
2. **Smaller grants**: Grant fewer credits at a time
3. **Signal overload**: Include metadata in `GrantCredits` indicating pressure

### SHM-Specific Backpressure

For shared memory transports:

- Slot exhaustion naturally provides backpressure
- Sender blocks on `alloc_slot()` until receiver frees slots
- No explicit credit messages needed (slot availability = credits)

## Health Checking

### Liveness vs Readiness

| Check | Purpose | Response |
|-------|---------|----------|
| Liveness | Is the process alive? | TCP connect succeeds |
| Readiness | Can it serve requests? | Ping/Pong succeeds |

### Rapace-Level Health Check

Use Ping/Pong on the control channel:

```
Client                                      Server
   │                                           │
   │  Ping(payload=[timestamp])                │
   ├──────────────────────────────────────────▶│
   │                                           │
   │                    Pong(payload=[timestamp])
   │◀──────────────────────────────────────────┤
   │                                           │
```

- **Success**: Pong received within timeout → server is healthy
- **Timeout**: No Pong → server is unhealthy
- **Connection closed**: Server is down

### Health Check Interval

Recommendations:

| Environment | Interval | Timeout |
|-------------|----------|---------|
| Local/SHM | 1 second | 100ms |
| Same datacenter | 5 seconds | 1 second |
| Cross-region | 30 seconds | 5 seconds |

### Application Health Endpoint

For deeper health checks, define a health RPC:

```rust
#[rapace::service]
trait Health {
    async fn check(&self, service: String) -> HealthResponse;
}

struct HealthResponse {
    status: HealthStatus,
    details: HashMap<String, ComponentStatus>,
}

enum HealthStatus {
    Serving,
    NotServing,
    Unknown,
}
```

## Client Retry Behavior

### Retry on Overload

r[overload.retry.retryable]
When receiving `RESOURCE_EXHAUSTED` or `UNAVAILABLE`, clients MUST check the `rapace.retryable` trailer; if it is `0`, the client MUST NOT retry.

r[overload.retry.retry-after]
Clients MUST wait at least `rapace.retry_after_ms` milliseconds before retrying if present.

If no `retry_after` is provided, clients should use exponential backoff with random jitter. Clients should also implement circuit breakers. See [Deployment Guide](/guide/deployment/#client-retry-behavior) for backoff formulas and circuit breaker patterns.

## Server Shutdown Sequence

Complete shutdown procedure:

```
1. Stop accepting new connections
2. Send GoAway to all existing connections
3. Wait for grace period (or all calls complete)
4. Cancel remaining calls with DeadlineExceeded
5. Close all transport connections
6. Exit process
```

### Kubernetes Integration

For Kubernetes:

```yaml
spec:
  terminationGracePeriodSeconds: 60
  containers:
  - name: myservice
    lifecycle:
      preStop:
        exec:
          command: ["/bin/sh", "-c", "kill -SIGTERM 1 && sleep 55"]
```

The application should:

1. Catch `SIGTERM`
2. Start draining (send GoAway)
3. Wait for connections to drain
4. Exit within `terminationGracePeriodSeconds`

## Metrics and Observability

Servers SHOULD expose these metrics:

| Metric | Type | Description |
|--------|------|-------------|
| `rapace_active_connections` | Gauge | Current connection count |
| `rapace_pending_calls` | Gauge | In-flight RPC count |
| `rapace_rejected_calls_total` | Counter | Calls rejected due to overload |
| `rapace_goaway_sent_total` | Counter | GoAway messages sent |
| `rapace_drain_duration_seconds` | Histogram | Time to drain connections |

## Summary

| Scenario | Server Action | Client Action |
|----------|---------------|---------------|
| Overloaded | Reject with `RESOURCE_EXHAUSTED` | Backoff and retry |
| Shutting down | Send `GoAway`, drain | Finish in-flight, reconnect |
| Slow client | Withhold credits | Speed up or cancel |
| Misbehaving client | `GoAway` + close | Fix bug |

## Next Steps

- [Cancellation & Deadlines](@/spec/cancellation.md) – Cancel semantics during drain
- [Error Handling](@/spec/errors.md) – UNAVAILABLE, RESOURCE_EXHAUSTED codes
- [Core Protocol](@/spec/core.md) – Control channel and flow control
