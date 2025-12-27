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

Servers SHOULD monitor these metrics to detect overload:

| Indicator | Threshold | Recommended Action |
|-----------|-----------|-------------------|
| Pending requests | > max_pending_calls | Reject new calls |
| Memory usage | > 80% of limit | Start shedding |
| CPU utilization | > 90% sustained | Shed low-priority |
| Request latency | > 2x baseline | Shed non-critical |
| SHM slot exhaustion | 0 free slots | Block or reject |
| Channel count | > max_channels | Reject new channels |

### Limit Violation Responses

When negotiated limits are exceeded, the server MUST respond as follows:

| Limit | Channel Kind | Response |
|-------|--------------|----------|
| `max_pending_calls` | CALL | `CallResult { status: RESOURCE_EXHAUSTED }` |
| `max_channels` | CALL | `CallResult { status: RESOURCE_EXHAUSTED }` |
| `max_channels` | STREAM/TUNNEL | `CancelChannel { reason: ResourceExhausted }` |
| `max_payload_size` | Any | Protocol error; close connection (see [Transport Bindings](@/spec/transport-bindings.md)) |

**Rationale**:
- CALL channels receive `CallResult` so clients can retry with backoff
- STREAM/TUNNEL channels receive `CancelChannel` since they have no response envelope
- Payload size violations are protocol errors (the peer violated negotiated limits)

### Client-Side Indicators

Clients SHOULD detect server overload from:

- Increasing latency (adaptive timeout)
- `RESOURCE_EXHAUSTED` errors
- `UNAVAILABLE` errors
- `GoAway` control message

## Load Shedding

### Shedding Strategies

When overloaded, servers SHOULD shed load in priority order:

1. **Reject new connections**: Stop accepting transport connections
2. **Reject new calls**: Return `RESOURCE_EXHAUSTED` immediately
3. **Cancel low-priority requests**: Cancel requests with `priority < 96`
4. **Deadline-based shedding**: Cancel requests that can't finish in time

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

When a peer sends `GoAway`:

1. **Existing calls continue**: Calls on `channel_id <= last_channel_id` proceed normally
2. **New calls rejected**: Calls on `channel_id > last_channel_id` get `UNAVAILABLE`
3. **No new channels**: The sender will not open new channels
4. **Drain timeout**: The sender closes the connection after a grace period

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

When receiving `GoAway`, clients MUST:

1. **Stop new calls on this connection**: Route new RPCs elsewhere
2. **Complete in-flight calls**: Allow pending calls to finish
3. **Reconnect proactively**: Establish new connection to same or different server
4. **Respect drain window**: Don't flood with retries

Clients SHOULD:

- Use exponential backoff if reconnecting to same server
- Load balance to different servers if available
- Log the GoAway reason for debugging

### Grace Period

The draining peer SHOULD wait a grace period before closing:

```
grace_period = max(max_pending_deadline, 30 seconds)
```

After the grace period:

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

When receiving `RESOURCE_EXHAUSTED` or `UNAVAILABLE`:

1. **Check retryable**: If `rapace.retryable` is `0`, don't retry
2. **Respect retry_after**: Wait at least `rapace.retry_after_ms`
3. **Exponential backoff**: If no retry_after, use exponential backoff
4. **Jitter**: Add random jitter to prevent thundering herd

### Backoff Formula

```rust
fn backoff(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    let backoff = base_ms * 2u64.pow(attempt.min(10));
    let capped = backoff.min(max_ms);
    let jitter = rand::random::<f64>() * 0.3;  // 0-30% jitter
    Duration::from_millis((capped as f64 * (1.0 + jitter)) as u64)
}
```

### Circuit Breaker

Clients SHOULD implement circuit breakers:

| State | Behavior |
|-------|----------|
| Closed | Normal operation |
| Open | Fail immediately, no RPC |
| Half-Open | Allow one probe RPC |

Transition rules:
- Closed → Open: N consecutive failures
- Open → Half-Open: After cooldown period
- Half-Open → Closed: Probe succeeds
- Half-Open → Open: Probe fails

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
